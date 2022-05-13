use super::eval_queries::{mk_eval_cx, op_to_const};
use super::machine::CompileTimeEvalContext;
use crate::interpret::{
    intern_const_alloc_recursive, ConstValue, ImmTy, Immediate, InternKind, MemPlaceMeta,
    MemoryKind, PlaceTy, Scalar, ScalarMaybeUninit,
};
use rustc_middle::mir::interpret::ConstAlloc;
use rustc_middle::ty::{self, ScalarInt, Ty, TyCtxt};
use rustc_span::source_map::DUMMY_SP;
use rustc_target::abi::{Align, VariantIdx};

use crate::interpret::MPlaceTy;
use crate::interpret::Value;

/// Convert an evaluated constant to a type level constant
#[instrument(skip(tcx), level = "debug")]
pub(crate) fn const_to_valtree<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    raw: ConstAlloc<'tcx>,
) -> Option<ty::ValTree<'tcx>> {
    let ecx = mk_eval_cx(
        tcx, DUMMY_SP, param_env,
        // It is absolutely crucial for soundness that
        // we do not read from static items or other mutable memory.
        false,
    );
    let place = ecx.raw_const_to_mplace(raw).unwrap();
    const_to_valtree_inner(&ecx, &place)
}

#[instrument(skip(ecx), level = "debug")]
fn branches<'tcx>(
    ecx: &CompileTimeEvalContext<'tcx, 'tcx>,
    place: &MPlaceTy<'tcx>,
    n: usize,
    variant: Option<VariantIdx>,
) -> Option<ty::ValTree<'tcx>> {
    let place = match variant {
        Some(variant) => ecx.mplace_downcast(&place, variant).unwrap(),
        None => *place,
    };
    let variant = variant.map(|variant| Some(ty::ValTree::Leaf(ScalarInt::from(variant.as_u32()))));
    debug!(?place, ?variant);

    let fields = (0..n).map(|i| {
        let field = ecx.mplace_field(&place, i).unwrap();
        const_to_valtree_inner(ecx, &field)
    });
    // For enums, we preped their variant index before the variant's fields so we can figure out
    // the variant again when just seeing a valtree.
    let branches = variant.into_iter().chain(fields);
    Some(ty::ValTree::Branch(ecx.tcx.arena.alloc_from_iter(branches.collect::<Option<Vec<_>>>()?)))
}

#[instrument(skip(ecx), level = "debug")]
fn slice_branches<'tcx>(
    ecx: &CompileTimeEvalContext<'tcx, 'tcx>,
    place: &MPlaceTy<'tcx>,
) -> Option<ty::ValTree<'tcx>> {
    let n = place
        .len(&ecx.tcx.tcx)
        .unwrap_or_else(|_| panic!("expected to use len of place {:?}", place));
    let branches = (0..n).map(|i| {
        let place_elem = ecx.mplace_index(place, i).unwrap();
        const_to_valtree_inner(ecx, &place_elem)
    });

    Some(ty::ValTree::Branch(ecx.tcx.arena.alloc_from_iter(branches.collect::<Option<Vec<_>>>()?)))
}

#[instrument(skip(ecx), level = "debug")]
fn const_to_valtree_inner<'tcx>(
    ecx: &CompileTimeEvalContext<'tcx, 'tcx>,
    place: &MPlaceTy<'tcx>,
) -> Option<ty::ValTree<'tcx>> {
    match place.layout.ty.kind() {
        ty::FnDef(..) => Some(ty::ValTree::zst()),
        ty::Bool | ty::Int(_) | ty::Uint(_) | ty::Float(_) | ty::Char => {
            let val = ecx.read_immediate(&place.into()).unwrap();
            let val = val.to_scalar().unwrap();
            Some(ty::ValTree::Leaf(val.assert_int()))
        }

        // Raw pointers are not allowed in type level constants, as we cannot properly test them for
        // equality at compile-time (see `ptr_guaranteed_eq`/`_ne`).
        // Technically we could allow function pointers (represented as `ty::Instance`), but this is not guaranteed to
        // agree with runtime equality tests.
        ty::FnPtr(_) | ty::RawPtr(_) => None,

        ty::Ref(_, _, _)  => {
            let derefd_place = ecx.deref_operand(&place.into()).unwrap_or_else(|e| bug!("couldn't deref {:?}, error: {:?}", place, e));
            debug!(?derefd_place);

            const_to_valtree_inner(ecx, &derefd_place)
        }

        ty::Str | ty::Slice(_) | ty::Array(_, _) => {
            let valtree = slice_branches(ecx, place);
            debug!(?valtree);

            valtree
        }
        // Trait objects are not allowed in type level constants, as we have no concept for
        // resolving their backing type, even if we can do that at const eval time. We may
        // hypothetically be able to allow `dyn StructuralEq` trait objects in the future,
        // but it is unclear if this is useful.
        ty::Dynamic(..) => None,

        ty::Tuple(substs) => branches(ecx, place, substs.len(), None),

        ty::Adt(def, _) => {
            if def.is_union() {
                return None
            } else if def.variants().is_empty() {
                bug!("uninhabited types should have errored and never gotten converted to valtree")
            }

            let variant = ecx.read_discriminant(&place.into()).unwrap().1;

            branches(ecx, place, def.variant(variant).fields.len(), def.is_enum().then_some(variant))
        }

        ty::Never
        | ty::Error(_)
        | ty::Foreign(..)
        | ty::Infer(ty::FreshIntTy(_))
        | ty::Infer(ty::FreshFloatTy(_))
        | ty::Projection(..)
        | ty::Param(_)
        | ty::Bound(..)
        | ty::Placeholder(..)
        // FIXME(oli-obk): we could look behind opaque types
        | ty::Opaque(..)
        | ty::Infer(_)
        // FIXME(oli-obk): we can probably encode closures just like structs
        | ty::Closure(..)
        | ty::Generator(..)
        | ty::GeneratorWitness(..) => None,
    }
}

