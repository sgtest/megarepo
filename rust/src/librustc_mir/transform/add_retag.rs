// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This pass adds validation calls (AcquireValid, ReleaseValid) where appropriate.
//! It has to be run really early, before transformations like inlining, because
//! introducing these calls *adds* UB -- so, conceptually, this pass is actually part
//! of MIR building, and only after this pass we think of the program has having the
//! normal MIR semantics.

use rustc::ty::{self, Ty, TyCtxt};
use rustc::mir::*;
use transform::{MirPass, MirSource};

pub struct AddRetag;

/// Determines whether this place is "stable": Whether, if we evaluate it again
/// after the assignment, we can be sure to obtain the same place value.
/// (Concurrent accesses by other threads are no problem as these are anyway non-atomic
/// copies.  Data races are UB.)
fn is_stable<'tcx>(
    place: &Place<'tcx>,
) -> bool {
    use rustc::mir::Place::*;

    match *place {
        // Locals and statics have stable addresses, for sure
        Local { .. } |
        Promoted { .. } |
        Static { .. } =>
            true,
        // Recurse for projections
        Projection(ref proj) => {
            match proj.elem {
                // Which place this evaluates to can change with any memory write,
                // so cannot assume this to be stable.
                ProjectionElem::Deref =>
                    false,
                // Array indices are intersting, but MIR building generates a *fresh*
                // temporary for every array access, so the index cannot be changed as
                // a side-effect.
                ProjectionElem::Index { .. } |
                // The rest is completely boring, they just offset by a constant.
                ProjectionElem::Field { .. } |
                ProjectionElem::ConstantIndex { .. } |
                ProjectionElem::Subslice { .. } |
                ProjectionElem::Downcast { .. } =>
                    is_stable(&proj.base),
            }
        }
    }
}

/// Determine whether this type may have a reference in it, recursing below compound types but
/// not below references.
fn may_have_reference<'a, 'gcx, 'tcx>(ty: Ty<'tcx>, tcx: TyCtxt<'a, 'gcx, 'tcx>) -> bool {
    match ty.sty {
        // Primitive types that are not references
        ty::Bool | ty::Char |
        ty::Float(_) | ty::Int(_) | ty::Uint(_) |
        ty::RawPtr(..) | ty::FnPtr(..) |
        ty::Str | ty::FnDef(..) | ty::Never =>
            false,
        // References
        ty::Ref(..) => true,
        ty::Adt(..) if ty.is_box() => true,
        // Compound types
        ty::Array(ty, ..) | ty::Slice(ty) =>
            may_have_reference(ty, tcx),
        ty::Tuple(tys) =>
            tys.iter().any(|ty| may_have_reference(ty, tcx)),
        ty::Adt(adt, substs) =>
            adt.variants.iter().any(|v| v.fields.iter().any(|f|
                may_have_reference(f.ty(tcx, substs), tcx)
            )),
        // Conservative fallback
        _ => true,
    }
}

impl MirPass for AddRetag {
    fn run_pass<'a, 'tcx>(&self,
                          tcx: TyCtxt<'a, 'tcx, 'tcx>,
                          _src: MirSource,
                          mir: &mut Mir<'tcx>)
    {
        if !tcx.sess.opts.debugging_opts.mir_emit_retag {
            return;
        }
        let (span, arg_count) = (mir.span, mir.arg_count);
        let (basic_blocks, local_decls) = mir.basic_blocks_and_local_decls_mut();
        let needs_retag = |place: &Place<'tcx>| {
            // FIXME: Instead of giving up for unstable places, we should introduce
            // a temporary and retag on that.
            is_stable(place) && may_have_reference(place.ty(&*local_decls, tcx).to_ty(tcx), tcx)
        };

        // PART 1
        // Retag arguments at the beginning of the start block.
        {
            let source_info = SourceInfo {
                scope: OUTERMOST_SOURCE_SCOPE,
                span: span, // FIXME: Consider using just the span covering the function
                            // argument declaration.
            };
            // Gather all arguments, skip return value.
            let places = local_decls.iter_enumerated().skip(1).take(arg_count)
                    .map(|(local, _)| Place::Local(local))
                    .filter(needs_retag)
                    .collect::<Vec<_>>();
            // Emit their retags.
            basic_blocks[START_BLOCK].statements.splice(0..0,
                places.into_iter().map(|place| Statement {
                    source_info,
                    kind: StatementKind::Retag { fn_entry: true, place },
                })
            );
        }

        // PART 2
        // Retag return values of functions.  Also escape-to-raw the argument of `drop`.
        // We collect the return destinations because we cannot mutate while iterating.
        let mut returns: Vec<(SourceInfo, Place<'tcx>, BasicBlock)> = Vec::new();
        for block_data in basic_blocks.iter_mut() {
            match block_data.terminator().kind {
                TerminatorKind::Call { ref destination, .. } => {
                    // Remember the return destination for later
                    if let Some(ref destination) = destination {
                        if needs_retag(&destination.0) {
                            returns.push((
                                block_data.terminator().source_info,
                                destination.0.clone(),
                                destination.1,
                            ));
                        }
                    }
                }
                TerminatorKind::Drop { .. } |
                TerminatorKind::DropAndReplace { .. } => {
                    // `Drop` is also a call, but it doesn't return anything so we are good.
                }
                _ => {
                    // Not a block ending in a Call -> ignore.
                }
            }
        }
        // Now we go over the returns we collected to retag the return values.
        for (source_info, dest_place, dest_block) in returns {
            basic_blocks[dest_block].statements.insert(0, Statement {
                source_info,
                kind: StatementKind::Retag { fn_entry: false, place: dest_place },
            });
        }

        // PART 3
        // Add retag after assignment.
        for block_data in basic_blocks {
            // We want to insert statements as we iterate.  To this end, we
            // iterate backwards using indices.
            for i in (0..block_data.statements.len()).rev() {
                match block_data.statements[i].kind {
                    // If we are casting *from* a reference, we may have to escape-to-raw.
                    StatementKind::Assign(_, box Rvalue::Cast(
                        CastKind::Misc,
                        ref src,
                        dest_ty,
                    )) => {
                        let src_ty = src.ty(&*local_decls, tcx);
                        if src_ty.is_region_ptr() {
                            // The only `Misc` casts on references are those creating raw pointers.
                            assert!(dest_ty.is_unsafe_ptr());
                            // Insert escape-to-raw before the cast.  We are not concerned
                            // with stability here: Our EscapeToRaw will not change the value
                            // that the cast will then use.
                            // `src` might be a "move", but we rely on this not actually moving
                            // but just doing a memcpy.  It is crucial that we do EscapeToRaw
                            // on the src because we need it with its original type.
                            let source_info = block_data.statements[i].source_info;
                            block_data.statements.insert(i, Statement {
                                source_info,
                                kind: StatementKind::EscapeToRaw(src.clone()),
                            });
                        }
                    }
                    // Assignments of reference or ptr type are the ones where we may have
                    // to update tags.  This includes `x = &[mut] ...` and hence
                    // we also retag after taking a reference!
                    StatementKind::Assign(ref place, _) if needs_retag(place) => {
                        // Insert a retag after the assignment.
                        let source_info = block_data.statements[i].source_info;
                        block_data.statements.insert(i+1, Statement {
                            source_info,
                            kind: StatementKind::Retag { fn_entry: false, place: place.clone() },
                        });
                    }
                    // Do nothing for the rest
                    _ => {},
                };
            }
        }
    }
}
