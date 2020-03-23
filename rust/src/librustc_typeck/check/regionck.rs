//! The region check is a final pass that runs over the AST after we have
//! inferred the type constraints but before we have actually finalized
//! the types. Its purpose is to embed a variety of region constraints.
//! Inserting these constraints as a separate pass is good because (1) it
//! localizes the code that has to do with region inference and (2) often
//! we cannot know what constraints are needed until the basic types have
//! been inferred.
//!
//! ### Interaction with the borrow checker
//!
//! In general, the job of the borrowck module (which runs later) is to
//! check that all soundness criteria are met, given a particular set of
//! regions. The job of *this* module is to anticipate the needs of the
//! borrow checker and infer regions that will satisfy its requirements.
//! It is generally true that the inference doesn't need to be sound,
//! meaning that if there is a bug and we inferred bad regions, the borrow
//! checker should catch it. This is not entirely true though; for
//! example, the borrow checker doesn't check subtyping, and it doesn't
//! check that region pointers are always live when they are used. It
//! might be worthwhile to fix this so that borrowck serves as a kind of
//! verification step -- that would add confidence in the overall
//! correctness of the compiler, at the cost of duplicating some type
//! checks and effort.
//!
//! ### Inferring the duration of borrows, automatic and otherwise
//!
//! Whenever we introduce a borrowed pointer, for example as the result of
//! a borrow expression `let x = &data`, the lifetime of the pointer `x`
//! is always specified as a region inference variable. `regionck` has the
//! job of adding constraints such that this inference variable is as
//! narrow as possible while still accommodating all uses (that is, every
//! dereference of the resulting pointer must be within the lifetime).
//!
//! #### Reborrows
//!
//! Generally speaking, `regionck` does NOT try to ensure that the data
//! `data` will outlive the pointer `x`. That is the job of borrowck. The
//! one exception is when "re-borrowing" the contents of another borrowed
//! pointer. For example, imagine you have a borrowed pointer `b` with
//! lifetime `L1` and you have an expression `&*b`. The result of this
//! expression will be another borrowed pointer with lifetime `L2` (which is
//! an inference variable). The borrow checker is going to enforce the
//! constraint that `L2 < L1`, because otherwise you are re-borrowing data
//! for a lifetime larger than the original loan. However, without the
//! routines in this module, the region inferencer would not know of this
//! dependency and thus it might infer the lifetime of `L2` to be greater
//! than `L1` (issue #3148).
//!
//! There are a number of troublesome scenarios in the tests
//! `region-dependent-*.rs`, but here is one example:
//!
//!     struct Foo { i: i32 }
//!     struct Bar { foo: Foo  }
//!     fn get_i<'a>(x: &'a Bar) -> &'a i32 {
//!        let foo = &x.foo; // Lifetime L1
//!        &foo.i            // Lifetime L2
//!     }
//!
//! Note that this comes up either with `&` expressions, `ref`
//! bindings, and `autorefs`, which are the three ways to introduce
//! a borrow.
//!
//! The key point here is that when you are borrowing a value that
//! is "guaranteed" by a borrowed pointer, you must link the
//! lifetime of that borrowed pointer (`L1`, here) to the lifetime of
//! the borrow itself (`L2`). What do I mean by "guaranteed" by a
//! borrowed pointer? I mean any data that is reached by first
//! dereferencing a borrowed pointer and then either traversing
//! interior offsets or boxes. We say that the guarantor
//! of such data is the region of the borrowed pointer that was
//! traversed. This is essentially the same as the ownership
//! relation, except that a borrowed pointer never owns its
//! contents.

use crate::check::dropck;
use crate::check::FnCtxt;
use crate::mem_categorization as mc;
use crate::middle::region;
use rustc::ty::adjustment;
use rustc::ty::subst::{GenericArgKind, SubstsRef};
use rustc::ty::{self, Ty};
use rustc_hir as hir;
use rustc_hir::def_id::DefId;
use rustc_hir::intravisit::{self, NestedVisitorMap, Visitor};
use rustc_hir::PatKind;
use rustc_infer::infer::outlives::env::OutlivesEnvironment;
use rustc_infer::infer::{self, RegionObligation, RegionckMode};
use rustc_span::Span;
use rustc_trait_selection::infer::OutlivesEnvironmentExt;
use rustc_trait_selection::opaque_types::InferCtxtExt;
use std::mem;
use std::ops::Deref;

// a variation on try that just returns unit
macro_rules! ignore_err {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(_) => {
                debug!("ignoring mem-categorization error!");
                return ();
            }
        }
    };
}

///////////////////////////////////////////////////////////////////////////
// PUBLIC ENTRY POINTS

impl<'a, 'tcx> FnCtxt<'a, 'tcx> {
    pub fn regionck_expr(&self, body: &'tcx hir::Body<'tcx>) {
        let subject = self.tcx.hir().body_owner_def_id(body.id());
        let id = body.value.hir_id;
        let mut rcx =
            RegionCtxt::new(self, RepeatingScope(id), id, Subject(subject), self.param_env);

        // There are no add'l implied bounds when checking a
        // standalone expr (e.g., the `E` in a type like `[u32; E]`).
        rcx.outlives_environment.save_implied_bounds(id);

        if !self.errors_reported_since_creation() {
            // regionck assumes typeck succeeded
            rcx.visit_body(body);
            rcx.visit_region_obligations(id);
        }
        rcx.resolve_regions_and_report_errors(RegionckMode::for_item_body(self.tcx));
    }

