// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! An iterator over the type substructure.
//! WARNING: this does not keep track of the region depth.

use ty::{self, Ty};
use std::iter::Iterator;
use std::vec::IntoIter;

pub struct TypeWalker<'tcx> {
    stack: Vec<Ty<'tcx>>,
    last_subtree: usize,
}

impl<'tcx> TypeWalker<'tcx> {
    pub fn new(ty: Ty<'tcx>) -> TypeWalker<'tcx> {
        TypeWalker { stack: vec!(ty), last_subtree: 1, }
    }

    /// Skips the subtree of types corresponding to the last type
    /// returned by `next()`.
    ///
    /// Example: Imagine you are walking `Foo<Bar<int>, usize>`.
    ///
    /// ```
    /// let mut iter: TypeWalker = ...;
    /// iter.next(); // yields Foo
    /// iter.next(); // yields Bar<int>
    /// iter.skip_current_subtree(); // skips int
    /// iter.next(); // yields usize
    /// ```
    pub fn skip_current_subtree(&mut self) {
        self.stack.truncate(self.last_subtree);
    }
}

impl<'tcx> Iterator for TypeWalker<'tcx> {
    type Item = Ty<'tcx>;

    fn next(&mut self) -> Option<Ty<'tcx>> {
        debug!("next(): stack={:?}", self.stack);
        match self.stack.pop() {
            None => {
                return None;
            }
            Some(ty) => {
                self.last_subtree = self.stack.len();
                push_subtypes(&mut self.stack, ty);
                debug!("next: stack={:?}", self.stack);
                Some(ty)
            }
        }
    }
}

pub fn walk_shallow<'tcx>(ty: Ty<'tcx>) -> IntoIter<Ty<'tcx>> {
    let mut stack = vec![];
    push_subtypes(&mut stack, ty);
    stack.into_iter()
}

fn push_subtypes<'tcx>(stack: &mut Vec<Ty<'tcx>>, parent_ty: Ty<'tcx>) {
    match parent_ty.sty {
        ty::TyBool | ty::TyChar | ty::TyInt(_) | ty::TyUint(_) | ty::TyFloat(_) |
        ty::TyStr | ty::TyInfer(_) | ty::TyParam(_) | ty::TyNever | ty::TyError => {
        }
        ty::TyBox(ty) | ty::TyArray(ty, _) | ty::TySlice(ty) => {
            stack.push(ty);
        }
        ty::TyRawPtr(ref mt) | ty::TyRef(_, ref mt) => {
            stack.push(mt.ty);
        }
        ty::TyProjection(ref data) => {
            push_reversed(stack, &data.trait_ref.substs.types);
        }
        ty::TyTrait(ref obj) => {
            push_reversed(stack, obj.principal.input_types());
            push_reversed(stack, &obj.projection_bounds.iter().map(|pred| {
                pred.0.ty
            }).collect::<Vec<_>>());
        }
        ty::TyEnum(_, ref substs) |
        ty::TyStruct(_, ref substs) |
        ty::TyAnon(_, ref substs) => {
            push_reversed(stack, &substs.types);
        }
        ty::TyClosure(_, ref substs) => {
            push_reversed(stack, &substs.func_substs.types);
            push_reversed(stack, &substs.upvar_tys);
        }
        ty::TyTuple(ref ts) => {
            push_reversed(stack, ts);
        }
        ty::TyFnDef(_, substs, ref ft) => {
            push_reversed(stack, &substs.types);
            push_sig_subtypes(stack, &ft.sig);
        }
        ty::TyFnPtr(ref ft) => {
            push_sig_subtypes(stack, &ft.sig);
        }
    }
}

fn push_sig_subtypes<'tcx>(stack: &mut Vec<Ty<'tcx>>, sig: &ty::PolyFnSig<'tcx>) {
    stack.push(sig.0.output);
    push_reversed(stack, &sig.0.inputs);
}

fn push_reversed<'tcx>(stack: &mut Vec<Ty<'tcx>>, tys: &[Ty<'tcx>]) {
    // We push slices on the stack in reverse order so as to
    // maintain a pre-order traversal. As of the time of this
    // writing, the fact that the traversal is pre-order is not
    // known to be significant to any code, but it seems like the
    // natural order one would expect (basically, the order of the
    // types as they are written).
    for &ty in tys.iter().rev() {
        stack.push(ty);
    }
}
