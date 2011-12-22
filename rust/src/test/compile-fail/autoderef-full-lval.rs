// error-pattern: binary operation + cannot be applied to type
type clam = {x: @int, y: @int};

type fish = {a: @int};

fn main() {
    let a: clam = {x: @1, y: @2};
    let b: clam = {x: @10, y: @20};
    let z: int = a.x + b.y;
    log_full(core::debug, z);
    assert (z == 21);
    let forty: fish = {a: @40};
    let two: fish = {a: @2};
    let answer: int = forty.a + two.a;
    log_full(core::debug, answer);
    assert (answer == 42);
}
