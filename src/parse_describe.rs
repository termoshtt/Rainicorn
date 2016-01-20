// Copyright 2015 Bruno Medeiros
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use ::util::core::*;
use ::util::string::*;
use ::source_model::*;

use ::syntex_syntax::syntax::ast;
use ::syntex_syntax::parse::{ self, ParseSess };
use ::syntex_syntax::visit;
use ::syntex_syntax::codemap:: { self, Span, CodeMap};
use ::syntex_syntax::errors:: { Handler, RenderSpan, Level, emitter };

use std::boxed::Box;
use std::path::Path;

use ::token_writer::TokenWriter;

use std::cell::RefCell;
use std::rc::*;
use std::io;
use std::io::Write;

/* ----------------- Model ----------------- */

pub enum StructureElementKind {
	Var,
	Function,
	Struct,
	Impl,
	Trait,
	Enum,
	EnumVariant,
	ExternCrate,
	Mod,
	Use,
	TypeAlias,
}


use std::fmt;

impl StructureElementKind {
	pub fn writeString(&self, out : &mut fmt::Write) -> fmt::Result {
		match *self {
			StructureElementKind::Var => out.write_str("Var"),
			StructureElementKind::Function => out.write_str("Function"),
			StructureElementKind::Struct => out.write_str("Struct"),
			StructureElementKind::Impl => out.write_str("Impl"),
			StructureElementKind::Trait => out.write_str("Trait"),
			StructureElementKind::Enum => out.write_str("Enum"),
			StructureElementKind::EnumVariant => out.write_str("EnumVariant"),
			StructureElementKind::ExternCrate => out.write_str("ExternCrate"),
			StructureElementKind::Mod => out.write_str("Mod"),
			StructureElementKind::Use => out.write_str("Use"),
			StructureElementKind::TypeAlias => out.write_str("TypeAlias"),
		}
	}
}


/* -----------------  ----------------- */

pub fn parse_analysis_forStdout(source : &str) {
	parse_analysis(source, StdoutWrite(io::stdout())).ok();
	println!("");
	io::stdout().flush().ok();
}


use ::structure_visitor::StructureVisitor;

pub fn parse_analysis<T : fmt::Write + 'static>(source : &str, out : T) -> Result<T> {
	let outRc = Rc::new(RefCell::new(out));
	try!(parse_analysis_do(source, outRc.clone()));
	let res = unwrapRcRefCell(outRc);
	return Ok(res);
}

pub fn parse_analysis_do(source : &str, out : Rc<RefCell<fmt::Write>>) -> Void {
	
	let tokenWriter = TokenWriter { out : out };
	let tokenWriterRc : Rc<RefCell<TokenWriter>> = Rc::new(RefCell::new(tokenWriter));
	
	try!(tokenWriterRc.borrow_mut().writeRaw("RUST_PARSE_DESCRIBE 0.1 {\n"));
	try!(parse_analysis_contents(source, tokenWriterRc.clone()));
	try!(tokenWriterRc.borrow_mut().writeRaw("\n}"));
	
	Ok(())
}

pub fn parse_analysis_contents(source : &str, tokenWriterRc : Rc<RefCell<TokenWriter>>) -> Void {
	
	let fileLoader = Box::new(DummyFileLoader::new());
	let codemap = Rc::new(CodeMap::with_file_loader(fileLoader));
	
	let myEmitter = MessagesHandler::new(codemap.clone());
	let messages = myEmitter.messages.clone();
	let handler = Handler::with_emitter(true, true , Box::new(myEmitter));
	let sess = ParseSess::with_span_handler(handler, codemap.clone());
	
	let krate_result = parse_crate(source, &sess);
	
	try!(tokenWriterRc.borrow_mut().writeRaw("MESSAGES {\n"));
	for msg in &messages.lock().unwrap() as &Vec<SourceMessage> {
		try!(output_message(&mut tokenWriterRc.borrow_mut(), msg.sourcerange, &msg.message, &msg.status_level));
	}
	try!(tokenWriterRc.borrow_mut().writeRaw("}"));
	
	let mut tokenWriter = tokenWriterRc.borrow_mut();
	
	match krate_result {
		Err(_err) => {
			// Error messages should have been written to out
		}
		Ok(ref krate) => { 
			let mut visitor : StructureVisitor = StructureVisitor::new(&codemap, &mut tokenWriter);  
			visit::walk_crate(&mut visitor, &krate);
		}
	};
	
	Ok(())
}


/* -----------------  ----------------- */


use std::ffi::OsStr;

/// A FileLoader that loads any file successfully
pub struct DummyFileLoader {
   	modName : &'static OsStr,
}

