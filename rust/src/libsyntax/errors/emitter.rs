// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use self::Destination::*;

use codemap::{self, COMMAND_LINE_SP, DUMMY_SP, Pos, Span, MultiSpan};
use diagnostics;

use errors::check_old_skool;
use errors::{Level, RenderSpan, CodeSuggestion, DiagnosticBuilder};
use errors::RenderSpan::*;
use errors::Level::*;
use errors::snippet::{RenderedLineKind, SnippetData, Style};

use std::{cmp, fmt};
use std::io::prelude::*;
use std::io;
use std::rc::Rc;
use term;

/// Emitter trait for emitting errors. Do not implement this directly:
/// implement `CoreEmitter` instead.
pub trait Emitter {
    /// Emit a standalone diagnostic message.
    fn emit(&mut self, span: &MultiSpan, msg: &str, code: Option<&str>, lvl: Level);

    /// Emit a structured diagnostic.
    fn emit_struct(&mut self, db: &DiagnosticBuilder);
}

pub trait CoreEmitter {
    fn emit_message(&mut self,
                    rsp: &RenderSpan,
                    msg: &str,
                    code: Option<&str>,
                    lvl: Level,
                    is_header: bool,
                    show_snippet: bool);
}

impl<T: CoreEmitter> Emitter for T {
    fn emit(&mut self,
            msp: &MultiSpan,
            msg: &str,
            code: Option<&str>,
            lvl: Level) {
        self.emit_message(&FullSpan(msp.clone()),
                          msg,
                          code,
                          lvl,
                          true,
                          true);
    }

    fn emit_struct(&mut self, db: &DiagnosticBuilder) {
        let old_school = check_old_skool();
        let db_span = FullSpan(db.span.clone());
        self.emit_message(&FullSpan(db.span.clone()),
                          &db.message,
                          db.code.as_ref().map(|s| &**s),
                          db.level,
                          true,
                          true);
        for child in &db.children {
            let render_span = child.render_span
                                   .clone()
                                   .unwrap_or_else(
                                       || FullSpan(child.span.clone()));

            if !old_school {
                self.emit_message(&render_span,
                                    &child.message,
                                    None,
                                    child.level,
                                    false,
                                    true);
            } else {
                let (render_span, show_snippet) = match render_span.span().primary_span() {
                    None => (db_span.clone(), false),
                    _ => (render_span, true)
                };
                self.emit_message(&render_span,
                                    &child.message,
                                    None,
                                    child.level,
                                    false,
                                    show_snippet);
            }
        }
    }
}

/// maximum number of lines we will print for each error; arbitrary.
pub const MAX_HIGHLIGHT_LINES: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorConfig {
    Auto,
    Always,
    Never,
}

impl ColorConfig {
    fn use_color(&self) -> bool {
        match *self {
            ColorConfig::Always => true,
            ColorConfig::Never  => false,
            ColorConfig::Auto   => stderr_isatty(),
        }
    }
}

/// A basic emitter for when we don't have access to a codemap or registry. Used
/// for reporting very early errors, etc.
pub struct BasicEmitter {
    dst: Destination,
}

impl CoreEmitter for BasicEmitter {
    fn emit_message(&mut self,
                    _rsp: &RenderSpan,
                    msg: &str,
                    code: Option<&str>,
                    lvl: Level,
                    _is_header: bool,
                    _show_snippet: bool) {
        // we ignore the span as we have no access to a codemap at this point
        if let Err(e) = print_diagnostic(&mut self.dst, "", lvl, msg, code) {
            panic!("failed to print diagnostics: {:?}", e);
        }
    }
}

impl BasicEmitter {
    pub fn stderr(color_config: ColorConfig) -> BasicEmitter {
        if color_config.use_color() {
            let dst = Destination::from_stderr();
            BasicEmitter { dst: dst }
        } else {
            BasicEmitter { dst: Raw(Box::new(io::stderr())) }
        }
    }
}

pub struct EmitterWriter {
    dst: Destination,
    registry: Option<diagnostics::registry::Registry>,
    cm: Rc<codemap::CodeMap>,

    /// Is this the first error emitted thus far? If not, we emit a
    /// `\n` before the top-level errors.
    first: bool,

    // For now, allow an old-school mode while we transition
    old_school: bool,
}

