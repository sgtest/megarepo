struct Foo {
    f: @mut int,
}

impl Drop for Foo { //~ ERROR cannot implement a destructor on a struct that is not Owned
    fn drop(&self) {
        *self.f = 10;
    }
}

fn main() { }
