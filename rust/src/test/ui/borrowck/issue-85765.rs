fn main() {
    let mut test = Vec::new();
    let rofl: &Vec<Vec<i32>> = &mut test;
    //~^ HELP consider changing this to be a mutable reference
    rofl.push(Vec::new());
    //~^ ERROR cannot borrow `*rofl` as mutable, as it is behind a `&` reference
    //~| NOTE `rofl` is a `&` reference, so the data it refers to cannot be borrowed as mutable

    let mut mutvar = 42;
    let r = &mutvar;
    //~^ HELP consider changing this to be a mutable reference
    *r = 0;
    //~^ ERROR cannot assign to `*r`, which is behind a `&` reference
    //~| NOTE `r` is a `&` reference, so the data it refers to cannot be written
}
