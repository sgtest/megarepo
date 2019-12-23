// Test `@` patterns combined with `box` patterns.

#![feature(bindings_after_at)]
#![feature(box_patterns)]
#![feature(slice_patterns)]

#[derive(Copy, Clone)]
struct C;

fn c() -> C { C }

struct NC;

fn nc() -> NC { NC }

fn main() {
    let a @ box &b = Box::new(&C);
    //~^ ERROR cannot bind by-move with sub-bindings
    //~| ERROR use of moved value

    let a @ box b = Box::new(C);
    //~^ ERROR cannot bind by-move with sub-bindings
    //~| ERROR use of moved value

    fn f1(a @ box &b: Box<&C>) {}
    //~^ ERROR cannot bind by-move with sub-bindings
    //~| ERROR use of moved value

    fn f2(a @ box b: Box<C>) {}
    //~^ ERROR cannot bind by-move with sub-bindings
    //~| ERROR use of moved value

    match Box::new(C) { a @ box b => {} }
    //~^ ERROR cannot bind by-move with sub-bindings
    //~| ERROR use of moved value

    let ref a @ box b = Box::new(NC); //~ ERROR cannot bind by-move and by-ref in the same pattern

    let ref a @ box ref mut b = Box::new(nc());
    //~^ ERROR cannot borrow `a` as mutable because it is also borrowed as immutable
    let ref a @ box ref mut b = Box::new(NC);
    //~^ ERROR cannot borrow `a` as mutable because it is also borrowed as immutable
    let ref a @ box ref mut b = Box::new(NC);
    //~^ ERROR cannot borrow `a` as mutable because it is also borrowed as immutable
    *b = NC;
    let ref a @ box ref mut b = Box::new(NC);
    //~^ ERROR cannot borrow `a` as mutable because it is also borrowed as immutable
    //~| ERROR cannot borrow `_` as mutable because it is also borrowed as immutable
    *b = NC;
    drop(a);

    let ref mut a @ box ref b = Box::new(NC);
    //~^ ERROR cannot borrow `a` as immutable because it is also borrowed as mutable
    //~| ERROR cannot borrow `_` as immutable because it is also borrowed as mutable
    *a = Box::new(NC);
    drop(b);

    fn f5(ref mut a @ box ref b: Box<NC>) {
        //~^ ERROR cannot borrow `a` as immutable because it is also borrowed as mutable
        //~| ERROR cannot borrow `_` as immutable because it is also borrowed as mutable
        *a = Box::new(NC);
        drop(b);
    }

    match Box::new(nc()) {
        ref mut a @ box ref b => {
            //~^ ERROR cannot borrow `a` as immutable because it is also borrowed as mutable
            //~| ERROR cannot borrow `_` as immutable because it is also borrowed as mutable
            *a = Box::new(NC);
            drop(b);
        }
    }

    match Box::new([Ok(c()), Err(nc()), Ok(c())]) {
        box [Ok(a), ref xs @ .., Err(b)] => {}
        //~^ ERROR cannot bind by-move and by-ref in the same pattern
        _ => {}
    }

    match [Ok(Box::new(c())), Err(Box::new(nc())), Ok(Box::new(c())), Ok(Box::new(c()))] {
        [Ok(box ref a), ref xs @ .., Err(box b), Err(box ref mut c)] => {}
        //~^ ERROR cannot bind by-move and by-ref in the same pattern
        _ => {}
    }
}
