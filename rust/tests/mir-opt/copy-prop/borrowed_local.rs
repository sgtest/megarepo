// EMIT_MIR_FOR_EACH_PANIC_STRATEGY
//@ test-mir-pass: CopyProp

#![feature(custom_mir, core_intrinsics, freeze)]
#![allow(unused_assignments)]
extern crate core;
use core::marker::Freeze;
use core::intrinsics::mir::*;

fn opaque(_: impl Sized) -> bool { true }

fn cmp_ref(a: &u8, b: &u8) -> bool {
    std::ptr::eq(a as *const u8, b as *const u8)
}

#[custom_mir(dialect = "analysis", phase = "post-cleanup")]
fn compare_address() -> bool {
    // CHECK-LABEL: fn compare_address(
    // CHECK: bb0: {
    // CHECK-NEXT: _1 = const 5_u8;
    // CHECK-NEXT: _2 = &_1;
    // CHECK-NEXT: _3 = _1;
    // CHECK-NEXT: _4 = &_3;
    // CHECK-NEXT: _0 = cmp_ref(_2, _4)
    // CHECK: bb1: {
    // CHECK-NEXT: _0 = opaque::<u8>(_3)
    mir!(
        {
            let a = 5_u8;
            let r1 = &a;
            let b = a;
            // We cannot propagate the place `a`.
            let r2 = &b;
            Call(RET = cmp_ref(r1, r2), ReturnTo(next), UnwindContinue())
        }
        next = {
            // But we can propagate the value `a`.
            Call(RET = opaque(b), ReturnTo(ret), UnwindContinue())
        }
        ret = {
            Return()
        }
    )
}

/// Generic type `T` is `Freeze`, so shared borrows are immutable.
#[custom_mir(dialect = "analysis", phase = "post-cleanup")]
fn borrowed<T: Copy + Freeze>(x: T) -> bool {
    // CHECK-LABEL: fn borrowed(
    // CHECK: bb0: {
    // CHECK-NEXT: _3 = &_1;
    // CHECK-NEXT: _0 = opaque::<&T>(_3)
    // CHECK: bb1: {
    // CHECK-NEXT: _0 = opaque::<T>(_1)
    mir!(
        {
            let a = x;
            let r1 = &x;
            Call(RET = opaque(r1), ReturnTo(next), UnwindContinue())
        }
        next = {
            Call(RET = opaque(a), ReturnTo(ret), UnwindContinue())
        }
        ret = {
            Return()
        }
    )
}

/// Generic type `T` is not known to be `Freeze`, so shared borrows may be mutable.
#[custom_mir(dialect = "analysis", phase = "post-cleanup")]
fn non_freeze<T: Copy>(x: T) -> bool {
    // CHECK-LABEL: fn non_freeze(
    // CHECK: bb0: {
    // CHECK-NEXT: _2 = _1;
    // CHECK-NEXT: _3 = &_1;
    // CHECK-NEXT: _0 = opaque::<&T>(_3)
    // CHECK: bb1: {
    // CHECK-NEXT: _0 = opaque::<T>(_2)
    mir!(
        {
            let a = x;
            let r1 = &x;
            Call(RET = opaque(r1), ReturnTo(next), UnwindContinue())
        }
        next = {
            Call(RET = opaque(a), ReturnTo(ret), UnwindContinue())
        }
        ret = {
            Return()
        }
    )
}

fn main() {
    assert!(!compare_address());
    non_freeze(5);
}

// EMIT_MIR borrowed_local.compare_address.CopyProp.diff
// EMIT_MIR borrowed_local.borrowed.CopyProp.diff
// EMIT_MIR borrowed_local.non_freeze.CopyProp.diff
