// Check that generators respect the muatability of their upvars.

#![feature(generators)]

fn mutate_upvar() {
    let x = 0;
    move || {
        x = 1;
        //~^ ERROR
        yield;
    };
}

fn main() {}
