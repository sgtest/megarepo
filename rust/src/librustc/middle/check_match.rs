// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::const_eval::{compare_const_vals, const_bool, const_float, const_nil, const_val};
use middle::const_eval::{const_expr_to_pat, eval_const_expr, lookup_const_by_id};
use middle::def::*;
use middle::expr_use_visitor::{ConsumeMode, Delegate, ExprUseVisitor, Init};
use middle::expr_use_visitor::{JustWrite, LoanCause, MutateMode};
use middle::expr_use_visitor::{WriteAndRead};
use middle::mem_categorization::cmt;
use middle::pat_util::*;
use middle::ty::*;
use middle::ty;
use std::fmt;
use std::gc::{Gc, GC};
use std::iter::AdditiveIterator;
use std::iter::range_inclusive;
use syntax::ast::*;
use syntax::ast_util::walk_pat;
use syntax::codemap::{Span, Spanned, DUMMY_SP};
use syntax::fold::{Folder, noop_fold_pat};
use syntax::print::pprust::pat_to_string;
use syntax::parse::token;
use syntax::visit;
use syntax::visit::{Visitor, FnKind};
use util::ppaux::ty_to_string;

struct Matrix(Vec<Vec<Gc<Pat>>>);

/// Pretty-printer for matrices of patterns, example:
/// ++++++++++++++++++++++++++
/// + _     + []             +
/// ++++++++++++++++++++++++++
/// + true  + [First]        +
/// ++++++++++++++++++++++++++
/// + true  + [Second(true)] +
/// ++++++++++++++++++++++++++
/// + false + [_]            +
/// ++++++++++++++++++++++++++
/// + _     + [_, _, ..tail] +
/// ++++++++++++++++++++++++++
impl fmt::Show for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "\n"));

        let &Matrix(ref m) = self;
        let pretty_printed_matrix: Vec<Vec<String>> = m.iter().map(|row| {
            row.iter()
               .map(|&pat| pat_to_string(&*pat))
               .collect::<Vec<String>>()
        }).collect();

        let column_count = m.iter().map(|row| row.len()).max().unwrap_or(0u);
        assert!(m.iter().all(|row| row.len() == column_count));
        let column_widths: Vec<uint> = range(0, column_count).map(|col| {
            pretty_printed_matrix.iter().map(|row| row.get(col).len()).max().unwrap_or(0u)
        }).collect();

        let total_width = column_widths.iter().map(|n| *n).sum() + column_count * 3 + 1;
        let br = String::from_char(total_width, '+');
        try!(write!(f, "{}\n", br));
        for row in pretty_printed_matrix.move_iter() {
            try!(write!(f, "+"));
            for (column, pat_str) in row.move_iter().enumerate() {
                try!(write!(f, " "));
                f.width = Some(*column_widths.get(column));
                try!(f.pad(pat_str.as_slice()));
                try!(write!(f, " +"));
            }
            try!(write!(f, "\n"));
            try!(write!(f, "{}\n", br));
        }
        Ok(())
    }
}

impl FromIterator<Vec<Gc<Pat>>> for Matrix {
    fn from_iter<T: Iterator<Vec<Gc<Pat>>>>(mut iterator: T) -> Matrix {
        Matrix(iterator.collect())
    }
}

pub struct MatchCheckCtxt<'a> {
    pub tcx: &'a ty::ctxt
}

#[deriving(Clone, PartialEq)]
pub enum Constructor {
    /// The constructor of all patterns that don't vary by constructor,
    /// e.g. struct patterns and fixed-length arrays.
    Single,
    /// Enum variants.
    Variant(DefId),
    /// Literal values.
    ConstantValue(const_val),
    /// Ranges of literal values (2..5).
    ConstantRange(const_val, const_val),
    /// Array patterns of length n.
    Slice(uint)
}

#[deriving(Clone, PartialEq)]
enum Usefulness {
    Useful,
    UsefulWithWitness(Vec<Gc<Pat>>),
    NotUseful
}

enum WitnessPreference {
    ConstructWitness,
    LeaveOutWitness
}

impl<'a> Visitor<()> for MatchCheckCtxt<'a> {
    fn visit_expr(&mut self, ex: &Expr, _: ()) {
        check_expr(self, ex);
    }
    fn visit_local(&mut self, l: &Local, _: ()) {
        check_local(self, l);
    }
    fn visit_fn(&mut self, fk: &FnKind, fd: &FnDecl, b: &Block, s: Span, _: NodeId, _: ()) {
        check_fn(self, fk, fd, b, s);
    }
}

pub fn check_crate(tcx: &ty::ctxt, krate: &Crate) {
    let mut cx = MatchCheckCtxt { tcx: tcx };
    visit::walk_crate(&mut cx, krate, ());
    tcx.sess.abort_if_errors();
}

