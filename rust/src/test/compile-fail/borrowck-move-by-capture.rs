pub fn main() {
    let foo = ~3;
    let _pfoo = &foo;
    let _f: @fn() -> int = || *foo + 5;
    //~^ ERROR cannot move `foo`

    let bar = ~3;
    let _g = || { //~ ERROR capture of moved value
        let _h: @fn() -> int = || *bar;
    };
}