impl CoreEmitter for EmitterWriter {
    fn emit_message(&mut self,
                    rsp: &RenderSpan,
                    msg: &str,
                    code: Option<&str>,
                    lvl: Level,
                    is_header: bool,
                    show_snippet: bool) {
        match self.emit_message_(rsp, msg, code, lvl, is_header, show_snippet) {
            Ok(()) => { }
            Err(e) => panic!("failed to emit error: {}", e)
        }
    }
}

/// Do not use this for messages that end in `\n` – use `println_maybe_styled` instead. See
/// `EmitterWriter::print_maybe_styled` for details.
macro_rules! print_maybe_styled {
    ($dst: expr, $style: expr, $($arg: tt)*) => {
        $dst.print_maybe_styled(format_args!($($arg)*), $style, false)
    }
}

macro_rules! println_maybe_styled {
    ($dst: expr, $style: expr, $($arg: tt)*) => {
        $dst.print_maybe_styled(format_args!($($arg)*), $style, true)
    }
}

impl EmitterWriter {
    pub fn stderr(color_config: ColorConfig,
                  registry: Option<diagnostics::registry::Registry>,
                  code_map: Rc<codemap::CodeMap>)
                  -> EmitterWriter {
        let old_school = check_old_skool();
        if color_config.use_color() {
            let dst = Destination::from_stderr();
            EmitterWriter { dst: dst,
                            registry: registry,
                            cm: code_map,
                            first: true,
                            old_school: old_school }
        } else {
            EmitterWriter { dst: Raw(Box::new(io::stderr())),
                            registry: registry,
                            cm: code_map,
                            first: true,
                            old_school: old_school }
        }
    }

    pub fn new(dst: Box<Write + Send>,
               registry: Option<diagnostics::registry::Registry>,
               code_map: Rc<codemap::CodeMap>)
               -> EmitterWriter {
        let old_school = check_old_skool();
        EmitterWriter { dst: Raw(dst),
                        registry: registry,
                        cm: code_map,
                        first: true,
                        old_school: old_school }
    }

    fn emit_message_(&mut self,
                     rsp: &RenderSpan,
                     msg: &str,
                     code: Option<&str>,
                     lvl: Level,
                     is_header: bool,
                     show_snippet: bool)
                     -> io::Result<()> {
        if is_header {
            if self.first {
                self.first = false;
            } else {
                if !self.old_school {
                    write!(self.dst, "\n")?;
                }
            }
        }

        match code {
            Some(code) if self.registry.as_ref()
                                       .and_then(|registry| registry.find_description(code))
                                       .is_some() => {
                let code_with_explain = String::from("--explain ") + code;
                if self.old_school {
                    let loc = match rsp.span().primary_span() {
                        Some(COMMAND_LINE_SP) | Some(DUMMY_SP) => "".to_string(),
                        Some(ps) => self.cm.span_to_string(ps),
                        None => "".to_string()
                    };
                    print_diagnostic(&mut self.dst, &loc, lvl, msg, Some(code))?
                }
                else {
                    print_diagnostic(&mut self.dst, "", lvl, msg, Some(&code_with_explain))?
                }
            }
            _ => {
                if self.old_school {
                    let loc = match rsp.span().primary_span() {
                        Some(COMMAND_LINE_SP) | Some(DUMMY_SP) => "".to_string(),
                        Some(ps) => self.cm.span_to_string(ps),
                        None => "".to_string()
                    };
                    print_diagnostic(&mut self.dst, &loc, lvl, msg, code)?
                }
                else {
                    print_diagnostic(&mut self.dst, "", lvl, msg, code)?
                }
            }
        }

        if !show_snippet {
            return Ok(());
        }

        // Watch out for various nasty special spans; don't try to
        // print any filename or anything for those.
        match rsp.span().primary_span() {
            Some(COMMAND_LINE_SP) | Some(DUMMY_SP) => {
                return Ok(());
            }
            _ => { }
        }

        // Otherwise, print out the snippet etc as needed.
        match *rsp {
            FullSpan(ref msp) => {
                self.highlight_lines(msp, lvl)?;
                if let Some(primary_span) = msp.primary_span() {
                    self.print_macro_backtrace(primary_span)?;
                }
            }
            Suggestion(ref suggestion) => {
                self.highlight_suggestion(suggestion)?;
                if let Some(primary_span) = rsp.span().primary_span() {
                    self.print_macro_backtrace(primary_span)?;
                }
            }
        }
        if self.old_school {
            match code {
                Some(code) if self.registry.as_ref()
                                        .and_then(|registry| registry.find_description(code))
                                        .is_some() => {
                    let loc = match rsp.span().primary_span() {
                        Some(COMMAND_LINE_SP) | Some(DUMMY_SP) => "".to_string(),
                        Some(ps) => self.cm.span_to_string(ps),
                        None => "".to_string()
                    };
                    let msg = "run `rustc --explain ".to_string() + &code.to_string() +
                        "` to see a detailed explanation";
                    print_diagnostic(&mut self.dst, &loc, Level::Help, &msg,
                        None)?
                }
                _ => ()
            }
        }
        Ok(())
    }