    /// Region checking during the WF phase for items. `wf_tys` are the
    /// types from which we should derive implied bounds, if any.
    pub fn regionck_item(&self, item_id: hir::HirId, span: Span, wf_tys: &[Ty<'tcx>]) {
        debug!("regionck_item(item.id={:?}, wf_tys={:?})", item_id, wf_tys);
        let subject = self.tcx.hir().local_def_id(item_id);
        let mut rcx = RegionCtxt::new(
            self,
            RepeatingScope(item_id),
            item_id,
            Subject(subject),
            self.param_env,
        );
        rcx.outlives_environment.add_implied_bounds(self, wf_tys, item_id, span);
        rcx.outlives_environment.save_implied_bounds(item_id);
        rcx.visit_region_obligations(item_id);
        rcx.resolve_regions_and_report_errors(RegionckMode::default());
    }

    /// Region check a function body. Not invoked on closures, but
    /// only on the "root" fn item (in which closures may be
    /// embedded). Walks the function body and adds various add'l
    /// constraints that are needed for region inference. This is
    /// separated both to isolate "pure" region constraints from the
    /// rest of type check and because sometimes we need type
    /// inference to have completed before we can determine which
    /// constraints to add.
    pub fn regionck_fn(&self, fn_id: hir::HirId, body: &'tcx hir::Body<'tcx>) {
        debug!("regionck_fn(id={})", fn_id);
        let subject = self.tcx.hir().body_owner_def_id(body.id());
        let hir_id = body.value.hir_id;
        let mut rcx =
            RegionCtxt::new(self, RepeatingScope(hir_id), hir_id, Subject(subject), self.param_env);

        if !self.errors_reported_since_creation() {
            // regionck assumes typeck succeeded
            rcx.visit_fn_body(fn_id, body, self.tcx.hir().span(fn_id));
        }

        rcx.resolve_regions_and_report_errors(RegionckMode::for_item_body(self.tcx));
    }
}

///////////////////////////////////////////////////////////////////////////
// INTERNALS

pub struct RegionCtxt<'a, 'tcx> {
    pub fcx: &'a FnCtxt<'a, 'tcx>,

    pub region_scope_tree: &'tcx region::ScopeTree,

    outlives_environment: OutlivesEnvironment<'tcx>,

    // id of innermost fn body id
    body_id: hir::HirId,
    body_owner: DefId,

    // call_site scope of innermost fn
    call_site_scope: Option<region::Scope>,

    // id of innermost fn or loop
    repeating_scope: hir::HirId,

    // id of AST node being analyzed (the subject of the analysis).
    subject_def_id: DefId,
}

impl<'a, 'tcx> Deref for RegionCtxt<'a, 'tcx> {
    type Target = FnCtxt<'a, 'tcx>;
    fn deref(&self) -> &Self::Target {
        &self.fcx
    }
}

pub struct RepeatingScope(hir::HirId);
pub struct Subject(DefId);

impl<'a, 'tcx> RegionCtxt<'a, 'tcx> {
    pub fn new(
        fcx: &'a FnCtxt<'a, 'tcx>,
        RepeatingScope(initial_repeating_scope): RepeatingScope,
        initial_body_id: hir::HirId,
        Subject(subject): Subject,
        param_env: ty::ParamEnv<'tcx>,
    ) -> RegionCtxt<'a, 'tcx> {
        let region_scope_tree = fcx.tcx.region_scope_tree(subject);
        let outlives_environment = OutlivesEnvironment::new(param_env);
        RegionCtxt {
            fcx,
            region_scope_tree,
            repeating_scope: initial_repeating_scope,
            body_id: initial_body_id,
            body_owner: subject,
            call_site_scope: None,
            subject_def_id: subject,
            outlives_environment,
        }
    }

    fn set_repeating_scope(&mut self, scope: hir::HirId) -> hir::HirId {
        mem::replace(&mut self.repeating_scope, scope)
    }

    /// Try to resolve the type for the given node, returning `t_err` if an error results. Note that
    /// we never care about the details of the error, the same error will be detected and reported
    /// in the writeback phase.
    ///
    /// Note one important point: we do not attempt to resolve *region variables* here. This is
    /// because regionck is essentially adding constraints to those region variables and so may yet
    /// influence how they are resolved.
    ///
    /// Consider this silly example:
    ///
    /// ```
    /// fn borrow(x: &i32) -> &i32 {x}
    /// fn foo(x: @i32) -> i32 {  // block: B
    ///     let b = borrow(x);    // region: <R0>
    ///     *b
    /// }
    /// ```
    ///
    /// Here, the region of `b` will be `<R0>`. `<R0>` is constrained to be some subregion of the
    /// block B and some superregion of the call. If we forced it now, we'd choose the smaller
    /// region (the call). But that would make the *b illegal. Since we don't resolve, the type
    /// of b will be `&<R0>.i32` and then `*b` will require that `<R0>` be bigger than the let and
    /// the `*b` expression, so we will effectively resolve `<R0>` to be the block B.
    pub fn resolve_type(&self, unresolved_ty: Ty<'tcx>) -> Ty<'tcx> {
        self.resolve_vars_if_possible(&unresolved_ty)
    }

    /// Try to resolve the type for the given node.
    fn resolve_node_type(&self, id: hir::HirId) -> Ty<'tcx> {
        let t = self.node_ty(id);
        self.resolve_type(t)
    }

