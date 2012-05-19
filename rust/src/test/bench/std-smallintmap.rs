// Microbenchmark for the smallintmap library

use std;
import std::smallintmap;
import std::smallintmap::{smallintmap, map};
import io::writer_util;

fn append_sequential(min: uint, max: uint, map: smallintmap<uint>) {
    uint::range(min, max) { |i|
        map.insert(i, i + 22u);
    }
}

fn check_sequential(min: uint, max: uint, map: smallintmap<uint>) {
    uint::range(min, max) { |i|
        assert map.get(i) == i + 22u;
    }
}

fn main(args: [str]) {
    let args = if vec::len(args) <= 1u {["", "1000000", "100"]} else {args};
    let max = uint::from_str(args[1]).get();
    let rep = uint::from_str(args[2]).get();

    let mut checkf = 0.0;
    let mut appendf = 0.0;

    uint::range(0u, rep) {|_r|
        let map = smallintmap::mk();
        let start = std::time::precise_time_s();
        append_sequential(0u, max, map);
        let mid = std::time::precise_time_s();
        check_sequential(0u, max, map);
        let end = std::time::precise_time_s();

        checkf += (end - mid) as float;
        appendf += (mid - start) as float;
    }

    let maxf = max as float;

    io::stdout().write_str(#fmt("insert(): %? seconds\n", checkf));
    io::stdout().write_str(#fmt("        : %f op/sec\n", maxf/checkf));
    io::stdout().write_str(#fmt("get()   : %? seconds\n", appendf));
    io::stdout().write_str(#fmt("        : %f op/sec\n", maxf/appendf));
}