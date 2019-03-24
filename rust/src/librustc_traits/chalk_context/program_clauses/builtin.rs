use rustc::traits::{
    GoalKind,
    Clause,
    ProgramClause,
    ProgramClauseCategory,
};
use rustc::ty;
use rustc::ty::subst::{InternalSubsts, Subst};
use rustc::hir::def_id::DefId;
use crate::lowering::Lower;
use crate::generic_types;

crate fn assemble_builtin_unsize_impls<'tcx>(
    tcx: ty::TyCtxt<'_, '_, 'tcx>,
    unsize_def_id: DefId,
    source: ty::Ty<'tcx>,
    target: ty::Ty<'tcx>,
    clauses: &mut Vec<Clause<'tcx>>
) {
    match (&source.sty, &target.sty) {
        (ty::Dynamic(data_a, ..), ty::Dynamic(data_b, ..)) => {
            if data_a.principal_def_id() != data_b.principal_def_id()
                || data_b.auto_traits().any(|b| data_a.auto_traits().all(|a| a != b))
            {
                return;
            }

            // FIXME: rules for trait upcast
        }

        (_, &ty::Dynamic(..)) => {
            // FIXME: basically, we should have something like:
            // ```
            // forall<T> {
            //     Implemented(T: Unsize< for<...> dyn Trait<...> >) :-
            //         for<...> Implemented(T: Trait<...>).
            // }
            // ```
            // The question is: how to correctly handle the higher-ranked
            // `for<...>` binder in order to have a generic rule?
            // (Having generic rules is useful for caching, as we may be able
            // to turn this function and others into tcx queries later on).
        }

        (ty::Array(_, length), ty::Slice(_)) => {
            let ty_param = generic_types::bound(tcx, 0);
            let array_ty = tcx.mk_ty(ty::Array(ty_param, length));
            let slice_ty = tcx.mk_ty(ty::Slice(ty_param));

            // `forall<T> { Implemented([T; N]: Unsize<[T]>). }`
            let clause = ProgramClause {
                goal: ty::TraitPredicate {
                    trait_ref: ty::TraitRef {
                        def_id: unsize_def_id,
                        substs: tcx.mk_substs_trait(array_ty, &[slice_ty.into()])
                    },
                }.lower(),
                hypotheses: ty::List::empty(),
                category: ProgramClauseCategory::Other,
            };

            clauses.push(Clause::ForAll(ty::Binder::bind(clause)));
        }

        (ty::Infer(ty::TyVar(_)), _) | (_, ty::Infer(ty::TyVar(_))) => {
            // FIXME: ambiguous
        }

        (ty::Adt(def_id_a, ..), ty::Adt(def_id_b, ..)) => {
            if def_id_a != def_id_b {
                return;
            }

            // FIXME: rules for struct unsizing
        }

        (&ty::Tuple(tys_a), &ty::Tuple(tys_b)) => {
            if tys_a.len() != tys_b.len() {
                return;
            }

            // FIXME: rules for tuple unsizing
        }

        _ => (),
    }
}

