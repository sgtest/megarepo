// error-pattern: unresolved name

mod foo {
    #[legacy_exports];
    export x;

    fn x() { }

    enum y { y1, }
}

fn main() { let z = foo::y1; }