fn check_expr(cx: &mut MatchCheckCtxt, ex: &Expr) {
    visit::walk_expr(cx, ex, ());
    match ex.node {
        ExprMatch(scrut, ref arms) => {
            // First, check legality of move bindings.
            for arm in arms.iter() {
                check_legality_of_move_bindings(cx,
                                                arm.guard.is_some(),
                                                arm.pats.as_slice());
                for pat in arm.pats.iter() {
                    check_legality_of_bindings_in_at_patterns(cx, &**pat);
                }
            }

            // Second, if there is a guard on each arm, make sure it isn't
            // assigning or borrowing anything mutably.
            for arm in arms.iter() {
                match arm.guard {
                    Some(guard) => check_for_mutation_in_guard(cx, &*guard),
                    None => {}
                }
            }

            let mut static_inliner = StaticInliner::new(cx.tcx);
            let inlined_arms = arms
                .iter()
                .map(|arm| Arm {
                    pats: arm.pats.iter().map(|pat| {
                        static_inliner.fold_pat(*pat)
                    }).collect(),
                    ..arm.clone()
                })
                .collect::<Vec<Arm>>();

            if static_inliner.failed {
                return;
            }

            // Third, check if there are any references to NaN that we should warn about.
            check_for_static_nan(cx, inlined_arms.as_slice());

            // Fourth, check for unreachable arms.
            check_arms(cx, inlined_arms.as_slice());

            // Finally, check if the whole match expression is exhaustive.
            // Check for empty enum, because is_useful only works on inhabited types.
            let pat_ty = node_id_to_type(cx.tcx, scrut.id);
            if inlined_arms.is_empty() {
                if !type_is_empty(cx.tcx, pat_ty) {
                    // We know the type is inhabited, so this must be wrong
                    span_err!(cx.tcx.sess, ex.span, E0002,
                        "non-exhaustive patterns: type {} is non-empty",
                        ty_to_string(cx.tcx, pat_ty)
                    );
                }
                // If the type *is* empty, it's vacuously exhaustive
                return;
            }

            let matrix: Matrix = inlined_arms
                .move_iter()
                .filter(|arm| arm.guard.is_none())
                .flat_map(|arm| arm.pats.move_iter())
                .map(|pat| vec![pat])
                .collect();
            check_exhaustive(cx, ex.span, &matrix);
        },
        ExprForLoop(ref pat, _, _, _) => {
            let mut static_inliner = StaticInliner::new(cx.tcx);
            match is_refutable(cx, static_inliner.fold_pat(*pat)) {
                Some(uncovered_pat) => {
                    cx.tcx.sess.span_err(
                        pat.span,
                        format!("refutable pattern in `for` loop binding: \
                                 `{}` not covered",
                                pat_to_string(&*uncovered_pat)).as_slice());
                },
                None => {}
            }

            // Check legality of move bindings.
            check_legality_of_move_bindings(cx, false, [ *pat ]);
            check_legality_of_bindings_in_at_patterns(cx, &**pat);
        }
        _ => ()
    }
}

fn is_expr_const_nan(tcx: &ty::ctxt, expr: &Expr) -> bool {
    match eval_const_expr(tcx, expr) {
        const_float(f) => f.is_nan(),
        _ => false
    }
}

// Check that we do not match against a static NaN (#6804)
fn check_for_static_nan(cx: &MatchCheckCtxt, arms: &[Arm]) {
    for arm in arms.iter() {
        for &pat in arm.pats.iter() {
            walk_pat(&*pat, |p| {
                match p.node {
                    PatLit(expr) if is_expr_const_nan(cx.tcx, &*expr) => {
                        span_warn!(cx.tcx.sess, p.span, E0003,
                            "unmatchable NaN in pattern, \
                             use the is_nan method in a guard instead");
                    }
                    _ => ()
                }
                true
            });
        }
    }
}

// Check for unreachable patterns
fn check_arms(cx: &MatchCheckCtxt, arms: &[Arm]) {
    let mut seen = Matrix(vec!());
    for arm in arms.iter() {
        for &pat in arm.pats.iter() {
            let v = vec![pat];
            match is_useful(cx, &seen, v.as_slice(), LeaveOutWitness) {
                NotUseful => span_err!(cx.tcx.sess, pat.span, E0001, "unreachable pattern"),
                Useful => (),
                UsefulWithWitness(_) => unreachable!()
            }
            if arm.guard.is_none() {
                let Matrix(mut rows) = seen;
                rows.push(v);
                seen = Matrix(rows);
            }
        }
    }
}

fn raw_pat(p: Gc<Pat>) -> Gc<Pat> {
    match p.node {
        PatIdent(_, _, Some(s)) => { raw_pat(s) }
        _ => { p }
    }
}

