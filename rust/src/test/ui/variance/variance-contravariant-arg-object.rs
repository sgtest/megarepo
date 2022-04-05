#![allow(dead_code)]

// Test that even when `T` is only used in contravariant position, it
// is treated as invariant.

// revisions: base nll
// ignore-compare-mode-nll
//[nll] compile-flags: -Z borrowck=mir

trait Get<T> : 'static {
    fn get(&self, t: T);
}

fn get_min_from_max<'min, 'max>(v: Box<dyn Get<&'max i32>>)
                                -> Box<dyn Get<&'min i32>>
    where 'max : 'min
{
    v
    //[base]~^ ERROR mismatched types
    //[nll]~^^ ERROR lifetime may not live long enough
}

fn get_max_from_min<'min, 'max, G>(v: Box<dyn Get<&'min i32>>)
                                   -> Box<dyn Get<&'max i32>>
    where 'max : 'min
{
    // Previously OK:
    v
    //[base]~^ ERROR mismatched types
    //[nll]~^^ ERROR lifetime may not live long enough
}

fn main() { }