    /// Try to resolve the type for the given node.
    pub fn resolve_expr_type_adjusted(&mut self, expr: &hir::Expr<'_>) -> Ty<'tcx> {
        let ty = self.tables.borrow().expr_ty_adjusted(expr);
        self.resolve_type(ty)
    }

    /// This is the "main" function when region-checking a function item or a closure
    /// within a function item. It begins by updating various fields (e.g., `call_site_scope`
    /// and `outlives_environment`) to be appropriate to the function and then adds constraints
    /// derived from the function body.
    ///
    /// Note that it does **not** restore the state of the fields that
    /// it updates! This is intentional, since -- for the main
    /// function -- we wish to be able to read the final
    /// `outlives_environment` and other fields from the caller. For
    /// closures, however, we save and restore any "scoped state"
    /// before we invoke this function. (See `visit_fn` in the
    /// `intravisit::Visitor` impl below.)
    fn visit_fn_body(
        &mut self,
        id: hir::HirId, // the id of the fn itself
        body: &'tcx hir::Body<'tcx>,
        span: Span,
    ) {
        // When we enter a function, we can derive
        debug!("visit_fn_body(id={:?})", id);

        let body_id = body.id();
        self.body_id = body_id.hir_id;
        self.body_owner = self.tcx.hir().body_owner_def_id(body_id);

        let call_site =
            region::Scope { id: body.value.hir_id.local_id, data: region::ScopeData::CallSite };
        self.call_site_scope = Some(call_site);

        let fn_sig = {
            match self.tables.borrow().liberated_fn_sigs().get(id) {
                Some(f) => *f,
                None => {
                    bug!("No fn-sig entry for id={:?}", id);
                }
            }
        };

        // Collect the types from which we create inferred bounds.
        // For the return type, if diverging, substitute `bool` just
        // because it will have no effect.
        //
        // FIXME(#27579) return types should not be implied bounds
        let fn_sig_tys: Vec<_> =
            fn_sig.inputs().iter().cloned().chain(Some(fn_sig.output())).collect();

        self.outlives_environment.add_implied_bounds(
            self.fcx,
            &fn_sig_tys[..],
            body_id.hir_id,
            span,
        );
        self.outlives_environment.save_implied_bounds(body_id.hir_id);
        self.link_fn_params(&body.params);
        self.visit_body(body);
        self.visit_region_obligations(body_id.hir_id);

        let call_site_scope = self.call_site_scope.unwrap();
        debug!("visit_fn_body body.id {:?} call_site_scope: {:?}", body.id(), call_site_scope);
        let call_site_region = self.tcx.mk_region(ty::ReScope(call_site_scope));

        self.type_of_node_must_outlive(infer::CallReturn(span), body_id.hir_id, call_site_region);

        self.constrain_opaque_types(
            &self.fcx.opaque_types.borrow(),
            self.outlives_environment.free_region_map(),
        );
    }

    fn visit_region_obligations(&mut self, hir_id: hir::HirId) {
        debug!("visit_region_obligations: hir_id={:?}", hir_id);

        // region checking can introduce new pending obligations
        // which, when processed, might generate new region
        // obligations. So make sure we process those.
        self.select_all_obligations_or_error();
    }

    fn resolve_regions_and_report_errors(&self, mode: RegionckMode) {
        self.infcx.process_registered_region_obligations(
            self.outlives_environment.region_bound_pairs_map(),
            self.implicit_region_bound,
            self.param_env,
        );

        self.fcx.resolve_regions_and_report_errors(
            self.subject_def_id,
            &self.region_scope_tree,
            &self.outlives_environment,
            mode,
        );
    }

    fn constrain_bindings_in_pat(&mut self, pat: &hir::Pat<'_>) {
        debug!("regionck::visit_pat(pat={:?})", pat);
        pat.each_binding(|_, hir_id, span, _| {
            // If we have a variable that contains region'd data, that
            // data will be accessible from anywhere that the variable is
            // accessed. We must be wary of loops like this:
            //
            //     // from src/test/compile-fail/borrowck-lend-flow.rs
            //     let mut v = box 3, w = box 4;
            //     let mut x = &mut w;
            //     loop {
            //         **x += 1;   // (2)
            //         borrow(v);  //~ ERROR cannot borrow
            //         x = &mut v; // (1)
            //     }
            //
            // Typically, we try to determine the region of a borrow from
            // those points where it is dereferenced. In this case, one
            // might imagine that the lifetime of `x` need only be the
            // body of the loop. But of course this is incorrect because
            // the pointer that is created at point (1) is consumed at
            // point (2), meaning that it must be live across the loop
            // iteration. The easiest way to guarantee this is to require
            // that the lifetime of any regions that appear in a
            // variable's type enclose at least the variable's scope.
            let var_scope = self.region_scope_tree.var_scope(hir_id.local_id);
            let var_region = self.tcx.mk_region(ty::ReScope(var_scope));

            let origin = infer::BindingTypeIsNotValidAtDecl(span);
            self.type_of_node_must_outlive(origin, hir_id, var_region);

            let typ = self.resolve_node_type(hir_id);
            let body_id = self.body_id;
            let _ = dropck::check_drop_obligations(self, typ, span, body_id);
        })
    }
}

impl<'a, 'tcx> Visitor<'tcx> for RegionCtxt<'a, 'tcx> {
    // (..) FIXME(#3238) should use visit_pat, not visit_arm/visit_local,
    // However, right now we run into an issue whereby some free
    // regions are not properly related if they appear within the
    // types of arguments that must be inferred. This could be
    // addressed by deferring the construction of the region
    // hierarchy, and in particular the relationships between free
    // regions, until regionck, as described in #3238.

    type Map = intravisit::ErasedMap<'tcx>;

    fn nested_visit_map(&mut self) -> NestedVisitorMap<Self::Map> {
        NestedVisitorMap::None
    }

    fn visit_fn(
        &mut self,
        fk: intravisit::FnKind<'tcx>,
        _: &'tcx hir::FnDecl<'tcx>,
        body_id: hir::BodyId,
        span: Span,
        hir_id: hir::HirId,
    ) {
        assert!(
            match fk {
                intravisit::FnKind::Closure(..) => true,
                _ => false,
            },
            "visit_fn invoked for something other than a closure"
        );

        // Save state of current function before invoking
        // `visit_fn_body`.  We will restore afterwards.
        let old_body_id = self.body_id;
        let old_body_owner = self.body_owner;
        let old_call_site_scope = self.call_site_scope;
        let env_snapshot = self.outlives_environment.push_snapshot_pre_closure();

        let body = self.tcx.hir().body(body_id);
        self.visit_fn_body(hir_id, body, span);

        // Restore state from previous function.
        self.outlives_environment.pop_snapshot_post_closure(env_snapshot);
        self.call_site_scope = old_call_site_scope;
        self.body_id = old_body_id;
        self.body_owner = old_body_owner;
    }

    //visit_pat: visit_pat, // (..) see above

    fn visit_arm(&mut self, arm: &'tcx hir::Arm<'tcx>) {
        // see above
        self.constrain_bindings_in_pat(&arm.pat);
        intravisit::walk_arm(self, arm);
    }

    fn visit_local(&mut self, l: &'tcx hir::Local<'tcx>) {
        // see above
        self.constrain_bindings_in_pat(&l.pat);
        self.link_local(l);
        intravisit::walk_local(self, l);
    }

    fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
        debug!("regionck::visit_expr(e={:?}, repeating_scope={:?})", expr, self.repeating_scope);