impl DummyFileLoader {
	fn new() -> DummyFileLoader {
		DummyFileLoader { modName : OsStr::new("mod.rs") } 
	}
}

impl codemap::FileLoader for DummyFileLoader {
    fn file_exists(&self, path: &Path) -> bool {
    	return path.file_name() == Some(self.modName);
    }
	
    fn read_file(&self, _path: &Path) -> io::Result<String> {
        Ok(String::new())
    }
}

pub fn parse_crate<'a>(source : &str, sess : &'a ParseSess) -> parse::PResult<'a, ast::Crate> 
{
	let cfg = vec![];
	let krateName = "_file_module_".to_string();
	
	return parse::new_parser_from_source_str(&sess, cfg, krateName, source.to_string()).parse_crate_mod();
}


struct MessagesHandler {
	codemap : Rc<CodeMap>,
	messages : Arc<Mutex<Vec<SourceMessage>>>,
}

use std::sync::{ Arc, Mutex };


unsafe impl ::std::marker::Send for MessagesHandler { } // FIXME: need to review this

impl MessagesHandler {
	
	fn new(codemap : Rc<CodeMap>, ) -> MessagesHandler {
		MessagesHandler { codemap : codemap, messages : Arc::new(Mutex::new(vec![])) }
	}
	
	fn writeMessage_handled(&mut self, sourcerange : Option<SourceRange>, msg: &str, lvl: StatusLevel) {
		
		let msg = SourceMessage{ status_level : lvl , sourcerange : sourcerange,  message : String::from(msg) };
		
		let mut messages = self.messages.lock().unwrap();
		
		messages.push(msg);
		
	}
	
}

impl emitter::Emitter for MessagesHandler {
	
    fn emit(&mut self, cmsp: Option<Span>, msg: &str, code: Option<&str>, lvl: Level) {
    	
    	match code {
    		None => {}
    		Some(code) => {
    			io::stderr().write_fmt(format_args!("Code: {}\n", code)).unwrap();
    			panic!("What is code: Option<&str>??");
			}
    	}
    	
    	
		let sourcerange = match cmsp {
			Some(span) => Some(SourceRange::new(&self.codemap, span)),
			None => None,
		};
		
		self.writeMessage_handled(sourcerange, msg, level_to_status_level(lvl));
    }
    
    fn custom_emit(&mut self, _: RenderSpan, msg: &str, lvl: Level) {
    	if match lvl { Level::Help | Level::Note => true, _ => false } {
    		return;
    	}
    	
    	self.writeMessage_handled(None, msg, level_to_status_level(lvl));
    }
	
}

fn level_to_status_level(lvl: Level) -> StatusLevel {
	match lvl { 
		/* FIXME: crash whole program */
		Level::Bug => panic!("StatusLevel : BUG"), 
		Level::Cancelled => panic!("StatusLevel : CANCELLED"),
		Level::Help | Level::Note => StatusLevel::OK, 
		Level::Warning => StatusLevel::WARNING,
		Level::Error | Level::Fatal => StatusLevel::ERROR,
	}
}

impl MessagesHandler {
}


/* -----------------  ----------------- */

fn output_message(tokenWriter: &mut TokenWriter, opt_sr : Option<SourceRange>, msg: & str, lvl: &StatusLevel) 
	-> Void
{
	
	try!(tokenWriter.out.borrow_mut().write_str("MESSAGE { "));
	
	try!(outputString_Level(&lvl, tokenWriter));
	
	try!(outputString_optSourceRange(&opt_sr, tokenWriter));
	
	try!(tokenWriter.writeStringToken(msg));
	
	try!(tokenWriter.out.borrow_mut().write_str("}\n"));
	
	Ok(())
}


pub fn outputString_Level(lvl : &StatusLevel, writer : &mut TokenWriter) -> Void {
	
	try!(lvl.output_string(&mut *writer.out.borrow_mut()));
	try!(writer.writeRaw(" "));
	
	Ok(())
}

pub fn outputString_SourceRange(sr : &SourceRange, writer : &mut TokenWriter) -> Void {
	let mut out = writer.out.borrow_mut(); 
	try!(out.write_fmt(format_args!("{{ {} {} {} {} }}", 
		sr.start_pos.line, sr.start_pos.col.0,
		sr.end_pos.line, sr.end_pos.col.0,
	)));
	
	Ok(())
}

pub fn outputString_optSourceRange(sr : &Option<SourceRange>, writer : &mut TokenWriter) -> Void {
	
	match sr {
		&None => try!(writer.out.borrow_mut().write_str("{ }")) ,
		&Some(ref sr) => try!(outputString_SourceRange(sr, writer)) ,
	}
	
	try!(writer.out.borrow_mut().write_str(" "));
	
	Ok(())
}
