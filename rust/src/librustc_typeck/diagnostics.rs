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

E0081: r##"
Enum discriminants are used to differentiate enum variants stored in memory.
This error indicates that the same value was used for two or more variants,
making them impossible to tell apart.

```
// Good.
enum Enum {
    P,
    X = 3,
    Y = 5
}

// Bad.
enum Enum {
    P = 3,
    X = 3,
    Y = 5
}
```

Note that variants without a manually specified discriminant are numbered from
top to bottom starting from 0, so clashes can occur with seemingly unrelated
variants.

```
enum Bad {
    X,
    Y = 0
}
```

Here `X` will have already been assigned the discriminant 0 by the time `Y` is
encountered, so a conflict occurs.
"##,

E0082: r##"
The default type for enum discriminants is `isize`, but it can be adjusted by
adding the `repr` attribute to the enum declaration. This error indicates that
an integer literal given as a discriminant is not a member of the discriminant
type. For example:

```
#[repr(u8)]
enum Thing {
    A = 1024,
    B = 5
}
```

Here, 1024 lies outside the valid range for `u8`, so the discriminant for `A` is
invalid. You may want to change representation types to fix this, or else change
invalid discriminant values so that they fit within the existing type.

Note also that without a representation manually defined, the compiler will
optimize by using the smallest integer type possible.
"##,

E0083: r##"
At present, it's not possible to define a custom representation for an enum with
a single variant. As a workaround you can add a `Dummy` variant.

See: https://github.com/rust-lang/rust/issues/10292
"##,

E0084: r##"
It is impossible to define an integer type to be used to represent zero-variant
enum values because there are no zero-variant enum values. There is no way to
construct an instance of the following type using only safe code:

```
enum Empty {}
```
"##

}

register_diagnostics! {
    E0023,
    E0024,
    E0025,
    E0026,
    E0027,
    E0029,
    E0030,
    E0031,
    E0033,
    E0034,
    E0035,
    E0036,
    E0038,
    E0040, // explicit use of destructor method
    E0044,
    E0045,
    E0046,
    E0049,
    E0050,
    E0053,
    E0054,
    E0055,
    E0057,
    E0059,
    E0060,
    E0061,
    E0062,
    E0063,
    E0066,
    E0067,
    E0068,
    E0069,
    E0070,
    E0071,
    E0072,
    E0073,
    E0074,
    E0075,
    E0076,
    E0077,
    E0085,
    E0086,
    E0087,
    E0088,
    E0089,
    E0090,
    E0091,
    E0092,
    E0093,
    E0094,
    E0101,
    E0102,
    E0103,
    E0104,
    E0106,
    E0107,
    E0116,
    E0117,
    E0118,
    E0119,
    E0120,
    E0121,
    E0122,
    E0123,
    E0124,
    E0127,
    E0128,
    E0129,
    E0130,
    E0131,
    E0132,
    E0141,
    E0159,
    E0163,
    E0164,
    E0166,
    E0167,
    E0168,
    E0172,
    E0173, // manual implementations of unboxed closure traits are experimental
    E0174, // explicit use of unboxed closure methods are experimental
    E0178,
    E0182,
    E0183,
    E0184,
    E0185,
    E0186,
    E0187, // can't infer the kind of the closure
    E0188, // types differ in mutability
    E0189, // can only cast a boxed pointer to a boxed object
    E0190, // can only cast a &-pointer to an &-object
    E0191, // value of the associated type must be specified
    E0192, // negative imples are allowed just for `Send` and `Sync`
    E0193, // cannot bound type where clause bounds may only be attached to types
           // involving type parameters
    E0194,
    E0195, // lifetime parameters or bounds on method do not match the trait declaration
    E0196, // cannot determine a type for this closure
    E0197, // inherent impls cannot be declared as unsafe
    E0198, // negative implementations are not unsafe
    E0199, // implementing trait is not unsafe
    E0200, // trait requires an `unsafe impl` declaration
    E0201, // duplicate method in trait impl
    E0202, // associated items are not allowed in inherent impls
    E0203, // type parameter has more than one relaxed default bound,
           // and only one is supported
    E0204, // trait `Copy` may not be implemented for this type; field
           // does not implement `Copy`
    E0205, // trait `Copy` may not be implemented for this type; variant
           // does not implement `copy`
    E0206, // trait `Copy` may not be implemented for this type; type is
           // not a structure or enumeration
    E0207, // type parameter is not constrained by the impl trait, self type, or predicate
    E0208,
    E0209, // builtin traits can only be implemented on structs or enums
    E0210, // type parameter is not constrained by any local type
    E0211,
    E0212, // cannot extract an associated type from a higher-ranked trait bound
    E0213, // associated types are not accepted in this context
    E0214, // parenthesized parameters may only be used with a trait
    E0215, // angle-bracket notation is not stable with `Fn`
    E0216, // parenthetical notation is only stable with `Fn`
    E0217, // ambiguous associated type, defined in multiple supertraits
    E0218, // no associated type defined
    E0219, // associated type defined in higher-ranked supertrait
    E0220, // associated type not found for type parameter
    E0221, // ambiguous associated type in bounds
    E0222, // variadic function must have C calling convention
    E0223, // ambiguous associated type
    E0224, // at least one non-builtin train is required for an object type
    E0225, // only the builtin traits can be used as closure or object bounds
    E0226, // only a single explicit lifetime bound is permitted
    E0227, // ambiguous lifetime bound, explicit lifetime bound required
    E0228, // explicit lifetime bound required
    E0229, // associated type bindings are not allowed here
    E0230, // there is no type parameter on trait
    E0231, // only named substitution parameters are allowed
    E0232, // this attribute must have a value
    E0233,
    E0234, // `for` loop expression has type which does not implement the `Iterator` trait
    E0235, // structure constructor specifies a structure of type but
    E0236, // no lang item for range syntax
    E0237, // no lang item for range syntax
    E0238, // parenthesized parameters may only be used with a trait
    E0239, // `next` method of `Iterator` trait has unexpected type
    E0240,
    E0241,
    E0242, // internal error looking up a definition
    E0243, // wrong number of type arguments
    E0244, // wrong number of type arguments
    E0245, // not a trait
    E0246, // illegal recursive type
    E0247, // found module name used as a type
    E0248, // found value name used as a type
    E0249, // expected constant expr for array length
    E0250, // expected constant expr for array length
    E0318, // can't create default impls for traits outside their crates
    E0319, // trait impls for defaulted traits allowed just for structs/enums
    E0320, // recursive overflow during dropck
    E0321, // extended coherence rules for defaulted traits violated
    E0322, // cannot implement Sized explicitly
    E0323, // implemented an associated const when another trait item expected
    E0324, // implemented a method when another trait item expected
    E0325, // implemented an associated type when another trait item expected
    E0326, // associated const implemented with different type from trait
    E0327, // referred to method instead of constant in match pattern
    E0366, // dropck forbid specialization to concrete type or region
    E0367, // dropck forbid specialization to predicate not in struct/enum
    E0368, // binary operation `<op>=` cannot be applied to types
    E0369, // binary operation `<op>` cannot be applied to types
    E0371, // impl Trait for Trait is illegal
    E0372  // impl Trait for Trait where Trait is not object safe
}
