// compile-flags: --document-private-items

#![feature(crate_visibility_modifier)]

#![crate_name = "foo"]

// @has 'foo/struct.FooPublic.html' '//pre' 'pub struct FooPublic'
pub struct FooPublic;
// @has 'foo/struct.FooJustCrate.html' '//pre' 'pub(crate) struct FooJustCrate'
crate struct FooJustCrate;
// @has 'foo/struct.FooPubCrate.html' '//pre' 'pub(crate) struct FooPubCrate'
pub(crate) struct FooPubCrate;
// @has 'foo/struct.FooSelf.html' '//pre' 'pub(crate) struct FooSelf'
pub(self) struct FooSelf;
// @has 'foo/struct.FooInSelf.html' '//pre' 'pub(crate) struct FooInSelf'
pub(in self) struct FooInSelf;
// @has 'foo/struct.FooPriv.html' '//pre' 'pub(crate) struct FooPriv'
struct FooPriv;

mod a {
    // @has 'foo/a/struct.FooASuper.html' '//pre' 'pub(crate) struct FooASuper'
    pub(super) struct FooASuper;
    // @has 'foo/a/struct.FooAInSuper.html' '//pre' 'pub(crate) struct FooAInSuper'
    pub(in super) struct FooAInSuper;
    // @has 'foo/a/struct.FooAInA.html' '//pre' 'struct FooAInA'
    // @!has 'foo/a/struct.FooAInA.html' '//pre' 'pub'
    pub(in a) struct FooAInA;
    // @has 'foo/a/struct.FooAPriv.html' '//pre' 'struct FooAPriv'
    // @!has 'foo/a/struct.FooAPriv.html' '//pre' 'pub'
    struct FooAPriv;

    mod b {
        // @has 'foo/a/b/struct.FooBSuper.html' '//pre' 'pub(super) struct FooBSuper'
        pub(super) struct FooBSuper;
        // @has 'foo/a/b/struct.FooBInSuperSuper.html' '//pre' 'pub(crate) struct FooBInSuperSuper'
        pub(in super::super) struct FooBInSuperSuper;
        // @has 'foo/a/b/struct.FooBInAB.html' '//pre' 'struct FooBInAB'
        // @!has 'foo/a/b/struct.FooBInAB.html' '//pre' 'pub'
        pub(in a::b) struct FooBInAB;
        // @has 'foo/a/b/struct.FooBPriv.html' '//pre' 'struct FooBPriv'
        // @!has 'foo/a/b/struct.FooBPriv.html' '//pre' 'pub'
        struct FooBPriv;
    }
}

// @has 'foo/trait.PubTrait.html' '//pre' 'pub trait PubTrait'
//
// @has 'foo/trait.PubTrait.html' '//pre' 'type Type;'
// @!has 'foo/trait.PubTrait.html' '//pre' 'pub type Type;'
//
// @has 'foo/trait.PubTrait.html' '//pre' 'const CONST: usize;'
// @!has 'foo/trait.PubTrait.html' '//pre' 'pub const CONST: usize;'
//
// @has 'foo/trait.PubTrait.html' '//pre' 'fn function();'
// @!has 'foo/trait.PubTrait.html' '//pre' 'pub fn function();'

pub trait PubTrait {
    type Type;
    const CONST: usize;
    fn function();
}

// @has 'foo/struct.FooPublic.html' '//code' 'type Type'
// @!has 'foo/struct.FooPublic.html' '//code' 'pub type Type'
//
// @has 'foo/struct.FooPublic.html' '//code' 'const CONST: usize'
// @!has 'foo/struct.FooPublic.html' '//code' 'pub const CONST: usize'
//
// @has 'foo/struct.FooPublic.html' '//code' 'fn function()'
// @!has 'foo/struct.FooPublic.html' '//code' 'pub fn function()'

impl PubTrait for FooPublic {
    type Type = usize;
    const CONST: usize = 0;
    fn function() {}
}