fn check_exhaustive(cx: &MatchCheckCtxt, sp: Span, matrix: &Matrix) {
    match is_useful(cx, matrix, [wild()], ConstructWitness) {
        UsefulWithWitness(pats) => {
            let witness = match pats.as_slice() {
                [witness] => witness,
                [] => wild(),
                _ => unreachable!()
            };
            span_err!(cx.tcx.sess, sp, E0004,
                "non-exhaustive patterns: `{}` not covered",
                pat_to_string(&*witness)
            );
        }
        NotUseful => {
            // This is good, wildcard pattern isn't reachable
        },
        _ => unreachable!()
    }
}

fn const_val_to_expr(value: &const_val) -> Gc<Expr> {
    let node = match value {
        &const_bool(b) => LitBool(b),
        &const_nil => LitNil,
        _ => unreachable!()
    };
    box (GC) Expr {
        id: 0,
        node: ExprLit(box(GC) Spanned { node: node, span: DUMMY_SP }),
        span: DUMMY_SP
    }
}

pub struct StaticInliner<'a> {
    pub tcx: &'a ty::ctxt,
    pub failed: bool
}

impl<'a> StaticInliner<'a> {
    pub fn new<'a>(tcx: &'a ty::ctxt) -> StaticInliner<'a> {
        StaticInliner {
            tcx: tcx,
            failed: false
        }
    }
}

impl<'a> Folder for StaticInliner<'a> {
    fn fold_pat(&mut self, pat: Gc<Pat>) -> Gc<Pat> {
        match pat.node {
            PatIdent(..) | PatEnum(..) => {
                let def = self.tcx.def_map.borrow().find_copy(&pat.id);
                match def {
                    Some(DefStatic(did, _)) => match lookup_const_by_id(self.tcx, did) {
                        Some(const_expr) => box (GC) Pat {
                            span: pat.span,
                            ..(*const_expr_to_pat(self.tcx, const_expr)).clone()
                        },
                        None => {
                            self.failed = true;
                            span_err!(self.tcx.sess, pat.span, E0158,
                                "extern statics cannot be referenced in patterns");
                            pat
                        }
                    },
                    _ => noop_fold_pat(pat, self)
                }
            }
            _ => noop_fold_pat(pat, self)
        }
    }
}

/// Constructs a partial witness for a pattern given a list of
/// patterns expanded by the specialization step.
///
/// When a pattern P is discovered to be useful, this function is used bottom-up
/// to reconstruct a complete witness, e.g. a pattern P' that covers a subset
/// of values, V, where each value in that set is not covered by any previously
/// used patterns and is covered by the pattern P'. Examples:
///
/// left_ty: tuple of 3 elements
/// pats: [10, 20, _]           => (10, 20, _)
///
/// left_ty: struct X { a: (bool, &'static str), b: uint}
/// pats: [(false, "foo"), 42]  => X { a: (false, "foo"), b: 42 }
fn construct_witness(cx: &MatchCheckCtxt, ctor: &Constructor,
                     pats: Vec<Gc<Pat>>, left_ty: ty::t) -> Gc<Pat> {
    let pat = match ty::get(left_ty).sty {
        ty::ty_tup(_) => PatTup(pats),

        ty::ty_enum(cid, _) | ty::ty_struct(cid, _)  => {
            let (vid, is_structure) = match ctor {
                &Variant(vid) =>
                    (vid, ty::enum_variant_with_id(cx.tcx, cid, vid).arg_names.is_some()),
                _ =>
                    (cid, ty::lookup_struct_fields(cx.tcx, cid).iter()
                        .any(|field| field.name != token::special_idents::unnamed_field.name))
            };
            if is_structure {
                let fields = ty::lookup_struct_fields(cx.tcx, vid);
                let field_pats: Vec<FieldPat> = fields.move_iter()
                    .zip(pats.iter())
                    .filter(|&(_, pat)| pat.node != PatWild(PatWildSingle))
                    .map(|(field, pat)| FieldPat {
                        ident: Ident::new(field.name),
                        pat: pat.clone()
                    }).collect();
                let has_more_fields = field_pats.len() < pats.len();
                PatStruct(def_to_path(cx.tcx, vid), field_pats, has_more_fields)
            } else {
                PatEnum(def_to_path(cx.tcx, vid), Some(pats))
            }
        }

        ty::ty_rptr(_, ty::mt { ty: ty, .. }) => {
            match ty::get(ty).sty {
               ty::ty_vec(_, Some(n)) => match ctor {
                    &Single => {
                        assert_eq!(pats.len(), n);
                        PatVec(pats, None, vec!())
                    },
                    _ => unreachable!()
                },
                ty::ty_vec(_, None) => match ctor {
                    &Slice(n) => {
                        assert_eq!(pats.len(), n);
                        PatVec(pats, None, vec!())
                    },
                    _ => unreachable!()
                },
                ty::ty_str => PatWild(PatWildSingle),

                _ => {
                    assert_eq!(pats.len(), 1);
                    PatRegion(pats.get(0).clone())
                }
            }
        }

        ty::ty_box(_) => {
            assert_eq!(pats.len(), 1);
            PatBox(pats.get(0).clone())
        }

        ty::ty_vec(_, Some(len)) => {
            assert_eq!(pats.len(), len);
            PatVec(pats, None, vec!())
        }

        _ => {
            match *ctor {
                ConstantValue(ref v) => PatLit(const_val_to_expr(v)),
                _ => PatWild(PatWildSingle),
            }
        }
    };

    box (GC) Pat {
        id: 0,
        node: pat,
        span: DUMMY_SP
    }
}

