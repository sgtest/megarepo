#![feature(generators, generator_trait)]

// Regression test for #58892, generator drop shims should not have blocks
// spuriously marked as cleanup

fn main() {
    let gen = || {
        yield;
    };
}

// END RUST SOURCE

// START rustc.main-{{closure}}.generator_drop.0.mir
// bb0: {
//     switchInt(((*_1).0: u32)) -> [0u32: bb4, 3u32: bb7, otherwise: bb8];
// }
// bb1: {
//     goto -> bb5;
// }
// bb2: {
//     return;
// }
// bb3: {
//     return;
// }
// bb4: {
//     goto -> bb6;
// }
// bb5: {
//     goto -> bb2;
// }
// bb6: {
//     goto -> bb3;
// }
// bb7: {
//     StorageLive(_3);
//     goto -> bb1;
// }
// bb8: {
//     return;
// }
// END rustc.main-{{closure}}.generator_drop.0.mir