    fn highlight_suggestion(&mut self, suggestion: &CodeSuggestion) -> io::Result<()>
    {
        let primary_span = suggestion.msp.primary_span().unwrap();
        let lines = self.cm.span_to_lines(primary_span).unwrap();
        assert!(!lines.lines.is_empty());

        let complete = suggestion.splice_lines(&self.cm);
        let line_count = cmp::min(lines.lines.len(), MAX_HIGHLIGHT_LINES);
        let display_lines = &lines.lines[..line_count];

        let fm = &*lines.file;
        // Calculate the widest number to format evenly
        let max_digits = line_num_max_digits(display_lines.last().unwrap());

        // print the suggestion without any line numbers, but leave
        // space for them. This helps with lining up with previous
        // snippets from the actual error being reported.
        let mut lines = complete.lines();
        for line in lines.by_ref().take(MAX_HIGHLIGHT_LINES) {
            write!(&mut self.dst, "{0}:{1:2$} {3}\n",
                   fm.name, "", max_digits, line)?;
        }

        // if we elided some lines, add an ellipsis
        if let Some(_) = lines.next() {
            write!(&mut self.dst, "{0:1$} {0:2$} ...\n",
                   "", fm.name.len(), max_digits)?;
        }

        Ok(())
    }

    fn highlight_lines(&mut self,
                       msp: &MultiSpan,
                       lvl: Level)
                       -> io::Result<()>
    {
        let mut snippet_data = SnippetData::new(self.cm.clone(),
                                                msp.primary_span());
        if self.old_school {
            let mut output_vec = vec![];

            for span_label in msp.span_labels() {
                let mut snippet_data = snippet_data.clone();

                snippet_data.push(span_label.span,
                                  span_label.is_primary,
                                  span_label.label);
                if span_label.is_primary {
                    output_vec.insert(0, snippet_data);
                }
                else {
                    output_vec.push(snippet_data);
                }
            }

            for snippet_data in output_vec.iter() {
                let rendered_lines = snippet_data.render_lines();
                for rendered_line in &rendered_lines {
                    for styled_string in &rendered_line.text {
                        self.dst.apply_style(lvl, &rendered_line.kind, styled_string.style)?;
                        write!(&mut self.dst, "{}", styled_string.text)?;
                        self.dst.reset_attrs()?;
                    }
                    write!(&mut self.dst, "\n")?;
                }
            }
        }
        else {
            for span_label in msp.span_labels() {
                snippet_data.push(span_label.span,
                                  span_label.is_primary,
                                  span_label.label);
            }
            let rendered_lines = snippet_data.render_lines();
            for rendered_line in &rendered_lines {
                for styled_string in &rendered_line.text {
                    self.dst.apply_style(lvl, &rendered_line.kind, styled_string.style)?;
                    write!(&mut self.dst, "{}", styled_string.text)?;
                    self.dst.reset_attrs()?;
                }
                write!(&mut self.dst, "\n")?;
            }
        }
        Ok(())
    }

    fn print_macro_backtrace(&mut self,
                             sp: Span)
                             -> io::Result<()> {
        for trace in self.cm.macro_backtrace(sp) {
            let mut diag_string =
                format!("in this expansion of {}", trace.macro_decl_name);
            if let Some(def_site_span) = trace.def_site_span {
                diag_string.push_str(
                    &format!(" (defined in {})",
                        self.cm.span_to_filename(def_site_span)));
            }
            let snippet = self.cm.span_to_string(sp);
            print_diagnostic(&mut self.dst, &snippet, Note, &diag_string, None)?;
        }
        Ok(())
    }
}

fn line_num_max_digits(line: &codemap::LineInfo) -> usize {
    let mut max_line_num = line.line_index + 1;
    let mut digits = 0;
    while max_line_num > 0 {
        max_line_num /= 10;
        digits += 1;
    }
    digits
}

