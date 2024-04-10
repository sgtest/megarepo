//@ run-pass
//! Test information regarding intrinsics and ensure we can retrieve the fallback body if it exists.
//!
//! This tests relies on the intrinsics implementation, and requires one intrinsic with and one
//! without a body. It doesn't matter which intrinsic is called here, and feel free to update that
//! if needed.

//@ ignore-stage1
//@ ignore-cross-compile
//@ ignore-remote
//@ ignore-windows-gnu mingw has troubles with linking https://github.com/rust-lang/rust/pull/116837

#![feature(rustc_private)]

extern crate rustc_hir;
#[macro_use]
extern crate rustc_smir;
extern crate rustc_driver;
extern crate rustc_interface;
extern crate stable_mir;

use rustc_smir::rustc_internal;
use stable_mir::mir::mono::{Instance, InstanceKind};
use stable_mir::mir::visit::{Location, MirVisitor};
use stable_mir::mir::{LocalDecl, Terminator, TerminatorKind};
use stable_mir::ty::{RigidTy, TyKind};
use std::collections::HashSet;
use std::convert::TryFrom;
use std::io::Write;
use std::ops::ControlFlow;

/// This function tests that we can correctly get type information from binary operations.
fn test_intrinsics() -> ControlFlow<()> {
    // Find items in the local crate.
    let main_def = stable_mir::all_local_items()[0];
    let main_instance = Instance::try_from(main_def).unwrap();
    let main_body = main_instance.body().unwrap();
    let mut visitor = CallsVisitor { locals: main_body.locals(), calls: Default::default() };
    visitor.visit_body(&main_body);

    let calls = visitor.calls;
    assert_eq!(calls.len(), 2, "Expected 2 calls, but found: {calls:?}");
    for intrinsic in &calls {
        check_intrinsic(intrinsic)
    }

    ControlFlow::Continue(())
}

/// This check is unfortunately tight to the implementation of intrinsics.
///
/// We want to ensure that StableMIR can handle intrinsics with and without fallback body.
///
/// If by any chance this test breaks because you changed how an intrinsic is implemented, please
/// update the test to invoke a different intrinsic.
fn check_intrinsic(intrinsic: &Instance) {
    assert_eq!(intrinsic.kind, InstanceKind::Intrinsic);
    let name = intrinsic.intrinsic_name().unwrap();
    if intrinsic.has_body() {
        let Some(body) = intrinsic.body() else { unreachable!("Expected a body") };
        assert!(!body.blocks.is_empty());
        assert_eq!(&name, "likely");
    } else {
        assert!(intrinsic.body().is_none());
        assert_eq!(&name, "size_of_val");
    }
}

struct CallsVisitor<'a> {
    locals: &'a [LocalDecl],
    calls: HashSet<Instance>,
}

impl<'a> MirVisitor for CallsVisitor<'a> {
    fn visit_terminator(&mut self, term: &Terminator, _loc: Location) {
        match &term.kind {
            TerminatorKind::Call { func, .. } => {
                let TyKind::RigidTy(RigidTy::FnDef(def, args)) =
                    func.ty(self.locals).unwrap().kind()
                    else {
                        return;
                    };
                self.calls.insert(Instance::resolve(def, &args).unwrap());
            }
            _ => {}
        }
    }
}

/// This test will generate and analyze a dummy crate using the stable mir.
/// For that, it will first write the dummy crate into a file.
/// Then it will create a `StableMir` using custom arguments and then
/// it will run the compiler.
fn main() {
    let path = "binop_input.rs";
    generate_input(&path).unwrap();
    let args = vec!["rustc".to_string(), "--crate-type=lib".to_string(), path.to_string()];
    run!(args, test_intrinsics).unwrap();
}

fn generate_input(path: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write!(
        file,
        r#"
        #![feature(core_intrinsics)]
        use std::intrinsics::*;
        pub fn use_intrinsics(init: bool) -> bool {{
            let sz = unsafe {{ size_of_val("hi") }};
            likely(init && sz == 2)
        }}
        "#
    )?;
    Ok(())
}