fn missing_constructor(cx: &MatchCheckCtxt, &Matrix(ref rows): &Matrix,
                       left_ty: ty::t, max_slice_length: uint) -> Option<Constructor> {
    let used_constructors: Vec<Constructor> = rows.iter()
        .flat_map(|row| pat_constructors(cx, *row.get(0), left_ty, max_slice_length).move_iter())
        .collect();
    all_constructors(cx, left_ty, max_slice_length)
        .move_iter()
        .find(|c| !used_constructors.contains(c))
}

/// This determines the set of all possible constructors of a pattern matching
/// values of type `left_ty`. For vectors, this would normally be an infinite set
/// but is instead bounded by the maximum fixed length of slice patterns in
/// the column of patterns being analyzed.
fn all_constructors(cx: &MatchCheckCtxt, left_ty: ty::t,
                    max_slice_length: uint) -> Vec<Constructor> {
    match ty::get(left_ty).sty {
        ty::ty_bool =>
            [true, false].iter().map(|b| ConstantValue(const_bool(*b))).collect(),

        ty::ty_nil =>
            vec!(ConstantValue(const_nil)),

        ty::ty_rptr(_, ty::mt { ty: ty, .. }) => match ty::get(ty).sty {
            ty::ty_vec(_, None) =>
                range_inclusive(0, max_slice_length).map(|length| Slice(length)).collect(),
            _ => vec!(Single)
        },

        ty::ty_enum(eid, _) =>
            ty::enum_variants(cx.tcx, eid)
                .iter()
                .map(|va| Variant(va.id))
                .collect(),

        _ =>
            vec!(Single)
    }
}

// Algorithm from http://moscova.inria.fr/~maranget/papers/warn/index.html
//
// Whether a vector `v` of patterns is 'useful' in relation to a set of such
// vectors `m` is defined as there being a set of inputs that will match `v`
// but not any of the sets in `m`.
//
// This is used both for reachability checking (if a pattern isn't useful in
// relation to preceding patterns, it is not reachable) and exhaustiveness
// checking (if a wildcard pattern is useful in relation to a matrix, the
// matrix isn't exhaustive).

// Note: is_useful doesn't work on empty types, as the paper notes.
// So it assumes that v is non-empty.
fn is_useful(cx: &MatchCheckCtxt,
             matrix: &Matrix,
             v: &[Gc<Pat>],
             witness: WitnessPreference)
             -> Usefulness {
    let &Matrix(ref rows) = matrix;
    debug!("{:}", matrix);
    if rows.len() == 0u {
        return match witness {
            ConstructWitness => UsefulWithWitness(vec!()),
            LeaveOutWitness => Useful
        };
    }
    if rows.get(0).len() == 0u {
        return NotUseful;
    }
    let real_pat = match rows.iter().find(|r| r.get(0).id != 0) {
        Some(r) => raw_pat(*r.get(0)),
        None if v.len() == 0 => return NotUseful,
        None => v[0]
    };
    let left_ty = if real_pat.id == 0 {
        ty::mk_nil()
    } else {
        ty::pat_ty(cx.tcx, &*real_pat)
    };

    let max_slice_length = rows.iter().filter_map(|row| match row.get(0).node {
        PatVec(ref before, _, ref after) => Some(before.len() + after.len()),
        _ => None
    }).max().map_or(0, |v| v + 1);

    let constructors = pat_constructors(cx, v[0], left_ty, max_slice_length);
    if constructors.is_empty() {
        match missing_constructor(cx, matrix, left_ty, max_slice_length) {
            None => {
                all_constructors(cx, left_ty, max_slice_length).move_iter().map(|c| {
                    match is_useful_specialized(cx, matrix, v, c.clone(), left_ty, witness) {
                        UsefulWithWitness(pats) => UsefulWithWitness({
                            let arity = constructor_arity(cx, &c, left_ty);
                            let subpats = {
                                let pat_slice = pats.as_slice();
                                Vec::from_fn(arity, |i| {
                                    pat_slice.get(i).map(|p| p.clone())
                                        .unwrap_or_else(|| wild())
                                })
                            };
                            let mut result = vec!(construct_witness(cx, &c, subpats, left_ty));
                            result.extend(pats.move_iter().skip(arity));
                            result
                        }),
                        result => result
                    }
                }).find(|result| result != &NotUseful).unwrap_or(NotUseful)
            },

            Some(constructor) => {
                let matrix = rows.iter().filter_map(|r| default(cx, r.as_slice())).collect();
                match is_useful(cx, &matrix, v.tail(), witness) {
                    UsefulWithWitness(pats) => {
                        let arity = constructor_arity(cx, &constructor, left_ty);
                        let wild_pats = Vec::from_elem(arity, wild());
                        let enum_pat = construct_witness(cx, &constructor, wild_pats, left_ty);
                        UsefulWithWitness(vec!(enum_pat).append(pats.as_slice()))
                    },
                    result => result
                }
            }
        }
    } else {
        constructors.move_iter().map(|c|
            is_useful_specialized(cx, matrix, v, c.clone(), left_ty, witness)
        ).find(|result| result != &NotUseful).unwrap_or(NotUseful)
    }
}

