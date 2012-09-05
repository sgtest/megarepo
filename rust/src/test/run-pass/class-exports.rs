/* Test that exporting a class also exports its
   public fields and methods */

use kitty::*;

mod kitty {
  export cat;
  struct cat {
    let meows: uint;
    let name: ~str;

    fn get_name() -> ~str {  self.name }
    new(in_name: ~str) { self.name = in_name; self.meows = 0u; }
  }
}

fn main() {
  assert(cat(~"Spreckles").get_name() == ~"Spreckles");
}