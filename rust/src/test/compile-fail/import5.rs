// error-pattern:unresolved import

mod m1 {
    fn foo() { #debug("foo"); }
}

mod m2 {
    import m1::foo;
}

mod m3 {
    import m2::foo;
}

fn main() { }
