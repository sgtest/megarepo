// revisions: base nll
// ignore-compare-mode-nll
//[nll] compile-flags: -Z borrowck=mir

fn foo(x: &mut Vec<&'_ u8>, y: &'_ u8) { x.push(y); }
//[base]~^ ERROR lifetime mismatch
//[nll]~^^ ERROR lifetime may not live long enough

fn main() {}
