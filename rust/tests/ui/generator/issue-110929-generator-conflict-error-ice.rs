// edition:2021
#![feature(generators)]

fn main() {
    let x = &mut ();
    || {
        let _c = || yield *&mut *x;
        || _ = &mut *x;
        //~^ cannot borrow `*x` as mutable more than once at a time
    };
}