fn is_useful_specialized(cx: &MatchCheckCtxt, &Matrix(ref m): &Matrix, v: &[Gc<Pat>],
                         ctor: Constructor, lty: ty::t, witness: WitnessPreference) -> Usefulness {
    let arity = constructor_arity(cx, &ctor, lty);
    let matrix = Matrix(m.iter().filter_map(|r| {
        specialize(cx, r.as_slice(), &ctor, 0u, arity)
    }).collect());
    match specialize(cx, v, &ctor, 0u, arity) {
        Some(v) => is_useful(cx, &matrix, v.as_slice(), witness),
        None => NotUseful
    }
}

/// Determines the constructors that the given pattern can be specialized to.
///
/// In most cases, there's only one constructor that a specific pattern
/// represents, such as a specific enum variant or a specific literal value.
/// Slice patterns, however, can match slices of different lengths. For instance,
/// `[a, b, ..tail]` can match a slice of length 2, 3, 4 and so on.
///
/// On the other hand, a wild pattern and an identifier pattern cannot be
/// specialized in any way.
fn pat_constructors(cx: &MatchCheckCtxt, p: Gc<Pat>,
                    left_ty: ty::t, max_slice_length: uint) -> Vec<Constructor> {
    let pat = raw_pat(p);
    match pat.node {
        PatIdent(..) =>
            match cx.tcx.def_map.borrow().find(&pat.id) {
                Some(&DefStatic(..)) =>
                    cx.tcx.sess.span_bug(pat.span, "static pattern should've been rewritten"),
                Some(&DefStruct(_)) => vec!(Single),
                Some(&DefVariant(_, id, _)) => vec!(Variant(id)),
                _ => vec!()
            },
        PatEnum(..) =>
            match cx.tcx.def_map.borrow().find(&pat.id) {
                Some(&DefStatic(..)) =>
                    cx.tcx.sess.span_bug(pat.span, "static pattern should've been rewritten"),
                Some(&DefVariant(_, id, _)) => vec!(Variant(id)),
                _ => vec!(Single)
            },
        PatStruct(..) =>
            match cx.tcx.def_map.borrow().find(&pat.id) {
                Some(&DefStatic(..)) =>
                    cx.tcx.sess.span_bug(pat.span, "static pattern should've been rewritten"),
                Some(&DefVariant(_, id, _)) => vec!(Variant(id)),
                _ => vec!(Single)
            },
        PatLit(expr) =>
            vec!(ConstantValue(eval_const_expr(cx.tcx, &*expr))),
        PatRange(lo, hi) =>
            vec!(ConstantRange(eval_const_expr(cx.tcx, &*lo), eval_const_expr(cx.tcx, &*hi))),
        PatVec(ref before, ref slice, ref after) =>
            match ty::get(left_ty).sty {
                ty::ty_vec(_, Some(_)) => vec!(Single),
                _                      => if slice.is_some() {
                    range_inclusive(before.len() + after.len(), max_slice_length)
                        .map(|length| Slice(length))
                        .collect()
                } else {
                    vec!(Slice(before.len() + after.len()))
                }
            },
        PatBox(_) | PatTup(_) | PatRegion(..) =>
            vec!(Single),
        PatWild(_) =>
            vec!(),
        PatMac(_) =>
            cx.tcx.sess.bug("unexpanded macro")
    }
}

