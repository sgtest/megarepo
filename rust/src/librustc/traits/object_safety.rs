// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! "Object safety" refers to the ability for a trait to be converted
//! to an object. In general, traits may only be converted to an
//! object if all of their methods meet certain criteria. In particular,
//! they must:
//!
//!   - have a suitable receiver from which we can extract a vtable;
//!   - not reference the erased type `Self` except for in this receiver;
//!   - not have generic type parameters

use super::elaborate_predicates;

use hir::def_id::DefId;
use ty::subst::{self, SelfSpace, TypeSpace};
use traits;
use ty::{self, ToPolyTraitRef, Ty, TyCtxt, TypeFoldable};
use std::rc::Rc;
use syntax::ast;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ObjectSafetyViolation<'tcx> {
    /// Self : Sized declared on the trait
    SizedSelf,

    /// Supertrait reference references `Self` an in illegal location
    /// (e.g. `trait Foo : Bar<Self>`)
    SupertraitSelf,

    /// Method has something illegal
    Method(Rc<ty::Method<'tcx>>, MethodViolationCode),
}

/// Reasons a method might not be object-safe.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MethodViolationCode {
    /// e.g., `fn foo()`
    StaticMethod,

    /// e.g., `fn foo(&self, x: Self)` or `fn foo(&self) -> Self`
    ReferencesSelf,

    /// e.g., `fn foo<A>()`
    Generic,
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    pub fn is_object_safe(self, trait_def_id: DefId) -> bool {
        // Because we query yes/no results frequently, we keep a cache:
        let def = self.lookup_trait_def(trait_def_id);

        let result = def.object_safety().unwrap_or_else(|| {
            let result = self.object_safety_violations(trait_def_id).is_empty();

            // Record just a yes/no result in the cache; this is what is
            // queried most frequently. Note that this may overwrite a
            // previous result, but always with the same thing.
            def.set_object_safety(result);

            result
        });

        debug!("is_object_safe({:?}) = {}", trait_def_id, result);

        result
    }

    /// Returns the object safety violations that affect
    /// astconv - currently, Self in supertraits. This is needed
    /// because `object_safety_violations` can't be used during
    /// type collection.
    pub fn astconv_object_safety_violations(self, trait_def_id: DefId)
                                            -> Vec<ObjectSafetyViolation<'tcx>>
    {
        let mut violations = vec![];

        if self.supertraits_reference_self(trait_def_id) {
            violations.push(ObjectSafetyViolation::SupertraitSelf);
        }

        debug!("astconv_object_safety_violations(trait_def_id={:?}) = {:?}",
               trait_def_id,
               violations);

        violations
    }

    pub fn object_safety_violations(self, trait_def_id: DefId)
                                    -> Vec<ObjectSafetyViolation<'tcx>>
    {
        traits::supertrait_def_ids(self, trait_def_id)
            .flat_map(|def_id| self.object_safety_violations_for_trait(def_id))
            .collect()
    }

    fn object_safety_violations_for_trait(self, trait_def_id: DefId)
                                          -> Vec<ObjectSafetyViolation<'tcx>>
    {
        // Check methods for violations.
        let mut violations: Vec<_> =
            self.trait_items(trait_def_id).iter()
            .filter_map(|item| {
                match *item {
                    ty::MethodTraitItem(ref m) => {
                        self.object_safety_violation_for_method(trait_def_id, &m)
                            .map(|code| ObjectSafetyViolation::Method(m.clone(), code))
                    }
                    _ => None,
                }
            })
            .collect();

        // Check the trait itself.
        if self.trait_has_sized_self(trait_def_id) {
            violations.push(ObjectSafetyViolation::SizedSelf);
        }
        if self.supertraits_reference_self(trait_def_id) {
            violations.push(ObjectSafetyViolation::SupertraitSelf);
        }

        debug!("object_safety_violations_for_trait(trait_def_id={:?}) = {:?}",
               trait_def_id,
               violations);

        violations
    }

    fn supertraits_reference_self(self, trait_def_id: DefId) -> bool {
        let trait_def = self.lookup_trait_def(trait_def_id);
        let trait_ref = trait_def.trait_ref.clone();
        let trait_ref = trait_ref.to_poly_trait_ref();
        let predicates = self.lookup_super_predicates(trait_def_id);
        predicates
            .predicates
            .into_iter()
            .map(|predicate| predicate.subst_supertrait(self, &trait_ref))
            .any(|predicate| {
                match predicate {
                    ty::Predicate::Trait(ref data) => {
                        // In the case of a trait predicate, we can skip the "self" type.
                        data.0.trait_ref.substs.types.get_slice(TypeSpace)
                                                     .iter()
                                                     .cloned()
                                                     .any(|t| t.has_self_ty())
                    }
                    ty::Predicate::Projection(..) |
                    ty::Predicate::WellFormed(..) |
                    ty::Predicate::ObjectSafe(..) |
                    ty::Predicate::TypeOutlives(..) |
                    ty::Predicate::RegionOutlives(..) |
                    ty::Predicate::ClosureKind(..) |
                    ty::Predicate::Rfc1592(..) |
                    ty::Predicate::Equate(..) => {
                        false
                    }
                }
            })
    }

    fn trait_has_sized_self(self, trait_def_id: DefId) -> bool {
        let trait_def = self.lookup_trait_def(trait_def_id);
        let trait_predicates = self.lookup_predicates(trait_def_id);
        self.generics_require_sized_self(&trait_def.generics, &trait_predicates)
    }

    fn generics_require_sized_self(self,
                                   generics: &ty::Generics<'gcx>,
                                   predicates: &ty::GenericPredicates<'gcx>)
                                   -> bool
    {
        let sized_def_id = match self.lang_items.sized_trait() {
            Some(def_id) => def_id,
            None => { return false; /* No Sized trait, can't require it! */ }
        };

        // Search for a predicate like `Self : Sized` amongst the trait bounds.
        let free_substs = self.construct_free_substs(generics,
            self.region_maps.node_extent(ast::DUMMY_NODE_ID));
        let predicates = predicates.instantiate(self, &free_substs).predicates.into_vec();
        elaborate_predicates(self, predicates)
            .any(|predicate| {
                match predicate {
                    ty::Predicate::Trait(ref trait_pred) if trait_pred.def_id() == sized_def_id => {
                        trait_pred.0.self_ty().is_self()
                    }
                    ty::Predicate::Projection(..) |
                    ty::Predicate::Trait(..) |
                    ty::Predicate::Rfc1592(..) |
                    ty::Predicate::Equate(..) |
                    ty::Predicate::RegionOutlives(..) |
                    ty::Predicate::WellFormed(..) |
                    ty::Predicate::ObjectSafe(..) |
                    ty::Predicate::ClosureKind(..) |
                    ty::Predicate::TypeOutlives(..) => {
                        false
                    }
                }
            })
    }

    /// Returns `Some(_)` if this method makes the containing trait not object safe.
    fn object_safety_violation_for_method(self,
                                          trait_def_id: DefId,
                                          method: &ty::Method<'gcx>)
                                          -> Option<MethodViolationCode>
    {
        // Any method that has a `Self : Sized` requisite is otherwise
        // exempt from the regulations.
        if self.generics_require_sized_self(&method.generics, &method.predicates) {
            return None;
        }

        self.virtual_call_violation_for_method(trait_def_id, method)
    }

    /// We say a method is *vtable safe* if it can be invoked on a trait
    /// object.  Note that object-safe traits can have some
    /// non-vtable-safe methods, so long as they require `Self:Sized` or
    /// otherwise ensure that they cannot be used when `Self=Trait`.
    pub fn is_vtable_safe_method(self,
                                 trait_def_id: DefId,
                                 method: &ty::Method<'gcx>)
                                 -> bool
    {
        // Any method that has a `Self : Sized` requisite can't be called.
        if self.generics_require_sized_self(&method.generics, &method.predicates) {
            return false;
        }

        self.virtual_call_violation_for_method(trait_def_id, method).is_none()
    }

    /// Returns `Some(_)` if this method cannot be called on a trait
    /// object; this does not necessarily imply that the enclosing trait
    /// is not object safe, because the method might have a where clause
    /// `Self:Sized`.
    fn virtual_call_violation_for_method(self,
                                         trait_def_id: DefId,
                                         method: &ty::Method<'tcx>)
                                         -> Option<MethodViolationCode>
    {
        // The method's first parameter must be something that derefs (or
        // autorefs) to `&self`. For now, we only accept `self`, `&self`
        // and `Box<Self>`.
        match method.explicit_self {
            ty::ExplicitSelfCategory::Static => {
                return Some(MethodViolationCode::StaticMethod);
            }

            ty::ExplicitSelfCategory::ByValue |
            ty::ExplicitSelfCategory::ByReference(..) |
            ty::ExplicitSelfCategory::ByBox => {
            }
        }

        // The `Self` type is erased, so it should not appear in list of
        // arguments or return type apart from the receiver.
        let ref sig = method.fty.sig;
        for &input_ty in &sig.0.inputs[1..] {
            if self.contains_illegal_self_type_reference(trait_def_id, input_ty) {
                return Some(MethodViolationCode::ReferencesSelf);
            }
        }
        if self.contains_illegal_self_type_reference(trait_def_id, sig.0.output) {
            return Some(MethodViolationCode::ReferencesSelf);
        }

        // We can't monomorphize things like `fn foo<A>(...)`.
        if !method.generics.types.is_empty_in(subst::FnSpace) {
            return Some(MethodViolationCode::Generic);
        }

        None
    }

    fn contains_illegal_self_type_reference(self,
                                            trait_def_id: DefId,
                                            ty: Ty<'tcx>)
                                            -> bool
    {
        // This is somewhat subtle. In general, we want to forbid
        // references to `Self` in the argument and return types,
        // since the value of `Self` is erased. However, there is one
        // exception: it is ok to reference `Self` in order to access
        // an associated type of the current trait, since we retain
        // the value of those associated types in the object type
        // itself.
        //
        // ```rust
        // trait SuperTrait {
        //     type X;
        // }
        //
        // trait Trait : SuperTrait {
        //     type Y;
        //     fn foo(&self, x: Self) // bad
        //     fn foo(&self) -> Self // bad
        //     fn foo(&self) -> Option<Self> // bad
        //     fn foo(&self) -> Self::Y // OK, desugars to next example
        //     fn foo(&self) -> <Self as Trait>::Y // OK
        //     fn foo(&self) -> Self::X // OK, desugars to next example
        //     fn foo(&self) -> <Self as SuperTrait>::X // OK
        // }
        // ```
        //
        // However, it is not as simple as allowing `Self` in a projected
        // type, because there are illegal ways to use `Self` as well:
        //
        // ```rust
        // trait Trait : SuperTrait {
        //     ...
        //     fn foo(&self) -> <Self as SomeOtherTrait>::X;
        // }
        // ```
        //
        // Here we will not have the type of `X` recorded in the
        // object type, and we cannot resolve `Self as SomeOtherTrait`
        // without knowing what `Self` is.

        let mut supertraits: Option<Vec<ty::PolyTraitRef<'tcx>>> = None;
        let mut error = false;
        ty.maybe_walk(|ty| {
            match ty.sty {
                ty::TyParam(ref param_ty) => {
                    if param_ty.space == SelfSpace {
                        error = true;
                    }

                    false // no contained types to walk
                }

                ty::TyProjection(ref data) => {
                    // This is a projected type `<Foo as SomeTrait>::X`.

                    // Compute supertraits of current trait lazily.
                    if supertraits.is_none() {
                        let trait_def = self.lookup_trait_def(trait_def_id);
                        let trait_ref = ty::Binder(trait_def.trait_ref.clone());
                        supertraits = Some(traits::supertraits(self, trait_ref).collect());
                    }

                    // Determine whether the trait reference `Foo as
                    // SomeTrait` is in fact a supertrait of the
                    // current trait. In that case, this type is
                    // legal, because the type `X` will be specified
                    // in the object type.  Note that we can just use
                    // direct equality here because all of these types
                    // are part of the formal parameter listing, and
                    // hence there should be no inference variables.
                    let projection_trait_ref = ty::Binder(data.trait_ref.clone());
                    let is_supertrait_of_current_trait =
                        supertraits.as_ref().unwrap().contains(&projection_trait_ref);

                    if is_supertrait_of_current_trait {
                        false // do not walk contained types, do not report error, do collect $200
                    } else {
                        true // DO walk contained types, POSSIBLY reporting an error
                    }
                }

                _ => true, // walk contained types, if any
            }
        });

        error
    }
}
