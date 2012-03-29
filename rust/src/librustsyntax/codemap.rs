type filename = str;

type file_pos = {ch: uint, byte: uint};

/* A codemap is a thing that maps uints to file/line/column positions
 * in a crate. This to make it possible to represent the positions
 * with single-word things, rather than passing records all over the
 * compiler.
 */

enum file_substr {
    fss_none,
    fss_internal(span),
    fss_external({filename: str, line: uint, col: uint})
}

type filemap =
    @{name: filename, substr: file_substr, src: @str,
      start_pos: file_pos, mut lines: [file_pos]};

type codemap = @{mut files: [filemap]};

type loc = {file: filemap, line: uint, col: uint};

fn new_codemap() -> codemap { @{mut files: [] } }

fn new_filemap_w_substr(filename: filename, substr: file_substr,
                        src: @str,
                        start_pos_ch: uint, start_pos_byte: uint)
   -> filemap {
    ret @{name: filename, substr: substr, src: src,
          start_pos: {ch: start_pos_ch, byte: start_pos_byte},
          mut lines: [{ch: start_pos_ch, byte: start_pos_byte}]};
}

fn new_filemap(filename: filename, src: @str,
               start_pos_ch: uint, start_pos_byte: uint)
    -> filemap {
    ret new_filemap_w_substr(filename, fss_none, src,
                             start_pos_ch, start_pos_byte);
}

fn mk_substr_filename(cm: codemap, sp: span) -> str
{
    let pos = lookup_char_pos(cm, sp.lo);
    ret #fmt("<%s:%u:%u>", pos.file.name, pos.line, pos.col);
}

fn next_line(file: filemap, chpos: uint, byte_pos: uint) {
    file.lines += [{ch: chpos, byte: byte_pos}];
}

type lookup_fn = fn@(file_pos) -> uint;

fn lookup_line(map: codemap, pos: uint, lookup: lookup_fn)
    -> {fm: filemap, line: uint}
{
    let len = vec::len(map.files);
    let mut a = 0u;
    let mut b = len;
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

fn lookup_char_pos_adj(map: codemap, pos: uint)
    -> {filename: str, line: uint, col: uint, file: option<filemap>}
{
    let loc = lookup_char_pos(map, pos);
    alt (loc.file.substr) {
      fss_none {
        {filename: loc.file.name, line: loc.line, col: loc.col,
         file: some(loc.file)}
      }
      fss_internal(sp) {
        lookup_char_pos_adj(map, sp.lo + (pos - loc.file.start_pos.ch))
      }
      fss_external(eloc) {
        {filename: eloc.filename,
         line: eloc.line + loc.line - 1u,
         col: if loc.line == 1u {eloc.col + loc.col} else {loc.col},
         file: none}
      }
    }
}

fn adjust_span(map: codemap, sp: span) -> span {
    fn lookup(pos: file_pos) -> uint { ret pos.ch; }
    let line = lookup_line(map, sp.lo, lookup);
    alt (line.fm.substr) {
      fss_none {sp}
      fss_internal(s) {
        adjust_span(map, {lo: s.lo + (sp.lo - line.fm.start_pos.ch),
                          hi: s.lo + (sp.hi - line.fm.start_pos.ch),
                          expn_info: sp.expn_info})}
      fss_external(_) {sp}
    }
}

enum expn_info_ {
    expanded_from({call_site: span,
                   callie: {name: str, span: option<span>}})
}
type expn_info = option<@expn_info_>;
type span = {lo: uint, hi: uint, expn_info: expn_info};

fn span_to_str_no_adj(sp: span, cm: codemap) -> str {
    let lo = lookup_char_pos(cm, sp.lo);
    let hi = lookup_char_pos(cm, sp.hi);
    ret #fmt("%s:%u:%u: %u:%u", lo.file.name,
             lo.line, lo.col, hi.line, hi.col)
}

fn span_to_str(sp: span, cm: codemap) -> str {
    let lo = lookup_char_pos_adj(cm, sp.lo);
    let hi = lookup_char_pos_adj(cm, sp.hi);
    ret #fmt("%s:%u:%u: %u:%u", lo.filename,
             lo.line, lo.col, hi.line, hi.col)
}

type file_lines = {file: filemap, lines: [uint]};

fn span_to_lines(sp: span, cm: codemap::codemap) -> @file_lines {
    let lo = lookup_char_pos(cm, sp.lo);
    let hi = lookup_char_pos(cm, sp.hi);
    let mut lines = [];
    uint::range(lo.line - 1u, hi.line as uint) {|i| lines += [i]; };
    ret @{file: lo.file, lines: lines};
}

fn get_line(fm: filemap, line: int) -> str unsafe {
    let begin: uint = fm.lines[line].byte - fm.start_pos.byte;
    let end = alt str::find_char_from(*fm.src, '\n', begin) {
      some(e) { e }
      none { str::len(*fm.src) }
    };
    str::slice(*fm.src, begin, end)
}

fn lookup_byte_offset(cm: codemap::codemap, chpos: uint)
    -> {fm: filemap, pos: uint} {
    let {fm, line} = lookup_line(cm, chpos, {|pos| pos.ch});
    let line_offset = fm.lines[line].byte - fm.start_pos.byte;
    let col = chpos - fm.lines[line].ch;
    let col_offset = str::count_bytes(*fm.src, line_offset, col);
    {fm: fm, pos: line_offset + col_offset}
}

fn span_to_snippet(sp: span, cm: codemap::codemap) -> str {
    let begin = lookup_byte_offset(cm, sp.lo);
    let end = lookup_byte_offset(cm, sp.hi);
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
