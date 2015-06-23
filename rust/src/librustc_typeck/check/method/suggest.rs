// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Give useful errors and suggestions to users when an item can't be
//! found or is otherwise invalid.

use CrateCtxt;

use astconv::AstConv;
use check::{self, FnCtxt};
use middle::ty::{self, Ty, ToPolyTraitRef, AsPredicate};
use middle::def;
use middle::lang_items::FnOnceTraitLangItem;
use middle::subst::Substs;
use middle::traits::{Obligation, SelectionContext};
use metadata::{csearch, cstore, decoder};

use syntax::{ast, ast_util};
use syntax::codemap::Span;
use syntax::print::pprust;

use std::cell;
use std::cmp::Ordering;

use super::{MethodError, NoMatchData, CandidateSource, impl_item, trait_item};
use super::probe::Mode;

pub fn report_error<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                              span: Span,
                              rcvr_ty: Ty<'tcx>,
                              item_name: ast::Name,
                              rcvr_expr: Option<&ast::Expr>,
                              error: MethodError<'tcx>)
{
    // avoid suggestions when we don't know what's going on.
    if ty::type_is_error(rcvr_ty) {
        return
    }

    match error {
        MethodError::NoMatch(NoMatchData { static_candidates: static_sources,
                                           unsatisfied_predicates,
                                           out_of_scope_traits,
                                           mode }) => {
            let cx = fcx.tcx();

            fcx.type_error_message(
                span,
                |actual| {
                    format!("no {} named `{}` found for type `{}` \
                             in the current scope",
                            if mode == Mode::MethodCall { "method" }
                            else { "associated item" },
                            item_name,
                            actual)
                },
                rcvr_ty,
                None);

            // If the item has the name of a field, give a help note
            if let (&ty::TyStruct(did, substs), Some(expr)) = (&rcvr_ty.sty, rcvr_expr) {
                let fields = ty::lookup_struct_fields(cx, did);

                if let Some(field) = fields.iter().find(|f| f.name == item_name) {
                    let expr_string = match cx.sess.codemap().span_to_snippet(expr.span) {
                        Ok(expr_string) => expr_string,
                        _ => "s".into() // Default to a generic placeholder for the
                                        // expression when we can't generate a string
                                        // snippet
                    };

                    let span_stored_function = || {
                        cx.sess.span_note(span,
                                          &format!("use `({0}.{1})(...)` if you meant to call \
                                                    the function stored in the `{1}` field",
                                                   expr_string, item_name));
                    };

                    let span_did_you_mean = || {
                        cx.sess.span_note(span, &format!("did you mean to write `{0}.{1}`?",
                                                         expr_string, item_name));
                    };

                    // Determine if the field can be used as a function in some way
                    let field_ty = ty::lookup_field_type(cx, did, field.id, substs);
                    if let Ok(fn_once_trait_did) = cx.lang_items.require(FnOnceTraitLangItem) {
                        let infcx = fcx.infcx();
                        infcx.probe(|_| {
                            let fn_once_substs = Substs::new_trait(vec![infcx.next_ty_var()],
                                                                   Vec::new(),
                                                                   field_ty);
                            let trait_ref = ty::TraitRef::new(fn_once_trait_did,
                                                              cx.mk_substs(fn_once_substs));
                            let poly_trait_ref = trait_ref.to_poly_trait_ref();
                            let obligation = Obligation::misc(span,
                                                              fcx.body_id,
                                                              poly_trait_ref.as_predicate());
                            let mut selcx = SelectionContext::new(infcx, fcx);

                            if selcx.evaluate_obligation(&obligation) {
                                span_stored_function();
                            } else {
                                span_did_you_mean();
                            }
                        });
                    } else {
                        match field_ty.sty {
                            // fallback to matching a closure or function pointer
                            ty::TyClosure(..) | ty::TyBareFn(..) => span_stored_function(),
                            _ => span_did_you_mean(),
                        }
                    }
                }
            }

            if !static_sources.is_empty() {
                cx.sess.fileline_note(
                    span,
                    "found defined static methods, maybe a `self` is missing?");

                report_candidates(fcx, span, item_name, static_sources);
            }

            if !unsatisfied_predicates.is_empty() {
                let bound_list = unsatisfied_predicates.iter()
                    .map(|p| format!("`{} : {}`",
                                     p.self_ty(),
                                     p))
                    .collect::<Vec<_>>()
                    .connect(", ");
                cx.sess.fileline_note(
                    span,
                    &format!("the method `{}` exists but the \
                             following trait bounds were not satisfied: {}",
                             item_name,
                             bound_list));
            }

            suggest_traits_to_import(fcx, span, rcvr_ty, item_name,
                                     rcvr_expr, out_of_scope_traits)
        }

        MethodError::Ambiguity(sources) => {
            span_err!(fcx.sess(), span, E0034,
                      "multiple applicable items in scope");

            report_candidates(fcx, span, item_name, sources);
        }

        MethodError::ClosureAmbiguity(trait_def_id) => {
            let msg = format!("the `{}` method from the `{}` trait cannot be explicitly \
                               invoked on this closure as we have not yet inferred what \
                               kind of closure it is",
                               item_name,
                               ty::item_path_str(fcx.tcx(), trait_def_id));
            let msg = if let Some(callee) = rcvr_expr {
                format!("{}; use overloaded call notation instead (e.g., `{}()`)",
                        msg, pprust::expr_to_string(callee))
            } else {
                msg
            };
            fcx.sess().span_err(span, &msg);
        }
    }

    fn report_candidates(fcx: &FnCtxt,
                         span: Span,
                         item_name: ast::Name,
                         mut sources: Vec<CandidateSource>) {
        sources.sort();
        sources.dedup();

        for (idx, source) in sources.iter().enumerate() {
            match *source {
                CandidateSource::ImplSource(impl_did) => {
                    // Provide the best span we can. Use the item, if local to crate, else
                    // the impl, if local to crate (item may be defaulted), else the call site.
                    let item = impl_item(fcx.tcx(), impl_did, item_name).unwrap();
                    let impl_span = fcx.tcx().map.def_id_span(impl_did, span);
                    let item_span = fcx.tcx().map.def_id_span(item.def_id(), impl_span);

                    let impl_ty = check::impl_self_ty(fcx, span, impl_did).ty;

                    let insertion = match ty::impl_trait_ref(fcx.tcx(), impl_did) {
                        None => format!(""),
                        Some(trait_ref) => format!(" of the trait `{}`",
                                                   ty::item_path_str(fcx.tcx(),
                                                                     trait_ref.def_id)),
                    };

                    span_note!(fcx.sess(), item_span,
                               "candidate #{} is defined in an impl{} for the type `{}`",
                               idx + 1,
                               insertion,
                               impl_ty);
                }
                CandidateSource::TraitSource(trait_did) => {
                    let (_, item) = trait_item(fcx.tcx(), trait_did, item_name).unwrap();
                    let item_span = fcx.tcx().map.def_id_span(item.def_id(), span);
                    span_note!(fcx.sess(), item_span,
                               "candidate #{} is defined in the trait `{}`",
                               idx + 1,
                               ty::item_path_str(fcx.tcx(), trait_did));
                }
            }
        }
    }
}


