// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use codemap::{Pos, Span};
use codemap;
use diagnostics;

use std::cell::{RefCell, Cell};
use std::fmt;
use std::io;
use std::iter::range;
use std::string::String;
use term::WriterWrapper;
use term;

/// maximum number of lines we will print for each error; arbitrary.
static MAX_LINES: uint = 6u;

#[deriving(Clone)]
pub enum RenderSpan {
    /// A FullSpan renders with both with an initial line for the
    /// message, prefixed by file:linenum, followed by a summary of
    /// the source code covered by the span.
    FullSpan(Span),

    /// A FileLine renders with just a line for the message prefixed
    /// by file:linenum.
    FileLine(Span),
}

impl RenderSpan {
    fn span(self) -> Span {
        match self {
            FullSpan(s) | FileLine(s) => s
        }
    }
    fn is_full_span(&self) -> bool {
        match self {
            &FullSpan(..) => true,
            &FileLine(..) => false,
        }
    }
}

#[deriving(Clone)]
pub enum ColorConfig {
    Auto,
    Always,
    Never
}

pub trait Emitter {
    fn emit(&mut self, cmsp: Option<(&codemap::CodeMap, Span)>,
            msg: &str, code: Option<&str>, lvl: Level);
    fn custom_emit(&mut self, cm: &codemap::CodeMap,
                   sp: RenderSpan, msg: &str, lvl: Level);
}

/// This structure is used to signify that a task has panicked with a fatal error
/// from the diagnostics. You can use this with the `Any` trait to figure out
/// how a rustc task died (if so desired).
pub struct FatalError;

/// Signifies that the compiler died with an explicit call to `.bug`
/// or `.span_bug` rather than a failed assertion, etc.
pub struct ExplicitBug;

/// A span-handler is like a handler but also
/// accepts span information for source-location
/// reporting.
pub struct SpanHandler {
    pub handler: Handler,
    pub cm: codemap::CodeMap,
}

impl SpanHandler {
    pub fn span_fatal(&self, sp: Span, msg: &str) -> ! {
        self.handler.emit(Some((&self.cm, sp)), msg, Fatal);
        panic!(FatalError);
    }
    pub fn span_err(&self, sp: Span, msg: &str) {
        self.handler.emit(Some((&self.cm, sp)), msg, Error);
        self.handler.bump_err_count();
    }
    pub fn span_err_with_code(&self, sp: Span, msg: &str, code: &str) {
        self.handler.emit_with_code(Some((&self.cm, sp)), msg, code, Error);
        self.handler.bump_err_count();
    }
    pub fn span_warn(&self, sp: Span, msg: &str) {
        self.handler.emit(Some((&self.cm, sp)), msg, Warning);
    }
    pub fn span_warn_with_code(&self, sp: Span, msg: &str, code: &str) {
        self.handler.emit_with_code(Some((&self.cm, sp)), msg, code, Warning);
    }
    pub fn span_note(&self, sp: Span, msg: &str) {
        self.handler.emit(Some((&self.cm, sp)), msg, Note);
    }
    pub fn span_end_note(&self, sp: Span, msg: &str) {
        self.handler.custom_emit(&self.cm, FullSpan(sp), msg, Note);
    }
    pub fn span_help(&self, sp: Span, msg: &str) {
        self.handler.emit(Some((&self.cm, sp)), msg, Help);
    }
    pub fn fileline_note(&self, sp: Span, msg: &str) {
        self.handler.custom_emit(&self.cm, FileLine(sp), msg, Note);
    }
    pub fn span_bug(&self, sp: Span, msg: &str) -> ! {
        self.handler.emit(Some((&self.cm, sp)), msg, Bug);
        panic!(ExplicitBug);
    }
    pub fn span_unimpl(&self, sp: Span, msg: &str) -> ! {
        self.span_bug(sp, format!("unimplemented {}", msg).as_slice());
    }
    pub fn handler<'a>(&'a self) -> &'a Handler {
        &self.handler
    }
}