/// This computes the arity of a constructor. The arity of a constructor
/// is how many subpattern patterns of that constructor should be expanded to.
///
/// For instance, a tuple pattern (_, 42u, Some([])) has the arity of 3.
/// A struct pattern's arity is the number of fields it contains, etc.
pub fn constructor_arity(cx: &MatchCheckCtxt, ctor: &Constructor, ty: ty::t) -> uint {
    match ty::get(ty).sty {
        ty::ty_tup(ref fs) => fs.len(),
        ty::ty_box(_) | ty::ty_uniq(_) => 1u,
        ty::ty_rptr(_, ty::mt { ty: ty, .. }) => match ty::get(ty).sty {
            ty::ty_vec(_, None) => match *ctor {
                Slice(length) => length,
                ConstantValue(_) => 0u,
                _ => unreachable!()
            },
            ty::ty_str => 0u,
            _ => 1u
        },
        ty::ty_enum(eid, _) => {
            match *ctor {
                Variant(id) => enum_variant_with_id(cx.tcx, eid, id).args.len(),
                _ => unreachable!()
            }
        }
        ty::ty_struct(cid, _) => ty::lookup_struct_fields(cx.tcx, cid).len(),
        ty::ty_vec(_, Some(n)) => n,
        _ => 0u
    }
}

fn range_covered_by_constructor(ctor: &Constructor,
                                from: &const_val, to: &const_val) -> Option<bool> {
    let (c_from, c_to) = match *ctor {
        ConstantValue(ref value)        => (value, value),
        ConstantRange(ref from, ref to) => (from, to),
        Single                          => return Some(true),
        _                               => unreachable!()
    };
    let cmp_from = compare_const_vals(c_from, from);
    let cmp_to = compare_const_vals(c_to, to);
    match (cmp_from, cmp_to) {
        (Some(val1), Some(val2)) => Some(val1 >= 0 && val2 <= 0),
        _ => None
    }
}

/// This is the main specialization step. It expands the first pattern in the given row
/// into `arity` patterns based on the constructor. For most patterns, the step is trivial,
/// for instance tuple patterns are flattened and box patterns expand into their inner pattern.
///
/// OTOH, slice patterns with a subslice pattern (..tail) can be expanded into multiple
/// different patterns.
/// Structure patterns with a partial wild pattern (Foo { a: 42, .. }) have their missing
/// fields filled with wild patterns.
pub fn specialize(cx: &MatchCheckCtxt, r: &[Gc<Pat>],
                  constructor: &Constructor, col: uint, arity: uint) -> Option<Vec<Gc<Pat>>> {
    let &Pat {
        id: pat_id, node: ref node, span: pat_span
    } = &(*raw_pat(r[col]));
    let head: Option<Vec<Gc<Pat>>> = match node {
        &PatWild(_) =>
            Some(Vec::from_elem(arity, wild())),

        &PatIdent(_, _, _) => {
            let opt_def = cx.tcx.def_map.borrow().find_copy(&pat_id);
            match opt_def {
                Some(DefStatic(..)) =>
                    cx.tcx.sess.span_bug(pat_span, "static pattern should've been rewritten"),
                Some(DefVariant(_, id, _)) => if *constructor == Variant(id) {
                    Some(vec!())
                } else {
                    None
                },
                _ => Some(Vec::from_elem(arity, wild()))
            }
        }

        &PatEnum(_, ref args) => {
            let def = cx.tcx.def_map.borrow().get_copy(&pat_id);
            match def {
                DefStatic(..) =>
                    cx.tcx.sess.span_bug(pat_span, "static pattern should've been rewritten"),
                DefVariant(_, id, _) if *constructor != Variant(id) => None,
                DefVariant(..) | DefFn(..) | DefStruct(..) => {
                    Some(match args {
                        &Some(ref args) => args.clone(),
                        &None => Vec::from_elem(arity, wild())
                    })
                }
                _ => None
            }
        }

        &PatStruct(_, ref pattern_fields, _) => {
            // Is this a struct or an enum variant?
            let def = cx.tcx.def_map.borrow().get_copy(&pat_id);
            let class_id = match def {
                DefStatic(..) =>
                    cx.tcx.sess.span_bug(pat_span, "static pattern should've been rewritten"),
                DefVariant(_, variant_id, _) => if *constructor == Variant(variant_id) {
                    Some(variant_id)
                } else {
                    None
                },
                _ => {
                    // Assume this is a struct.
                    match ty::ty_to_def_id(node_id_to_type(cx.tcx, pat_id)) {
                        None => {
                            cx.tcx.sess.span_bug(pat_span,
                                                 "struct pattern wasn't of a \
                                                  type with a def ID?!")
                        }
                        Some(def_id) => Some(def_id),
                    }
                }
            };
            class_id.map(|variant_id| {
                let struct_fields = ty::lookup_struct_fields(cx.tcx, variant_id);
                let args = struct_fields.iter().map(|sf| {
                    match pattern_fields.iter().find(|f| f.ident.name == sf.name) {
                        Some(f) => f.pat,
                        _ => wild()
                    }
                }).collect();
                args
            })
        }

        &PatTup(ref args) =>
            Some(args.clone()),

        &PatBox(ref inner) | &PatRegion(ref inner) =>
            Some(vec!(inner.clone())),

        &PatLit(ref expr) => {
            let expr_value = eval_const_expr(cx.tcx, &**expr);
            match range_covered_by_constructor(constructor, &expr_value, &expr_value) {
                Some(true) => Some(vec!()),
                Some(false) => None,
                None => {
                    cx.tcx.sess.span_err(pat_span, "mismatched types between arms");
                    None
                }
            }
        }

        &PatRange(ref from, ref to) => {
            let from_value = eval_const_expr(cx.tcx, &**from);
            let to_value = eval_const_expr(cx.tcx, &**to);
            match range_covered_by_constructor(constructor, &from_value, &to_value) {
                Some(true) => Some(vec!()),
                Some(false) => None,
                None => {
                    cx.tcx.sess.span_err(pat_span, "mismatched types between arms");
                    None
                }
            }
        }

        &PatVec(ref before, ref slice, ref after) => {
            match *constructor {
                // Fixed-length vectors.
                Single => {
                    let mut pats = before.clone();
                    pats.grow_fn(arity - before.len() - after.len(), |_| wild());
                    pats.push_all(after.as_slice());
                    Some(pats)
                },
                Slice(length) if before.len() + after.len() <= length && slice.is_some() => {
                    let mut pats = before.clone();
                    pats.grow_fn(arity - before.len() - after.len(), |_| wild());
                    pats.push_all(after.as_slice());
                    Some(pats)
                },
                Slice(length) if before.len() + after.len() == length => {
                    let mut pats = before.clone();
                    pats.push_all(after.as_slice());
                    Some(pats)
                },
                _ => None
            }
        }

        &PatMac(_) => {
            cx.tcx.sess.span_err(pat_span, "unexpanded macro");
            None
        }
    };
    head.map(|head| head.append(r.slice_to(col)).append(r.slice_from(col + 1)))
}

