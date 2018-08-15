// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::hir;
use rustc::mir::{self, BindingForm, ClearCrossCrate, Local, Location, Mir};
use rustc::mir::{Mutability, Place, Projection, ProjectionElem, Static};
use rustc::ty::{self, TyCtxt};
use rustc_data_structures::indexed_vec::Idx;
use syntax_pos::Span;

use borrow_check::MirBorrowckCtxt;
use util::borrowck_errors::{BorrowckErrors, Origin};
use util::collect_writes::FindAssignments;
use util::suggest_ref_mut;

#[derive(Copy, Clone, Debug)]
pub(super) enum AccessKind {
    MutableBorrow,
    Mutate,
    Move,
}

impl<'a, 'gcx, 'tcx> MirBorrowckCtxt<'a, 'gcx, 'tcx> {
    pub(super) fn report_mutability_error(
        &mut self,
        access_place: &Place<'tcx>,
        span: Span,
        the_place_err: &Place<'tcx>,
        error_access: AccessKind,
        location: Location,
    ) {
        debug!(
            "report_mutability_error(\
                access_place={:?}, span={:?}, the_place_err={:?}, error_access={:?}, location={:?},\
            )",
            access_place, span, the_place_err, error_access, location,
        );

        let mut err;
        let item_msg;
        let reason;
        let access_place_desc = self.describe_place(access_place);
        debug!("report_mutability_error: access_place_desc={:?}", access_place_desc);

        match the_place_err {
            Place::Local(local) => {
                item_msg = format!("`{}`", access_place_desc.unwrap());
                if let Place::Local(_) = access_place {
                    reason = ", as it is not declared as mutable".to_string();
                } else {
                    let name = self.mir.local_decls[*local]
                        .name
                        .expect("immutable unnamed local");
                    reason = format!(", as `{}` is not declared as mutable", name);
                }
            }

            Place::Projection(box Projection {
                base,
                elem: ProjectionElem::Field(upvar_index, _),
            }) => {
                debug_assert!(is_closure_or_generator(
                    base.ty(self.mir, self.tcx).to_ty(self.tcx)
                ));

                item_msg = format!("`{}`", access_place_desc.unwrap());
                if access_place.is_upvar_field_projection(self.mir, &self.tcx).is_some() {
                    reason = ", as it is not declared as mutable".to_string();
                } else {
                    let name = self.mir.upvar_decls[upvar_index.index()].debug_name;
                    reason = format!(", as `{}` is not declared as mutable", name);
                }
            }

            Place::Projection(box Projection {
                base,
                elem: ProjectionElem::Deref,
            }) => {
                if *base == Place::Local(Local::new(1)) && !self.mir.upvar_decls.is_empty() {
                    item_msg = format!("`{}`", access_place_desc.unwrap());
                    debug_assert!(self.mir.local_decls[Local::new(1)].ty.is_region_ptr());
                    debug_assert!(is_closure_or_generator(
                        the_place_err.ty(self.mir, self.tcx).to_ty(self.tcx)
                    ));

                    reason = if access_place.is_upvar_field_projection(self.mir,
                                                                       &self.tcx).is_some() {
                        ", as it is a captured variable in a `Fn` closure".to_string()
                    } else {
                        ", as `Fn` closures cannot mutate their captured variables".to_string()
                    }
                } else if {
                    if let Place::Local(local) = *base {
                        if let Some(ClearCrossCrate::Set(BindingForm::RefForGuard))
                            = self.mir.local_decls[local].is_user_variable {
                                true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } {
                    item_msg = format!("`{}`", access_place_desc.unwrap());
                    reason = ", as it is immutable for the pattern guard".to_string();
                } else {
                    let pointer_type =
                        if base.ty(self.mir, self.tcx).to_ty(self.tcx).is_region_ptr() {
                            "`&` reference"
                        } else {
                            "`*const` pointer"
                        };
                    if let Some(desc) = access_place_desc {
                        item_msg = format!("`{}`", desc);
                        reason = match error_access {
                            AccessKind::Move |
                            AccessKind::Mutate => format!(" which is behind a {}", pointer_type),
                            AccessKind::MutableBorrow => {
                                format!(", as it is behind a {}", pointer_type)
                            }
                        }
                    } else {
                        item_msg = format!("data in a {}", pointer_type);
                        reason = "".to_string();
                    }
                }
            }

            Place::Promoted(_) => unreachable!(),

            Place::Static(box Static { def_id, ty: _ }) => {
                if let Place::Static(_) = access_place {
                    item_msg = format!("immutable static item `{}`", access_place_desc.unwrap());
                    reason = "".to_string();
                } else {
                    item_msg = format!("`{}`", access_place_desc.unwrap());
                    let static_name = &self.tcx.item_name(*def_id);
                    reason = format!(", as `{}` is an immutable static item", static_name);
                }
            }

            Place::Projection(box Projection {
                base: _,
                elem: ProjectionElem::Index(_),
            })
            | Place::Projection(box Projection {
                base: _,
                elem: ProjectionElem::ConstantIndex { .. },
            })
            | Place::Projection(box Projection {
                base: _,
                elem: ProjectionElem::Subslice { .. },
            })
            | Place::Projection(box Projection {
                base: _,
                elem: ProjectionElem::Downcast(..),
            }) => bug!("Unexpected immutable place."),
        }

        debug!("report_mutability_error: item_msg={:?}, reason={:?}", item_msg, reason);

        // `act` and `acted_on` are strings that let us abstract over
        // the verbs used in some diagnostic messages.
        let act;
        let acted_on;

        let span = match error_access {
            AccessKind::Move => {
                err = self.tcx
                    .cannot_move_out_of(span, &(item_msg + &reason), Origin::Mir);
                act = "move";
                acted_on = "moved";
                span
            }
            AccessKind::Mutate => {
                err = self.tcx
                    .cannot_assign(span, &(item_msg + &reason), Origin::Mir);
                act = "assign";
                acted_on = "written";
                span
            }
            AccessKind::MutableBorrow => {
                act = "borrow as mutable";
                acted_on = "borrowed as mutable";

                let borrow_spans = self.borrow_spans(span, location);
                let borrow_span = borrow_spans.args_or_use();
                err = self.tcx.cannot_borrow_path_as_mutable_because(
                    borrow_span,
                    &item_msg,
                    &reason,
                    Origin::Mir,
                );
                borrow_spans.var_span_label(
                    &mut err,
                    format!(
                        "mutable borrow occurs due to use of `{}` in closure",
                        // always Some() if the message is printed.
                        self.describe_place(access_place).unwrap_or(String::new()),
                    )
                );
                borrow_span
            }
        };

        debug!("report_mutability_error: act={:?}, acted_on={:?}", act, acted_on);

        match the_place_err {
            // We want to suggest users use `let mut` for local (user
            // variable) mutations...
            Place::Local(local) if self.mir.local_decls[*local].can_be_made_mutable() => {
                // ... but it doesn't make sense to suggest it on
                // variables that are `ref x`, `ref mut x`, `&self`,
                // or `&mut self` (such variables are simply not
                // mutable).
                let local_decl = &self.mir.local_decls[*local];
                assert_eq!(local_decl.mutability, Mutability::Not);

                err.span_label(span, format!("cannot {ACT}", ACT = act));
                err.span_suggestion(
                    local_decl.source_info.span,
                    "consider changing this to be mutable",
                    format!("mut {}", local_decl.name.unwrap()),
                );
            }

            // Also suggest adding mut for upvars
            Place::Projection(box Projection {
                base,
                elem: ProjectionElem::Field(upvar_index, _),
            }) => {
                debug_assert!(is_closure_or_generator(
                    base.ty(self.mir, self.tcx).to_ty(self.tcx)
                ));

                err.span_label(span, format!("cannot {ACT}", ACT = act));

                let upvar_hir_id = self.mir.upvar_decls[upvar_index.index()]
                    .var_hir_id
                    .assert_crate_local();
                let upvar_node_id = self.tcx.hir.hir_to_node_id(upvar_hir_id);
                if let Some(hir::map::NodeBinding(pat)) = self.tcx.hir.find(upvar_node_id) {
                    if let hir::PatKind::Binding(
                        hir::BindingAnnotation::Unannotated,
                        _,
                        upvar_ident,
                        _,
                    ) = pat.node
                    {
                        err.span_suggestion(
                            upvar_ident.span,
                            "consider changing this to be mutable",
                            format!("mut {}", upvar_ident.name),
                        );
                    }
                }
            }

            // complete hack to approximate old AST-borrowck
            // diagnostic: if the span starts with a mutable borrow of
            // a local variable, then just suggest the user remove it.
            Place::Local(_)
                if {
                    if let Ok(snippet) = self.tcx.sess.codemap().span_to_snippet(span) {
                        snippet.starts_with("&mut ")
                    } else {
                        false
                    }
                } =>
            {
                err.span_label(span, format!("cannot {ACT}", ACT = act));
                err.span_label(span, "try removing `&mut` here");
            }

            Place::Projection(box Projection {
                base: Place::Local(local),
                elem: ProjectionElem::Deref,
            }) if {
                if let Some(ClearCrossCrate::Set(BindingForm::RefForGuard)) =
                    self.mir.local_decls[*local].is_user_variable
                {
                    true
                } else {
                    false
                }
            } =>
            {
                err.span_label(span, format!("cannot {ACT}", ACT = act));
                err.note(
                    "variables bound in patterns are immutable until the end of the pattern guard",
                );
            }

            // We want to point out when a `&` can be readily replaced
            // with an `&mut`.
            //
            // FIXME: can this case be generalized to work for an
            // arbitrary base for the projection?
            Place::Projection(box Projection {
                base: Place::Local(local),
                elem: ProjectionElem::Deref,
            }) if self.mir.local_decls[*local].is_user_variable.is_some() =>
            {
                let local_decl = &self.mir.local_decls[*local];
                let suggestion = match local_decl.is_user_variable.as_ref().unwrap() {
                    ClearCrossCrate::Set(mir::BindingForm::ImplicitSelf) => {
                        Some(suggest_ampmut_self(self.tcx, local_decl))
                    }

                    ClearCrossCrate::Set(mir::BindingForm::Var(mir::VarBindingForm {
                        binding_mode: ty::BindingMode::BindByValue(_),
                        opt_ty_info,
                        ..
                    })) => Some(suggest_ampmut(
                        self.tcx,
                        self.mir,
                        *local,
                        local_decl,
                        *opt_ty_info,
                    )),

                    ClearCrossCrate::Set(mir::BindingForm::Var(mir::VarBindingForm {
                        binding_mode: ty::BindingMode::BindByReference(_),
                        ..
                    })) => suggest_ref_mut(self.tcx, local_decl.source_info.span),

                    //
                    ClearCrossCrate::Set(mir::BindingForm::RefForGuard) => unreachable!(),

                    ClearCrossCrate::Clear => bug!("saw cleared local state"),
                };

                let (pointer_sigil, pointer_desc) = if local_decl.ty.is_region_ptr() {
                    ("&", "reference")
                } else {
                    ("*const", "pointer")
                };

                if let Some((err_help_span, suggested_code)) = suggestion {
                    err.span_suggestion(
                        err_help_span,
                        &format!("consider changing this to be a mutable {}", pointer_desc),
                        suggested_code,
                    );
                }

                if let Some(name) = local_decl.name {
                    err.span_label(
                        span,
                        format!(
                            "`{NAME}` is a `{SIGIL}` {DESC}, \
                             so the data it refers to cannot be {ACTED_ON}",
                            NAME = name,
                            SIGIL = pointer_sigil,
                            DESC = pointer_desc,
                            ACTED_ON = acted_on
                        ),
                    );
                } else {
                    err.span_label(
                        span,
                        format!(
                            "cannot {ACT} through `{SIGIL}` {DESC}",
                            ACT = act,
                            SIGIL = pointer_sigil,
                            DESC = pointer_desc
                        ),
                    );
                }
            }

            Place::Projection(box Projection {
                base,
                elem: ProjectionElem::Deref,
            }) if *base == Place::Local(Local::new(1)) && !self.mir.upvar_decls.is_empty() =>
            {
                err.span_label(span, format!("cannot {ACT}", ACT = act));
                err.span_help(
                    self.mir.span,
                    "consider changing this to accept closures that implement `FnMut`"
                );
            }

            _ => {
                err.span_label(span, format!("cannot {ACT}", ACT = act));
            }
        }

        err.buffer(&mut self.errors_buffer);
    }
}

fn suggest_ampmut_self<'cx, 'gcx, 'tcx>(
    tcx: TyCtxt<'cx, 'gcx, 'tcx>,
    local_decl: &mir::LocalDecl<'tcx>,
) -> (Span, String) {
    let sp = local_decl.source_info.span;
    (sp, match tcx.sess.codemap().span_to_snippet(sp) {
        Ok(snippet) => {
            let lt_pos = snippet.find('\'');
            if let Some(lt_pos) = lt_pos {
                format!("&{}mut self", &snippet[lt_pos..snippet.len() - 4])
            } else {
                "&mut self".to_string()
            }
        }
        _ => "&mut self".to_string()
    })
}

// When we want to suggest a user change a local variable to be a `&mut`, there
// are three potential "obvious" things to highlight:
//
// let ident [: Type] [= RightHandSideExpression];
//     ^^^^^    ^^^^     ^^^^^^^^^^^^^^^^^^^^^^^
//     (1.)     (2.)              (3.)
//
// We can always fallback on highlighting the first. But chances are good that
// the user experience will be better if we highlight one of the others if possible;
// for example, if the RHS is present and the Type is not, then the type is going to
// be inferred *from* the RHS, which means we should highlight that (and suggest
// that they borrow the RHS mutably).
//
// This implementation attempts to emulate AST-borrowck prioritization
// by trying (3.), then (2.) and finally falling back on (1.).
fn suggest_ampmut<'cx, 'gcx, 'tcx>(
    tcx: TyCtxt<'cx, 'gcx, 'tcx>,
    mir: &Mir<'tcx>,
    local: Local,
    local_decl: &mir::LocalDecl<'tcx>,
    opt_ty_info: Option<Span>,
) -> (Span, String) {
    let locations = mir.find_assignments(local);
    if locations.len() > 0 {
        let assignment_rhs_span = mir.source_info(locations[0]).span;
        if let Ok(src) = tcx.sess.codemap().span_to_snippet(assignment_rhs_span) {
            if let (true, Some(ws_pos)) = (
                src.starts_with("&'"),
                src.find(|c: char| -> bool { c.is_whitespace() }),
            ) {
                let lt_name = &src[1..ws_pos];
                let ty = &src[ws_pos..];
                return (assignment_rhs_span, format!("&{} mut {}", lt_name, ty));
            } else if src.starts_with('&') {
                let borrowed_expr = src[1..].to_string();
                return (assignment_rhs_span, format!("&mut {}", borrowed_expr));
            }
        }
    }

    let highlight_span = match opt_ty_info {
        // if this is a variable binding with an explicit type,
        // try to highlight that for the suggestion.
        Some(ty_span) => ty_span,

        // otherwise, just highlight the span associated with
        // the (MIR) LocalDecl.
        None => local_decl.source_info.span,
    };

    if let Ok(src) = tcx.sess.codemap().span_to_snippet(highlight_span) {
        if let (true, Some(ws_pos)) = (
            src.starts_with("&'"),
            src.find(|c: char| -> bool { c.is_whitespace() }),
        ) {
            let lt_name = &src[1..ws_pos];
            let ty = &src[ws_pos..];
            return (highlight_span, format!("&{} mut{}", lt_name, ty));
        }
    }

    let ty_mut = local_decl.ty.builtin_deref(true).unwrap();
    assert_eq!(ty_mut.mutbl, hir::MutImmutable);
    (highlight_span,
     if local_decl.ty.is_region_ptr() {
         format!("&mut {}", ty_mut.ty)
     } else {
         format!("*mut {}", ty_mut.ty)
     })
}

fn is_closure_or_generator(ty: ty::Ty) -> bool {
    ty.is_closure() || ty.is_generator()
}