/// A handler deals with errors; certain errors
/// (fatal, bug, unimpl) may cause immediate exit,
/// others log errors for later reporting.
pub struct Handler {
    err_count: Cell<uint>,
    emit: RefCell<Box<Emitter + Send>>,
}

impl Handler {
    pub fn fatal(&self, msg: &str) -> ! {
        self.emit.borrow_mut().emit(None, msg, None, Fatal);
        panic!(FatalError);
    }
    pub fn err(&self, msg: &str) {
        self.emit.borrow_mut().emit(None, msg, None, Error);
        self.bump_err_count();
    }
    pub fn bump_err_count(&self) {
        self.err_count.set(self.err_count.get() + 1u);
    }
    pub fn err_count(&self) -> uint {
        self.err_count.get()
    }
    pub fn has_errors(&self) -> bool {
        self.err_count.get()> 0u
    }
    pub fn abort_if_errors(&self) {
        let s;
        match self.err_count.get() {
          0u => return,
          1u => s = "aborting due to previous error".to_string(),
          _  => {
            s = format!("aborting due to {} previous errors",
                        self.err_count.get());
          }
        }
        self.fatal(s.as_slice());
    }
    pub fn warn(&self, msg: &str) {
        self.emit.borrow_mut().emit(None, msg, None, Warning);
    }
    pub fn note(&self, msg: &str) {
        self.emit.borrow_mut().emit(None, msg, None, Note);
    }
    pub fn help(&self, msg: &str) {
        self.emit.borrow_mut().emit(None, msg, None, Help);
    }
    pub fn bug(&self, msg: &str) -> ! {
        self.emit.borrow_mut().emit(None, msg, None, Bug);
        panic!(ExplicitBug);
    }
    pub fn unimpl(&self, msg: &str) -> ! {
        self.bug(format!("unimplemented {}", msg).as_slice());
    }
    pub fn emit(&self,
                cmsp: Option<(&codemap::CodeMap, Span)>,
                msg: &str,
                lvl: Level) {
        self.emit.borrow_mut().emit(cmsp, msg, None, lvl);
    }
    pub fn emit_with_code(&self,
                          cmsp: Option<(&codemap::CodeMap, Span)>,
                          msg: &str,
                          code: &str,
                          lvl: Level) {
        self.emit.borrow_mut().emit(cmsp, msg, Some(code), lvl);
    }
    pub fn custom_emit(&self, cm: &codemap::CodeMap,
                       sp: RenderSpan, msg: &str, lvl: Level) {
        self.emit.borrow_mut().custom_emit(cm, sp, msg, lvl);
    }
}

pub fn mk_span_handler(handler: Handler, cm: codemap::CodeMap) -> SpanHandler {
    SpanHandler {
        handler: handler,
        cm: cm,
    }
}

pub fn default_handler(color_config: ColorConfig,
                       registry: Option<diagnostics::registry::Registry>) -> Handler {
    mk_handler(box EmitterWriter::stderr(color_config, registry))
}

pub fn mk_handler(e: Box<Emitter + Send>) -> Handler {
    Handler {
        err_count: Cell::new(0),
        emit: RefCell::new(e),
    }
}

#[deriving(PartialEq, Clone)]
pub enum Level {
    Bug,
    Fatal,
    Error,
    Warning,
    Note,
    Help,
}

impl fmt::Show for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use std::fmt::Show;

        match *self {
            Bug => "error: internal compiler error".fmt(f),
            Fatal | Error => "error".fmt(f),
            Warning => "warning".fmt(f),
            Note => "note".fmt(f),
            Help => "help".fmt(f),
        }
    }
}

impl Level {
    fn color(self) -> term::color::Color {
        match self {
            Bug | Fatal | Error => term::color::BRIGHT_RED,
            Warning => term::color::BRIGHT_YELLOW,
            Note => term::color::BRIGHT_GREEN,
            Help => term::color::BRIGHT_CYAN,
        }
    }
}

