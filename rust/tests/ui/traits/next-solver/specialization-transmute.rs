// compile-flags: -Znext-solver

#![feature(specialization)]
//~^ WARN the feature `specialization` is incomplete

trait Default {
    type Id;

    fn intu(&self) -> &Self::Id;
}

impl<T> Default for T {
    default type Id = T; //~ ERROR type annotations needed
    // This will be fixed by #111994
    fn intu(&self) -> &Self::Id {
        self
    }
}

fn transmute<T: Default<Id = U>, U: Copy>(t: T) -> U {
    *t.intu()
}

use std::num::NonZeroU8;
fn main() {
    let s = transmute::<u8, Option<NonZeroU8>>(0); // this call should then error
    assert_eq!(s, None);
}