#[instrument(skip(ecx), level = "debug")]
fn create_mplace_from_layout<'tcx>(
    ecx: &mut CompileTimeEvalContext<'tcx, 'tcx>,
    ty: Ty<'tcx>,
) -> MPlaceTy<'tcx> {
    let tcx = ecx.tcx;
    let param_env = ecx.param_env;
    let layout = tcx.layout_of(param_env.and(ty)).unwrap();
    debug!(?layout);

    ecx.allocate(layout, MemoryKind::Stack).unwrap()
}

// Walks custom DSTs and gets the type of the unsized field and the number of elements
// in the unsized field.
fn get_info_on_unsized_field<'tcx>(
    ty: Ty<'tcx>,
    valtree: ty::ValTree<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> (Ty<'tcx>, usize) {
    let mut last_valtree = valtree;
    let tail = tcx.struct_tail_with_normalize(
        ty,
        |ty| ty,
        || {
            let branches = last_valtree.unwrap_branch();
            last_valtree = branches[branches.len() - 1];
            debug!(?branches, ?last_valtree);
        },
    );
    let unsized_inner_ty = match tail.kind() {
        ty::Slice(t) => *t,
        ty::Str => tail,
        _ => bug!("expected Slice or Str"),
    };

    // Have to adjust type for ty::Str
    let unsized_inner_ty = match unsized_inner_ty.kind() {
        ty::Str => tcx.mk_ty(ty::Uint(ty::UintTy::U8)),
        _ => unsized_inner_ty,
    };

    // Get the number of elements in the unsized field
    let num_elems = last_valtree.unwrap_branch().len();

    (unsized_inner_ty, num_elems)
}

#[instrument(skip(ecx), level = "debug")]
fn create_pointee_place<'tcx>(
    ecx: &mut CompileTimeEvalContext<'tcx, 'tcx>,
    ty: Ty<'tcx>,
    valtree: ty::ValTree<'tcx>,
) -> MPlaceTy<'tcx> {
    let tcx = ecx.tcx.tcx;

    if !ty.is_sized(ecx.tcx, ty::ParamEnv::empty()) {
        // We need to create `Allocation`s for custom DSTs

        let (unsized_inner_ty, num_elems) = get_info_on_unsized_field(ty, valtree, tcx);
        let unsized_inner_ty = match unsized_inner_ty.kind() {
            ty::Str => tcx.mk_ty(ty::Uint(ty::UintTy::U8)),
            _ => unsized_inner_ty,
        };
        let unsized_inner_ty_size =
            tcx.layout_of(ty::ParamEnv::empty().and(unsized_inner_ty)).unwrap().layout.size();
        debug!(?unsized_inner_ty, ?unsized_inner_ty_size, ?num_elems);

        // for custom DSTs only the last field/element is unsized, but we need to also allocate
        // space for the other fields/elements
        let layout = tcx.layout_of(ty::ParamEnv::empty().and(ty)).unwrap();
        let size_of_sized_part = layout.layout.size();

        // Get the size of the memory behind the DST
        let dst_size = unsized_inner_ty_size.checked_mul(num_elems as u64, &tcx).unwrap();

        let ptr = ecx
            .allocate_ptr(
                size_of_sized_part.checked_add(dst_size, &tcx).unwrap(),
                Align::from_bytes(1).unwrap(),
                MemoryKind::Stack,
            )
            .unwrap();
        debug!(?ptr);

        let mut place = MPlaceTy::from_aligned_ptr(ptr.into(), layout);
        place.meta = MemPlaceMeta::Meta(Scalar::from_u64(num_elems as u64));
        debug!(?place);

        place
    } else {
        create_mplace_from_layout(ecx, ty)
    }
}