fn print_maybe_styled(w: &mut EmitterWriter,
                      msg: &str,
                      color: term::attr::Attr) -> io::IoResult<()> {
    match w.dst {
        Terminal(ref mut t) => {
            try!(t.attr(color));
            // If `msg` ends in a newline, we need to reset the color before
            // the newline. We're making the assumption that we end up writing
            // to a `LineBufferedWriter`, which means that emitting the reset
            // after the newline ends up buffering the reset until we print
            // another line or exit. Buffering the reset is a problem if we're
            // sharing the terminal with any other programs (e.g. other rustc
            // instances via `make -jN`).
            //
            // Note that if `msg` contains any internal newlines, this will
            // result in the `LineBufferedWriter` flushing twice instead of
            // once, which still leaves the opportunity for interleaved output
            // to be miscolored. We assume this is rare enough that we don't
            // have to worry about it.
            if msg.ends_with("\n") {
                try!(t.write_str(msg.slice_to(msg.len()-1)));
                try!(t.reset());
                try!(t.write_str("\n"));
            } else {
                try!(t.write_str(msg));
                try!(t.reset());
            }
            Ok(())
        }
        Raw(ref mut w) => {
            w.write_str(msg)
        }
    }
}

fn print_diagnostic(dst: &mut EmitterWriter, topic: &str, lvl: Level,
                    msg: &str, code: Option<&str>) -> io::IoResult<()> {
    if !topic.is_empty() {
        try!(write!(&mut dst.dst, "{} ", topic));
    }

    try!(print_maybe_styled(dst,
                            format!("{}: ", lvl.to_string()).as_slice(),
                            term::attr::ForegroundColor(lvl.color())));
    try!(print_maybe_styled(dst,
                            format!("{}", msg).as_slice(),
                            term::attr::Bold));

    match code {
        Some(code) => {
            let style = term::attr::ForegroundColor(term::color::BRIGHT_MAGENTA);
            try!(print_maybe_styled(dst, format!(" [{}]", code.clone()).as_slice(), style));
        }
        None => ()
    }
    try!(dst.dst.write_char('\n'));
    Ok(())
}

pub struct EmitterWriter {
    dst: Destination,
    registry: Option<diagnostics::registry::Registry>
}

enum Destination {
    Terminal(Box<term::Terminal<WriterWrapper> + Send>),
    Raw(Box<Writer + Send>),
}

impl EmitterWriter {
    pub fn stderr(color_config: ColorConfig,
                  registry: Option<diagnostics::registry::Registry>) -> EmitterWriter {
        let stderr = io::stderr();

        let use_color = match color_config {
            Always => true,
            Never  => false,
            Auto   => stderr.get_ref().isatty()
        };

        if use_color {
            let dst = match term::stderr() {
                Some(t) => Terminal(t),
                None    => Raw(box stderr),
            };
            EmitterWriter { dst: dst, registry: registry }
        } else {
            EmitterWriter { dst: Raw(box stderr), registry: registry }
        }
    }

    pub fn new(dst: Box<Writer + Send>,
               registry: Option<diagnostics::registry::Registry>) -> EmitterWriter {
        EmitterWriter { dst: Raw(dst), registry: registry }
    }
}

impl Writer for Destination {
    fn write(&mut self, bytes: &[u8]) -> io::IoResult<()> {
        match *self {
            Terminal(ref mut t) => t.write(bytes),
            Raw(ref mut w) => w.write(bytes),
        }
    }
}

impl Emitter for EmitterWriter {
    fn emit(&mut self,
            cmsp: Option<(&codemap::CodeMap, Span)>,
            msg: &str, code: Option<&str>, lvl: Level) {
        let error = match cmsp {
            Some((cm, sp)) => emit(self, cm, FullSpan(sp), msg, code, lvl, false),
            None => print_diagnostic(self, "", lvl, msg, code),
        };

        match error {
            Ok(()) => {}
            Err(e) => panic!("failed to print diagnostics: {}", e),
        }
    }

