// Issue #521

fn f() { let x = alt true { true { 10 } false { return } }; }

fn main() { }
