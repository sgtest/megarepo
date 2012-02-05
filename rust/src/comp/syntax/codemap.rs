import core::{vec, uint, str, option, result};
import option::{some, none};

type filename = str;

type file_pos = {ch: uint, byte: uint};

/* A codemap is a thing that maps uints to file/line/column positions
 * in a crate. This to make it possible to represent the positions
 * with single-word things, rather than passing records all over the
 * compiler.
 */

type file_substr_ = {lo: uint, hi: uint, col: uint, line: uint};
type file_substr = option<file_substr_>;

type filemap =
    @{name: filename, substr: file_substr, src: @str,
      start_pos: file_pos, mutable lines: [file_pos]};

type codemap = @{mutable files: [filemap]};

type loc = {file: filemap, line: uint, col: uint};

fn new_codemap() -> codemap { @{mutable files: [] } }

fn new_filemap_w_substr(filename: filename, substr: file_substr,
                        src: @str,
                        start_pos_ch: uint, start_pos_byte: uint)
   -> filemap {
    ret @{name: filename, substr: substr, src: src,
          start_pos: {ch: start_pos_ch, byte: start_pos_byte},
          mutable lines: [{ch: start_pos_ch, byte: start_pos_byte}]};
}

fn new_filemap(filename: filename, src: @str,
               start_pos_ch: uint, start_pos_byte: uint)
    -> filemap {
    ret new_filemap_w_substr(filename, none, src,
                             start_pos_ch, start_pos_byte);
}

fn get_substr_info(cm: codemap, lo: uint, hi: uint)
    -> (filename, file_substr_)
{
    let pos = lookup_char_pos(cm, lo);
    let name = #fmt("<%s:%u:%u>", pos.file.name, pos.line, pos.col);
    ret (name, {lo: lo, hi: hi, col: pos.col, line: pos.line});
}

fn next_line(file: filemap, chpos: uint, byte_pos: uint) {
    file.lines += [{ch: chpos, byte: byte_pos}];
}

type lookup_fn = fn@(file_pos) -> uint;

fn lookup_line(map: codemap, pos: uint, lookup: lookup_fn)
    -> {fm: filemap, line: uint}
{
    let len = vec::len(map.files);
    let a = 0u;
    let b = len;
    while b - a > 1u {
        let m = (a + b) / 2u;
        if lookup(map.files[m].start_pos) > pos { b = m; } else { a = m; }
    }
    if (a >= len) {
        fail #fmt("position %u does not resolve to a source location", pos)
    }
    let f = map.files[a];
    a = 0u;
    b = vec::len(f.lines);
    while b - a > 1u {
        let m = (a + b) / 2u;
        if lookup(f.lines[m]) > pos { b = m; } else { a = m; }
    }
    ret {fm: f, line: a};
}

fn lookup_pos(map: codemap, pos: uint, lookup: lookup_fn) -> loc {
    let {fm: f, line: a} = lookup_line(map, pos, lookup);
    ret {file: f, line: a + 1u, col: pos - lookup(f.lines[a])};
}

fn lookup_char_pos(map: codemap, pos: uint) -> loc {
    fn lookup(pos: file_pos) -> uint { ret pos.ch; }
    ret lookup_pos(map, pos, lookup);
}

fn lookup_byte_pos(map: codemap, pos: uint) -> loc {
    fn lookup(pos: file_pos) -> uint { ret pos.byte; }
    ret lookup_pos(map, pos, lookup);
}

enum expn_info_ {
    expanded_from({call_site: span,
                   callie: {name: str, span: option<span>}})
}
type expn_info = option<@expn_info_>;
type span = {lo: uint, hi: uint, expn_info: expn_info};

fn span_to_str(sp: span, cm: codemap) -> str {
    let lo = lookup_char_pos(cm, sp.lo);
    let hi = lookup_char_pos(cm, sp.hi);
    ret #fmt("%s:%u:%u: %u:%u", lo.file.name,
             lo.line, lo.col, hi.line, hi.col)
}

type file_lines = {file: filemap, lines: [uint]};

fn span_to_lines(sp: span, cm: codemap::codemap) -> @file_lines {
    let lo = lookup_char_pos(cm, sp.lo);
    let hi = lookup_char_pos(cm, sp.hi);
    // FIXME: Check for filemap?
    let lines = [];
    uint::range(lo.line - 1u, hi.line as uint) {|i| lines += [i]; };
    ret @{file: lo.file, lines: lines};
}

fn get_line(fm: filemap, line: int) -> str unsafe {
    let begin: uint = fm.lines[line].byte - fm.start_pos.byte;
    let end: uint;
    if line as uint < vec::len(fm.lines) - 1u {
        end = fm.lines[line + 1].byte - fm.start_pos.byte;
    } else {
        // If we're not done parsing the file, we're at the limit of what's
        // parsed. If we just slice the rest of the string, we'll print out
        // the remainder of the file, which is undesirable.
        end = str::byte_len(*fm.src);
        let rest = str::unsafe::slice_bytes(*fm.src, begin, end);
        let newline = str::index(rest, '\n' as u8);
        if newline != -1 { end = begin + (newline as uint); }
    }
    ret str::unsafe::slice_bytes(*fm.src, begin, end);
}

fn lookup_byte_offset(cm: codemap::codemap, chpos: uint)
    -> {fm: filemap, pos: uint}
{
    fn lookup(pos: file_pos) -> uint { ret pos.ch; }
    let {fm,line} = lookup_line(cm,chpos,lookup);
    let line_offset = fm.lines[line].byte - fm.start_pos.byte;
    let col = chpos - fm.lines[line].ch;
    let col_offset = str::byte_len_range(*fm.src, line_offset, col);
    ret {fm: fm, pos: line_offset + col_offset};
}

fn span_to_snippet(sp: span, cm: codemap::codemap) -> str {
    let begin = lookup_byte_offset(cm,sp.lo);
    let end   = lookup_byte_offset(cm,sp.hi);
    assert begin.fm == end.fm;
    ret str::slice(*begin.fm.src, begin.pos, end.pos);
}

fn get_snippet(cm: codemap::codemap, fidx: uint, lo: uint, hi: uint) -> str
{
    let fm = cm.files[fidx];
    ret str::slice(*fm.src, lo, hi)
}

fn get_filemap(cm: codemap, filename: str) -> filemap {
    for fm: filemap in cm.files { if fm.name == filename { ret fm; } }
    //XXjdm the following triggers a mismatched type bug
    //      (or expected function, found _|_)
    fail; // ("asking for " + filename + " which we don't know about");
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
