//! This tries to prove the APIT's bounds in a canonical query,
//! which doesn't know anything about the defining scope of either
//! opaque type and thus makes a random choice as to which opaque type
//! becomes the hidden type of the other. When we leave the canonical
//! query, we attempt to actually check the defining anchor, but now we
//! have a situation where the RPIT gets constrained outside its anchor.

// revisions: current next
//[next] compile-flags: -Ztrait-solver=next
// check-pass

#![feature(type_alias_impl_trait)]

type Opaque = impl Sized;

fn get_rpit() -> impl Clone {}

fn query(_: impl FnOnce() -> Opaque) {}

fn test() -> Opaque {
    query(get_rpit);
    get_rpit()
}

fn main() {}
