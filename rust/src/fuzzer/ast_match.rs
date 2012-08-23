use std;
import vec;

fn vec_equal<T>(v: ~[T], u: ~[T],
                element_equality_test: fn@(&&T, &&T) -> bool) ->
   bool {
    let Lv = vec::len(v);
    if Lv != vec::len(u) { return false; }
    let i = 0u;
    while i < Lv {
        if !element_equality_test(v[i], u[i]) { return false; }
        i += 1u;
    }
    return true;
}

pure fn builtin_equal<T>(&&a: T, &&b: T) -> bool { return a == b; }
pure fn builtin_equal_int(&&a: int, &&b: int) -> bool { return a == b; }

fn main() {
    assert (builtin_equal(5, 5));
    assert (!builtin_equal(5, 4));
    assert (!vec_equal(~[5, 5], ~[5], bind builtin_equal(_, _)));
    assert (!vec_equal(~[5, 5], ~[5], builtin_equal_int));
    assert (!vec_equal(~[5, 5], ~[5, 4], builtin_equal_int));
    assert (!vec_equal(~[5, 5], ~[4, 5], builtin_equal_int));
    assert (vec_equal(~[5, 5], ~[5, 5], builtin_equal_int));

    error!("Pass");
}
