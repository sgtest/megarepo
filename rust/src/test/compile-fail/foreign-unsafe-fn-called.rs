// -*- rust -*-

#[abi = "cdecl"]
extern mod test {
    #[legacy_exports];
    unsafe fn free();
}

fn main() {
    test::free();
    //~^ ERROR access to unsafe function requires unsafe function or block
}

