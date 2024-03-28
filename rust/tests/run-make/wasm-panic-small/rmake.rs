//@ only-wasm32-wasip1
#![deny(warnings)]

extern crate run_make_support;

use run_make_support::{rustc, tmp_dir};

fn main() {
    test("a");
    test("b");
    test("c");
    test("d");
}

fn test(cfg: &str) {
    eprintln!("running cfg {cfg:?}");

    rustc().input("foo.rs").target("wasm32-wasip1").arg("-Clto").opt().cfg(cfg).run();

    let bytes = std::fs::read(&tmp_dir().join("foo.wasm")).unwrap();
    println!("{}", bytes.len());
    assert!(bytes.len() < 40_000);
}
