//@ edition: 2024
//@ compile-flags: -Zunstable-options
#![allow(incomplete_features)]
#![feature(ref_pat_eat_one_layer_2024)]

pub fn main() {
    if let Some(&mut Some(&_)) = &Some(&Some(0)) {
        //~^ ERROR: mismatched types
    }
    if let Some(&Some(&mut _)) = &Some(&mut Some(0)) {
        //~^ ERROR: mismatched types
    }
    if let Some(&Some(x)) = &mut Some(&Some(0)) {
        let _: &mut u32 = x;
        //~^ ERROR: mismatched types
    }
    if let Some(&Some(&mut _)) = &mut Some(&Some(0)) {
        //~^ ERROR: mismatched types
    }
    if let Some(&Some(Some((&mut _)))) = &Some(Some(&mut Some(0))) {
        //~^ ERROR: mismatched types
    }
    if let Some(&mut Some(x)) = &Some(Some(0)) {
        //~^ ERROR: mismatched types
    }
    if let Some(&mut Some(x)) = &Some(Some(0)) {
        //~^ ERROR: mismatched types
    }

    let &mut _ = &&0;
    //~^ ERROR: mismatched types

    let &mut _ = &&&&&&&&&&&&&&&&&&&&&&&&&&&&0;
    //~^ ERROR: mismatched types
}
