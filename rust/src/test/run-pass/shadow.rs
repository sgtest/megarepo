// -*- rust -*-
fn foo(c: [int]) {
    let a: int = 5;
    let mut b: [int] = [];


    alt none::<int> {
      some::<int>(_) {
        for i: int in c {
            log(debug, a);
            let a = 17;
            b += [a];
        }
      }
      _ { }
    }
}

enum t<T> { none, some(T), }

fn main() { let x = 10; let x = x + 20; assert (x == 30); foo([]); }
