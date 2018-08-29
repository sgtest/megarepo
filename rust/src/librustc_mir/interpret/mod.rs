// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! An interpreter for MIR used in CTFE and by miri

mod cast;
mod eval_context;
mod place;
mod operand;
mod machine;
mod memory;
mod operator;
mod step;
mod terminator;
mod traits;
mod validity;
mod intrinsics;

pub use self::eval_context::{
    EvalContext, Frame, StackPopCleanup, LocalValue,
};

pub use self::place::{Place, PlaceTy, MemPlace, MPlaceTy};

pub use self::memory::{Memory, MemoryKind};

pub use self::machine::Machine;

pub use self::operand::{Value, ValTy, Operand, OpTy};

// reexports for compatibility
pub use const_eval::{
    eval_promoted,
    mk_borrowck_eval_cx,
    mk_eval_cx,
    CompileTimeEvaluator,
    const_to_allocation_provider,
    const_eval_provider,
    const_field,
    const_variant_index,
    op_to_const,
};
