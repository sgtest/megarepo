// Copyright 2012-2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use hir::def_id::DefId;
use ty::{self, TyCtxt, layout};
use ty::subst::Substs;
use mir::interpret::ConstValue;
use errors::DiagnosticBuilder;

use graphviz::IntoCow;
use syntax_pos::Span;

use std::borrow::Cow;
use rustc_data_structures::sync::Lrc;

pub type EvalResult<'tcx> = Result<&'tcx ty::Const<'tcx>, ConstEvalErr<'tcx>>;

#[derive(Copy, Clone, Debug, Hash, RustcEncodable, RustcDecodable, Eq, PartialEq)]
pub enum ConstVal<'tcx> {
    Unevaluated(DefId, &'tcx Substs<'tcx>),
    Value(ConstValue<'tcx>),
}

#[derive(Clone, Debug)]
pub struct ConstEvalErr<'tcx> {
    pub span: Span,
    pub kind: Lrc<ErrKind<'tcx>>,
}

#[derive(Clone, Debug)]
pub enum ErrKind<'tcx> {

    NonConstPath,
    UnimplementedConstVal(&'static str),
    IndexOutOfBounds { len: u64, index: u64 },

    LayoutError(layout::LayoutError<'tcx>),

    TypeckError,
    CheckMatchError,
    Miri(::mir::interpret::EvalError<'tcx>, Vec<FrameInfo>),
}

#[derive(Clone, Debug)]
pub struct FrameInfo {
    pub span: Span,
    pub location: String,
}

#[derive(Clone, Debug)]
pub enum ConstEvalErrDescription<'a, 'tcx: 'a> {
    Simple(Cow<'a, str>),
    Backtrace(&'a ::mir::interpret::EvalError<'tcx>, &'a [FrameInfo]),
}

impl<'a, 'tcx> ConstEvalErrDescription<'a, 'tcx> {
    /// Return a one-line description of the error, for lints and such
    pub fn into_oneline(self) -> Cow<'a, str> {
        match self {
            ConstEvalErrDescription::Simple(simple) => simple,
            ConstEvalErrDescription::Backtrace(miri, _) => format!("{}", miri).into_cow(),
        }
    }
}

impl<'a, 'gcx, 'tcx> ConstEvalErr<'tcx> {
    pub fn description(&'a self) -> ConstEvalErrDescription<'a, 'tcx> {
        use self::ErrKind::*;
        use self::ConstEvalErrDescription::*;

        macro_rules! simple {
            ($msg:expr) => ({ Simple($msg.into_cow()) });
            ($fmt:expr, $($arg:tt)+) => ({
                Simple(format!($fmt, $($arg)+).into_cow())
            })
        }

        match *self.kind {
            NonConstPath        => simple!("non-constant path in constant expression"),
            UnimplementedConstVal(what) =>
                simple!("unimplemented constant expression: {}", what),
            IndexOutOfBounds { len, index } => {
                simple!("index out of bounds: the len is {} but the index is {}",
                        len, index)
            }

            LayoutError(ref err) => Simple(err.to_string().into_cow()),

            TypeckError => simple!("type-checking failed"),
            CheckMatchError => simple!("match-checking failed"),
            Miri(ref err, ref trace) => Backtrace(err, trace),
        }
    }

    pub fn struct_error(&self,
        tcx: TyCtxt<'a, 'gcx, 'tcx>,
        primary_span: Span,
        primary_kind: &str)
        -> DiagnosticBuilder<'gcx>
    {
        let mut diag = struct_error(tcx, self.span, "constant evaluation error");
        self.note(tcx, primary_span, primary_kind, &mut diag);
        diag
    }

    pub fn note(&self,
        _tcx: TyCtxt<'a, 'gcx, 'tcx>,
        primary_span: Span,
        primary_kind: &str,
        diag: &mut DiagnosticBuilder)
    {
        match self.description() {
            ConstEvalErrDescription::Simple(message) => {
                diag.span_label(self.span, message);
            }
            ConstEvalErrDescription::Backtrace(miri, frames) => {
                diag.span_label(self.span, format!("{}", miri));
                for frame in frames {
                    diag.span_label(frame.span, format!("inside call to `{}`", frame.location));
                }
            }
        }

        if !primary_span.contains(self.span) {
            diag.span_note(primary_span,
                        &format!("for {} here", primary_kind));
        }
    }

    pub fn report(&self,
        tcx: TyCtxt<'a, 'gcx, 'tcx>,
        primary_span: Span,
        primary_kind: &str)
    {
        match *self.kind {
            ErrKind::TypeckError | ErrKind::CheckMatchError => return,
            ErrKind::Miri(ref miri, _) => {
                match miri.kind {
                    ::mir::interpret::EvalErrorKind::TypeckError |
                    ::mir::interpret::EvalErrorKind::Layout(_) => return,
                    _ => {},
                }
            }
            _ => {}
        }
        self.struct_error(tcx, primary_span, primary_kind).emit();
    }
}

pub fn struct_error<'a, 'gcx, 'tcx>(
    tcx: TyCtxt<'a, 'gcx, 'tcx>,
    span: Span,
    msg: &str,
) -> DiagnosticBuilder<'gcx> {
    struct_span_err!(tcx.sess, span, E0080, "{}", msg)
}