    fn custom_emit(&mut self, cm: &codemap::CodeMap,
                   sp: RenderSpan, msg: &str, lvl: Level) {
        match emit(self, cm, sp, msg, None, lvl, true) {
            Ok(()) => {}
            Err(e) => panic!("failed to print diagnostics: {}", e),
        }
    }
}

fn emit(dst: &mut EmitterWriter, cm: &codemap::CodeMap, rsp: RenderSpan,
        msg: &str, code: Option<&str>, lvl: Level, custom: bool) -> io::IoResult<()> {
    let sp = rsp.span();
    let ss = cm.span_to_string(sp);
    let lines = cm.span_to_lines(sp);
    if custom {
        // we want to tell compiletest/runtest to look at the last line of the
        // span (since `custom_highlight_lines` displays an arrow to the end of
        // the span)
        let span_end = Span { lo: sp.hi, hi: sp.hi, expn_id: sp.expn_id};
        let ses = cm.span_to_string(span_end);
        try!(print_diagnostic(dst, ses.as_slice(), lvl, msg, code));
        if rsp.is_full_span() {
            try!(custom_highlight_lines(dst, cm, sp, lvl, lines));
        }
    } else {
        try!(print_diagnostic(dst, ss.as_slice(), lvl, msg, code));
        if rsp.is_full_span() {
            try!(highlight_lines(dst, cm, sp, lvl, lines));
        }
    }
    try!(print_macro_backtrace(dst, cm, sp));
    match code {
        Some(code) =>
            match dst.registry.as_ref().and_then(|registry| registry.find_description(code)) {
                Some(_) => {
                    try!(print_diagnostic(dst, ss.as_slice(), Help,
                                          format!("pass `--explain {}` to see a detailed \
                                                   explanation", code).as_slice(), None));
                }
                None => ()
            },
        None => (),
    }
    Ok(())
}

fn highlight_lines(err: &mut EmitterWriter,
                   cm: &codemap::CodeMap,
                   sp: Span,
                   lvl: Level,
                   lines: codemap::FileLines) -> io::IoResult<()> {
    let fm = &*lines.file;

    let mut elided = false;
    let mut display_lines = lines.lines.as_slice();
    if display_lines.len() > MAX_LINES {
        display_lines = display_lines[0u..MAX_LINES];
        elided = true;
    }
    // Print the offending lines
    for line in display_lines.iter() {
        try!(write!(&mut err.dst, "{}:{} {}\n", fm.name, *line + 1,
                    fm.get_line(*line as int)));
    }
    if elided {
        let last_line = display_lines[display_lines.len() - 1u];
        let s = format!("{}:{} ", fm.name, last_line + 1u);
        try!(write!(&mut err.dst, "{0:1$}...\n", "", s.len()));
    }

    // FIXME (#3260)
    // If there's one line at fault we can easily point to the problem
    if lines.lines.len() == 1u {
        let lo = cm.lookup_char_pos(sp.lo);
        let mut digits = 0u;
        let mut num = (lines.lines[0] + 1u) / 10u;

        // how many digits must be indent past?
        while num > 0u { num /= 10u; digits += 1u; }

        // indent past |name:## | and the 0-offset column location
        let left = fm.name.len() + digits + lo.col.to_uint() + 3u;
        let mut s = String::new();
        // Skip is the number of characters we need to skip because they are
        // part of the 'filename:line ' part of the previous line.
        let skip = fm.name.len() + digits + 3u;
        for _ in range(0, skip) {
            s.push(' ');
        }
        let orig = fm.get_line(lines.lines[0] as int);
        for pos in range(0u, left-skip) {
            let cur_char = orig.as_bytes()[pos] as char;
            // Whenever a tab occurs on the previous line, we insert one on
            // the error-point-squiggly-line as well (instead of a space).
            // That way the squiggly line will usually appear in the correct
            // position.
            match cur_char {
                '\t' => s.push('\t'),
                _ => s.push(' '),
            };
        }
        try!(write!(&mut err.dst, "{}", s));
        let mut s = String::from_str("^");
        let hi = cm.lookup_char_pos(sp.hi);
        if hi.col != lo.col {
            // the ^ already takes up one space
            let num_squigglies = hi.col.to_uint()-lo.col.to_uint()-1u;
            for _ in range(0, num_squigglies) {
                s.push('~');
            }
        }
        try!(print_maybe_styled(err,
                                format!("{}\n", s).as_slice(),
                                term::attr::ForegroundColor(lvl.color())));
    }
    Ok(())
}

