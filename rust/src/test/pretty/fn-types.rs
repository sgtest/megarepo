// pp-exact

fn from_foreign_fn(x: extern fn()) { }
fn from_stack_closure(x: fn&()) { }
fn from_box_closure(x: fn@()) { }
fn from_unique_closure(x: fn~()) { }
fn main() { }
