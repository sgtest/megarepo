struct Foo<'self> {
    x: &'self int
}

pub fn main() {
    let f = Foo { x: @3 };
    assert_eq!(*f.x, 3);
}
