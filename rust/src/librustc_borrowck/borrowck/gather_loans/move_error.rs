// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use borrowck::BorrowckCtxt;
use rustc::middle::mem_categorization as mc;
use rustc::middle::mem_categorization::Categorization;
use rustc::middle::mem_categorization::InteriorOffsetKind as Kind;
use rustc::ty;
use syntax::ast;
use syntax::codemap;
use syntax::errors::DiagnosticBuilder;
use rustc_front::hir;

pub struct MoveErrorCollector<'tcx> {
    errors: Vec<MoveError<'tcx>>
}

impl<'tcx> MoveErrorCollector<'tcx> {
    pub fn new() -> MoveErrorCollector<'tcx> {
        MoveErrorCollector {
            errors: Vec::new()
        }
    }

    pub fn add_error(&mut self, error: MoveError<'tcx>) {
        self.errors.push(error);
    }

    pub fn report_potential_errors<'a>(&self, bccx: &BorrowckCtxt<'a, 'tcx>) {
        report_move_errors(bccx, &self.errors)
    }
}

pub struct MoveError<'tcx> {
    move_from: mc::cmt<'tcx>,
    move_to: Option<MoveSpanAndPath>
}

impl<'tcx> MoveError<'tcx> {
    pub fn with_move_info(move_from: mc::cmt<'tcx>,
                          move_to: Option<MoveSpanAndPath>)
                          -> MoveError<'tcx> {
        MoveError {
            move_from: move_from,
            move_to: move_to,
        }
    }
}

#[derive(Clone)]
pub struct MoveSpanAndPath {
    pub span: codemap::Span,
    pub name: ast::Name,
}

pub struct GroupedMoveErrors<'tcx> {
    move_from: mc::cmt<'tcx>,
    move_to_places: Vec<MoveSpanAndPath>
}

fn report_move_errors<'a, 'tcx>(bccx: &BorrowckCtxt<'a, 'tcx>,
                                errors: &Vec<MoveError<'tcx>>) {
    let grouped_errors = group_errors_with_same_origin(errors);
    for error in &grouped_errors {
        let mut err = report_cannot_move_out_of(bccx, error.move_from.clone());
        let mut is_first_note = true;
        for move_to in &error.move_to_places {
            note_move_destination(&mut err, move_to.span,
                                  move_to.name, is_first_note);
            is_first_note = false;
        }
        err.emit();
    }
}

fn group_errors_with_same_origin<'tcx>(errors: &Vec<MoveError<'tcx>>)
                                       -> Vec<GroupedMoveErrors<'tcx>> {
    let mut grouped_errors = Vec::new();
    for error in errors {
        append_to_grouped_errors(&mut grouped_errors, error)
    }
    return grouped_errors;

    fn append_to_grouped_errors<'tcx>(grouped_errors: &mut Vec<GroupedMoveErrors<'tcx>>,
                                      error: &MoveError<'tcx>) {
        let move_from_id = error.move_from.id;
        debug!("append_to_grouped_errors(move_from_id={})", move_from_id);
        let move_to = if error.move_to.is_some() {
            vec!(error.move_to.clone().unwrap())
        } else {
            Vec::new()
        };
        for ge in &mut *grouped_errors {
            if move_from_id == ge.move_from.id && error.move_to.is_some() {
                debug!("appending move_to to list");
                ge.move_to_places.extend(move_to);
                return
            }
        }
        debug!("found a new move from location");
        grouped_errors.push(GroupedMoveErrors {
            move_from: error.move_from.clone(),
            move_to_places: move_to
        })
    }
}

// (keep in sync with gather_moves::check_and_get_illegal_move_origin )
fn report_cannot_move_out_of<'a, 'tcx>(bccx: &BorrowckCtxt<'a, 'tcx>,
                                       move_from: mc::cmt<'tcx>)
                                       -> DiagnosticBuilder<'a> {
    match move_from.cat {
        Categorization::Deref(_, _, mc::BorrowedPtr(..)) |
        Categorization::Deref(_, _, mc::Implicit(..)) |
        Categorization::Deref(_, _, mc::UnsafePtr(..)) |
        Categorization::StaticItem => {
            struct_span_err!(bccx, move_from.span, E0507,
                             "cannot move out of {}",
                             move_from.descriptive_string(bccx.tcx))
        }

        Categorization::Interior(ref b, mc::InteriorElement(Kind::Index, _)) => {
            let expr = bccx.tcx.map.expect_expr(move_from.id);
            if let hir::ExprIndex(..) = expr.node {
                struct_span_err!(bccx, move_from.span, E0508,
                                 "cannot move out of type `{}`, \
                                  a non-copy fixed-size array",
                                 b.ty)
            } else {
                bccx.span_bug(move_from.span, "this path should not cause illegal move");
                unreachable!();
            }
        }

        Categorization::Downcast(ref b, _) |
        Categorization::Interior(ref b, mc::InteriorField(_)) => {
            match b.ty.sty {
                ty::TyStruct(def, _) |
                ty::TyEnum(def, _) if def.has_dtor() => {
                    struct_span_err!(bccx, move_from.span, E0509,
                                     "cannot move out of type `{}`, \
                                      which defines the `Drop` trait",
                                     b.ty)
                },
                _ => {
                    bccx.span_bug(move_from.span, "this path should not cause illegal move");
                    unreachable!();
                }
            }
        }
        _ => {
            bccx.span_bug(move_from.span, "this path should not cause illegal move");
            unreachable!();
        }
    }
}

fn note_move_destination(err: &mut DiagnosticBuilder,
                         move_to_span: codemap::Span,
                         pat_name: ast::Name,
                         is_first_note: bool) {
    if is_first_note {
        err.span_note(
            move_to_span,
            "attempting to move value to here");
        err.fileline_help(
            move_to_span,
            &format!("to prevent the move, \
                      use `ref {0}` or `ref mut {0}` to capture value by \
                      reference",
                     pat_name));
    } else {
        err.span_note(move_to_span,
                      &format!("and here (use `ref {0}` or `ref mut {0}`)",
                               pat_name));
    }
}
