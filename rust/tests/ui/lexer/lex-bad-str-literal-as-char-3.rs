//@ revisions: rust2015 rust2018 rust2021
//@[rust2018] edition:2018
//@[rust2021] edition:2021
fn main() {
    println!('hello world');
    //[rust2015,rust2018,rust2021]~^ ERROR unterminated character literal
}