/// Converts a `ValTree` to a `ConstValue`, which is needed after mir
/// construction has finished.
// FIXME Merge `valtree_to_const_value` and `fill_place_recursively` into one function
#[instrument(skip(tcx), level = "debug")]
pub fn valtree_to_const_value<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env_ty: ty::ParamEnvAnd<'tcx, Ty<'tcx>>,
    valtree: ty::ValTree<'tcx>,
) -> ConstValue<'tcx> {
    // Basic idea: We directly construct `Scalar` values from trivial `ValTree`s
    // (those for constants with type bool, int, uint, float or char).
    // For all other types we create an `MPlace` and fill that by walking
    // the `ValTree` and using `place_projection` and `place_field` to
    // create inner `MPlace`s which are filled recursively.
    // FIXME Does this need an example?

    let (param_env, ty) = param_env_ty.into_parts();
    let mut ecx = mk_eval_cx(tcx, DUMMY_SP, param_env, false);

    match ty.kind() {
        ty::FnDef(..) => {
            assert!(valtree.unwrap_branch().is_empty());
            ConstValue::Scalar(Scalar::ZST)
        }
        ty::Bool | ty::Int(_) | ty::Uint(_) | ty::Float(_) | ty::Char => match valtree {
            ty::ValTree::Leaf(scalar_int) => ConstValue::Scalar(Scalar::Int(scalar_int)),
            ty::ValTree::Branch(_) => bug!(
                "ValTrees for Bool, Int, Uint, Float or Char should have the form ValTree::Leaf"
            ),
        },
        ty::Ref(_, _, _) | ty::Tuple(_) | ty::Array(_, _) | ty::Adt(..) => {
            let mut place = match ty.kind() {
                ty::Ref(_, inner_ty, _) => {
                    // Need to create a place for the pointee to fill for Refs
                    create_pointee_place(&mut ecx, *inner_ty, valtree)
                }
                _ => create_mplace_from_layout(&mut ecx, ty),
            };
            debug!(?place);

            fill_place_recursively(&mut ecx, &mut place, valtree);
            dump_place(&ecx, place.into());
            intern_const_alloc_recursive(&mut ecx, InternKind::Constant, &place).unwrap();

            let const_val = match ty.kind() {
                ty::Ref(_, _, _) => {
                    let ref_place = place.to_ref(&tcx);
                    let imm =
                        ImmTy::from_immediate(ref_place, tcx.layout_of(param_env_ty).unwrap());

                    op_to_const(&ecx, &imm.into())
                }
                _ => op_to_const(&ecx, &place.into()),
            };
            debug!(?const_val);

            const_val
        }
        ty::Never
        | ty::Error(_)
        | ty::Foreign(..)
        | ty::Infer(ty::FreshIntTy(_))
        | ty::Infer(ty::FreshFloatTy(_))
        | ty::Projection(..)
        | ty::Param(_)
        | ty::Bound(..)
        | ty::Placeholder(..)
        | ty::Opaque(..)
        | ty::Infer(_)
        | ty::Closure(..)
        | ty::Generator(..)
        | ty::GeneratorWitness(..)
        | ty::FnPtr(_)
        | ty::RawPtr(_)
        | ty::Str
        | ty::Slice(_)
        | ty::Dynamic(..) => bug!("no ValTree should have been created for type {:?}", ty.kind()),
    }
}

