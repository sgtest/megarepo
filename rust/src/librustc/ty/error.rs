// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use hir::def_id::DefId;
use infer::type_variable;
use ty::{self, BoundRegion, DefIdTree, Region, Ty, TyCtxt};

use std::fmt;
use syntax::abi;
use syntax::ast;
use errors::DiagnosticBuilder;
use syntax_pos::Span;

use hir;

#[derive(Clone, Copy, Debug)]
pub struct ExpectedFound<T> {
    pub expected: T,
    pub found: T,
}

// Data structures used in type unification
#[derive(Clone, Debug)]
pub enum TypeError<'tcx> {
    Mismatch,
    UnsafetyMismatch(ExpectedFound<hir::Unsafety>),
    AbiMismatch(ExpectedFound<abi::Abi>),
    Mutability,
    TupleSize(ExpectedFound<usize>),
    FixedArraySize(ExpectedFound<usize>),
    ArgCount,

    RegionsDoesNotOutlive(Region<'tcx>, Region<'tcx>),
    RegionsInsufficientlyPolymorphic(BoundRegion, Region<'tcx>),
    RegionsOverlyPolymorphic(BoundRegion, Region<'tcx>),

    Sorts(ExpectedFound<Ty<'tcx>>),
    IntMismatch(ExpectedFound<ty::IntVarValue>),
    FloatMismatch(ExpectedFound<ast::FloatTy>),
    Traits(ExpectedFound<DefId>),
    VariadicMismatch(ExpectedFound<bool>),
    CyclicTy,
    ProjectionMismatched(ExpectedFound<DefId>),
    ProjectionBoundsLength(ExpectedFound<usize>),
    TyParamDefaultMismatch(ExpectedFound<type_variable::Default<'tcx>>),
    ExistentialMismatch(ExpectedFound<&'tcx ty::Slice<ty::ExistentialPredicate<'tcx>>>),
}

#[derive(Clone, RustcEncodable, RustcDecodable, PartialEq, Eq, Hash, Debug, Copy)]
pub enum UnconstrainedNumeric {
    UnconstrainedFloat,
    UnconstrainedInt,
    Neither,
}

/// Explains the source of a type err in a short, human readable way. This is meant to be placed
/// in parentheses after some larger message. You should also invoke `note_and_explain_type_err()`
/// afterwards to present additional details, particularly when it comes to lifetime-related
/// errors.
impl<'tcx> fmt::Display for TypeError<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::TypeError::*;
        fn report_maybe_different(f: &mut fmt::Formatter,
                                  expected: String, found: String) -> fmt::Result {
            // A naive approach to making sure that we're not reporting silly errors such as:
            // (expected closure, found closure).
            if expected == found {
                write!(f, "expected {}, found a different {}", expected, found)
            } else {
                write!(f, "expected {}, found {}", expected, found)
            }
        }

        match *self {
            CyclicTy => write!(f, "cyclic type of infinite size"),
            Mismatch => write!(f, "types differ"),
            UnsafetyMismatch(values) => {
                write!(f, "expected {} fn, found {} fn",
                       values.expected,
                       values.found)
            }
            AbiMismatch(values) => {
                write!(f, "expected {} fn, found {} fn",
                       values.expected,
                       values.found)
            }
            Mutability => write!(f, "types differ in mutability"),
            FixedArraySize(values) => {
                write!(f, "expected an array with a fixed size of {} elements, \
                           found one with {} elements",
                       values.expected,
                       values.found)
            }
            TupleSize(values) => {
                write!(f, "expected a tuple with {} elements, \
                           found one with {} elements",
                       values.expected,
                       values.found)
            }
            ArgCount => {
                write!(f, "incorrect number of function parameters")
            }
            RegionsDoesNotOutlive(..) => {
                write!(f, "lifetime mismatch")
            }
            RegionsInsufficientlyPolymorphic(br, _) => {
                write!(f,
                       "expected bound lifetime parameter{}{}, found concrete lifetime",
                       if br.is_named() { " " } else { "" },
                       br)
            }
            RegionsOverlyPolymorphic(br, _) => {
                write!(f,
                       "expected concrete lifetime, found bound lifetime parameter{}{}",
                       if br.is_named() { " " } else { "" },
                       br)
            }
            Sorts(values) => ty::tls::with(|tcx| {
                report_maybe_different(f, values.expected.sort_string(tcx),
                                       values.found.sort_string(tcx))
            }),
            Traits(values) => ty::tls::with(|tcx| {
                report_maybe_different(f,
                                       format!("trait `{}`",
                                               tcx.item_path_str(values.expected)),
                                       format!("trait `{}`",
                                               tcx.item_path_str(values.found)))
            }),
            IntMismatch(ref values) => {
                write!(f, "expected `{:?}`, found `{:?}`",
                       values.expected,
                       values.found)
            }
            FloatMismatch(ref values) => {
                write!(f, "expected `{:?}`, found `{:?}`",
                       values.expected,
                       values.found)
            }
            VariadicMismatch(ref values) => {
                write!(f, "expected {} fn, found {} function",
                       if values.expected { "variadic" } else { "non-variadic" },
                       if values.found { "variadic" } else { "non-variadic" })
            }
            ProjectionMismatched(ref values) => ty::tls::with(|tcx| {
                write!(f, "expected {}, found {}",
                       tcx.item_path_str(values.expected),
                       tcx.item_path_str(values.found))
            }),
            ProjectionBoundsLength(ref values) => {
                write!(f, "expected {} associated type bindings, found {}",
                       values.expected,
                       values.found)
            },
            TyParamDefaultMismatch(ref values) => {
                write!(f, "conflicting type parameter defaults `{}` and `{}`",
                       values.expected.ty,
                       values.found.ty)
            }
            ExistentialMismatch(ref values) => {
                report_maybe_different(f, format!("trait `{}`", values.expected),
                                       format!("trait `{}`", values.found))
            }
        }
    }
}

impl<'a, 'gcx, 'lcx, 'tcx> ty::TyS<'tcx> {
    pub fn sort_string(&self, tcx: TyCtxt<'a, 'gcx, 'lcx>) -> String {
        match self.sty {
            ty::TyBool | ty::TyChar | ty::TyInt(_) |
            ty::TyUint(_) | ty::TyFloat(_) | ty::TyStr | ty::TyNever => self.to_string(),
            ty::TyTuple(ref tys, _) if tys.is_empty() => self.to_string(),

            ty::TyAdt(def, _) => format!("{} `{}`", def.descr(), tcx.item_path_str(def.did)),
            ty::TyArray(_, n) => format!("array of {} elements", n),
            ty::TySlice(_) => "slice".to_string(),
            ty::TyRawPtr(_) => "*-ptr".to_string(),
            ty::TyRef(region, tymut) => {
                let tymut_string = tymut.to_string();
                if tymut_string == "_" ||         //unknown type name,
                   tymut_string.len() > 10 ||     //name longer than saying "reference",
                   region.to_string() != ""       //... or a complex type
                {
                    match tymut {
                        ty::TypeAndMut{mutbl, ..} => {
                            format!("{}reference", match mutbl {
                                hir::Mutability::MutMutable => "mutable ",
                                _ => ""
                            })
                        }
                    }
                } else {
                    format!("&{}", tymut_string)
                }
            }
            ty::TyFnDef(..) => format!("fn item"),
            ty::TyFnPtr(_) => "fn pointer".to_string(),
            ty::TyDynamic(ref inner, ..) => {
                inner.principal().map_or_else(|| "trait".to_string(),
                    |p| format!("trait {}", tcx.item_path_str(p.def_id())))
            }
            ty::TyClosure(..) => "closure".to_string(),
            ty::TyGenerator(..) => "generator".to_string(),
            ty::TyTuple(..) => "tuple".to_string(),
            ty::TyInfer(ty::TyVar(_)) => "inferred type".to_string(),
            ty::TyInfer(ty::IntVar(_)) => "integral variable".to_string(),
            ty::TyInfer(ty::FloatVar(_)) => "floating-point variable".to_string(),
            ty::TyInfer(ty::FreshTy(_)) => "skolemized type".to_string(),
            ty::TyInfer(ty::FreshIntTy(_)) => "skolemized integral type".to_string(),
            ty::TyInfer(ty::FreshFloatTy(_)) => "skolemized floating-point type".to_string(),
            ty::TyProjection(_) => "associated type".to_string(),
            ty::TyParam(ref p) => {
                if p.is_self() {
                    "Self".to_string()
                } else {
                    "type parameter".to_string()
                }
            }
            ty::TyAnon(..) => "anonymized type".to_string(),
            ty::TyError => "type error".to_string(),
        }
    }
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    pub fn note_and_explain_type_err(self,
                                     db: &mut DiagnosticBuilder,
                                     err: &TypeError<'tcx>,
                                     sp: Span) {
        use self::TypeError::*;

        match err.clone() {
            Sorts(values) => {
                let expected_str = values.expected.sort_string(self);
                let found_str = values.found.sort_string(self);
                if expected_str == found_str && expected_str == "closure" {
                    db.span_note(sp,
                        "no two closures, even if identical, have the same type");
                    db.span_help(sp,
                        "consider boxing your closure and/or using it as a trait object");
                }
            },
            TyParamDefaultMismatch(values) => {
                let expected = values.expected;
                let found = values.found;
                db.span_note(sp, &format!("conflicting type parameter defaults `{}` and `{}`",
                                          expected.ty,
                                          found.ty));

                match self.hir.span_if_local(expected.def_id) {
                    Some(span) => {
                        db.span_note(span, "a default was defined here...");
                    }
                    None => {
                        let item_def_id = self.parent(expected.def_id).unwrap();
                        db.note(&format!("a default is defined on `{}`",
                                         self.item_path_str(item_def_id)));
                    }
                }

                db.span_note(
                    expected.origin_span,
                    "...that was applied to an unconstrained type variable here");

                match self.hir.span_if_local(found.def_id) {
                    Some(span) => {
                        db.span_note(span, "a second default was defined here...");
                    }
                    None => {
                        let item_def_id = self.parent(found.def_id).unwrap();
                        db.note(&format!("a second default is defined on `{}`",
                                         self.item_path_str(item_def_id)));
                    }
                }

                db.span_note(found.origin_span,
                             "...that also applies to the same type variable here");
            }
            _ => {}
        }
    }
}
