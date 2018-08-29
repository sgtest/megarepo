// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Intrinsics and other functions that the miri engine executes without
//! looking at their MIR.  Intrinsics/functions supported here are shared by CTFE
//! and miri.

use syntax::symbol::Symbol;
use rustc::ty;
use rustc::ty::layout::{LayoutOf, Primitive};
use rustc::mir::interpret::{
    EvalResult, EvalErrorKind, Scalar,
};

use super::{
    Machine, PlaceTy, OpTy, EvalContext,
};


fn numeric_intrinsic<'tcx>(
    name: &str,
    bits: u128,
    kind: Primitive,
) -> EvalResult<'tcx, Scalar> {
    let size = match kind {
        Primitive::Int(integer, _) => integer.size(),
        _ => bug!("invalid `{}` argument: {:?}", name, bits),
    };
    let extra = 128 - size.bits() as u128;
    let bits_out = match name {
        "ctpop" => bits.count_ones() as u128,
        "ctlz" => bits.leading_zeros() as u128 - extra,
        "cttz" => (bits << extra).trailing_zeros() as u128 - extra,
        "bswap" => (bits << extra).swap_bytes(),
        _ => bug!("not a numeric intrinsic: {}", name),
    };
    Ok(Scalar::Bits { bits: bits_out, size: size.bytes() as u8 })
}

impl<'a, 'mir, 'tcx, M: Machine<'mir, 'tcx>> EvalContext<'a, 'mir, 'tcx, M> {
    /// Returns whether emulation happened.
    pub fn emulate_intrinsic(
        &mut self,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx>],
        dest: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx, bool> {
        let substs = instance.substs;

        let intrinsic_name = &self.tcx.item_name(instance.def_id()).as_str()[..];
        match intrinsic_name {
            "min_align_of" => {
                let elem_ty = substs.type_at(0);
                let elem_align = self.layout_of(elem_ty)?.align.abi();
                let align_val = Scalar::Bits {
                    bits: elem_align as u128,
                    size: dest.layout.size.bytes() as u8,
                };
                self.write_scalar(align_val, dest)?;
            }

            "size_of" => {
                let ty = substs.type_at(0);
                let size = self.layout_of(ty)?.size.bytes() as u128;
                let size_val = Scalar::Bits {
                    bits: size,
                    size: dest.layout.size.bytes() as u8,
                };
                self.write_scalar(size_val, dest)?;
            }

            "type_id" => {
                let ty = substs.type_at(0);
                let type_id = self.tcx.type_id_hash(ty) as u128;
                let id_val = Scalar::Bits {
                    bits: type_id,
                    size: dest.layout.size.bytes() as u8,
                };
                self.write_scalar(id_val, dest)?;
            }
            "ctpop" | "cttz" | "cttz_nonzero" | "ctlz" | "ctlz_nonzero" | "bswap" => {
                let ty = substs.type_at(0);
                let layout_of = self.layout_of(ty)?;
                let bits = self.read_scalar(args[0])?.to_bits(layout_of.size)?;
                let kind = match layout_of.abi {
                    ty::layout::Abi::Scalar(ref scalar) => scalar.value,
                    _ => Err(::rustc::mir::interpret::EvalErrorKind::TypeNotPrimitive(ty))?,
                };
                let out_val = if intrinsic_name.ends_with("_nonzero") {
                    if bits == 0 {
                        return err!(Intrinsic(format!("{} called on 0", intrinsic_name)));
                    }
                    numeric_intrinsic(intrinsic_name.trim_right_matches("_nonzero"), bits, kind)?
                } else {
                    numeric_intrinsic(intrinsic_name, bits, kind)?
                };
                self.write_scalar(out_val, dest)?;
            }

            _ => return Ok(false),
        }

        Ok(true)
    }

    /// "Intercept" a function call because we have something special to do for it.
    /// Returns whether an intercept happened.
    pub fn hook_fn(
        &mut self,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx>],
        dest: Option<PlaceTy<'tcx>>,
    ) -> EvalResult<'tcx, bool> {
        let def_id = instance.def_id();
        // Some fn calls are actually BinOp intrinsics
        if let Some((op, oflo)) = self.tcx.is_binop_lang_item(def_id) {
            let dest = dest.expect("128 lowerings can't diverge");
            let l = self.read_value(args[0])?;
            let r = self.read_value(args[1])?;
            if oflo {
                self.binop_with_overflow(op, l, r, dest)?;
            } else {
                self.binop_ignore_overflow(op, l, r, dest)?;
            }
            return Ok(true);
        } else if Some(def_id) == self.tcx.lang_items().panic_fn() {
            assert!(args.len() == 1);
            // &(&'static str, &'static str, u32, u32)
            let ptr = self.read_value(args[0])?;
            let place = self.ref_to_mplace(ptr)?;
            let (msg, file, line, col) = (
                self.mplace_field(place, 0)?,
                self.mplace_field(place, 1)?,
                self.mplace_field(place, 2)?,
                self.mplace_field(place, 3)?,
            );

            let msg_place = self.ref_to_mplace(self.read_value(msg.into())?)?;
            let msg = Symbol::intern(self.read_str(msg_place)?);
            let file_place = self.ref_to_mplace(self.read_value(file.into())?)?;
            let file = Symbol::intern(self.read_str(file_place)?);
            let line = self.read_scalar(line.into())?.to_u32()?;
            let col = self.read_scalar(col.into())?.to_u32()?;
            return Err(EvalErrorKind::Panic { msg, file, line, col }.into());
        } else if Some(def_id) == self.tcx.lang_items().begin_panic_fn() {
            assert!(args.len() == 2);
            // &'static str, &(&'static str, u32, u32)
            let msg = args[0];
            let ptr = self.read_value(args[1])?;
            let place = self.ref_to_mplace(ptr)?;
            let (file, line, col) = (
                self.mplace_field(place, 0)?,
                self.mplace_field(place, 1)?,
                self.mplace_field(place, 2)?,
            );

            let msg_place = self.ref_to_mplace(self.read_value(msg.into())?)?;
            let msg = Symbol::intern(self.read_str(msg_place)?);
            let file_place = self.ref_to_mplace(self.read_value(file.into())?)?;
            let file = Symbol::intern(self.read_str(file_place)?);
            let line = self.read_scalar(line.into())?.to_u32()?;
            let col = self.read_scalar(col.into())?.to_u32()?;
            return Err(EvalErrorKind::Panic { msg, file, line, col }.into());
        } else {
            return Ok(false);
        }
    }
}
