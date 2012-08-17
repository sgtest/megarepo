#[cfg(bogus)]
const b: bool = false;

const b: bool = true;

#[cfg(bogus)]
#[abi = "cdecl"]
extern mod rustrt {
    // This symbol doesn't exist and would be a link error if this
    // module was translated
    fn bogus();
}

#[abi = "cdecl"]
extern mod rustrt { }

#[cfg(bogus)]
type t = int;

type t = bool;

#[cfg(bogus)]
enum tg { foo, }

enum tg { bar, }

#[cfg(bogus)]
struct r {
  let i: int;
  new(i:int) { self.i = i; }
}

struct r {
  let i: int;
  new(i:int) { self.i = i; }
}

#[cfg(bogus)]
mod m {
    // This needs to parse but would fail in typeck. Since it's not in
    // the current config it should not be typechecked.
    fn bogus() { return 0; }
}

mod m {

    // Submodules have slightly different code paths than the top-level
    // module, so let's make sure this jazz works here as well
    #[cfg(bogus)]
    fn f() { }

    fn f() { }
}

// Since the bogus configuration isn't defined main will just be
// parsed, but nothing further will be done with it
#[cfg(bogus)]
fn main() { fail }

fn main() {
    // Exercise some of the configured items in ways that wouldn't be possible
    // if they had the bogus definition
    assert (b);
    let x: t = true;
    let y: tg = bar;

    test_in_fn_ctxt();
}

fn test_in_fn_ctxt() {
    #[cfg(bogus)]
    fn f() { fail }
    fn f() { }
    f();

    #[cfg(bogus)]
    const i: int = 0;
    const i: int = 1;
    assert (i == 1);
}

mod test_foreign_items {
    #[abi = "cdecl"]
    extern mod rustrt {
        #[cfg(bogus)]
        fn vec_from_buf_shared();
        fn vec_from_buf_shared();
    }
}

mod test_use_statements {
    #[cfg(bogus)]
    use flippity_foo;

    extern mod rustrt {
        #[cfg(bogus)]
        use flippity_foo;
    }
}