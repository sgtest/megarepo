// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use check::FnCtxt;
use rustc::infer::InferOk;
use rustc::traits::ObligationCause;

use syntax::ast;
use syntax_pos::{self, Span};
use rustc::hir;
use rustc::hir::print;
use rustc::hir::def::Def;
use rustc::ty::{self, Ty, AssociatedItem};
use errors::{DiagnosticBuilder, CodeMapper};

use super::method::probe;

impl<'a, 'gcx, 'tcx> FnCtxt<'a, 'gcx, 'tcx> {
    // Requires that the two types unify, and prints an error message if
    // they don't.
    pub fn demand_suptype(&self, sp: Span, expected: Ty<'tcx>, actual: Ty<'tcx>) {
        self.demand_suptype_diag(sp, expected, actual).map(|mut e| e.emit());
    }

    pub fn demand_suptype_diag(&self,
                               sp: Span,
                               expected: Ty<'tcx>,
                               actual: Ty<'tcx>) -> Option<DiagnosticBuilder<'tcx>> {
        let cause = &self.misc(sp);
        match self.at(cause, self.param_env).sup(expected, actual) {
            Ok(InferOk { obligations, value: () }) => {
                self.register_predicates(obligations);
                None
            },
            Err(e) => {
                Some(self.report_mismatched_types(&cause, expected, actual, e))
            }
        }
    }

    pub fn demand_eqtype(&self, sp: Span, expected: Ty<'tcx>, actual: Ty<'tcx>) {
        if let Some(mut err) = self.demand_eqtype_diag(sp, expected, actual) {
            err.emit();
        }
    }

    pub fn demand_eqtype_diag(&self,
                             sp: Span,
                             expected: Ty<'tcx>,
                             actual: Ty<'tcx>) -> Option<DiagnosticBuilder<'tcx>> {
        self.demand_eqtype_with_origin(&self.misc(sp), expected, actual)
    }

    pub fn demand_eqtype_with_origin(&self,
                                     cause: &ObligationCause<'tcx>,
                                     expected: Ty<'tcx>,
                                     actual: Ty<'tcx>) -> Option<DiagnosticBuilder<'tcx>> {
        match self.at(cause, self.param_env).eq(expected, actual) {
            Ok(InferOk { obligations, value: () }) => {
                self.register_predicates(obligations);
                None
            },
            Err(e) => {
                Some(self.report_mismatched_types(cause, expected, actual, e))
            }
        }
    }

    pub fn demand_coerce(&self, expr: &hir::Expr, checked_ty: Ty<'tcx>, expected: Ty<'tcx>) {
        if let Some(mut err) = self.demand_coerce_diag(expr, checked_ty, expected) {
            err.emit();
        }
    }

    // Checks that the type of `expr` can be coerced to `expected`.
    //
    // NB: This code relies on `self.diverges` to be accurate.  In
    // particular, assignments to `!` will be permitted if the
    // diverges flag is currently "always".
    pub fn demand_coerce_diag(&self,
                              expr: &hir::Expr,
                              checked_ty: Ty<'tcx>,
                              expected: Ty<'tcx>) -> Option<DiagnosticBuilder<'tcx>> {
        let expected = self.resolve_type_vars_with_obligations(expected);

        if let Err(e) = self.try_coerce(expr, checked_ty, self.diverges.get(), expected) {
            let cause = self.misc(expr.span);
            let expr_ty = self.resolve_type_vars_with_obligations(checked_ty);
            let mut err = self.report_mismatched_types(&cause, expected, expr_ty, e);

            // If the expected type is an enum with any variants whose sole
            // field is of the found type, suggest such variants. See Issue
            // #42764.
            if let ty::TyAdt(expected_adt, substs) = expected.sty {
                let mut compatible_variants = vec![];
                for variant in &expected_adt.variants {
                    if variant.fields.len() == 1 {
                        let sole_field = &variant.fields[0];
                        let sole_field_ty = sole_field.ty(self.tcx, substs);
                        if self.can_coerce(expr_ty, sole_field_ty) {
                            let mut variant_path = self.tcx.item_path_str(variant.did);
                            variant_path = variant_path.trim_left_matches("std::prelude::v1::")
                                .to_string();
                            compatible_variants.push(variant_path);
                        }
                    }
                }
                if !compatible_variants.is_empty() {
                    let expr_text = print::to_string(print::NO_ANN, |s| s.print_expr(expr));
                    let suggestions = compatible_variants.iter()
                        .map(|v| format!("{}({})", v, expr_text)).collect::<Vec<_>>();
                    err.span_suggestions(expr.span,
                                         "try using a variant of the expected type",
                                         suggestions);
                }
            }

            if let Some(suggestion) = self.check_ref(expr,
                                                     checked_ty,
                                                     expected) {
                err.help(&suggestion);
            } else {
                let mode = probe::Mode::MethodCall;
                let suggestions = self.probe_for_return_type(syntax_pos::DUMMY_SP,
                                                             mode,
                                                             expected,
                                                             checked_ty,
                                                             ast::DUMMY_NODE_ID);
                if suggestions.len() > 0 {
                    err.help(&format!("here are some functions which \
                                       might fulfill your needs:\n{}",
                                      self.get_best_match(&suggestions).join("\n")));
                }
            }
            return Some(err);
        }
        None
    }

    fn format_method_suggestion(&self, method: &AssociatedItem) -> String {
        format!("- .{}({})",
                method.name,
                if self.has_no_input_arg(method) {
                    ""
                } else {
                    "..."
                })
    }

    fn display_suggested_methods(&self, methods: &[AssociatedItem]) -> Vec<String> {
        methods.iter()
               .take(5)
               .map(|method| self.format_method_suggestion(&*method))
               .collect::<Vec<String>>()
    }

    fn get_best_match(&self, methods: &[AssociatedItem]) -> Vec<String> {
        let no_argument_methods: Vec<_> =
            methods.iter()
                   .filter(|ref x| self.has_no_input_arg(&*x))
                   .map(|x| x.clone())
                   .collect();
        if no_argument_methods.len() > 0 {
            self.display_suggested_methods(&no_argument_methods)
        } else {
            self.display_suggested_methods(&methods)
        }
    }

    // This function checks if the method isn't static and takes other arguments than `self`.
    fn has_no_input_arg(&self, method: &AssociatedItem) -> bool {
        match method.def() {
            Def::Method(def_id) => {
                self.tcx.fn_sig(def_id).inputs().skip_binder().len() == 1
            }
            _ => false,
        }
    }

    /// This function is used to determine potential "simple" improvements or users' errors and
    /// provide them useful help. For example:
    ///
    /// ```
    /// fn some_fn(s: &str) {}
    ///
    /// let x = "hey!".to_owned();
    /// some_fn(x); // error
    /// ```
    ///
    /// No need to find every potential function which could make a coercion to transform a
    /// `String` into a `&str` since a `&` would do the trick!
    ///
    /// In addition of this check, it also checks between references mutability state. If the
    /// expected is mutable but the provided isn't, maybe we could just say "Hey, try with
    /// `&mut`!".
    fn check_ref(&self,
                 expr: &hir::Expr,
                 checked_ty: Ty<'tcx>,
                 expected: Ty<'tcx>)
                 -> Option<String> {
        match (&expected.sty, &checked_ty.sty) {
            (&ty::TyRef(_, _), &ty::TyRef(_, _)) => None,
            (&ty::TyRef(_, mutability), _) => {
                // Check if it can work when put into a ref. For example:
                //
                // ```
                // fn bar(x: &mut i32) {}
                //
                // let x = 0u32;
                // bar(&x); // error, expected &mut
                // ```
                let ref_ty = match mutability.mutbl {
                    hir::Mutability::MutMutable => self.tcx.mk_mut_ref(
                                                       self.tcx.mk_region(ty::ReStatic),
                                                       checked_ty),
                    hir::Mutability::MutImmutable => self.tcx.mk_imm_ref(
                                                       self.tcx.mk_region(ty::ReStatic),
                                                       checked_ty),
                };
                if self.can_coerce(ref_ty, expected) {
                    // Use the callsite's span if this is a macro call. #41858
                    let sp = self.sess().codemap().call_span_if_macro(expr.span);
                    if let Ok(src) = self.tcx.sess.codemap().span_to_snippet(sp) {
                        return Some(format!("try with `{}{}`",
                                            match mutability.mutbl {
                                                hir::Mutability::MutMutable => "&mut ",
                                                hir::Mutability::MutImmutable => "&",
                                            },
                                            &src));
                    }
                }
                None
            }
            _ => None,
        }
    }
}
