fn match_imm_box(v: &const @option<int>) -> int {
    match *v {
      @some(i) => {i}
      @none => {0}
    }
}

fn match_const_box(v: &const @const option<int>) -> int {
    match *v {
      @some(i) => { i } // ok because this is pure
      @none => {0}
    }
}

pure fn pure_process(_i: int) {}

fn match_const_box_and_do_pure_things(v: &const @const option<int>) {
    match *v {
      @some(i) => {
        pure_process(i)
      }
      @none => {}
    }
}

fn process(_i: int) {}

fn match_const_box_and_do_bad_things(v: &const @const option<int>) {
    match *v {
      @some(i) => { //~ ERROR illegal borrow unless pure: enum variant in aliasable, mutable location
        process(i) //~ NOTE impure due to access to impure function
      }
      @none => {}
    }
}

fn main() {
}