fn print_diagnostic(dst: &mut Destination,
                    topic: &str,
                    lvl: Level,
                    msg: &str,
                    code: Option<&str>)
                    -> io::Result<()> {
    if !topic.is_empty() {
        let old_school = check_old_skool();
        if !old_school {
            write!(dst, "{}: ", topic)?;
        }
        else {
            write!(dst, "{} ", topic)?;
        }
        dst.reset_attrs()?;
    }
    dst.start_attr(term::Attr::Bold)?;
    dst.start_attr(term::Attr::ForegroundColor(lvl.color()))?;
    write!(dst, "{}", lvl.to_string())?;
    dst.reset_attrs()?;
    write!(dst, ": ")?;
    dst.start_attr(term::Attr::Bold)?;
    write!(dst, "{}", msg)?;

    if let Some(code) = code {
        let style = term::Attr::ForegroundColor(term::color::BRIGHT_MAGENTA);
        print_maybe_styled!(dst, style, " [{}]", code.clone())?;
    }

    dst.reset_attrs()?;
    write!(dst, "\n")?;
    Ok(())
}

#[cfg(unix)]
fn stderr_isatty() -> bool {
    use libc;
    unsafe { libc::isatty(libc::STDERR_FILENO) != 0 }
}
#[cfg(windows)]
fn stderr_isatty() -> bool {
    type DWORD = u32;
    type BOOL = i32;
    type HANDLE = *mut u8;
    const STD_ERROR_HANDLE: DWORD = -12i32 as DWORD;
    extern "system" {
        fn GetStdHandle(which: DWORD) -> HANDLE;
        fn GetConsoleMode(hConsoleHandle: HANDLE,
                          lpMode: *mut DWORD) -> BOOL;
    }
    unsafe {
        let handle = GetStdHandle(STD_ERROR_HANDLE);
        let mut out = 0;
        GetConsoleMode(handle, &mut out) != 0
    }
}

enum Destination {
    Terminal(Box<term::StderrTerminal>),
    Raw(Box<Write + Send>),
}

impl Destination {
    fn from_stderr() -> Destination {
        match term::stderr() {
            Some(t) => Terminal(t),
            None    => Raw(Box::new(io::stderr())),
        }
    }

    fn apply_style(&mut self,
                   lvl: Level,
                   _kind: &RenderedLineKind,
                   style: Style)
                   -> io::Result<()> {
        match style {
            Style::FileNameStyle |
            Style::LineAndColumn => {
            }
            Style::LineNumber => {
                self.start_attr(term::Attr::Bold)?;
                self.start_attr(term::Attr::ForegroundColor(term::color::BRIGHT_BLUE))?;
            }
            Style::Quotation => {
            }
            Style::UnderlinePrimary | Style::LabelPrimary => {
                self.start_attr(term::Attr::Bold)?;
                self.start_attr(term::Attr::ForegroundColor(lvl.color()))?;
            }
            Style::UnderlineSecondary | Style::LabelSecondary => {
                self.start_attr(term::Attr::Bold)?;
                self.start_attr(term::Attr::ForegroundColor(term::color::BRIGHT_BLUE))?;
            }
            Style::NoStyle => {
            }
        }
        Ok(())
    }

    fn start_attr(&mut self, attr: term::Attr) -> io::Result<()> {
        match *self {
            Terminal(ref mut t) => { t.attr(attr)?; }
            Raw(_) => { }
        }
        Ok(())
    }

    fn reset_attrs(&mut self) -> io::Result<()> {
        match *self {
            Terminal(ref mut t) => { t.reset()?; }
            Raw(_) => { }
        }
        Ok(())
    }