pub type AllTraitsVec = Vec<TraitInfo>;

fn suggest_traits_to_import<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                      span: Span,
                                      rcvr_ty: Ty<'tcx>,
                                      item_name: ast::Name,
                                      rcvr_expr: Option<&ast::Expr>,
                                      valid_out_of_scope_traits: Vec<ast::DefId>)
{
    let tcx = fcx.tcx();

    if !valid_out_of_scope_traits.is_empty() {
        let mut candidates = valid_out_of_scope_traits;
        candidates.sort();
        candidates.dedup();
        let msg = format!(
            "items from traits can only be used if the trait is in scope; \
             the following {traits_are} implemented but not in scope, \
             perhaps add a `use` for {one_of_them}:",
            traits_are = if candidates.len() == 1 {"trait is"} else {"traits are"},
            one_of_them = if candidates.len() == 1 {"it"} else {"one of them"});

        fcx.sess().fileline_help(span, &msg[..]);

        for (i, trait_did) in candidates.iter().enumerate() {
            fcx.sess().fileline_help(span,
                                     &*format!("candidate #{}: use `{}`",
                                               i + 1,
                                               ty::item_path_str(fcx.tcx(), *trait_did)))

        }
        return
    }

    let type_is_local = type_derefs_to_local(fcx, span, rcvr_ty, rcvr_expr);

    // there's no implemented traits, so lets suggest some traits to
    // implement, by finding ones that have the item name, and are
    // legal to implement.
    let mut candidates = all_traits(fcx.ccx)
        .filter(|info| {
            // we approximate the coherence rules to only suggest
            // traits that are legal to implement by requiring that
            // either the type or trait is local. Multidispatch means
            // this isn't perfect (that is, there are cases when
            // implementing a trait would be legal but is rejected
            // here).
            (type_is_local || ast_util::is_local(info.def_id))
                && trait_item(tcx, info.def_id, item_name).is_some()
        })
        .collect::<Vec<_>>();

    if !candidates.is_empty() {
        // sort from most relevant to least relevant
        candidates.sort_by(|a, b| a.cmp(b).reverse());
        candidates.dedup();

        // FIXME #21673 this help message could be tuned to the case
        // of a type parameter: suggest adding a trait bound rather
        // than implementing.
        let msg = format!(
            "items from traits can only be used if the trait is implemented and in scope; \
             the following {traits_define} an item `{name}`, \
             perhaps you need to implement {one_of_them}:",
            traits_define = if candidates.len() == 1 {"trait defines"} else {"traits define"},
            one_of_them = if candidates.len() == 1 {"it"} else {"one of them"},
            name = item_name);

        fcx.sess().fileline_help(span, &msg[..]);

        for (i, trait_info) in candidates.iter().enumerate() {
            fcx.sess().fileline_help(span,
                                     &*format!("candidate #{}: `{}`",
                                               i + 1,
                                               ty::item_path_str(fcx.tcx(), trait_info.def_id)))
        }
    }
}