        // No matter what, the type of each expression must outlive the
        // scope of that expression. This also guarantees basic WF.
        let expr_ty = self.resolve_node_type(expr.hir_id);
        // the region corresponding to this expression
        let expr_region = self.tcx.mk_region(ty::ReScope(region::Scope {
            id: expr.hir_id.local_id,
            data: region::ScopeData::Node,
        }));
        self.type_must_outlive(
            infer::ExprTypeIsNotInScope(expr_ty, expr.span),
            expr_ty,
            expr_region,
        );

        let is_method_call = self.tables.borrow().is_method_call(expr);

        // If we are calling a method (either explicitly or via an
        // overloaded operator), check that all of the types provided as
        // arguments for its type parameters are well-formed, and all the regions
        // provided as arguments outlive the call.
        if is_method_call {
            let origin = match expr.kind {
                hir::ExprKind::MethodCall(..) => infer::ParameterOrigin::MethodCall,
                hir::ExprKind::Unary(op, _) if op == hir::UnOp::UnDeref => {
                    infer::ParameterOrigin::OverloadedDeref
                }
                _ => infer::ParameterOrigin::OverloadedOperator,
            };

            let substs = self.tables.borrow().node_substs(expr.hir_id);
            self.substs_wf_in_scope(origin, substs, expr.span, expr_region);
            // Arguments (sub-expressions) are checked via `constrain_call`, below.
        }

        // Check any autoderefs or autorefs that appear.
        let cmt_result = self.constrain_adjustments(expr);

        // If necessary, constrain destructors in this expression. This will be
        // the adjusted form if there is an adjustment.
        match cmt_result {
            Ok(head_cmt) => {
                self.check_safety_of_rvalue_destructor_if_necessary(&head_cmt, expr.span);
            }
            Err(..) => {
                self.tcx.sess.delay_span_bug(expr.span, "cat_expr Errd");
            }
        }

