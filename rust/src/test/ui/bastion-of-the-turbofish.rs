// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-pass

// Bastion of the Turbofish
// ------------------------
// Beware travellers, lest you venture into waters callous and unforgiving,
// where hope must be abandoned, ere it is cruelly torn from you. For here
// stands the bastion of the Turbofish: an impenetrable fortress holding
// unshaking against those who would dare suggest the supererogation of the
// Turbofish.
//
// Once I was young and foolish and had the impudence to imagine that I could
// shake free from the coils by which that creature had us tightly bound. I
// dared to suggest that there was a better way: a brighter future, in which
// Rustaceans both new and old could be rid of that vile beast. But alas! In
// my foolhardiness my ignorance was unveiled and my dreams were dashed
// unforgivingly against the rock of syntactic ambiguity.
//
// This humble program, small and insignificant though it might seem,
// demonstrates that to which we had previously cast a blind eye: an ambiguity
// in permitting generic arguments to be provided without the consent of the
// Great Turbofish. Should you be so naïve as to try to revolt against its
// mighty clutches, here shall its wrath be indomitably displayed. This
// program must pass for all eternity, fundamentally at odds with an impetuous
// rebellion against the Turbofish.
//
// My heart aches in sorrow, for I know I am defeated. Let this be a warning
// to all those who come after. Here stands the bastion of the Turbofish.

fn main() {
    let (oh, woe, is, me) = ("the", "Turbofish", "remains", "undefeated");
    let _: (bool, bool) = (oh<woe, is>(me));
}
