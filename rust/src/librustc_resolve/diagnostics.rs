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

// Error messages for EXXXX errors.
// Each message should start and end with a new line, and be wrapped to 80 characters.
// In vim you can `:set tw=80` and use `gq` to wrap paragraphs. Use `:set tw=0` to disable.
register_long_diagnostics! {

E0154: r##"
Imports (`use` statements) are not allowed after non-item statements, such as
variable declarations and expression statements.

Here is an example that demonstrates the error:

```
fn f() {
    // Variable declaration before import
    let x = 0;
    use std::io::Read;
    ...
}
```

The solution is to declare the imports at the top of the block, function, or
file.

Here is the previous example again, with the correct order:

```
fn f() {
    use std::io::Read;
    let x = 0;
    ...
}
```

See the Declaration Statements section of the reference for more information
about what constitutes an Item declaration and what does not:

http://doc.rust-lang.org/reference.html#statements
"##,

E0251: r##"
Two items of the same name cannot be imported without rebinding one of the
items under a new local name.

An example of this error:

```
use foo::baz;
use bar::*; // error, do `use foo::baz as quux` instead on the previous line

fn main() {}

mod foo {
    pub struct baz;
}

mod bar {
    pub mod baz {}
}
```
"##,

E0252: r##"
Two items of the same name cannot be imported without rebinding one of the
items under a new local name.

An example of this error:

```
use foo::baz;
use bar::baz; // error, do `use bar::baz as quux` instead

fn main() {}

mod foo {
    pub struct baz;
}

mod bar {
    pub mod baz {}
}
```
"##,

E0253: r##"
Attempt was made to import an unimportable value. This can happen when
trying to import a method from a trait. An example of this error:

```
mod foo {
    pub trait MyTrait {
        fn do_something();
    }
}
use foo::MyTrait::do_something;
```

It's invalid to directly import methods belonging to a trait or concrete type.
"##,

E0255: r##"
You can't import a value whose name is the same as another value defined in the
module.

An example of this error:

```
use bar::foo; // error, do `use bar::foo as baz` instead

fn foo() {}

mod bar {
     pub fn foo() {}
}

fn main() {}
```
"##,

E0256: r##"
You can't import a type or module when the name of the item being imported is
the same as another type or submodule defined in the module.

An example of this error:

```
use foo::Bar; // error

type Bar = u32;

mod foo {
    pub mod Bar { }
}

fn main() {}
```
"##,

E0259: r##"
The name chosen for an external crate conflicts with another external crate that
has been imported into the current module.

Wrong example:

```
extern crate a;
extern crate crate_a as a;
```

The solution is to choose a different name that doesn't conflict with any
external crate imported into the current module.

Correct example:

```
extern crate a;
extern crate crate_a as other_name;
```
"##,

E0260: r##"
The name for an item declaration conflicts with an external crate's name.

For instance,

```
extern crate abc;

struct abc;
```

There are two possible solutions:

Solution #1: Rename the item.

```
extern crate abc;

struct xyz;
```

Solution #2: Import the crate with a different name.

```
extern crate abc as xyz;

struct abc;
```

See the Declaration Statements section of the reference for more information
about what constitutes an Item declaration and what does not:

http://doc.rust-lang.org/reference.html#statements
"##,

E0317: r##"
User-defined types or type parameters cannot shadow the primitive types.
This error indicates you tried to define a type, struct or enum with the same
name as an existing primitive type.

See the Types section of the reference for more information about the primitive
types:

http://doc.rust-lang.org/reference.html#types
"##,

E0364: r##"
Private items cannot be publicly re-exported.  This error indicates that
you attempted to `pub use` a type or value that was not itself public.

Here is an example that demonstrates the error:

```
mod foo {
    const X: u32 = 1;
}
pub use foo::X;
```

The solution to this problem is to ensure that the items that you are
re-exporting are themselves marked with `pub`:

```
mod foo {
    pub const X: u32 = 1;
}
pub use foo::X;
```

See the 'Use Declarations' section of the reference for more information
on this topic:

http://doc.rust-lang.org/reference.html#use-declarations
"##,

E0365: r##"
Private modules cannot be publicly re-exported.  This error indicates
that you attempted to `pub use` a module that was not itself public.

Here is an example that demonstrates the error:

```
mod foo {
    pub const X: u32 = 1;
}
pub use foo as foo2;

```
The solution to this problem is to ensure that the module that you are
re-exporting is itself marked with `pub`:

```
pub mod foo {
    pub const X: u32 = 1;
}
pub use foo as foo2;
```

See the 'Use Declarations' section of the reference for more information
on this topic:

http://doc.rust-lang.org/reference.html#use-declarations
"##,

E0403: r##"
Some type parameters have the same name. Example of erroneous code:

```
fn foo<T, T>(s: T, u: T) {} // error: the name `T` is already used for a type
                            //        parameter in this type parameter list
```

Please verify that none of the type parameterss are misspelled, and rename any
clashing parameters. Example:

```
fn foo<T, Y>(s: T, u: Y) {} // ok!
```
"##,

E0404: r##"
You tried to implement something which was not a trait on an object. Example of
erroneous code:

```
struct Foo;
struct Bar;

impl Foo for Bar {} // error: `Foo` is not a trait
```

Please verify that you didn't misspell the trait's name or otherwise use the
wrong identifier. Example:

```
trait Foo {
    // some functions
}
struct Bar;

impl Foo for Bar { // ok!
    // functions implementation
}
```
"##,

E0405: r##"
An unknown trait was implemented. Example of erroneous code:

```
struct Foo;

impl SomeTrait for Foo {} // error: use of undeclared trait name `SomeTrait`
```

Please verify that the name of the trait wasn't misspelled and ensure that it
was imported. Example:

```
// solution 1:
use some_file::SomeTrait;

// solution 2:
trait SomeTrait {
    // some functions
}

struct Foo;

impl SomeTrait for Foo { // ok!
    // implements functions
}
```
"##,

E0407: r##"
A definition of a method not in the implemented trait was given in a trait
implementation. Example of erroneous code:

```
trait Foo {
    fn a();
}

struct Bar;

impl Foo for Bar {
    fn a() {}
    fn b() {} // error: method `b` is not a member of trait `Foo`
}
```

Please verify you didn't misspell the method name and you used the correct
trait. First example:

```
trait Foo {
    fn a();
    fn b();
}

struct Bar;

impl Foo for Bar {
    fn a() {}
    fn b() {} // ok!
}
```

Second example:

```
trait Foo {
    fn a();
}

struct Bar;

impl Foo for Bar {
    fn a() {}
}

impl Bar {
    fn b() {}
}
```
"##,

E0417: r##"
A static variable was referenced in a pattern. Example of erroneous code:

```
static FOO : i32 = 0;

match 0 {
    FOO => {} // error: static variables cannot be referenced in a
              //        pattern, use a `const` instead
    _ => {}
}
```

The compiler needs to know the value of the pattern at compile time;
compile-time patterns can defined via const or enum items. Please verify
that the identifier is spelled correctly, and if so, use a const instead
of static to define it. Example:

```
const FOO : i32 = 0;

match 0 {
    FOO => {} // ok!
    _ => {}
}
```
"##,

E0424: r##"
The `self` keyword was used in a static method. Example of erroneous code:

```
struct Foo;

impl Foo {
    fn bar(self) {}

    fn foo() {
        self.bar(); // error: `self` is not available in a static method.
    }
}
```

Please check if the method's argument list should have contained `self`,
`&self`, or `&mut self` (in case you didn't want to create a static
method), and add it if so. Example:

```
struct Foo;

impl Foo {
    fn bar(self) {}

    fn foo(self) {
        self.bar(); // ok!
    }
}
```
"##,

E0425: r##"
An unresolved name was used. Example of erroneous codes:

```
something_that_doesnt_exist::foo;
// error: unresolved name `something_that_doesnt_exist::foo`

// or:
trait Foo {
    fn bar() {
        Self; // error: unresolved name `Self`
    }
}
```

Please verify you didn't misspell the name or that you're not using an
invalid object. Example:

```
enum something_that_does_exist {
    foo
}
// or:
mod something_that_does_exist {
    pub static foo : i32 = 0i32;
}

something_that_does_exist::foo; // ok!
```
"##,

E0426: r##"
An undeclared label was used. Example of erroneous code:

```
loop {
    break 'a; // error: use of undeclared label `'a`
}
```

Please verify you spelt or declare the label correctly. Example:

```
'a: loop {
    break 'a; // ok!
}
```
"##,

E0428: r##"
A type or module has been defined more than once. Example of erroneous
code:

```
struct Bar;
struct Bar; // error: duplicate definition of value `Bar`
```

Please verify you didn't misspell the type/module's name or remove/rename the
duplicated one. Example:

```
struct Bar;
struct Bar2; // ok!
```
"##,

E0430: r##"
The `self` import appears more than once in the list. Erroneous code example:

```
use something::{self, self}; // error: `self` import can only appear once in
                             //        the list
