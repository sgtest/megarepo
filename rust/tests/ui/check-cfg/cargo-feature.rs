// This test checks that when no features are passed by Cargo we
// suggest adding some in the Cargo.toml instead of vomitting a
// list of all the expected names
//
// check-pass
// revisions: some none
// rustc-env:CARGO=/usr/bin/cargo
// compile-flags: --check-cfg=cfg() -Z unstable-options
// [some]compile-flags: --check-cfg=cfg(feature,values("bitcode"))
// [some]compile-flags: --check-cfg=cfg(CONFIG_NVME,values("y"))
// [none]error-pattern:Cargo.toml

#[cfg(feature = "serde")]
//[none]~^ WARNING unexpected `cfg` condition name
//[some]~^^ WARNING unexpected `cfg` condition value
fn ser() {}

#[cfg(tokio_unstable)]
//~^ WARNING unexpected `cfg` condition name
fn tokio() {}

#[cfg(CONFIG_NVME = "m")]
//[none]~^ WARNING unexpected `cfg` condition name
//[some]~^^ WARNING unexpected `cfg` condition value
fn tokio() {}

fn main() {}
