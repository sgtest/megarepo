import std::{io, term};
import io::writer_util;
import syntax::codemap;
import codemap::span;

export emit_warning, emit_error, emit_note;
export handler, codemap_handler;

iface handler {
    fn span_fatal(sp: span, msg: str) -> !;
    fn fatal(msg: str) -> !;
    fn span_err(sp: span, msg: str);
    fn err(msg: str);
    fn has_errors() -> bool;
    fn abort_if_errors();
    fn span_warn(sp: span, msg: str);
    fn warn(msg: str);
    fn span_note(sp: span, msg: str);
    fn note(msg: str);
    fn span_bug(sp: span, msg: str) -> !;
    fn bug(msg: str) -> !;
    fn span_unimpl(sp: span, msg: str) -> !;
    fn unimpl(msg: str) -> !;
}

type codemap_t = @{
    cm: codemap::codemap,
    mutable err_count: uint
};

impl codemap_handler of handler for codemap_t {
    fn span_fatal(sp: span, msg: str) -> ! {
        emit_error(some((self.cm, sp)), msg);
        fail;
    }
    fn fatal(msg: str) -> ! {
        emit_error(none, msg);
        fail;
    }
    fn span_err(sp: span, msg: str) {
        emit_error(some((self.cm, sp)), msg);
        self.err_count += 1u;
    }
    fn err(msg: str) {
        emit_error(none, msg);
        self.err_count += 1u;
    }
    fn has_errors() -> bool { self.err_count > 0u }
    fn abort_if_errors() {
        if self.err_count > 0u {
            self.fatal("aborting due to previous errors");
        }
    }
    fn span_warn(sp: span, msg: str) {
        emit_warning(some((self.cm, sp)), msg);
    }
    fn warn(msg: str) {
        emit_warning(none, msg);
    }
    fn span_note(sp: span, msg: str) {
        emit_note(some((self.cm, sp)), msg);
    }
    fn note(msg: str) {
        emit_note(none, msg);
    }
    fn span_bug(sp: span, msg: str) -> ! {
        self.span_fatal(sp, #fmt["internal compiler error %s", msg]);
    }
    fn bug(msg: str) -> ! {
        self.fatal(#fmt["internal compiler error %s", msg]);
    }
    fn span_unimpl(sp: span, msg: str) -> ! {
        self.span_bug(sp, "unimplemented " + msg);
    }
    fn unimpl(msg: str) -> ! { self.bug("unimplemented " + msg); }
}

fn mk_codemap_handler(cm: codemap::codemap) -> handler {
    @{
        cm: cm,
        mutable err_count: 0u,
    } as handler
}

tag diagnostictype {
    warning;
    error;
    note;
}

fn diagnosticstr(t: diagnostictype) -> str {
    alt t {
      warning. { "warning" }
      error. { "error" }
      note. { "note" }
    }
}

fn diagnosticcolor(t: diagnostictype) -> u8 {
    alt t {
      warning. { term::color_bright_yellow }
      error. { term::color_bright_red }
      note. { term::color_bright_green }
    }
}

fn print_diagnostic(topic: str, t: diagnostictype, msg: str) {
    if str::is_not_empty(topic) {
        io::stdout().write_str(#fmt["%s ", topic]);
    }
    if term::color_supported() {
        term::fg(io::stdout(), diagnosticcolor(t));
    }
    io::stdout().write_str(#fmt["%s:", diagnosticstr(t)]);
    if term::color_supported() {
        term::reset(io::stdout());
    }
    io::stdout().write_str(#fmt[" %s\n", msg]);
}

fn emit_diagnostic(cmsp: option<(codemap::codemap, span)>,
                   msg: str, t: diagnostictype) {
    alt cmsp {
      some((cm, sp)) {
        let ss = codemap::span_to_str(sp, cm);
        let lines = codemap::span_to_lines(sp, cm);
        print_diagnostic(ss, t, msg);
        highlight_lines(cm, sp, lines);
      }
      none. {
        print_diagnostic("", t, msg);
      }
    }
}

fn highlight_lines(cm: codemap::codemap, sp: span,
                   lines: @codemap::file_lines) {

    // If we're not looking at a real file then we can't re-open it to
    // pull out the lines
    if lines.name == "-" { ret; }

    // FIXME: reading in the entire file is the worst possible way to
    //        get access to the necessary lines.
    let file = alt io::read_whole_file_str(lines.name) {
      result::ok(file) { file }
      result::err(e) {
        emit_error(none, e);
        fail;
      }
    };
    let fm = codemap::get_filemap(cm, lines.name);

    // arbitrarily only print up to six lines of the error
    let max_lines = 6u;
    let elided = false;
    let display_lines = lines.lines;
    if vec::len(display_lines) > max_lines {
        display_lines = vec::slice(display_lines, 0u, max_lines);
        elided = true;
    }
    // Print the offending lines
    for line: uint in display_lines {
        io::stdout().write_str(#fmt["%s:%u ", fm.name, line + 1u]);
        let s = codemap::get_line(fm, line as int, file);
        if !str::ends_with(s, "\n") { s += "\n"; }
        io::stdout().write_str(s);
    }
    if elided {
        let last_line = display_lines[vec::len(display_lines) - 1u];
        let s = #fmt["%s:%u ", fm.name, last_line + 1u];
        let indent = str::char_len(s);
        let out = "";
        while indent > 0u { out += " "; indent -= 1u; }
        out += "...\n";
        io::stdout().write_str(out);
    }


    // If there's one line at fault we can easily point to the problem
    if vec::len(lines.lines) == 1u {
        let lo = codemap::lookup_char_pos(cm, sp.lo);
        let digits = 0u;
        let num = (lines.lines[0] + 1u) / 10u;

        // how many digits must be indent past?
        while num > 0u { num /= 10u; digits += 1u; }

        // indent past |name:## | and the 0-offset column location
        let left = str::char_len(fm.name) + digits + lo.col + 3u;
        let s = "";
        while left > 0u { str::push_char(s, ' '); left -= 1u; }

        s += "^";
        let hi = codemap::lookup_char_pos(cm, sp.hi);
        if hi.col != lo.col {
            // the ^ already takes up one space
            let width = hi.col - lo.col - 1u;
            while width > 0u { str::push_char(s, '~'); width -= 1u; }
        }
        io::stdout().write_str(s + "\n");
    }
}

fn emit_warning(cmsp: option<(codemap::codemap, span)>, msg: str) {
    emit_diagnostic(cmsp, msg, warning);
}
fn emit_error(cmsp: option<(codemap::codemap, span)>, msg: str) {
    emit_diagnostic(cmsp, msg, error);
}
fn emit_note(cmsp: option<(codemap::codemap, span)>, msg: str) {
    emit_diagnostic(cmsp, msg, note);
}
