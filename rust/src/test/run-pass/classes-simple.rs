// xfail-test
class cat {
  priv {
    let mutable meows : uint;
  }

  let how_hungry : int;

  new(in_x : uint, in_y : int) { meows = in_x; how_hungry = in_y; }
}

fn main() {
  let nyan : cat = cat(52u, 99);
  let kitty = cat(1000u, 2);
  log(debug, nyan.how_hungry);
  log(debug, kitty.how_hungry);
}