// FIXME Needs a better/correct name
#[instrument(skip(ecx), level = "debug")]
fn fill_place_recursively<'tcx>(
    ecx: &mut CompileTimeEvalContext<'tcx, 'tcx>,
    place: &mut MPlaceTy<'tcx>,
    valtree: ty::ValTree<'tcx>,
) {
    // This will match on valtree and write the value(s) corresponding to the ValTree
    // inside the place recursively.

    let tcx = ecx.tcx.tcx;
    let ty = place.layout.ty;

    match ty.kind() {
        ty::FnDef(_, _) => {
            ecx.write_immediate(
                Immediate::Scalar(ScalarMaybeUninit::Scalar(Scalar::ZST)),
                &(*place).into(),
            )
            .unwrap();
        }
        ty::Bool | ty::Int(_) | ty::Uint(_) | ty::Float(_) | ty::Char => {
            let scalar_int = valtree.unwrap_leaf();
            debug!("writing trivial valtree {:?} to place {:?}", scalar_int, place);
            ecx.write_immediate(
                Immediate::Scalar(ScalarMaybeUninit::Scalar(scalar_int.into())),
                &(*place).into(),
            )
            .unwrap();
        }
        ty::Ref(_, inner_ty, _) => {
            let mut pointee_place = create_pointee_place(ecx, *inner_ty, valtree);
            debug!(?pointee_place);

            fill_place_recursively(ecx, &mut pointee_place, valtree);
            dump_place(ecx, pointee_place.into());
            intern_const_alloc_recursive(ecx, InternKind::Constant, &pointee_place).unwrap();

            let imm = match inner_ty.kind() {
                ty::Slice(_) | ty::Str => {
                    let len = valtree.unwrap_branch().len();
                    let len_scalar = ScalarMaybeUninit::Scalar(Scalar::from_u64(len as u64));

                    Immediate::ScalarPair(
                        ScalarMaybeUninit::from_maybe_pointer((*pointee_place).ptr, &tcx),
                        len_scalar,
                    )
                }
                _ => pointee_place.to_ref(&tcx),
            };
            debug!(?imm);

            ecx.write_immediate(imm, &(*place).into()).unwrap();
        }
        ty::Adt(_, _) | ty::Tuple(_) | ty::Array(_, _) | ty::Str | ty::Slice(_) => {
            let branches = valtree.unwrap_branch();

            // Need to downcast place for enums
            let (place_adjusted, branches, variant_idx) = match ty.kind() {
                ty::Adt(def, _) if def.is_enum() => {
                    // First element of valtree corresponds to variant
                    let scalar_int = branches[0].unwrap_leaf();
                    let variant_idx = VariantIdx::from_u32(scalar_int.try_to_u32().unwrap());
                    let variant = def.variant(variant_idx);
                    debug!(?variant);

                    (
                        place.project_downcast(ecx, variant_idx).unwrap(),
                        &branches[1..],
                        Some(variant_idx),
                    )
                }
                _ => (*place, branches, None),
            };
            debug!(?place_adjusted, ?branches);

            // Create the places (by indexing into `place`) for the fields and fill
            // them recursively
            for (i, inner_valtree) in branches.iter().enumerate() {
                debug!(?i, ?inner_valtree);

                let mut place_inner = match ty.kind() {
                    ty::Str | ty::Slice(_) => ecx.mplace_index(&place, i as u64).unwrap(),
                    _ if !ty.is_sized(ecx.tcx, ty::ParamEnv::empty())
                        && i == branches.len() - 1 =>
                    {
                        // Note: For custom DSTs we need to manually process the last unsized field.
                        // We created a `Pointer` for the `Allocation` of the complete sized version of
                        // the Adt in `create_pointee_place` and now we fill that `Allocation` with the
                        // values in the ValTree. For the unsized field we have to additionally add the meta
                        // data.

                        let (unsized_inner_ty, num_elems) =
                            get_info_on_unsized_field(ty, valtree, tcx);
                        debug!(?unsized_inner_ty);

                        let inner_ty = match ty.kind() {
                            ty::Adt(def, substs) => {
                                def.variant(VariantIdx::from_u32(0)).fields[i].ty(tcx, substs)
                            }
                            ty::Tuple(inner_tys) => inner_tys[i],
                            _ => bug!("unexpected unsized type {:?}", ty),
                        };

                        let inner_layout =
                            tcx.layout_of(ty::ParamEnv::empty().and(inner_ty)).unwrap();
                        debug!(?inner_layout);

                        let offset = place_adjusted.layout.fields.offset(i);
                        place
                            .offset(
                                offset,
                                MemPlaceMeta::Meta(Scalar::from_u64(num_elems as u64)),
                                inner_layout,
                                &tcx,
                            )
                            .unwrap()
                    }
                    _ => ecx.mplace_field(&place_adjusted, i).unwrap(),
                };

                debug!(?place_inner);
                fill_place_recursively(ecx, &mut place_inner, *inner_valtree);
                dump_place(&ecx, place_inner.into());
            }

            debug!("dump of place_adjusted:");
            dump_place(ecx, place_adjusted.into());

            if let Some(variant_idx) = variant_idx {
                // don't forget filling the place with the discriminant of the enum
                ecx.write_discriminant(variant_idx, &(*place).into()).unwrap();
            }

            debug!("dump of place after writing discriminant:");
            dump_place(ecx, (*place).into());
        }
        _ => bug!("shouldn't have created a ValTree for {:?}", ty),
    }
}

fn dump_place<'tcx>(ecx: &CompileTimeEvalContext<'tcx, 'tcx>, place: PlaceTy<'tcx>) {
    trace!("{:?}", ecx.dump_place(*place));
}
