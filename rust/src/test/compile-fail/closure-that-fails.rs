fn foo(f: &fn() -> !) {}

fn main() {
    // Type inference didn't use to be able to handle this:
    foo(|| fail!());
    foo(|| 22); //~ ERROR mismatched types
}
