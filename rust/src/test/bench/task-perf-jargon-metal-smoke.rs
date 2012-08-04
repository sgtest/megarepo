// Test performance of a task "spawn ladder", in which children task have many
// many ancestor taskgroups, but with only a few such groups alive at a time.
// Each child task has to enlist as a descendant in each of its ancestor
// groups, but that shouldn't have to happen for already-dead groups.
//
// The filename is a song reference; google it in quotes.

fn child_generation(gens_left: uint) {
    // This used to be O(n^2) in the number of generations that ever existed.
    // With this code, only as many generations are alive at a time as tasks
    // alive at a time,
    do task::spawn_supervised {
        if gens_left & 1 == 1 {
            task::yield(); // shake things up a bit
        }
        if gens_left > 0 {
            child_generation(gens_left - 1); // recurse
        }
    }
}

fn main(args: ~[~str]) {
    let args = if os::getenv(~"RUST_BENCH").is_some() {
        ~[~"", ~"100000"]
    } else if args.len() <= 1u {
        ~[~"", ~"100"]
    } else {
        copy args
    };

    child_generation(uint::from_str(args[1]).get());
}
