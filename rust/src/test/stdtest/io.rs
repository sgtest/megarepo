// -*- rust -*-
use std;
import std::io;
import std::str;
import std::result;

#[cfg(target_os = "linux")]
#[cfg(target_os = "win32")]
#[test]
fn test_simple() {
    let tmpfile: str = "test/run-pass/lib-io-test-simple.tmp";
    log tmpfile;
    let frood: str = "A hoopy frood who really knows where his towel is.";
    log frood;
    {
        let out: io::writer =
            result::get(io::file_writer(tmpfile, [io::create, io::truncate]));
        out.write_str(frood);
    }
    let inp: io::reader = result::get(io::file_reader(tmpfile));
    let frood2: str = inp.read_c_str();
    log frood2;
    assert (str::eq(frood, frood2));
}

// FIXME (726)
#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn test_simple() { }

#[test]
fn file_reader_not_exist() {
    alt io::file_reader("not a file") {
      result::err(e) {
        assert e == "error opening not a file";
      }
      result::ok(_) { fail; }
    }
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "win32")]
#[test]
fn file_buf_writer_bad_name() {
    alt io::file_buf_writer("/?", []) {
      result::err(e) {
        assert e == "error opening /?";
      }
      result::ok(_) { fail; }
    }
}

// FIXME (726)
#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn file_buf_writer_bad_name() { }

#[cfg(target_os = "linux")]
#[cfg(target_os = "win32")]
#[test]
fn buffered_file_buf_writer_bad_name() {
    alt io::buffered_file_buf_writer("/?") {
      result::err(e) {
        assert e == "error opening /?";
      }
      result::ok(_) { fail; }
    }
}

// FIXME (726)
#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn buffered_file_buf_writer_bad_name() { }