```

Please verify you didn't misspell the import name or remove the duplicated
`self` import. Example:

```
use something::self; // ok!
```
"##,

E0431: r##"
`self` import was made. Erroneous code example:

```
use {self}; // error: `self` import can only appear in an import list with a
            //        non-empty prefix
```

You cannot import the current module into itself, please remove this import
or verify you didn't misspell it.
"##,

E0432: r##"
An import was unresolved. Erroneous code example:

```
use something::Foo; // error: unresolved import `something::Foo`.
```

Please verify you didn't misspell the import name or the import does exist
in the module from where you tried to import it. Example:

```
use something::Foo; // ok!

mod something {
    pub struct Foo;
}
```
"##,

E0433: r##"
Invalid import. Example of erroneous code:

```
use something_which_doesnt_exist;
// error: unresolved import `something_which_doesnt_exist`
```

Please verify you didn't misspell the import's name.
"##,

E0437: r##"
Trait implementations can only implement associated types that are members of
the trait in question. This error indicates that you attempted to implement
an associated type whose name does not match the name of any associated type
in the trait.

Here is an example that demonstrates the error:

```
trait Foo {}

impl Foo for i32 {
    type Bar = bool;
}
```

The solution to this problem is to remove the extraneous associated type:

```
trait Foo {}

