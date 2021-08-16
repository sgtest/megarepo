// gate-test-let_chains

// Here we test feature gating for ´let_chains`.
// See `disallowed-positions.rs` for the grammar
// defining the language for gated allowed positions.

#![allow(irrefutable_let_patterns)]

use std::ops::Range;

fn _if() {
    if let 0 = 1 {} // Stable!

    if (let 0 = 1) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    if (((let 0 = 1))) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    if true && let 0 = 1 {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    if let 0 = 1 && true {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    if (let 0 = 1) && true {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    if true && (let 0 = 1) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    if (let 0 = 1) && (let 0 = 1) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]

    if let 0 = 1 && let 1 = 2 && (let 2 = 3 && let 3 = 4 && let 4 = 5) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]

    if let Range { start: _, end: _ } = (true..true) && false {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]
}

fn _while() {
    while let 0 = 1 {} // Stable!

    while (let 0 = 1) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    while (((let 0 = 1))) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    while true && let 0 = 1 {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    while let 0 = 1 && true {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    while (let 0 = 1) && true {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    while true && (let 0 = 1) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    while (let 0 = 1) && (let 0 = 1) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]

    while let 0 = 1 && let 1 = 2 && (let 2 = 3 && let 3 = 4 && let 4 = 5) {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]
    //~| ERROR `let` expressions in this position are experimental [E0658]

    while let Range { start: _, end: _ } = (true..true) && false {}
    //~^ ERROR `let` expressions in this position are experimental [E0658]
}

fn _macros() {
    macro_rules! noop_expr { ($e:expr) => {}; }

    noop_expr!((let 0 = 1));
    //~^ ERROR `let` expressions in this position are experimental [E0658]

    macro_rules! use_expr {
        ($e:expr) => {
            if $e {}
            while $e {}
        }
    }
    use_expr!((let 0 = 1 && 0 == 0));
    //~^ ERROR `let` expressions in this position are experimental [E0658]
    use_expr!((let 0 = 1));
    //~^ ERROR `let` expressions in this position are experimental [E0658]
    #[cfg(FALSE)] (let 0 = 1);
    //~^ ERROR `let` expressions in this position are experimental [E0658]
    use_expr!(let 0 = 1);
    //~^ ERROR no rules expected the token `let`
    // ^--- FIXME(53667): Consider whether `Let` can be added to `ident_can_begin_expr`.
}

fn main() {}