fn default(cx: &MatchCheckCtxt, r: &[Gc<Pat>]) -> Option<Vec<Gc<Pat>>> {
    if pat_is_binding_or_wild(&cx.tcx.def_map, &*raw_pat(r[0])) {
        Some(Vec::from_slice(r.tail()))
    } else {
        None
    }
}

fn check_local(cx: &mut MatchCheckCtxt, loc: &Local) {
    visit::walk_local(cx, loc, ());

    let name = match loc.source {
        LocalLet => "local",
        LocalFor => "`for` loop"
    };

    let mut static_inliner = StaticInliner::new(cx.tcx);
    match is_refutable(cx, static_inliner.fold_pat(loc.pat)) {
        Some(pat) => {
            span_err!(cx.tcx.sess, loc.pat.span, E0005,
                "refutable pattern in {} binding: `{}` not covered",
                name, pat_to_string(&*pat)
            );
        },
        None => ()
    }

    // Check legality of move bindings and `@` patterns.
    check_legality_of_move_bindings(cx, false, [ loc.pat ]);
    check_legality_of_bindings_in_at_patterns(cx, &*loc.pat);
}

fn check_fn(cx: &mut MatchCheckCtxt,
            kind: &FnKind,
            decl: &FnDecl,
            body: &Block,
            sp: Span) {
    visit::walk_fn(cx, kind, decl, body, sp, ());
    for input in decl.inputs.iter() {
        match is_refutable(cx, input.pat) {
            Some(pat) => {
                span_err!(cx.tcx.sess, input.pat.span, E0006,
                    "refutable pattern in function argument: `{}` not covered",
                    pat_to_string(&*pat)
                );
            },
            None => ()
        }
        check_legality_of_move_bindings(cx, false, [input.pat]);
        check_legality_of_bindings_in_at_patterns(cx, &*input.pat);
    }
}

fn is_refutable(cx: &MatchCheckCtxt, pat: Gc<Pat>) -> Option<Gc<Pat>> {
    let pats = Matrix(vec!(vec!(pat)));
    match is_useful(cx, &pats, [wild()], ConstructWitness) {
        UsefulWithWitness(pats) => {
            assert_eq!(pats.len(), 1);
            Some(pats.get(0).clone())
        },
        NotUseful => None,
        Useful => unreachable!()
    }
}