impl Foo for i32 {}
```
"##,

E0438: r##"
Trait implementations can only implement associated constants that are
members of the trait in question. This error indicates that you
attempted to implement an associated constant whose name does not
match the name of any associated constant in the trait.

Here is an example that demonstrates the error:

```
#![feature(associated_consts)]

trait Foo {}

impl Foo for i32 {
    const BAR: bool = true;
}
```

The solution to this problem is to remove the extraneous associated constant:

```
trait Foo {}

impl Foo for i32 {}
```
"##

}

register_diagnostics! {
    E0153, // called no where
    E0157, // called from no where
    E0254, // import conflicts with imported crate in this module
    E0257,
    E0258,
    E0401, // can't use type parameters from outer function
    E0402, // cannot use an outer type parameter in this context
    E0406, // undeclared associated type
    E0408, // variable from pattern #1 is not bound in pattern #
    E0409, // variable is bound with different mode in pattern # than in
           // pattern #1
    E0410, // variable from pattern is not bound in pattern 1
    E0411, // use of `Self` outside of an impl or trait
    E0412, // use of undeclared
    E0413, // declaration of shadows an enum variant or unit-like struct in
           // scope
    E0414, // only irrefutable patterns allowed here
    E0415, // identifier is bound more than once in this parameter list
    E0416, // identifier is bound more than once in the same pattern
    E0418, // is not an enum variant, struct or const
    E0419, // unresolved enum variant, struct or const
    E0420, // is not an associated const
    E0421, // unresolved associated const
    E0422, // does not name a structure
    E0423, // is a struct variant name, but this expression uses it like a
           // function name
    E0427, // cannot use `ref` binding mode with ...
    E0429, // `self` imports are only allowed within a { } list
    E0434, // can't capture dynamic environment in a fn item
    E0435, // attempt to use a non-constant value in a constant
}