/// Checks whether there is a local type somewhere in the chain of
/// autoderefs of `rcvr_ty`.
fn type_derefs_to_local<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                  span: Span,
                                  rcvr_ty: Ty<'tcx>,
                                  rcvr_expr: Option<&ast::Expr>) -> bool {
    fn is_local(ty: Ty) -> bool {
        match ty.sty {
            ty::TyEnum(did, _) | ty::TyStruct(did, _) => ast_util::is_local(did),

            ty::TyTrait(ref tr) => ast_util::is_local(tr.principal_def_id()),

            ty::TyParam(_) => true,

            // everything else (primitive types etc.) is effectively
            // non-local (there are "edge" cases, e.g. (LocalType,), but
            // the noise from these sort of types is usually just really
            // annoying, rather than any sort of help).
            _ => false
        }
    }

    // This occurs for UFCS desugaring of `T::method`, where there is no
    // receiver expression for the method call, and thus no autoderef.
    if rcvr_expr.is_none() {
        return is_local(fcx.resolve_type_vars_if_possible(rcvr_ty));
    }

    check::autoderef(fcx, span, rcvr_ty, None,
                     check::UnresolvedTypeAction::Ignore, check::NoPreference,
                     |ty, _| {
        if is_local(ty) {
            Some(())
        } else {
            None
        }
    }).2.is_some()
}

#[derive(Copy, Clone)]
pub struct TraitInfo {
    pub def_id: ast::DefId,
}

impl TraitInfo {
    fn new(def_id: ast::DefId) -> TraitInfo {
        TraitInfo {
            def_id: def_id,
        }
    }
}
impl PartialEq for TraitInfo {
    fn eq(&self, other: &TraitInfo) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for TraitInfo {}
impl PartialOrd for TraitInfo {
    fn partial_cmp(&self, other: &TraitInfo) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for TraitInfo {
    fn cmp(&self, other: &TraitInfo) -> Ordering {
        // accessible traits are more important/relevant than
        // inaccessible ones, local crates are more important than
        // remote ones (local: cnum == 0), and NodeIds just for
        // totality.

        let lhs = (other.def_id.krate, other.def_id.node);
        let rhs = (self.def_id.krate, self.def_id.node);
        lhs.cmp(&rhs)
    }
}

/// Retrieve all traits in this crate and any dependent crates.
pub fn all_traits<'a>(ccx: &'a CrateCtxt) -> AllTraits<'a> {
    if ccx.all_traits.borrow().is_none() {
        use syntax::visit;

        let mut traits = vec![];

        // Crate-local:
        //
        // meh.
        struct Visitor<'a> {
            traits: &'a mut AllTraitsVec,
        }
        impl<'v, 'a> visit::Visitor<'v> for Visitor<'a> {
            fn visit_item(&mut self, i: &'v ast::Item) {
                match i.node {
                    ast::ItemTrait(..) => {
                        self.traits.push(TraitInfo::new(ast_util::local_def(i.id)));
                    }
                    _ => {}
                }
                visit::walk_item(self, i)
            }
        }
        visit::walk_crate(&mut Visitor {
            traits: &mut traits
        }, ccx.tcx.map.krate());

        // Cross-crate:
        fn handle_external_def(traits: &mut AllTraitsVec,
                               ccx: &CrateCtxt,
                               cstore: &cstore::CStore,
                               dl: decoder::DefLike) {
            match dl {
                decoder::DlDef(def::DefTrait(did)) => {
                    traits.push(TraitInfo::new(did));
                }
                decoder::DlDef(def::DefMod(did)) => {
                    csearch::each_child_of_item(cstore, did, |dl, _, _| {
                        handle_external_def(traits, ccx, cstore, dl)
                    })
                }
                _ => {}
            }
        }
        let cstore = &ccx.tcx.sess.cstore;
        cstore.iter_crate_data(|cnum, _| {
            csearch::each_top_level_item_of_crate(cstore, cnum, |dl, _, _| {
                handle_external_def(&mut traits, ccx, cstore, dl)
            })
        });

        *ccx.all_traits.borrow_mut() = Some(traits);
    }

    let borrow = ccx.all_traits.borrow();
    assert!(borrow.is_some());
    AllTraits {
        borrow: borrow,
        idx: 0
    }
}

pub struct AllTraits<'a> {
    borrow: cell::Ref<'a, Option<AllTraitsVec>>,
    idx: usize
}

impl<'a> Iterator for AllTraits<'a> {
    type Item = TraitInfo;

    fn next(&mut self) -> Option<TraitInfo> {
        let AllTraits { ref borrow, ref mut idx } = *self;
        // ugh.
        borrow.as_ref().unwrap().get(*idx).map(|info| {
            *idx += 1;
            *info
        })
    }
}
