// error-pattern:meh
use std;

fn main() { let str_var: ~str = ~"meh"; fail fmt!("%s", str_var); }
