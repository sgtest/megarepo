// -*- rust -*-
use std;
import option;
import option::some;

// error-pattern: mismatched types

enum bar { t1((), option<~[int]>), t2, }

fn foo(t: bar) {
    match t {
      t1(_, some::<int>(x)) => {
        log(debug, x);
      }
      _ => { fail; }
    }
}

fn main() { }