        debug!(
            "regionck::visit_expr(e={:?}, repeating_scope={:?}) - visiting subexprs",
            expr, self.repeating_scope
        );
        match expr.kind {
            hir::ExprKind::Path(_) => {
                let substs = self.tables.borrow().node_substs(expr.hir_id);
                let origin = infer::ParameterOrigin::Path;
                self.substs_wf_in_scope(origin, substs, expr.span, expr_region);
            }

            hir::ExprKind::Call(ref callee, ref args) => {
                if is_method_call {
                    self.constrain_call(expr, Some(&callee), args.iter().map(|e| &*e));
                } else {
                    self.constrain_callee(&callee);
                    self.constrain_call(expr, None, args.iter().map(|e| &*e));
                }

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::MethodCall(.., ref args) => {
                self.constrain_call(expr, Some(&args[0]), args[1..].iter().map(|e| &*e));

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::AssignOp(_, ref lhs, ref rhs) => {
                if is_method_call {
                    self.constrain_call(expr, Some(&lhs), Some(&**rhs).into_iter());
                }

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Index(ref lhs, ref rhs) if is_method_call => {
                self.constrain_call(expr, Some(&lhs), Some(&**rhs).into_iter());

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Binary(_, ref lhs, ref rhs) if is_method_call => {
                // As `ExprKind::MethodCall`, but the call is via an overloaded op.
                self.constrain_call(expr, Some(&lhs), Some(&**rhs).into_iter());

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Binary(_, ref lhs, ref rhs) => {
                // If you do `x OP y`, then the types of `x` and `y` must
                // outlive the operation you are performing.
                let lhs_ty = self.resolve_expr_type_adjusted(&lhs);
                let rhs_ty = self.resolve_expr_type_adjusted(&rhs);
                for &ty in &[lhs_ty, rhs_ty] {
                    self.type_must_outlive(infer::Operand(expr.span), ty, expr_region);
                }
                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Unary(hir::UnOp::UnDeref, ref base) => {
                // For *a, the lifetime of a must enclose the deref
                if is_method_call {
                    self.constrain_call(expr, Some(base), None::<hir::Expr<'_>>.iter());
                }
                // For overloaded derefs, base_ty is the input to `Deref::deref`,
                // but it's a reference type uing the same region as the output.
                let base_ty = self.resolve_expr_type_adjusted(base);
                if let ty::Ref(r_ptr, _, _) = base_ty.kind {
                    self.mk_subregion_due_to_dereference(expr.span, expr_region, r_ptr);
                }

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Unary(_, ref lhs) if is_method_call => {
                // As above.
                self.constrain_call(expr, Some(&lhs), None::<hir::Expr<'_>>.iter());

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Index(ref vec_expr, _) => {
                // For a[b], the lifetime of a must enclose the deref
                let vec_type = self.resolve_expr_type_adjusted(&vec_expr);
                self.constrain_index(expr, vec_type);

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Cast(ref source, _) => {
                // Determine if we are casting `source` to a trait
                // instance.  If so, we have to be sure that the type of
                // the source obeys the trait's region bound.
                self.constrain_cast(expr, &source);
                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::AddrOf(hir::BorrowKind::Ref, m, ref base) => {
                self.link_addr_of(expr, m, &base);

                // Require that when you write a `&expr` expression, the
                // resulting pointer has a lifetime that encompasses the
                // `&expr` expression itself. Note that we constraining
                // the type of the node expr.id here *before applying
                // adjustments*.
                //
                // FIXME(https://github.com/rust-lang/rfcs/issues/811)
                // nested method calls requires that this rule change
                let ty0 = self.resolve_node_type(expr.hir_id);
                self.type_must_outlive(infer::AddrOf(expr.span), ty0, expr_region);
                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Match(ref discr, ref arms, _) => {
                self.link_match(&discr, &arms[..]);

                intravisit::walk_expr(self, expr);
            }

            hir::ExprKind::Closure(.., body_id, _, _) => {
                self.check_expr_fn_block(expr, body_id);
            }

            hir::ExprKind::Loop(ref body, _, _) => {
                let repeating_scope = self.set_repeating_scope(body.hir_id);
                intravisit::walk_expr(self, expr);
                self.set_repeating_scope(repeating_scope);
            }

            hir::ExprKind::Ret(Some(ref ret_expr)) => {
                let call_site_scope = self.call_site_scope;
                debug!(
                    "visit_expr ExprKind::Ret ret_expr.hir_id {} call_site_scope: {:?}",
                    ret_expr.hir_id, call_site_scope
                );
                let call_site_region = self.tcx.mk_region(ty::ReScope(call_site_scope.unwrap()));
                self.type_of_node_must_outlive(
                    infer::CallReturn(ret_expr.span),
                    ret_expr.hir_id,
                    call_site_region,
                );
                intravisit::walk_expr(self, expr);
            }

            _ => {
                intravisit::walk_expr(self, expr);
            }
        }
    }
}

impl<'a, 'tcx> RegionCtxt<'a, 'tcx> {
    fn constrain_cast(&mut self, cast_expr: &hir::Expr<'_>, source_expr: &hir::Expr<'_>) {
        debug!("constrain_cast(cast_expr={:?}, source_expr={:?})", cast_expr, source_expr);

        let source_ty = self.resolve_node_type(source_expr.hir_id);
        let target_ty = self.resolve_node_type(cast_expr.hir_id);

        self.walk_cast(cast_expr, source_ty, target_ty);
    }

    fn walk_cast(&mut self, cast_expr: &hir::Expr<'_>, from_ty: Ty<'tcx>, to_ty: Ty<'tcx>) {
        debug!("walk_cast(from_ty={:?}, to_ty={:?})", from_ty, to_ty);
        match (&from_ty.kind, &to_ty.kind) {
            /*From:*/
            (&ty::Ref(from_r, from_ty, _), /*To:  */ &ty::Ref(to_r, to_ty, _)) => {
                // Target cannot outlive source, naturally.
                self.sub_regions(infer::Reborrow(cast_expr.span), to_r, from_r);
                self.walk_cast(cast_expr, from_ty, to_ty);
            }

            /*From:*/
            (_, /*To:  */ &ty::Dynamic(.., r)) => {
                // When T is existentially quantified as a trait
                // `Foo+'to`, it must outlive the region bound `'to`.
                self.type_must_outlive(infer::RelateObjectBound(cast_expr.span), from_ty, r);
            }

            /*From:*/
            (&ty::Adt(from_def, _), /*To:  */ &ty::Adt(to_def, _))
                if from_def.is_box() && to_def.is_box() =>
            {
                self.walk_cast(cast_expr, from_ty.boxed_ty(), to_ty.boxed_ty());
            }

            _ => {}
        }
    }

    fn check_expr_fn_block(&mut self, expr: &'tcx hir::Expr<'tcx>, body_id: hir::BodyId) {
        let repeating_scope = self.set_repeating_scope(body_id.hir_id);
        intravisit::walk_expr(self, expr);
        self.set_repeating_scope(repeating_scope);
    }

    fn constrain_callee(&mut self, callee_expr: &hir::Expr<'_>) {
        let callee_ty = self.resolve_node_type(callee_expr.hir_id);
        match callee_ty.kind {
            ty::FnDef(..) | ty::FnPtr(_) => {}
            _ => {
                // this should not happen, but it does if the program is
                // erroneous
                //
                // bug!(
                //     callee_expr.span,
                //     "Calling non-function: {}",
                //     callee_ty);
            }
        }
    }

    fn constrain_call<'b, I: Iterator<Item = &'b hir::Expr<'b>>>(
        &mut self,
        call_expr: &hir::Expr<'_>,
        receiver: Option<&hir::Expr<'_>>,
        arg_exprs: I,
    ) {
        //! Invoked on every call site (i.e., normal calls, method calls,
        //! and overloaded operators). Constrains the regions which appear
        //! in the type of the function. Also constrains the regions that
        //! appear in the arguments appropriately.

        debug!("constrain_call(call_expr={:?}, receiver={:?})", call_expr, receiver);

        // `callee_region` is the scope representing the time in which the
        // call occurs.
        //
        // FIXME(#6268) to support nested method calls, should be callee_id
        let callee_scope =
            region::Scope { id: call_expr.hir_id.local_id, data: region::ScopeData::Node };
        let callee_region = self.tcx.mk_region(ty::ReScope(callee_scope));

        debug!("callee_region={:?}", callee_region);

        for arg_expr in arg_exprs {
            debug!("argument: {:?}", arg_expr);

            // ensure that any regions appearing in the argument type are
            // valid for at least the lifetime of the function:
            self.type_of_node_must_outlive(
                infer::CallArg(arg_expr.span),
                arg_expr.hir_id,
                callee_region,
            );
        }

        // as loop above, but for receiver
        if let Some(r) = receiver {
            debug!("receiver: {:?}", r);
            self.type_of_node_must_outlive(infer::CallRcvr(r.span), r.hir_id, callee_region);
        }
    }

    /// Creates a temporary `MemCategorizationContext` and pass it to the closure.
    fn with_mc<F, R>(&self, f: F) -> R
    where
        F: for<'b> FnOnce(mc::MemCategorizationContext<'b, 'tcx>) -> R,
    {
        f(mc::MemCategorizationContext::new(
            &self.infcx,
            self.outlives_environment.param_env,
            self.body_owner,
            &self.tables.borrow(),
        ))
    }

    /// Invoked on any adjustments that occur. Checks that if this is a region pointer being
    /// dereferenced, the lifetime of the pointer includes the deref expr.
    fn constrain_adjustments(&mut self, expr: &hir::Expr<'_>) -> mc::McResult<mc::Place<'tcx>> {
        debug!("constrain_adjustments(expr={:?})", expr);

        let mut cmt = self.with_mc(|mc| mc.cat_expr_unadjusted(expr))?;

        let tables = self.tables.borrow();
        let adjustments = tables.expr_adjustments(&expr);
        if adjustments.is_empty() {
            return Ok(cmt);
        }

        debug!("constrain_adjustments: adjustments={:?}", adjustments);

        // If necessary, constrain destructors in the unadjusted form of this
        // expression.
        self.check_safety_of_rvalue_destructor_if_necessary(&cmt, expr.span);

        let expr_region = self.tcx.mk_region(ty::ReScope(region::Scope {
            id: expr.hir_id.local_id,
            data: region::ScopeData::Node,
        }));
        for adjustment in adjustments {
            debug!("constrain_adjustments: adjustment={:?}, cmt={:?}", adjustment, cmt);

            if let adjustment::Adjust::Deref(Some(deref)) = adjustment.kind {
                debug!("constrain_adjustments: overloaded deref: {:?}", deref);

                // Treat overloaded autoderefs as if an AutoBorrow adjustment
                // was applied on the base type, as that is always the case.
                let input = self
                    .tcx
                    .mk_ref(deref.region, ty::TypeAndMut { ty: cmt.ty, mutbl: deref.mutbl });
                let output = self.tcx.mk_ref(
                    deref.region,
                    ty::TypeAndMut { ty: adjustment.target, mutbl: deref.mutbl },
                );

                self.link_region(
                    expr.span,
                    deref.region,
                    ty::BorrowKind::from_mutbl(deref.mutbl),
                    &cmt,
                );

                // Specialized version of constrain_call.
                self.type_must_outlive(infer::CallRcvr(expr.span), input, expr_region);
                self.type_must_outlive(infer::CallReturn(expr.span), output, expr_region);
            }

            if let adjustment::Adjust::Borrow(ref autoref) = adjustment.kind {
                self.link_autoref(expr, &cmt, autoref);

                // Require that the resulting region encompasses
                // the current node.
                //
                // FIXME(#6268) remove to support nested method calls
                self.type_of_node_must_outlive(
                    infer::AutoBorrow(expr.span),
                    expr.hir_id,
                    expr_region,
                );
            }

            cmt = self.with_mc(|mc| mc.cat_expr_adjusted(expr, cmt, &adjustment))?;
        }

        Ok(cmt)
    }

    pub fn mk_subregion_due_to_dereference(
        &mut self,
        deref_span: Span,
        minimum_lifetime: ty::Region<'tcx>,
        maximum_lifetime: ty::Region<'tcx>,
    ) {
        self.sub_regions(infer::DerefPointer(deref_span), minimum_lifetime, maximum_lifetime)
    }

    fn check_safety_of_rvalue_destructor_if_necessary(
        &mut self,
        place: &mc::Place<'tcx>,
        span: Span,
    ) {
        if let mc::PlaceBase::Rvalue = place.base {
            if place.projections.is_empty() {
                let typ = self.resolve_type(place.ty);
                let body_id = self.body_id;
                let _ = dropck::check_drop_obligations(self, typ, span, body_id);
            }
        }
    }

    /// Invoked on any index expression that occurs. Checks that if this is a slice
    /// being indexed, the lifetime of the pointer includes the deref expr.
    fn constrain_index(&mut self, index_expr: &hir::Expr<'_>, indexed_ty: Ty<'tcx>) {
        debug!("constrain_index(index_expr=?, indexed_ty={}", self.ty_to_string(indexed_ty));

        let r_index_expr = ty::ReScope(region::Scope {
            id: index_expr.hir_id.local_id,
            data: region::ScopeData::Node,
        });
        if let ty::Ref(r_ptr, r_ty, _) = indexed_ty.kind {
            match r_ty.kind {
                ty::Slice(_) | ty::Str => {
                    self.sub_regions(
                        infer::IndexSlice(index_expr.span),
                        self.tcx.mk_region(r_index_expr),
                        r_ptr,
                    );
                }
                _ => {}
            }
        }
    }

    /// Guarantees that any lifetimes that appear in the type of the node `id` (after applying
    /// adjustments) are valid for at least `minimum_lifetime`.
    fn type_of_node_must_outlive(
        &mut self,
        origin: infer::SubregionOrigin<'tcx>,
        hir_id: hir::HirId,
        minimum_lifetime: ty::Region<'tcx>,
    ) {
        // Try to resolve the type.  If we encounter an error, then typeck
        // is going to fail anyway, so just stop here and let typeck
        // report errors later on in the writeback phase.
        let ty0 = self.resolve_node_type(hir_id);

        let ty = self
            .tables
            .borrow()
            .adjustments()
            .get(hir_id)
            .and_then(|adj| adj.last())
            .map_or(ty0, |adj| adj.target);
        let ty = self.resolve_type(ty);
        debug!(
            "constrain_regions_in_type_of_node(\
             ty={}, ty0={}, id={:?}, minimum_lifetime={:?})",
            ty, ty0, hir_id, minimum_lifetime
        );
        self.type_must_outlive(origin, ty, minimum_lifetime);
    }

    /// Adds constraints to inference such that `T: 'a` holds (or
    /// reports an error if it cannot).
    ///
    /// # Parameters
    ///
    /// - `origin`, the reason we need this constraint
    /// - `ty`, the type `T`
    /// - `region`, the region `'a`
    pub fn type_must_outlive(
        &self,
        origin: infer::SubregionOrigin<'tcx>,
        ty: Ty<'tcx>,
        region: ty::Region<'tcx>,
    ) {
        self.infcx.register_region_obligation(
            self.body_id,
            RegionObligation { sub_region: region, sup_type: ty, origin },
        );
    }

    /// Computes the guarantor for an expression `&base` and then ensures that the lifetime of the
    /// resulting pointer is linked to the lifetime of its guarantor (if any).
    fn link_addr_of(
        &mut self,
        expr: &hir::Expr<'_>,
        mutability: hir::Mutability,
        base: &hir::Expr<'_>,
    ) {
        debug!("link_addr_of(expr={:?}, base={:?})", expr, base);

        let cmt = ignore_err!(self.with_mc(|mc| mc.cat_expr(base)));

        debug!("link_addr_of: cmt={:?}", cmt);

        self.link_region_from_node_type(expr.span, expr.hir_id, mutability, &cmt);
    }

    /// Computes the guarantors for any ref bindings in a `let` and
    /// then ensures that the lifetime of the resulting pointer is
    /// linked to the lifetime of the initialization expression.
    fn link_local(&self, local: &hir::Local<'_>) {
        debug!("regionck::for_local()");
        let init_expr = match local.init {
            None => {
                return;
            }
            Some(ref expr) => &**expr,
        };
        let discr_cmt = ignore_err!(self.with_mc(|mc| mc.cat_expr(init_expr)));
        self.link_pattern(discr_cmt, &local.pat);
    }

    /// Computes the guarantors for any ref bindings in a match and
    /// then ensures that the lifetime of the resulting pointer is
    /// linked to the lifetime of its guarantor (if any).
    fn link_match(&self, discr: &hir::Expr<'_>, arms: &[hir::Arm<'_>]) {
        debug!("regionck::for_match()");
        let discr_cmt = ignore_err!(self.with_mc(|mc| mc.cat_expr(discr)));
        debug!("discr_cmt={:?}", discr_cmt);
        for arm in arms {
            self.link_pattern(discr_cmt.clone(), &arm.pat);
        }
    }

    /// Computes the guarantors for any ref bindings in a match and
    /// then ensures that the lifetime of the resulting pointer is
    /// linked to the lifetime of its guarantor (if any).
    fn link_fn_params(&self, params: &[hir::Param<'_>]) {
        for param in params {
            let param_ty = self.node_ty(param.hir_id);
            let param_cmt =
                self.with_mc(|mc| mc.cat_rvalue(param.hir_id, param.pat.span, param_ty));
            debug!("param_ty={:?} param_cmt={:?} param={:?}", param_ty, param_cmt, param);
            self.link_pattern(param_cmt, &param.pat);
        }
    }

    /// Link lifetimes of any ref bindings in `root_pat` to the pointers found
    /// in the discriminant, if needed.
    fn link_pattern(&self, discr_cmt: mc::Place<'tcx>, root_pat: &hir::Pat<'_>) {
        debug!("link_pattern(discr_cmt={:?}, root_pat={:?})", discr_cmt, root_pat);
        ignore_err!(self.with_mc(|mc| {
            mc.cat_pattern(discr_cmt, root_pat, |sub_cmt, hir::Pat { kind, span, hir_id }| {
                // `ref x` pattern
                if let PatKind::Binding(..) = kind {
                    if let Some(ty::BindByReference(mutbl)) =
                        mc.tables.extract_binding_mode(self.tcx.sess, *hir_id, *span)
                    {
                        self.link_region_from_node_type(*span, *hir_id, mutbl, &sub_cmt);
                    }
                }
            })
        }));
    }

    /// Link lifetime of borrowed pointer resulting from autoref to lifetimes in the value being
    /// autoref'd.
    fn link_autoref(
        &self,
        expr: &hir::Expr<'_>,
        expr_cmt: &mc::Place<'tcx>,
        autoref: &adjustment::AutoBorrow<'tcx>,
    ) {
        debug!("link_autoref(autoref={:?}, expr_cmt={:?})", autoref, expr_cmt);

        match *autoref {
            adjustment::AutoBorrow::Ref(r, m) => {
                self.link_region(expr.span, r, ty::BorrowKind::from_mutbl(m.into()), expr_cmt);
            }

            adjustment::AutoBorrow::RawPtr(m) => {
                let r = self.tcx.mk_region(ty::ReScope(region::Scope {
                    id: expr.hir_id.local_id,
                    data: region::ScopeData::Node,
                }));
                self.link_region(expr.span, r, ty::BorrowKind::from_mutbl(m), expr_cmt);
            }
        }
    }

    /// Like `link_region()`, except that the region is extracted from the type of `id`,
    /// which must be some reference (`&T`, `&str`, etc).
    fn link_region_from_node_type(
        &self,
        span: Span,
        id: hir::HirId,
        mutbl: hir::Mutability,
        cmt_borrowed: &mc::Place<'tcx>,
    ) {
        debug!(
            "link_region_from_node_type(id={:?}, mutbl={:?}, cmt_borrowed={:?})",
            id, mutbl, cmt_borrowed
        );

        let rptr_ty = self.resolve_node_type(id);
        if let ty::Ref(r, _, _) = rptr_ty.kind {
            debug!("rptr_ty={}", rptr_ty);
            self.link_region(span, r, ty::BorrowKind::from_mutbl(mutbl), cmt_borrowed);
        }
    }

    /// Informs the inference engine that `borrow_cmt` is being borrowed with
    /// kind `borrow_kind` and lifetime `borrow_region`.
    /// In order to ensure borrowck is satisfied, this may create constraints
    /// between regions, as explained in `link_reborrowed_region()`.
    fn link_region(
        &self,
        span: Span,
        borrow_region: ty::Region<'tcx>,
        borrow_kind: ty::BorrowKind,
        borrow_place: &mc::Place<'tcx>,
    ) {
        let origin = infer::DataBorrowed(borrow_place.ty, span);
        self.type_must_outlive(origin, borrow_place.ty, borrow_region);

        for pointer_ty in borrow_place.deref_tys() {
            debug!(
                "link_region(borrow_region={:?}, borrow_kind={:?}, pointer_ty={:?})",
                borrow_region, borrow_kind, borrow_place
            );
            match pointer_ty.kind {
                ty::RawPtr(_) => return,
                ty::Ref(ref_region, _, ref_mutability) => {
                    if self.link_reborrowed_region(span, borrow_region, ref_region, ref_mutability)
                    {
                        return;
                    }
                }
                _ => assert!(pointer_ty.is_box(), "unexpected built-in deref type {}", pointer_ty),
            }
        }
        if let mc::PlaceBase::Upvar(upvar_id) = borrow_place.base {
            self.link_upvar_region(span, borrow_region, upvar_id);
        }
    }

    /// This is the most complicated case: the path being borrowed is
    /// itself the referent of a borrowed pointer. Let me give an
    /// example fragment of code to make clear(er) the situation:
    ///
    ///    let r: &'a mut T = ...;  // the original reference "r" has lifetime 'a
    ///    ...
    ///    &'z *r                   // the reborrow has lifetime 'z
    ///
    /// Now, in this case, our primary job is to add the inference
    /// constraint that `'z <= 'a`. Given this setup, let's clarify the
    /// parameters in (roughly) terms of the example:
    ///
    /// ```plain,ignore (pseudo-Rust)
    ///     A borrow of: `& 'z bk * r` where `r` has type `& 'a bk T`
    ///     borrow_region   ^~                 ref_region    ^~
    ///     borrow_kind        ^~               ref_kind        ^~
    ///     ref_cmt                 ^
    /// ```
    ///
    /// Here `bk` stands for some borrow-kind (e.g., `mut`, `uniq`, etc).
    ///
    /// There is a complication beyond the simple scenario I just painted: there
    /// may in fact be more levels of reborrowing. In the example, I said the
    /// borrow was like `&'z *r`, but it might in fact be a borrow like
    /// `&'z **q` where `q` has type `&'a &'b mut T`. In that case, we want to
    /// ensure that `'z <= 'a` and `'z <= 'b`.
    ///
    /// The return value of this function indicates whether we *don't* need to
    /// the recurse to the next reference up.
    ///
    /// This is explained more below.
    fn link_reborrowed_region(
        &self,
        span: Span,
        borrow_region: ty::Region<'tcx>,
        ref_region: ty::Region<'tcx>,
        ref_mutability: hir::Mutability,
    ) -> bool {
        debug!("link_reborrowed_region: {:?} <= {:?}", borrow_region, ref_region);
        self.sub_regions(infer::Reborrow(span), borrow_region, ref_region);

        // Decide whether we need to recurse and link any regions within
        // the `ref_cmt`. This is concerned for the case where the value
        // being reborrowed is in fact a borrowed pointer found within
        // another borrowed pointer. For example:
        //
        //    let p: &'b &'a mut T = ...;
        //    ...
        //    &'z **p
        //
        // What makes this case particularly tricky is that, if the data
        // being borrowed is a `&mut` or `&uniq` borrow, borrowck requires
        // not only that `'z <= 'a`, (as before) but also `'z <= 'b`
        // (otherwise the user might mutate through the `&mut T` reference
        // after `'b` expires and invalidate the borrow we are looking at
        // now).
        //
        // So let's re-examine our parameters in light of this more
        // complicated (possible) scenario:
        //
        //     A borrow of: `& 'z bk * * p` where `p` has type `&'b bk & 'a bk T`
        //     borrow_region   ^~                 ref_region             ^~
        //     borrow_kind        ^~               ref_kind                 ^~
        //     ref_cmt                 ^~~
        //
        // (Note that since we have not examined `ref_cmt.cat`, we don't
        // know whether this scenario has occurred; but I wanted to show
        // how all the types get adjusted.)
        match ref_mutability {
            hir::Mutability::Not => {
                // The reference being reborrowed is a shareable ref of
                // type `&'a T`. In this case, it doesn't matter where we
                // *found* the `&T` pointer, the memory it references will
                // be valid and immutable for `'a`. So we can stop here.
                true
            }

            hir::Mutability::Mut => {
                // The reference being reborrowed is either an `&mut T`. This is
                // the case where recursion is needed.
                false
            }
        }
    }

    /// An upvar may be behind up to 2 references:
    ///
    /// * One can come from the reference to a "by-reference" upvar.
    /// * Another one can come from the reference to the closure itself if it's
    ///   a `FnMut` or `Fn` closure.
    ///
    /// This function links the lifetimes of those references to the lifetime
    /// of the borrow that's provided. See [link_reborrowed_region] for some
    /// more explanation of this in the general case.
    ///
    /// We also supply a *cause*, and in this case we set the cause to
    /// indicate that the reference being "reborrowed" is itself an upvar. This
    /// provides a nicer error message should something go wrong.
    fn link_upvar_region(
        &self,
        span: Span,
        borrow_region: ty::Region<'tcx>,
        upvar_id: ty::UpvarId,
    ) {
        debug!("link_upvar_region(borrorw_region={:?}, upvar_id={:?}", borrow_region, upvar_id);
        // A by-reference upvar can't be borrowed for longer than the
        // upvar is borrowed from the environment.
        match self.tables.borrow().upvar_capture(upvar_id) {
            ty::UpvarCapture::ByRef(upvar_borrow) => {
                self.sub_regions(
                    infer::ReborrowUpvar(span, upvar_id),
                    borrow_region,
                    upvar_borrow.region,
                );
                if let ty::ImmBorrow = upvar_borrow.kind {
                    debug!("link_upvar_region: capture by shared ref");
                    return;
                }
            }
            ty::UpvarCapture::ByValue => {}
        }
        let fn_hir_id = self.tcx.hir().local_def_id_to_hir_id(upvar_id.closure_expr_id);
        let ty = self.resolve_node_type(fn_hir_id);
        debug!("link_upvar_region: ty={:?}", ty);

        // A closure capture can't be borrowed for longer than the
        // reference to the closure.
        if let ty::Closure(_, substs) = ty.kind {
            match self.infcx.closure_kind(substs) {
                Some(ty::ClosureKind::Fn) | Some(ty::ClosureKind::FnMut) => {
                    // Region of environment pointer
                    let env_region = self.tcx.mk_region(ty::ReFree(ty::FreeRegion {
                        scope: upvar_id.closure_expr_id.to_def_id(),
                        bound_region: ty::BrEnv,
                    }));
                    self.sub_regions(
                        infer::ReborrowUpvar(span, upvar_id),
                        borrow_region,
                        env_region,
                    );
                }
                Some(ty::ClosureKind::FnOnce) => {}
                None => {
                    span_bug!(span, "Have not inferred closure kind before regionck");
                }
            }
        }
    }

    /// Checks that the values provided for type/region arguments in a given
    /// expression are well-formed and in-scope.
    fn substs_wf_in_scope(
        &mut self,
        origin: infer::ParameterOrigin,
        substs: SubstsRef<'tcx>,
        expr_span: Span,
        expr_region: ty::Region<'tcx>,
    ) {
        debug!(
            "substs_wf_in_scope(substs={:?}, \
             expr_region={:?}, \
             origin={:?}, \
             expr_span={:?})",
            substs, expr_region, origin, expr_span
        );

        let origin = infer::ParameterInScope(origin, expr_span);

        for kind in substs {
            match kind.unpack() {
                GenericArgKind::Lifetime(lt) => {
                    self.sub_regions(origin.clone(), expr_region, lt);
                }
                GenericArgKind::Type(ty) => {
                    let ty = self.resolve_type(ty);
                    self.type_must_outlive(origin.clone(), ty, expr_region);
                }
                GenericArgKind::Const(_) => {
                    // Const parameters don't impose constraints.
                }
            }
        }
    }
}
