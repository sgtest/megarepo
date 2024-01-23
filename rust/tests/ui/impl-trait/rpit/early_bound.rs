use std::convert::identity;

fn test<'a: 'a>(n: bool) -> impl Sized + 'a {
    //~^ ERROR concrete type differs from previous defining opaque type use
    let true = n else { loop {} };
    let _ = || {
        let _ = identity::<&'a ()>(test(false));
        //~^ ERROR hidden type for `impl Sized + 'a` captures lifetime that does not appear in bounds
    };
    loop {}
}

fn main() {}
