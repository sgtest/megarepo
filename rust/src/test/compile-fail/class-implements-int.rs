struct cat : int { //~ ERROR trait
  let meows: uint;
  new(in_x : uint) { self.meows = in_x; }
}

fn main() {
  let nyan = cat(0u);
}
