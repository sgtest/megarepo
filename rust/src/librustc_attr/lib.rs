//! Functions and types dealing with attributes and meta items.
//!
//! FIXME(Centril): For now being, much of the logic is still in `syntax::attr`.
//! The goal is to move the definition of `MetaItem` and things that don't need to be in `syntax`
//! to this crate.

mod builtin;

pub use builtin::*;
pub use IntType::*;
pub use ReprAttr::*;
pub use StabilityLevel::*;

pub use syntax::attr::*;

pub(crate) use syntax::HashStableContext;