// Legality of move bindings checking
fn check_legality_of_move_bindings(cx: &MatchCheckCtxt,
                                   has_guard: bool,
                                   pats: &[Gc<Pat>]) {
    let tcx = cx.tcx;
    let def_map = &tcx.def_map;
    let mut by_ref_span = None;
    for pat in pats.iter() {
        pat_bindings(def_map, &**pat, |bm, _, span, _path| {
            match bm {
                BindByRef(_) => {
                    by_ref_span = Some(span);
                }
                BindByValue(_) => {
                }
            }
        })
    }

    let check_move: |&Pat, Option<Gc<Pat>>| = |p, sub| {
        // check legality of moving out of the enum

        // x @ Foo(..) is legal, but x @ Foo(y) isn't.
        if sub.map_or(false, |p| pat_contains_bindings(def_map, &*p)) {
            span_err!(cx.tcx.sess, p.span, E0007, "cannot bind by-move with sub-bindings");
        } else if has_guard {
            span_err!(cx.tcx.sess, p.span, E0008, "cannot bind by-move into a pattern guard");
        } else if by_ref_span.is_some() {
            span_err!(cx.tcx.sess, p.span, E0009,
                "cannot bind by-move and by-ref in the same pattern");
            span_note!(cx.tcx.sess, by_ref_span.unwrap(), "by-ref binding occurs here");
        }
    };

    for pat in pats.iter() {
        walk_pat(&**pat, |p| {
            if pat_is_binding(def_map, &*p) {
                match p.node {
                    PatIdent(BindByValue(_), _, sub) => {
                        let pat_ty = ty::node_id_to_type(tcx, p.id);
                        if ty::type_moves_by_default(tcx, pat_ty) {
                            check_move(p, sub);
                        }
                    }
                    PatIdent(BindByRef(_), _, _) => {
                    }
                    _ => {
                        cx.tcx.sess.span_bug(
                            p.span,
                            format!("binding pattern {} is not an \
                                     identifier: {:?}",
                                    p.id,
                                    p.node).as_slice());
                    }
                }
            }
            true
        });
    }
}

/// Ensures that a pattern guard doesn't borrow by mutable reference or
/// assign.
fn check_for_mutation_in_guard<'a>(cx: &'a MatchCheckCtxt<'a>, guard: &Expr) {
    let mut checker = MutationChecker {
        cx: cx,
    };
    let mut visitor = ExprUseVisitor::new(&mut checker, checker.cx.tcx);
    visitor.walk_expr(guard);
}

struct MutationChecker<'a> {
    cx: &'a MatchCheckCtxt<'a>,
}

impl<'a> Delegate for MutationChecker<'a> {
    fn consume(&mut self, _: NodeId, _: Span, _: cmt, _: ConsumeMode) {}
    fn consume_pat(&mut self, _: &Pat, _: cmt, _: ConsumeMode) {}
    fn borrow(&mut self,
              _: NodeId,
              span: Span,
              _: cmt,
              _: Region,
              kind: BorrowKind,
              _: LoanCause) {
        match kind {
            MutBorrow => {
                self.cx
                    .tcx
                    .sess
                    .span_err(span,
                              "cannot mutably borrow in a pattern guard")
            }
            ImmBorrow | UniqueImmBorrow => {}
        }
    }
    fn decl_without_init(&mut self, _: NodeId, _: Span) {}
    fn mutate(&mut self, _: NodeId, span: Span, _: cmt, mode: MutateMode) {
        match mode {
            JustWrite | WriteAndRead => {
                self.cx
                    .tcx
                    .sess
                    .span_err(span, "cannot assign in a pattern guard")
            }
            Init => {}
        }
    }
}

/// Forbids bindings in `@` patterns. This is necessary for memory safety,
/// because of the way rvalues are handled in the borrow check. (See issue
/// #14587.)
fn check_legality_of_bindings_in_at_patterns(cx: &MatchCheckCtxt, pat: &Pat) {
    let mut visitor = AtBindingPatternVisitor {
        cx: cx,
    };
    visitor.visit_pat(pat, true);
}

struct AtBindingPatternVisitor<'a,'b> {
    cx: &'a MatchCheckCtxt<'b>,
}

impl<'a,'b> Visitor<bool> for AtBindingPatternVisitor<'a,'b> {
    fn visit_pat(&mut self, pat: &Pat, bindings_allowed: bool) {
        if !bindings_allowed && pat_is_binding(&self.cx.tcx.def_map, pat) {
            self.cx.tcx.sess.span_err(pat.span,
                                      "pattern bindings are not allowed \
                                       after an `@`");
        }

        match pat.node {
            PatIdent(_, _, Some(_)) => visit::walk_pat(self, pat, false),
            _ => visit::walk_pat(self, pat, bindings_allowed),
        }
    }
}

