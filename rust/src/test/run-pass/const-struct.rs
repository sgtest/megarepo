
struct foo { a: int, b: int, c: int }

impl foo : cmp::Eq {
    pure fn eq(other: &foo) -> bool {
        self.a == (*other).a && self.b == (*other).b && self.c == (*other).c
    }
    pure fn ne(other: &foo) -> bool { !self.eq(other) }
}

const x : foo = foo { a:1, b:2, c: 3 };
const y : foo = foo { b:2, c:3, a: 1 };
const z : &foo = &foo { a: 10, b: 22, c: 12 };

fn main() {
    assert x.b == 2;
    assert x == y;
    assert z.b == 22;
    io::println(fmt!("0x%x", x.b as uint));
    io::println(fmt!("0x%x", z.c as uint));
}
