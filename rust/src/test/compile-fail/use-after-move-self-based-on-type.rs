struct S {
    x: int,
}

impl Drop for S {
    fn finalize(&self) {}
}

impl S {
    fn foo(self) -> int {
        self.bar();
        return self.x;  //~ ERROR use of moved value
    }

    fn bar(self) {}
}

fn main() {
    let x = S { x: 1 };
    io::println(x.foo().to_str());
}

