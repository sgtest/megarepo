// error-pattern:meh
// no-valgrind
use std;

fn main() { let str_var: istr = ~"meh"; fail #fmt["%s", str_var]; }
