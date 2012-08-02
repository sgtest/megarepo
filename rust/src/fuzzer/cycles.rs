use std;
import std::rand;
import uint::range;

// random uint less than n
fn under(r : rand::rng, n : uint) -> uint {
    assert n != 0u; r.next() as uint % n
}

// random choice from a vec
fn choice<T: copy>(r : rand::rng, v : ~[const T]) -> T {
    assert vec::len(v) != 0u; v[under(r, vec::len(v))]
}

// k in n chance of being true
fn likelihood(r : rand::rng, k : uint, n : uint) -> bool { under(r, n) < k }


const iters : uint = 1000u;
const vlen  : uint = 100u;

enum maybe_pointy {
    none,
    p(@pointy)
}

type pointy = {
    mut a : maybe_pointy,
    mut b : ~maybe_pointy,
    mut c : @maybe_pointy,

    mut f : fn@()->(),
    mut g : fn~()->(),

    mut m : ~[maybe_pointy],
    mut n : ~[mut maybe_pointy],
    mut o : {x : int, y : maybe_pointy}
};
// To add: objects; traits; anything type-parameterized?

fn empty_pointy() -> @pointy {
    return @{
        mut a : none,
        mut b : ~none,
        mut c : @none,

        mut f : fn@()->(){},
        mut g : fn~()->(){},

        mut m : ~[],
        mut n : ~[mut],
        mut o : {x : 0, y : none}
    }
}

fn nopP(_x : @pointy) { }
fn nop<T>(_x: T) { }

fn test_cycles(r : rand::rng, k: uint, n: uint)
{
    let v : ~[mut @pointy] = ~[mut];

    // Create a graph with no edges
    range(0u, vlen) {|_i|
        vec::push(v, empty_pointy());
    }

    // Fill in the graph with random edges, with density k/n
    range(0u, vlen) {|i|
        if (likelihood(r, k, n)) { v[i].a = p(choice(r, v)); }
        if (likelihood(r, k, n)) { v[i].b = ~p(choice(r, v)); }
        if (likelihood(r, k, n)) { v[i].c = @p(choice(r, v)); }

        if (likelihood(r, k, n)) { v[i].f = bind nopP(choice(r, v)); }
        //if (false)               { v[i].g = bind (fn~(_x: @pointy) { })(
        // choice(r, v)); }
          // https://github.com/mozilla/rust/issues/1899

        if (likelihood(r, k, n)) { v[i].m = [p(choice(r, v))]; }
        if (likelihood(r, k, n)) { vec::push(v[i].n, mut p(choice(r, v))); }
        if (likelihood(r, k, n)) { v[i].o = {x: 0, y: p(choice(r, v))}; }
    }

    // Drop refs one at a time
    range(0u, vlen) {|i|
        v[i] = empty_pointy()
    }
}

fn main()
{
    let r = rand::rng();
    range(0u, iters) {|i|
        test_cycles(r, i, iters);
    }
}
