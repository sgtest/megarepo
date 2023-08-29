//@aux-build:proc_macros.rs
#![feature(if_let_guard)]
#![allow(clippy::no_effect, unused)]
#![warn(clippy::redundant_guards)]

#[macro_use]
extern crate proc_macros;

struct A(u32);

struct B {
    e: Option<A>,
}

struct C(u32, u32);

#[derive(PartialEq)]
struct FloatWrapper(f32);
fn issue11304() {
    match 0.1 {
        x if x == 0.0 => todo!(),
        _ => todo!(),
    }
    match FloatWrapper(0.1) {
        x if x == FloatWrapper(0.0) => todo!(),
        _ => todo!(),
    }
}

fn main() {
    let c = C(1, 2);
    match c {
        C(x, y) if let 1 = y => ..,
        _ => todo!(),
    };

    let x = Some(Some(1));
    match x {
        Some(x) if matches!(x, Some(1) if true) => ..,
        Some(x) if matches!(x, Some(1)) => {
            println!("a");
            ..
        },
        Some(x) if let Some(1) = x => ..,
        Some(x) if x == Some(2) => ..,
        // Don't lint, since x is used in the body
        Some(x) if let Some(1) = x => {
            x;
            ..
        }
        _ => todo!(),
    };
    let y = 1;
    match x {
        // Don't inline these, since y is not from the pat
        Some(x) if matches!(y, 1 if true) => ..,
        Some(x) if let 1 = y => ..,
        Some(x) if y == 2 => ..,
        _ => todo!(),
    };
    let a = A(1);
    match a {
        _ if a.0 == 1 => {},
        _ => todo!(),
    }
    let b = B { e: Some(A(0)) };
    match b {
        B { e } if matches!(e, Some(A(2))) => ..,
        _ => todo!(),
    };
    // Do not lint, since we cannot represent this as a pattern (at least, without a conversion)
    let v = Some(vec![1u8, 2, 3]);
    match v {
        Some(x) if x == [1] => {},
        _ => {},
    }

    external! {
        let x = Some(Some(1));
        match x {
            Some(x) if let Some(1) = x => ..,
            _ => todo!(),
        };
    }
    with_span! {
        span
        let x = Some(Some(1));
        match x {
            Some(x) if let Some(1) = x => ..,
            _ => todo!(),
        };
    }
}

enum E {
    A(&'static str),
    B(&'static str),
    C(&'static str),
}

fn i() {
    match E::A("") {
        // Do not lint
        E::A(x) | E::B(x) | E::C(x) if x == "from an or pattern" => {},
        E::A(y) if y == "not from an or pattern" => {},
        _ => {},
    };
}

fn h(v: Option<u32>) {
    match v {
        x if matches!(x, Some(0)) => ..,
        _ => ..,
    };
}

// Do not lint

fn f(s: Option<std::ffi::OsString>) {
    match s {
        Some(x) if x == "a" => {},
        _ => {},
    }
}

struct S {
    a: usize,
}

impl PartialEq for S {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for S {}

static CONST_S: S = S { a: 1 };

fn g(opt_s: Option<S>) {
    match opt_s {
        Some(x) if x == CONST_S => {},
        _ => {},
    }
}
