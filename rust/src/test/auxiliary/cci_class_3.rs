mod kitties {

struct cat {
  priv {
    let mut meows : uint;
  }

  let how_hungry : int;

  new(in_x : uint, in_y : int) { self.meows = in_x; self.how_hungry = in_y; }

  fn speak() { self.meows += 1u; }
  fn meow_count() -> uint { self.meows }

}

}