crate fn assemble_builtin_sized_impls<'tcx>(
    tcx: ty::TyCtxt<'_, '_, 'tcx>,
    sized_def_id: DefId,
    ty: ty::Ty<'tcx>,
    clauses: &mut Vec<Clause<'tcx>>
) {
    let mut push_builtin_impl = |ty: ty::Ty<'tcx>, nested: &[ty::Ty<'tcx>]| {
        let clause = ProgramClause {
            goal: ty::TraitPredicate {
                trait_ref: ty::TraitRef {
                    def_id: sized_def_id,
                    substs: tcx.mk_substs_trait(ty, &[]),
                },
            }.lower(),
            hypotheses: tcx.mk_goals(
                nested.iter()
                    .cloned()
                    .map(|nested_ty| ty::TraitRef {
                        def_id: sized_def_id,
                        substs: tcx.mk_substs_trait(nested_ty, &[]),
                    })
                    .map(|trait_ref| ty::TraitPredicate { trait_ref })
                    .map(|pred| GoalKind::DomainGoal(pred.lower()))
                    .map(|goal_kind| tcx.mk_goal(goal_kind))
            ),
            category: ProgramClauseCategory::Other,
        };
        // Bind innermost bound vars that may exist in `ty` and `nested`.
        clauses.push(Clause::ForAll(ty::Binder::bind(clause)));
    };

    match &ty.sty {
        // Non parametric primitive types.
        ty::Bool |
        ty::Char |
        ty::Int(..) |
        ty::Uint(..) |
        ty::Float(..) |
        ty::Error |
        ty::Never => push_builtin_impl(ty, &[]),

        // These ones are always `Sized`.
        &ty::Array(_, length) => {
            push_builtin_impl(tcx.mk_ty(ty::Array(generic_types::bound(tcx, 0), length)), &[]);
        }
        ty::RawPtr(ptr) => {
            push_builtin_impl(generic_types::raw_ptr(tcx, ptr.mutbl), &[]);
        }
        &ty::Ref(_, _, mutbl) => {
            push_builtin_impl(generic_types::ref_ty(tcx, mutbl), &[]);
        }
        ty::FnPtr(fn_ptr) => {
            let fn_ptr = fn_ptr.skip_binder();
            let fn_ptr = generic_types::fn_ptr(
                tcx,
                fn_ptr.inputs_and_output.len(),
                fn_ptr.c_variadic,
                fn_ptr.unsafety,
                fn_ptr.abi
            );
            push_builtin_impl(fn_ptr, &[]);
        }
        &ty::FnDef(def_id, ..) => {
            push_builtin_impl(generic_types::fn_def(tcx, def_id), &[]);
        }
        &ty::Closure(def_id, ..) => {
            push_builtin_impl(generic_types::closure(tcx, def_id), &[]);
        }
        &ty::Generator(def_id, ..) => {
            push_builtin_impl(generic_types::generator(tcx, def_id), &[]);
        }

        // `Sized` if the last type is `Sized` (because else we will get a WF error anyway).
        &ty::Tuple(type_list) => {
            let type_list = generic_types::type_list(tcx, type_list.len());
            push_builtin_impl(tcx.mk_ty(ty::Tuple(type_list)), &**type_list);
        }

        // Struct def
        ty::Adt(adt_def, _) => {
            let substs = InternalSubsts::bound_vars_for_item(tcx, adt_def.did);
            let adt = tcx.mk_ty(ty::Adt(adt_def, substs));
            let sized_constraint = adt_def.sized_constraint(tcx)
                .iter()
                .map(|ty| ty.subst(tcx, substs))
                .collect::<Vec<_>>();
            push_builtin_impl(adt, &sized_constraint);
        }

        // Artificially trigger an ambiguity.
        ty::Infer(..) => {
            // Everybody can find at least two types to unify against:
            // general ty vars, int vars and float vars.
            push_builtin_impl(tcx.types.i32, &[]);
            push_builtin_impl(tcx.types.u32, &[]);
            push_builtin_impl(tcx.types.f32, &[]);
            push_builtin_impl(tcx.types.f64, &[]);
        }

        ty::Projection(_projection_ty) => {
            // FIXME: add builtin impls from the associated type values found in
            // trait impls of `projection_ty.trait_ref(tcx)`.
        }

        // The `Sized` bound can only come from the environment.
        ty::Param(..) |
        ty::Placeholder(..) |
        ty::UnnormalizedProjection(..) => (),

        // Definitely not `Sized`.
        ty::Foreign(..) |
        ty::Str |
        ty::Slice(..) |
        ty::Dynamic(..) |
        ty::Opaque(..) => (),

        ty::Bound(..) |
        ty::GeneratorWitness(..) => bug!("unexpected type {:?}", ty),
    }
}
