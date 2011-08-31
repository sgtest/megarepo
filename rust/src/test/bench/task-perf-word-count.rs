/**
   A parallel word-frequency counting program.

   This is meant primarily to demonstrate Rust's MapReduce framework.

   It takes a list of files on the command line and outputs a list of
   words along with how many times each word is used.

*/

use std;

import option = std::option::t;
import std::option::some;
import std::option::none;
import std::istr;
import std::map;
import std::vec;
import std::io;

import std::time;
import std::u64;

import std::task;
import std::task::joinable_task;
import std::comm;
import std::comm::chan;
import std::comm::port;
import std::comm::recv;
import std::comm::send;

fn map(filename: &istr, emit: map_reduce::putter) {
    let f = io::file_reader(filename);


    while true {
        alt read_word(f) { some(w) { emit(w, 1); } none. { break; } }
    }
}

fn reduce(word: &istr, get: map_reduce::getter) {
    let count = 0;


    while true { alt get() { some(_) { count += 1; } none. { break } } }
}

mod map_reduce {
    export putter;
    export getter;
    export mapper;
    export reducer;
    export map_reduce;

    type putter = fn(&istr, int);

    type mapper = fn(&istr, putter);

    type getter = fn() -> option<int>;

    type reducer = fn(&istr, getter);

    tag ctrl_proto {
        find_reducer(istr, chan<chan<reduce_proto>>);
        mapper_done;
    }

    tag reduce_proto { emit_val(int); done; ref; release; }

    fn start_mappers(ctrl: chan<ctrl_proto>, inputs: &[istr])
        -> [joinable_task] {
        let tasks = [];
        for i: istr in inputs {
            tasks += [task::spawn_joinable(bind map_task(ctrl, i))];
        }
        ret tasks;
    }

    fn map_task(ctrl: chan<ctrl_proto>, input: &istr) {
        // log_err "map_task " + input;
        let intermediates = map::new_str_hash();

        fn emit(im: &map::hashmap<istr, chan<reduce_proto>>,
                ctrl: chan<ctrl_proto>, key: &istr, val: int) {
            let c;
            alt im.find(key) {
              some(_c) {

                c = _c
              }
              none. {
                let p = port();
                send(ctrl, find_reducer(key, chan(p)));
                c = recv(p);
                im.insert(key, c);
                send(c, ref);
              }
            }
            send(c, emit_val(val));
        }

        map(input, bind emit(intermediates, ctrl, _, _));

        for each kv: @{key: istr, val: chan<reduce_proto>} in
                 intermediates.items() {
            send(kv.val, release);
        }

        send(ctrl, mapper_done);
    }

    fn reduce_task(key: &istr, out: chan<chan<reduce_proto>>) {
        let p = port();

        send(out, chan(p));

        let ref_count = 0;
        let is_done = false;

        fn get(p: &port<reduce_proto>, ref_count: &mutable int,
               is_done: &mutable bool) -> option<int> {
            while !is_done || ref_count > 0 {
                alt recv(p) {
                  emit_val(v) {
                    // log_err #ifmt("received %d", v);
                    ret some(v);
                  }
                  done. {
                    // log_err "all done";
                    is_done = true;
                  }
                  ref. { ref_count += 1; }
                  release. { ref_count -= 1; }
                }
            }
            ret none;
        }

        reduce(key, bind get(p, ref_count, is_done));
    }

    fn map_reduce(inputs: &[istr]) {
        let ctrl = port::<ctrl_proto>();

        // This task becomes the master control task. It task::_spawns
        // to do the rest.

        let reducers: map::hashmap<istr, chan<reduce_proto>>;

        reducers = map::new_str_hash();

        let tasks = start_mappers(chan(ctrl), inputs);

        let num_mappers = vec::len(inputs) as int;

        while num_mappers > 0 {
            alt recv(ctrl) {
              mapper_done. {
                // log_err "received mapper terminated.";
                num_mappers -= 1;
              }
              find_reducer(k, cc) {
                let c;
                // log_err "finding reducer for " + k;
                alt reducers.find(k) {
                  some(_c) {
                    // log_err "reusing existing reducer for " + k;
                    c = _c;
                  }
                  none. {
                    // log_err "creating new reducer for " + k;
                    let p = port();
                    tasks +=
                        [task::spawn_joinable(
                            bind reduce_task(k, chan(p)))];
                    c = recv(p);
                    reducers.insert(k, c);
                  }
                }
                send(cc, c);
              }
            }
        }

        for each kv: @{key: istr, val: chan<reduce_proto>} in reducers.items()
                 {
            send(kv.val, done);
        }

        for t in tasks { task::join(t); }
    }
}

fn main(argv: [istr]) {
    if vec::len(argv) < 2u {
        let out = io::stdout();

        out.write_line(
            #ifmt["Usage: %s <filename> ...", argv[0]]);

        // TODO: run something just to make sure the code hasn't
        // broken yet. This is the unit test mode of this program.

        ret;
    }

    // We can get by with 8k stacks, and we'll probably exhaust our
    // address space otherwise.
    task::set_min_stack(8192u);

    let start = time::precise_time_ns();

    map_reduce::map_reduce(vec::slice(argv, 1u, vec::len(argv)));
    let stop = time::precise_time_ns();

    let elapsed = stop - start;
    elapsed /= 1000000u64;

    log_err ~"MapReduce completed in " + u64::str(elapsed) + ~"ms";
}

fn read_word(r: io::reader) -> option<istr> {
    let w = ~"";

    while !r.eof() {
        let c = r.read_char();


        if is_word_char(c) {
            w += istr::from_char(c);
        } else { if w != ~"" { ret some(w); } }
    }
    ret none;
}

fn is_digit(c: char) -> bool {
    alt c {
      '0' { true }
      '1' { true }
      '2' { true }
      '3' { true }
      '4' { true }
      '5' { true }
      '6' { true }
      '7' { true }
      '8' { true }
      '9' { true }
      _ { false }
    }
}

fn is_alpha_lower(c: char) -> bool {
    alt c {
      'a' { true }
      'b' { true }
      'c' { true }
      'd' { true }
      'e' { true }
      'f' { true }
      'g' { true }
      'h' { true }
      'i' { true }
      'j' { true }
      'k' { true }
      'l' { true }
      'm' { true }
      'n' { true }
      'o' { true }
      'p' { true }
      'q' { true }
      'r' { true }
      's' { true }
      't' { true }
      'u' { true }
      'v' { true }
      'w' { true }
      'x' { true }
      'y' { true }
      'z' { true }
      _ { false }
    }
}

fn is_alpha_upper(c: char) -> bool {
    alt c {
      'A' { true }
      'B' { true }
      'C' { true }
      'D' { true }
      'E' { true }
      'F' { true }
      'G' { true }
      'H' { true }
      'I' { true }
      'J' { true }
      'K' { true }
      'L' { true }
      'M' { true }
      'N' { true }
      'O' { true }
      'P' { true }
      'Q' { true }
      'R' { true }
      'S' { true }
      'T' { true }
      'U' { true }
      'V' { true }
      'W' { true }
      'X' { true }
      'Y' { true }
      'Z' { true }
      _ { false }
    }
}

fn is_alpha(c: char) -> bool { is_alpha_upper(c) || is_alpha_lower(c) }

fn is_word_char(c: char) -> bool { is_alpha(c) || is_digit(c) || c == '_' }
