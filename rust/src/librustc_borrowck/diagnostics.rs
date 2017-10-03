// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_snake_case)]

register_long_diagnostics! {

E0373: r##"
This error occurs when an attempt is made to use data captured by a closure,
when that data may no longer exist. It's most commonly seen when attempting to
return a closure:

```compile_fail,E0373
fn foo() -> Box<Fn(u32) -> u32> {
    let x = 0u32;
    Box::new(|y| x + y)
}
```

Notice that `x` is stack-allocated by `foo()`. By default, Rust captures
closed-over data by reference. This means that once `foo()` returns, `x` no
longer exists. An attempt to access `x` within the closure would thus be
unsafe.

Another situation where this might be encountered is when spawning threads:

```compile_fail,E0373
fn foo() {
    let x = 0u32;
    let y = 1u32;

    let thr = std::thread::spawn(|| {
        x + y
    });
}
```

Since our new thread runs in parallel, the stack frame containing `x` and `y`
may well have disappeared by the time we try to use them. Even if we call
`thr.join()` within foo (which blocks until `thr` has completed, ensuring the
stack frame won't disappear), we will not succeed: the compiler cannot prove
that this behaviour is safe, and so won't let us do it.

The solution to this problem is usually to switch to using a `move` closure.
This approach moves (or copies, where possible) data into the closure, rather
than taking references to it. For example:

```
fn foo() -> Box<Fn(u32) -> u32> {
    let x = 0u32;
    Box::new(move |y| x + y)
}
```

Now that the closure has its own copy of the data, there's no need to worry
about safety.
"##,

E0382: r##"
This error occurs when an attempt is made to use a variable after its contents
have been moved elsewhere. For example:

```compile_fail,E0382
struct MyStruct { s: u32 }

fn main() {
    let mut x = MyStruct{ s: 5u32 };
    let y = x;
    x.s = 6;
    println!("{}", x.s);
}
```

Since `MyStruct` is a type that is not marked `Copy`, the data gets moved out
of `x` when we set `y`. This is fundamental to Rust's ownership system: outside
of workarounds like `Rc`, a value cannot be owned by more than one variable.

If we own the type, the easiest way to address this problem is to implement
`Copy` and `Clone` on it, as shown below. This allows `y` to copy the
information in `x`, while leaving the original version owned by `x`. Subsequent
changes to `x` will not be reflected when accessing `y`.

```
#[derive(Copy, Clone)]
struct MyStruct { s: u32 }

fn main() {
    let mut x = MyStruct{ s: 5u32 };
    let y = x;
    x.s = 6;
    println!("{}", x.s);
}
```

Alternatively, if we don't control the struct's definition, or mutable shared
ownership is truly required, we can use `Rc` and `RefCell`:

```
use std::cell::RefCell;
use std::rc::Rc;

struct MyStruct { s: u32 }

fn main() {
    let mut x = Rc::new(RefCell::new(MyStruct{ s: 5u32 }));
    let y = x.clone();
    x.borrow_mut().s = 6;
    println!("{}", x.borrow().s);
}
```

With this approach, x and y share ownership of the data via the `Rc` (reference
count type). `RefCell` essentially performs runtime borrow checking: ensuring
that at most one writer or multiple readers can access the data at any one time.

If you wish to learn more about ownership in Rust, start with the chapter in the
Book:

https://doc.rust-lang.org/book/first-edition/ownership.html
"##,

E0383: r##"
This error occurs when an attempt is made to partially reinitialize a
structure that is currently uninitialized.

For example, this can happen when a drop has taken place:

```compile_fail,E0383
struct Foo {
    a: u32,
}
impl Drop for Foo {
    fn drop(&mut self) { /* ... */ }
}

let mut x = Foo { a: 1 };
drop(x); // `x` is now uninitialized
x.a = 2; // error, partial reinitialization of uninitialized structure `t`
```

This error can be fixed by fully reinitializing the structure in question:

```
struct Foo {
    a: u32,
}
impl Drop for Foo {
    fn drop(&mut self) { /* ... */ }
}

let mut x = Foo { a: 1 };
drop(x);
x = Foo { a: 2 };
```
"##,

/*E0386: r##"
This error occurs when an attempt is made to mutate the target of a mutable
reference stored inside an immutable container.

For example, this can happen when storing a `&mut` inside an immutable `Box`:

```compile_fail,E0386
let mut x: i64 = 1;
let y: Box<_> = Box::new(&mut x);
**y = 2; // error, cannot assign to data in an immutable container
```

This error can be fixed by making the container mutable:

```
let mut x: i64 = 1;
let mut y: Box<_> = Box::new(&mut x);
**y = 2;
```

It can also be fixed by using a type with interior mutability, such as `Cell`
or `RefCell`:

```
use std::cell::Cell;

let x: i64 = 1;
let y: Box<Cell<_>> = Box::new(Cell::new(x));
y.set(2);
```
"##,*/

E0387: r##"
This error occurs when an attempt is made to mutate or mutably reference data
that a closure has captured immutably. Examples of this error are shown below:

```compile_fail,E0387
// Accepts a function or a closure that captures its environment immutably.
// Closures passed to foo will not be able to mutate their closed-over state.
fn foo<F: Fn()>(f: F) { }

// Attempts to mutate closed-over data. Error message reads:
// `cannot assign to data in a captured outer variable...`
fn mutable() {
    let mut x = 0u32;
    foo(|| x = 2);
}

// Attempts to take a mutable reference to closed-over data.  Error message
// reads: `cannot borrow data mutably in a captured outer variable...`
fn mut_addr() {
    let mut x = 0u32;
    foo(|| { let y = &mut x; });
}
```

The problem here is that foo is defined as accepting a parameter of type `Fn`.
Closures passed into foo will thus be inferred to be of type `Fn`, meaning that
they capture their context immutably.

If the definition of `foo` is under your control, the simplest solution is to
capture the data mutably. This can be done by defining `foo` to take FnMut
rather than Fn:

```
fn foo<F: FnMut()>(f: F) { }
```

Alternatively, we can consider using the `Cell` and `RefCell` types to achieve
interior mutability through a shared reference. Our example's `mutable`
function could be redefined as below:

```
use std::cell::Cell;

fn foo<F: Fn()>(f: F) { }

fn mutable() {
    let x = Cell::new(0u32);
    foo(|| x.set(2));
}
```

You can read more about cell types in the API documentation:

https://doc.rust-lang.org/std/cell/
"##,

E0388: r##"
E0388 was removed and is no longer issued.
"##,

E0389: r##"
An attempt was made to mutate data using a non-mutable reference. This
commonly occurs when attempting to assign to a non-mutable reference of a
mutable reference (`&(&mut T)`).

Example of erroneous code:

```compile_fail,E0389
struct FancyNum {
    num: u8,
}

fn main() {
    let mut fancy = FancyNum{ num: 5 };
    let fancy_ref = &(&mut fancy);
    fancy_ref.num = 6; // error: cannot assign to data in a `&` reference
    println!("{}", fancy_ref.num);
}
```

Here, `&mut fancy` is mutable, but `&(&mut fancy)` is not. Creating an
immutable reference to a value borrows it immutably. There can be multiple
references of type `&(&mut T)` that point to the same value, so they must be
immutable to prevent multiple mutable references to the same value.

To fix this, either remove the outer reference:

```
struct FancyNum {
    num: u8,
}

fn main() {
    let mut fancy = FancyNum{ num: 5 };

    let fancy_ref = &mut fancy;
    // `fancy_ref` is now &mut FancyNum, rather than &(&mut FancyNum)

    fancy_ref.num = 6; // No error!

    println!("{}", fancy_ref.num);
}
```

Or make the outer reference mutable:

```
struct FancyNum {
    num: u8
}

fn main() {
    let mut fancy = FancyNum{ num: 5 };

    let fancy_ref = &mut (&mut fancy);
    // `fancy_ref` is now &mut(&mut FancyNum), rather than &(&mut FancyNum)

    fancy_ref.num = 6; // No error!

    println!("{}", fancy_ref.num);
}
```
"##,

E0507: r##"
You tried to move out of a value which was borrowed. Erroneous code example:

```compile_fail,E0507
use std::cell::RefCell;

struct TheDarkKnight;

impl TheDarkKnight {
    fn nothing_is_true(self) {}
}

fn main() {
    let x = RefCell::new(TheDarkKnight);

    x.borrow().nothing_is_true(); // error: cannot move out of borrowed content
}
```

Here, the `nothing_is_true` method takes the ownership of `self`. However,
`self` cannot be moved because `.borrow()` only provides an `&TheDarkKnight`,
which is a borrow of the content owned by the `RefCell`. To fix this error,
you have three choices:

* Try to avoid moving the variable.
* Somehow reclaim the ownership.
* Implement the `Copy` trait on the type.

Examples:

```
use std::cell::RefCell;

struct TheDarkKnight;

impl TheDarkKnight {
    fn nothing_is_true(&self) {} // First case, we don't take ownership
}

fn main() {
    let x = RefCell::new(TheDarkKnight);

    x.borrow().nothing_is_true(); // ok!
}
```

Or:

```
use std::cell::RefCell;

struct TheDarkKnight;

impl TheDarkKnight {
    fn nothing_is_true(self) {}
}

fn main() {
    let x = RefCell::new(TheDarkKnight);
    let x = x.into_inner(); // we get back ownership

    x.nothing_is_true(); // ok!
}
```

Or:

```
use std::cell::RefCell;

#[derive(Clone, Copy)] // we implement the Copy trait
struct TheDarkKnight;

impl TheDarkKnight {
    fn nothing_is_true(self) {}
}

fn main() {
    let x = RefCell::new(TheDarkKnight);

    x.borrow().nothing_is_true(); // ok!
}
```

Moving a member out of a mutably borrowed struct will also cause E0507 error:

```compile_fail,E0507
struct TheDarkKnight;

impl TheDarkKnight {
    fn nothing_is_true(self) {}
}

struct Batcave {
    knight: TheDarkKnight
}

fn main() {
    let mut cave = Batcave {
        knight: TheDarkKnight
    };
    let borrowed = &mut cave;

    borrowed.knight.nothing_is_true(); // E0507
}
```

It is fine only if you put something back. `mem::replace` can be used for that:

```
# struct TheDarkKnight;
# impl TheDarkKnight { fn nothing_is_true(self) {} }
# struct Batcave { knight: TheDarkKnight }
use std::mem;

let mut cave = Batcave {
    knight: TheDarkKnight
};
let borrowed = &mut cave;

mem::replace(&mut borrowed.knight, TheDarkKnight).nothing_is_true(); // ok!
```

You can find more information about borrowing in the rust-book:
http://doc.rust-lang.org/book/first-edition/references-and-borrowing.html
"##,

E0508: r##"
A value was moved out of a non-copy fixed-size array.

Example of erroneous code:

```compile_fail,E0508
struct NonCopy;

fn main() {
    let array = [NonCopy; 1];
    let _value = array[0]; // error: cannot move out of type `[NonCopy; 1]`,
                           //        a non-copy fixed-size array
}
```

The first element was moved out of the array, but this is not
possible because `NonCopy` does not implement the `Copy` trait.

Consider borrowing the element instead of moving it:

```
struct NonCopy;

fn main() {
    let array = [NonCopy; 1];
    let _value = &array[0]; // Borrowing is allowed, unlike moving.
}
```

Alternatively, if your type implements `Clone` and you need to own the value,
consider borrowing and then cloning:

```
#[derive(Clone)]
struct NonCopy;

fn main() {
    let array = [NonCopy; 1];
    // Now you can clone the array element.
    let _value = array[0].clone();
}
```
"##,

E0509: r##"
This error occurs when an attempt is made to move out of a value whose type
implements the `Drop` trait.

Example of erroneous code:

```compile_fail,E0509
struct FancyNum {
    num: usize
}

struct DropStruct {
    fancy: FancyNum
}

impl Drop for DropStruct {
    fn drop(&mut self) {
        // Destruct DropStruct, possibly using FancyNum
    }
}

fn main() {
    let drop_struct = DropStruct{fancy: FancyNum{num: 5}};
    let fancy_field = drop_struct.fancy; // Error E0509
    println!("Fancy: {}", fancy_field.num);
    // implicit call to `drop_struct.drop()` as drop_struct goes out of scope
}
```

Here, we tried to move a field out of a struct of type `DropStruct` which
implements the `Drop` trait. However, a struct cannot be dropped if one or
more of its fields have been moved.

Structs implementing the `Drop` trait have an implicit destructor that gets
called when they go out of scope. This destructor may use the fields of the
struct, so moving out of the struct could make it impossible to run the
destructor. Therefore, we must think of all values whose type implements the
`Drop` trait as single units whose fields cannot be moved.

This error can be fixed by creating a reference to the fields of a struct,
enum, or tuple using the `ref` keyword:

```
struct FancyNum {
    num: usize
}

struct DropStruct {
    fancy: FancyNum
}

impl Drop for DropStruct {
    fn drop(&mut self) {
        // Destruct DropStruct, possibly using FancyNum
    }
}

fn main() {
    let drop_struct = DropStruct{fancy: FancyNum{num: 5}};
    let ref fancy_field = drop_struct.fancy; // No more errors!
    println!("Fancy: {}", fancy_field.num);
    // implicit call to `drop_struct.drop()` as drop_struct goes out of scope
}
```

Note that this technique can also be used in the arms of a match expression:

```
struct FancyNum {
    num: usize
}

enum DropEnum {
    Fancy(FancyNum)
}

impl Drop for DropEnum {
    fn drop(&mut self) {
        // Destruct DropEnum, possibly using FancyNum
    }
}

fn main() {
    // Creates and enum of type `DropEnum`, which implements `Drop`
    let drop_enum = DropEnum::Fancy(FancyNum{num: 10});
    match drop_enum {
        // Creates a reference to the inside of `DropEnum::Fancy`
        DropEnum::Fancy(ref fancy_field) => // No error!
            println!("It was fancy-- {}!", fancy_field.num),
    }
    // implicit call to `drop_enum.drop()` as drop_enum goes out of scope
}
```
"##,

E0595: r##"
Closures cannot mutate immutable captured variables.

Erroneous code example:

```compile_fail,E0595
let x = 3; // error: closure cannot assign to immutable local variable `x`
let mut c = || { x += 1 };
```

Make the variable binding mutable:

```
let mut x = 3; // ok!
let mut c = || { x += 1 };
```
"##,

E0596: r##"
This error occurs because you tried to mutably borrow a non-mutable variable.

Example of erroneous code:

```compile_fail,E0596
let x = 1;
let y = &mut x; // error: cannot borrow mutably
```

In here, `x` isn't mutable, so when we try to mutably borrow it in `y`, it
fails. To fix this error, you need to make `x` mutable:

```
let mut x = 1;
let y = &mut x; // ok!
```
"##,

E0597: r##"
This error occurs because a borrow was made inside a variable which has a
greater lifetime than the borrowed one.

Example of erroneous code:

```compile_fail,E0597
struct Foo<'a> {
    x: Option<&'a u32>,
}

let mut x = Foo { x: None };
let y = 0;
x.x = Some(&y); // error: `y` does not live long enough
```

In here, `x` is created before `y` and therefore has a greater lifetime. Always
keep in mind that values in a scope are dropped in the opposite order they are
created. So to fix the previous example, just make the `y` lifetime greater than
the `x`'s one:

```
struct Foo<'a> {
    x: Option<&'a u32>,
}

let y = 0;
let mut x = Foo { x: None };
x.x = Some(&y);
```
"##,

E0626: r##"
This error occurs because a borrow in a generator persists across a
yield point.

```compile_fail,E0626
# #![feature(generators, generator_trait)]
# use std::ops::Generator;
let mut b = || {
    let a = &String::new(); // <-- This borrow...
    yield (); // ...is still in scope here, when the yield occurs.
    println!("{}", a);
};
b.resume();
```

At present, it is not permitted to have a yield that occurs while a
borrow is still in scope. To resolve this error, the borrow must
either be "contained" to a smaller scope that does not overlap the
yield or else eliminated in another way. So, for example, we might
resolve the previous example by removing the borrow and just storing
the integer by value:

```
# #![feature(generators, generator_trait)]
# use std::ops::Generator;
let mut b = || {
    let a = 3;
    yield ();
    println!("{}", a);
};
b.resume();
```

This is a very simple case, of course. In more complex cases, we may
wish to have more than one reference to the value that was borrowed --
in those cases, something like the `Rc` or `Arc` types may be useful.

This error also frequently arises with iteration:

```compile_fail,E0626
# #![feature(generators, generator_trait)]
# use std::ops::Generator;
let mut b = || {
  let v = vec![1,2,3];
  for &x in &v { // <-- borrow of `v` is still in scope...
    yield x; // ...when this yield occurs.
  }
};
b.resume();
```

Such cases can sometimes be resolved by iterating "by value" (or using
`into_iter()`) to avoid borrowing:

```
# #![feature(generators, generator_trait)]
# use std::ops::Generator;
let mut b = || {
  let v = vec![1,2,3];
  for x in v { // <-- Take ownership of the values instead!
    yield x; // <-- Now yield is OK.
  }
};
b.resume();
```

If taking ownership is not an option, using indices can work too:

```
# #![feature(generators, generator_trait)]
# use std::ops::Generator;
let mut b = || {
  let v = vec![1,2,3];
  let len = v.len(); // (*)
  for i in 0..len {
    let x = v[i]; // (*)
    yield x; // <-- Now yield is OK.
  }
};
b.resume();

// (*) -- Unfortunately, these temporaries are currently required.
// See <https://github.com/rust-lang/rust/issues/43122>.
```
"##,

}

register_diagnostics! {
//    E0385, // {} in an aliasable location
    E0598, // lifetime of {} is too short to guarantee its contents can be...
}