/// Here are the differences between this and the normal `highlight_lines`:
/// `custom_highlight_lines` will always put arrow on the last byte of the
/// span (instead of the first byte). Also, when the span is too long (more
/// than 6 lines), `custom_highlight_lines` will print the first line, then
/// dot dot dot, then last line, whereas `highlight_lines` prints the first
/// six lines.
fn custom_highlight_lines(w: &mut EmitterWriter,
                          cm: &codemap::CodeMap,
                          sp: Span,
                          lvl: Level,
                          lines: codemap::FileLines)
                          -> io::IoResult<()> {
    let fm = &*lines.file;

    let lines = lines.lines.as_slice();
    if lines.len() > MAX_LINES {
        try!(write!(&mut w.dst, "{}:{} {}\n", fm.name,
                    lines[0] + 1, fm.get_line(lines[0] as int)));
        try!(write!(&mut w.dst, "...\n"));
        let last_line = lines[lines.len()-1];
        try!(write!(&mut w.dst, "{}:{} {}\n", fm.name,
                    last_line + 1, fm.get_line(last_line as int)));
    } else {
        for line in lines.iter() {
            try!(write!(&mut w.dst, "{}:{} {}\n", fm.name,
                        *line + 1, fm.get_line(*line as int)));
        }
    }
    let last_line_start = format!("{}:{} ", fm.name, lines[lines.len()-1]+1);
    let hi = cm.lookup_char_pos(sp.hi);
    // Span seems to use half-opened interval, so subtract 1
    let skip = last_line_start.len() + hi.col.to_uint() - 1;
    let mut s = String::new();
    for _ in range(0, skip) {
        s.push(' ');
    }
    s.push('^');
    s.push('\n');
    print_maybe_styled(w,
                       s.as_slice(),
                       term::attr::ForegroundColor(lvl.color()))
}

fn print_macro_backtrace(w: &mut EmitterWriter,
                         cm: &codemap::CodeMap,
                         sp: Span)
                         -> io::IoResult<()> {
    let cs = try!(cm.with_expn_info(sp.expn_id, |expn_info| match expn_info {
        Some(ei) => {
            let ss = ei.callee.span.map_or(String::new(), |span| cm.span_to_string(span));
            let (pre, post) = match ei.callee.format {
                codemap::MacroAttribute => ("#[", "]"),
                codemap::MacroBang => ("", "!")
            };
            try!(print_diagnostic(w, ss.as_slice(), Note,
                                  format!("in expansion of {}{}{}", pre,
                                          ei.callee.name,
                                          post).as_slice(), None));
            let ss = cm.span_to_string(ei.call_site);
            try!(print_diagnostic(w, ss.as_slice(), Note, "expansion site", None));
            Ok(Some(ei.call_site))
        }
        None => Ok(None)
    }));
    cs.map_or(Ok(()), |call_site| print_macro_backtrace(w, cm, call_site))
}

pub fn expect<T>(diag: &SpanHandler, opt: Option<T>, msg: || -> String) -> T {
    match opt {
        Some(t) => t,
        None => diag.handler().bug(msg().as_slice()),
    }
}
