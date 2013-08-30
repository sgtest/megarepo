// xfail-pretty
// xfail-test

use std::cast::transmute;
use std::libc::{STDOUT_FILENO, c_int, fdopen, fgets, fopen, fputc, fwrite};
use std::libc::{size_t};
use std::ptr::null;

static LINE_LEN: u32 = 80;

static COMPLEMENTS: [u8, ..256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,

    0,
    'T' as u8,
    'V' as u8,
    'G' as u8,
    'H' as u8,
    0,
    0,
    'C' as u8,
    'D' as u8,
    0,
    0,
    'M' as u8,
    0,
    'K' as u8,
    'N' as u8,
    0,
    0,
    0,
    'Y' as u8,
    'S' as u8,
    'A' as u8,
    'A' as u8,
    'B' as u8,
    'W' as u8,
    0,
    'R' as u8,
    0,
    0,
    0,
    0,
    0,
    0,

    0,
    'T' as u8,
    'V' as u8,
    'G' as u8,
    'H' as u8,
    0,
    0,
    'C' as u8,
    'D' as u8,
    0,
    0,
    'M' as u8,
    0,
    'K' as u8,
    'N' as u8,
    0,
    0,
    0,
    'Y' as u8,
    'S' as u8,
    'A' as u8,
    'A' as u8,
    'B' as u8,
    'W' as u8,
    0,
    'R' as u8,
    0,
    0,
    0,
    0,
    0,
    0,

    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[fixed_stack_segment]
fn main() {
    unsafe {
        let mode = "r";
        //let stdin = fdopen(STDIN_FILENO as c_int, transmute(&mode[0]));
        let path = "reversecomplement-input.txt";
        let stdin = fopen(transmute(&path[0]), transmute(&mode[0]));
        let mode = "w";
        let stdout = fdopen(STDOUT_FILENO as c_int, transmute(&mode[0]));

        let mut out: ~[u8] = ~[];
        out.reserve(12777888);
        let mut pos = 0;

        loop {
            let needed = pos + (LINE_LEN as uint) + 1;
            if out.capacity() < needed {
                out.reserve_at_least(needed);
            }

            let mut ptr = out.unsafe_mut_ref(pos);
            if fgets(transmute(ptr), LINE_LEN as c_int, stdin) == null() {
                break;
            }

            // Don't change lines that begin with '>' or ';'.
            let first = *ptr;
            if first == ('>' as u8) {
                while *ptr != 0 {
                    ptr = ptr.offset(1);
                }
                *ptr = '\n' as u8;

                pos = (ptr as uint) - (out.unsafe_ref(0) as uint);
                fwrite(transmute(out.unsafe_ref(0)),
                       1,
                       pos as size_t,
                       stdout);

                pos = 0;
                loop;
            }

            // Complement other lines.
            loop {
                let ch = *ptr;
                if ch == 0 {
                    break;
                }
                *ptr = COMPLEMENTS.unsafe_get(ch as uint);
                ptr = ptr.offset(1);
            }
            *ptr = '\n' as u8;

            pos = (ptr as uint) - (out.unsafe_ref(0) as uint);
        }

        fwrite(transmute(out.unsafe_ref(0)), 1, pos as size_t, stdout);
    }
}
