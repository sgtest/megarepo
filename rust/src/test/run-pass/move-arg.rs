fn test(-foo: int) { assert (foo == 10); }

fn main() { let x = 10; test(move x); }