    fn print_maybe_styled(&mut self,
                          args: fmt::Arguments,
                          color: term::Attr,
                          print_newline_at_end: bool)
                          -> io::Result<()> {
        match *self {
            Terminal(ref mut t) => {
                t.attr(color)?;
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
                t.write_fmt(args)?;
                t.reset()?;
                if print_newline_at_end {
                    t.write_all(b"\n")
                } else {
                    Ok(())
                }
            }
            Raw(ref mut w) => {
                w.write_fmt(args)?;
                if print_newline_at_end {
                    w.write_all(b"\n")
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Write for Destination {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match *self {
            Terminal(ref mut t) => t.write(bytes),
            Raw(ref mut w) => w.write(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match *self {
            Terminal(ref mut t) => t.flush(),
            Raw(ref mut w) => w.flush(),
        }
    }
}


#[cfg(test)]
mod test {
    use errors::{Level, CodeSuggestion};
    use super::EmitterWriter;
    use codemap::{mk_sp, CodeMap, Span, MultiSpan, BytePos, NO_EXPANSION};
    use std::sync::{Arc, Mutex};
    use std::io::{self, Write};
    use std::str::from_utf8;
    use std::rc::Rc;

    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            Write::write(&mut *self.0.lock().unwrap(), data)
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }

    /// Given a string like " ^~~~~~~~~~~~ ", produces a span
    /// coverting that range. The idea is that the string has the same
    /// length as the input, and we uncover the byte positions.  Note
    /// that this can span lines and so on.
    fn span_from_selection(input: &str, selection: &str) -> Span {
        assert_eq!(input.len(), selection.len());
        let left_index = selection.find('~').unwrap() as u32;
        let right_index = selection.rfind('~').map(|x|x as u32).unwrap_or(left_index);
        Span { lo: BytePos(left_index), hi: BytePos(right_index + 1), expn_id: NO_EXPANSION }
    }

    // Diagnostic doesn't align properly in span where line number increases by one digit
    #[test]
    fn test_hilight_suggestion_issue_11715() {
        let data = Arc::new(Mutex::new(Vec::new()));
        let cm = Rc::new(CodeMap::new());
        let mut ew = EmitterWriter::new(Box::new(Sink(data.clone())), None, cm.clone());
        let content = "abcdefg
        koksi
        line3
        line4
        cinq
        line6
        line7
        line8
        line9
        line10
        e-lä-vän
        tolv
        dreizehn
        ";
        let file = cm.new_filemap_and_lines("dummy.txt", content);
        let start = file.lines.borrow()[10];
        let end = file.lines.borrow()[11];
        let sp = mk_sp(start, end);
        let lvl = Level::Error;
        println!("highlight_lines");
        ew.highlight_lines(&sp.into(), lvl).unwrap();
        println!("done");
        let vec = data.lock().unwrap().clone();
        let vec: &[u8] = &vec;
        let str = from_utf8(vec).unwrap();
        println!("r#\"\n{}\"#", str);
        assert_eq!(str, &r#"
  --> dummy.txt:11:1
11 |>         e-lä-vän
   |> ^
"#[1..]);
    }

    #[test]
    fn test_single_span_splice() {
        // Test that a `MultiSpan` containing a single span splices a substition correctly
        let cm = CodeMap::new();
        let inputtext = "aaaaa\nbbbbBB\nCCC\nDDDDDddddd\neee\n";
        let selection = "     \n    ~~\n~~~\n~~~~~     \n   \n";
        cm.new_filemap_and_lines("blork.rs", inputtext);
        let sp = span_from_selection(inputtext, selection);
        let msp: MultiSpan = sp.into();

        // check that we are extracting the text we thought we were extracting
        assert_eq!(&cm.span_to_snippet(sp).unwrap(), "BB\nCCC\nDDDDD");

        let substitute = "ZZZZZZ".to_owned();
        let expected = "bbbbZZZZZZddddd";
        let suggest = CodeSuggestion {
            msp: msp,
            substitutes: vec![substitute],
        };
        assert_eq!(suggest.splice_lines(&cm), expected);
    }

    #[test]
    fn test_multi_span_splice() {
        // Test that a `MultiSpan` containing multiple spans splices a substition correctly
        let cm = CodeMap::new();
        let inputtext  = "aaaaa\nbbbbBB\nCCC\nDDDDDddddd\neee\n";
        let selection1 = "     \n      \n   \n          \n ~ \n"; // intentionally out of order
        let selection2 = "     \n    ~~\n~~~\n~~~~~     \n   \n";
        cm.new_filemap_and_lines("blork.rs", inputtext);
        let sp1 = span_from_selection(inputtext, selection1);
        let sp2 = span_from_selection(inputtext, selection2);
        let msp: MultiSpan = MultiSpan::from_spans(vec![sp1, sp2]);

        let expected = "bbbbZZZZZZddddd\neXYZe";
        let suggest = CodeSuggestion {
            msp: msp,
            substitutes: vec!["ZZZZZZ".to_owned(),
                              "XYZ".to_owned()]
        };

        assert_eq!(suggest.splice_lines(&cm), expected);
    }

    #[test]
    fn test_multispan_highlight() {
        let data = Arc::new(Mutex::new(Vec::new()));
        let cm = Rc::new(CodeMap::new());
        let mut diag = EmitterWriter::new(Box::new(Sink(data.clone())), None, cm.clone());

        let inp =       "_____aaaaaa____bbbbbb__cccccdd_";
        let sp1 =       "     ~~~~~~                    ";
        let sp2 =       "               ~~~~~~          ";
        let sp3 =       "                       ~~~~~   ";
        let sp4 =       "                          ~~~~ ";
        let sp34 =      "                       ~~~~~~~ ";

        let expect_start = &r#"
 --> dummy.txt:1:6
1 |> _____aaaaaa____bbbbbb__cccccdd_
  |>      ^^^^^^    ^^^^^^  ^^^^^^^
"#[1..];

        let span = |sp, expected| {
            let sp = span_from_selection(inp, sp);
            assert_eq!(&cm.span_to_snippet(sp).unwrap(), expected);
            sp
        };
        cm.new_filemap_and_lines("dummy.txt", inp);
        let sp1 = span(sp1, "aaaaaa");
        let sp2 = span(sp2, "bbbbbb");
        let sp3 = span(sp3, "ccccc");
        let sp4 = span(sp4, "ccdd");
        let sp34 = span(sp34, "cccccdd");

        let spans = vec![sp1, sp2, sp3, sp4];

        let test = |expected, highlight: &mut FnMut()| {
            data.lock().unwrap().clear();
            highlight();
            let vec = data.lock().unwrap().clone();
            let actual = from_utf8(&vec[..]).unwrap();
            println!("actual=\n{}", actual);
            assert_eq!(actual, expected);
        };

        let msp = MultiSpan::from_spans(vec![sp1, sp2, sp34]);
        test(expect_start, &mut || {
            diag.highlight_lines(&msp, Level::Error).unwrap();
        });
        test(expect_start, &mut || {
            let msp = MultiSpan::from_spans(spans.clone());
            diag.highlight_lines(&msp, Level::Error).unwrap();
        });
    }

    #[test]
    fn test_huge_multispan_highlight() {
        let data = Arc::new(Mutex::new(Vec::new()));
        let cm = Rc::new(CodeMap::new());
        let mut diag = EmitterWriter::new(Box::new(Sink(data.clone())), None, cm.clone());

        let inp = "aaaaa\n\
                   aaaaa\n\
                   aaaaa\n\
                   bbbbb\n\
                   ccccc\n\
                   xxxxx\n\
                   yyyyy\n\
                   _____\n\
                   ddd__eee_\n\
                   elided\n\
                   __f_gg";
        let file = cm.new_filemap_and_lines("dummy.txt", inp);

        let span = |lo, hi, (off_lo, off_hi)| {
            let lines = file.lines.borrow();
            let (mut lo, mut hi): (BytePos, BytePos) = (lines[lo], lines[hi]);
            lo.0 += off_lo;
            hi.0 += off_hi;
            mk_sp(lo, hi)
        };
        let sp0 = span(4, 6, (0, 5));
        let sp1 = span(0, 6, (0, 5));
        let sp2 = span(8, 8, (0, 3));
        let sp3 = span(8, 8, (5, 8));
        let sp4 = span(10, 10, (2, 3));
        let sp5 = span(10, 10, (4, 6));

        let expect0 = &r#"
   --> dummy.txt:5:1
5   |> ccccc
    |> ^
...
9   |> ddd__eee_
    |> ^^^  ^^^
10  |> elided
11  |> __f_gg
    |>   ^ ^^
"#[1..];

        let expect = &r#"
   --> dummy.txt:1:1
1   |> aaaaa
    |> ^
...
9   |> ddd__eee_
    |> ^^^  ^^^
10  |> elided
11  |> __f_gg
    |>   ^ ^^
"#[1..];

        macro_rules! test {
            ($expected: expr, $highlight: expr) => ({
                data.lock().unwrap().clear();
                $highlight();
                let vec = data.lock().unwrap().clone();
                let actual = from_utf8(&vec[..]).unwrap();
                println!("actual:");
                println!("{}", actual);
                println!("expected:");
                println!("{}", $expected);
                assert_eq!(&actual[..], &$expected[..]);
            });
        }

        let msp0 = MultiSpan::from_spans(vec![sp0, sp2, sp3, sp4, sp5]);
        let msp = MultiSpan::from_spans(vec![sp1, sp2, sp3, sp4, sp5]);

        test!(expect0, || {
            diag.highlight_lines(&msp0, Level::Error).unwrap();
        });
        test!(expect, || {
            diag.highlight_lines(&msp, Level::Error).unwrap();
        });
    }
}
