

// Simple Extensible Binary Markup Language (ebml) reader and writer on a
// cursor model. See the specification here:
//     http://www.matroska.org/technical/specs/rfc/index.html
import core::option;
import option::{some, none};

type ebml_tag = {id: uint, size: uint};

type ebml_state = {ebml_tag: ebml_tag, tag_pos: uint, data_pos: uint};


// TODO: When we have module renaming, make "reader" and "writer" separate
// modules within this file.

// ebml reading
type doc = {data: @[u8], start: uint, end: uint};

type tagged_doc = {tag: uint, doc: doc};

fn vint_at(data: [u8], start: uint) -> {val: uint, next: uint} {
    let a = data[start];
    if a & 0x80u8 != 0u8 {
        ret {val: (a & 0x7fu8) as uint, next: start + 1u};
    }
    if a & 0x40u8 != 0u8 {
        ret {val: ((a & 0x3fu8) as uint) << 8u | (data[start + 1u] as uint),
             next: start + 2u};
    } else if a & 0x20u8 != 0u8 {
        ret {val: ((a & 0x1fu8) as uint) << 16u |
                 (data[start + 1u] as uint) << 8u |
                 (data[start + 2u] as uint),
             next: start + 3u};
    } else if a & 0x10u8 != 0u8 {
        ret {val: ((a & 0x0fu8) as uint) << 24u |
                 (data[start + 1u] as uint) << 16u |
                 (data[start + 2u] as uint) << 8u |
                 (data[start + 3u] as uint),
             next: start + 4u};
    } else { #error("vint too big"); fail; }
}

fn new_doc(data: @[u8]) -> doc {
    ret {data: data, start: 0u, end: vec::len::<u8>(*data)};
}

fn doc_at(data: @[u8], start: uint) -> tagged_doc {
    let elt_tag = vint_at(*data, start);
    let elt_size = vint_at(*data, elt_tag.next);
    let end = elt_size.next + elt_size.val;
    ret {tag: elt_tag.val,
         doc: {data: data, start: elt_size.next, end: end}};
}

fn maybe_get_doc(d: doc, tg: uint) -> option<doc> {
    let pos = d.start;
    while pos < d.end {
        let elt_tag = vint_at(*d.data, pos);
        let elt_size = vint_at(*d.data, elt_tag.next);
        pos = elt_size.next + elt_size.val;
        if elt_tag.val == tg {
            ret some::<doc>({data: d.data, start: elt_size.next, end: pos});
        }
    }
    ret none::<doc>;
}

fn get_doc(d: doc, tg: uint) -> doc {
    alt maybe_get_doc(d, tg) {
      some(d) { ret d; }
      none {
        #error("failed to find block with enum %u", tg);
        fail;
      }
    }
}

fn docs(d: doc, it: fn(uint, doc)) {
    let pos = d.start;
    while pos < d.end {
        let elt_tag = vint_at(*d.data, pos);
        let elt_size = vint_at(*d.data, elt_tag.next);
        pos = elt_size.next + elt_size.val;
        it(elt_tag.val, {data: d.data, start: elt_size.next, end: pos});
    }
}

fn tagged_docs(d: doc, tg: uint, it: fn(doc)) {
    let pos = d.start;
    while pos < d.end {
        let elt_tag = vint_at(*d.data, pos);
        let elt_size = vint_at(*d.data, elt_tag.next);
        pos = elt_size.next + elt_size.val;
        if elt_tag.val == tg {
            it({data: d.data, start: elt_size.next, end: pos});
        }
    }
}

fn doc_data(d: doc) -> [u8] { ret vec::slice::<u8>(*d.data, d.start, d.end); }

fn doc_str(d: doc) -> str { ret str::from_bytes(doc_data(d)); }

fn be_uint_from_bytes(data: @[u8], start: uint, size: uint) -> uint {
    let sz = size;
    assert (sz <= 4u);
    let val = 0u;
    let pos = start;
    while sz > 0u {
        sz -= 1u;
        val += (data[pos] as uint) << sz * 8u;
        pos += 1u;
    }
    ret val;
}

fn doc_as_uint(d: doc) -> uint {
    ret be_uint_from_bytes(d.data, d.start, d.end - d.start);
}


// ebml writing
type writer = {writer: io::writer, mutable size_positions: [uint]};

fn write_sized_vint(w: io::writer, n: u64, size: uint) {
    let buf: [u8];
    alt size {
      1u { buf = [0x80u8 | (n as u8)]; }
      2u { buf = [0x40u8 | ((n >> 8_u64) as u8), n as u8]; }
      3u {
        buf = [0x20u8 | ((n >> 16_u64) as u8), (n >> 8_u64) as u8,
               n as u8];
      }
      4u {
        buf = [0x10u8 | ((n >> 24_u64) as u8), (n >> 16_u64) as u8,
               (n >> 8_u64) as u8, n as u8];
      }
      _ { #error("vint to write too big"); fail; }
    }
    w.write(buf);
}

fn write_vint(w: io::writer, n: u64) {
    if n < 0x7f_u64 { write_sized_vint(w, n, 1u); ret; }
    if n < 0x4000_u64 { write_sized_vint(w, n, 2u); ret; }
    if n < 0x200000_u64 { write_sized_vint(w, n, 3u); ret; }
    if n < 0x10000000_u64 { write_sized_vint(w, n, 4u); ret; }
    #error("vint to write too big");
    fail;
}

fn create_writer(w: io::writer) -> writer {
    let size_positions: [uint] = [];
    ret {writer: w, mutable size_positions: size_positions};
}


// TODO: Provide a function to write the standard ebml header.
fn start_tag(w: writer, tag_id: uint) {
    // Write the enum ID:
    write_vint(w.writer, tag_id as u64);

    // Write a placeholder four-byte size.
    w.size_positions += [w.writer.tell()];
    let zeroes: [u8] = [0u8, 0u8, 0u8, 0u8];
    w.writer.write(zeroes);
}

fn end_tag(w: writer) {
    let last_size_pos = vec::pop::<uint>(w.size_positions);
    let cur_pos = w.writer.tell();
    w.writer.seek(last_size_pos as int, io::seek_set);
    write_sized_vint(w.writer, (cur_pos - last_size_pos - 4u) as u64, 4u);
    w.writer.seek(cur_pos as int, io::seek_set);
}

impl writer_util for writer {
    fn wr_tag(tag_id: uint, blk: fn()) {
        start_tag(self, tag_id);
        blk();
        end_tag(self);
    }

    fn wr_u64(id: u64) {
        write_vint(self.writer, id);
    }

    fn wr_uint(id: uint) {
        self.wr_u64(id as u64);
    }

    fn wr_bytes(b: [u8]) {
        self.writer.write(b);
    }

    fn wr_str(s: str) {
        self.wr_bytes(str::bytes(s));
    }
}

// TODO: optionally perform "relaxations" on end_tag to more efficiently
// encode sizes; this is a fixed point iteration
