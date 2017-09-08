// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::hir;
use rustc::hir::def_id::DefId;
use rustc::infer;
use rustc::middle::const_val::ConstVal;
use rustc::mir::*;
use rustc::mir::transform::MirSource;
use rustc::ty::{self, Ty};
use rustc::ty::subst::{Kind, Subst, Substs};
use rustc::ty::maps::Providers;
use rustc_const_math::{ConstInt, ConstUsize};

use rustc_data_structures::indexed_vec::{IndexVec, Idx};

use syntax::abi::Abi;
use syntax::ast;
use syntax_pos::Span;

use std::fmt;
use std::iter;

use transform::{add_call_guards, no_landing_pads, simplify};
use util::elaborate_drops::{self, DropElaborator, DropStyle, DropFlagMode};
use util::patch::MirPatch;

pub fn provide(providers: &mut Providers) {
    providers.mir_shims = make_shim;
}

fn make_shim<'a, 'tcx>(tcx: ty::TyCtxt<'a, 'tcx, 'tcx>,
                       instance: ty::InstanceDef<'tcx>)
                       -> &'tcx Mir<'tcx>
{
    debug!("make_shim({:?})", instance);

    let mut result = match instance {
        ty::InstanceDef::Item(..) =>
            bug!("item {:?} passed to make_shim", instance),
        ty::InstanceDef::FnPtrShim(def_id, ty) => {
            let trait_ = tcx.trait_of_item(def_id).unwrap();
            let adjustment = match tcx.lang_items().fn_trait_kind(trait_) {
                Some(ty::ClosureKind::FnOnce) => Adjustment::Identity,
                Some(ty::ClosureKind::FnMut) |
                Some(ty::ClosureKind::Fn) => Adjustment::Deref,
                None => bug!("fn pointer {:?} is not an fn", ty)
            };
            // HACK: we need the "real" argument types for the MIR,
            // but because our substs are (Self, Args), where Args
            // is a tuple, we must include the *concrete* argument
            // types in the MIR. They will be substituted again with
            // the param-substs, but because they are concrete, this
            // will not do any harm.
            let sig = tcx.erase_late_bound_regions(&ty.fn_sig(tcx));
            let arg_tys = sig.inputs();

            build_call_shim(
                tcx,
                def_id,
                adjustment,
                CallKind::Indirect,
                Some(arg_tys)
            )
        }
        ty::InstanceDef::Virtual(def_id, _) => {
            // We are translating a call back to our def-id, which
            // trans::mir knows to turn to an actual virtual call.
            build_call_shim(
                tcx,
                def_id,
                Adjustment::Identity,
                CallKind::Direct(def_id),
                None
            )
        }
        ty::InstanceDef::ClosureOnceShim { call_once } => {
            let fn_mut = tcx.lang_items().fn_mut_trait().unwrap();
            let call_mut = tcx.global_tcx()
                .associated_items(fn_mut)
                .find(|it| it.kind == ty::AssociatedKind::Method)
                .unwrap().def_id;

            build_call_shim(
                tcx,
                call_once,
                Adjustment::RefMut,
                CallKind::Direct(call_mut),
                None
            )
        }
        ty::InstanceDef::DropGlue(def_id, ty) => {
            build_drop_shim(tcx, def_id, ty)
        }
        ty::InstanceDef::CloneShim(def_id, ty) => {
            let name = tcx.item_name(def_id);
            if name == "clone" {
                build_clone_shim(tcx, def_id, ty)
            } else if name == "clone_from" {
                debug!("make_shim({:?}: using default trait implementation", instance);
                return tcx.optimized_mir(def_id);
            } else {
                bug!("builtin clone shim {:?} not supported", instance)
            }
        }
        ty::InstanceDef::Intrinsic(_) => {
            bug!("creating shims from intrinsics ({:?}) is unsupported", instance)
        }
    };
    debug!("make_shim({:?}) = untransformed {:?}", instance, result);
    no_landing_pads::no_landing_pads(tcx, &mut result);
    simplify::simplify_cfg(&mut result);
    add_call_guards::CriticalCallEdges.add_call_guards(&mut result);
    debug!("make_shim({:?}) = {:?}", instance, result);

    tcx.alloc_mir(result)
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum Adjustment {
    Identity,
    Deref,
    RefMut,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum CallKind {
    Indirect,
    Direct(DefId),
}

fn temp_decl(mutability: Mutability, ty: Ty, span: Span) -> LocalDecl {
    LocalDecl {
        mutability, ty, name: None,
        source_info: SourceInfo { scope: ARGUMENT_VISIBILITY_SCOPE, span },
        internal: false,
        is_user_variable: false
    }
}

fn local_decls_for_sig<'tcx>(sig: &ty::FnSig<'tcx>, span: Span)
    -> IndexVec<Local, LocalDecl<'tcx>>
{
    iter::once(temp_decl(Mutability::Mut, sig.output(), span))
        .chain(sig.inputs().iter().map(
            |ity| temp_decl(Mutability::Not, ity, span)))
        .collect()
}

fn build_drop_shim<'a, 'tcx>(tcx: ty::TyCtxt<'a, 'tcx, 'tcx>,
                             def_id: DefId,
                             ty: Option<Ty<'tcx>>)
                             -> Mir<'tcx>
{
    debug!("build_drop_shim(def_id={:?}, ty={:?})", def_id, ty);

    // Check if this is a generator, if so, return the drop glue for it
    if let Some(&ty::TyS { sty: ty::TyGenerator(gen_def_id, substs, _), .. }) = ty {
        let mir = &**tcx.optimized_mir(gen_def_id).generator_drop.as_ref().unwrap();
        return mir.subst(tcx, substs.substs);
    }

    let substs = if let Some(ty) = ty {
        tcx.mk_substs(iter::once(Kind::from(ty)))
    } else {
        Substs::identity_for_item(tcx, def_id)
    };
    let sig = tcx.fn_sig(def_id).subst(tcx, substs);
    let sig = tcx.erase_late_bound_regions(&sig);
    let span = tcx.def_span(def_id);

    let source_info = SourceInfo { span, scope: ARGUMENT_VISIBILITY_SCOPE };

    let return_block = BasicBlock::new(1);
    let mut blocks = IndexVec::new();
    let block = |blocks: &mut IndexVec<_, _>, kind| {
        blocks.push(BasicBlockData {
            statements: vec![],
            terminator: Some(Terminator { source_info, kind }),
            is_cleanup: false
        })
    };
    block(&mut blocks, TerminatorKind::Goto { target: return_block });
    block(&mut blocks, TerminatorKind::Return);

    let mut mir = Mir::new(
        blocks,
        IndexVec::from_elem_n(
            VisibilityScopeData { span: span, parent_scope: None }, 1
        ),
        IndexVec::new(),
        sig.output(),
        None,
        local_decls_for_sig(&sig, span),
        sig.inputs().len(),
        vec![],
        span
    );

    if let Some(..) = ty {
        let patch = {
            let param_env = tcx.param_env(def_id);
            let mut elaborator = DropShimElaborator {
                mir: &mir,
                patch: MirPatch::new(&mir),
                tcx,
                param_env
            };
            let dropee = Lvalue::Local(Local::new(1+0)).deref();
            let resume_block = elaborator.patch.resume_block();
            elaborate_drops::elaborate_drop(
                &mut elaborator,
                source_info,
                &dropee,
                (),
                return_block,
                elaborate_drops::Unwind::To(resume_block),
                START_BLOCK
            );
            elaborator.patch
        };
        patch.apply(&mut mir);
    }

    mir
}

pub struct DropShimElaborator<'a, 'tcx: 'a> {
    pub mir: &'a Mir<'tcx>,
    pub patch: MirPatch<'tcx>,
    pub tcx: ty::TyCtxt<'a, 'tcx, 'tcx>,
    pub param_env: ty::ParamEnv<'tcx>,
}

impl<'a, 'tcx> fmt::Debug for DropShimElaborator<'a, 'tcx> {
    fn fmt(&self, _f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        Ok(())
    }
}

impl<'a, 'tcx> DropElaborator<'a, 'tcx> for DropShimElaborator<'a, 'tcx> {
    type Path = ();

    fn patch(&mut self) -> &mut MirPatch<'tcx> { &mut self.patch }
    fn mir(&self) -> &'a Mir<'tcx> { self.mir }
    fn tcx(&self) -> ty::TyCtxt<'a, 'tcx, 'tcx> { self.tcx }
    fn param_env(&self) -> ty::ParamEnv<'tcx> { self.param_env }

    fn drop_style(&self, _path: Self::Path, mode: DropFlagMode) -> DropStyle {
        if let DropFlagMode::Shallow = mode {
            DropStyle::Static
        } else {
            DropStyle::Open
        }
    }

    fn get_drop_flag(&mut self, _path: Self::Path) -> Option<Operand<'tcx>> {
        None
    }

    fn clear_drop_flag(&mut self, _location: Location, _path: Self::Path, _mode: DropFlagMode) {
    }

    fn field_subpath(&self, _path: Self::Path, _field: Field) -> Option<Self::Path> {
        None
    }
    fn deref_subpath(&self, _path: Self::Path) -> Option<Self::Path> {
        None
    }
    fn downcast_subpath(&self, _path: Self::Path, _variant: usize) -> Option<Self::Path> {
        Some(())
    }
}

/// Build a `Clone::clone` shim for `self_ty`. Here, `def_id` is `Clone::clone`.
fn build_clone_shim<'a, 'tcx>(tcx: ty::TyCtxt<'a, 'tcx, 'tcx>,
                              def_id: DefId,
                              self_ty: ty::Ty<'tcx>)
                              -> Mir<'tcx>
{
    debug!("build_clone_shim(def_id={:?})", def_id);

    let mut builder = CloneShimBuilder::new(tcx, def_id);
    let is_copy = !self_ty.moves_by_default(tcx, tcx.param_env(def_id), builder.span);

    match self_ty.sty {
        _ if is_copy => builder.copy_shim(),
        ty::TyArray(ty, len) => builder.array_shim(ty, len),
        ty::TyTuple(tys, _) => builder.tuple_shim(tys),
        _ => {
            bug!("clone shim for `{:?}` which is not `Copy` and is not an aggregate", self_ty);
        }
    };

    builder.into_mir()
}

struct CloneShimBuilder<'a, 'tcx: 'a> {
    tcx: ty::TyCtxt<'a, 'tcx, 'tcx>,
    def_id: DefId,
    local_decls: IndexVec<Local, LocalDecl<'tcx>>,
    blocks: IndexVec<BasicBlock, BasicBlockData<'tcx>>,
    span: Span,
    sig: ty::FnSig<'tcx>,
}

impl<'a, 'tcx> CloneShimBuilder<'a, 'tcx> {
    fn new(tcx: ty::TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId) -> Self {
        let sig = tcx.fn_sig(def_id);
        let sig = tcx.erase_late_bound_regions(&sig);
        let span = tcx.def_span(def_id);

        CloneShimBuilder {
            tcx,
            def_id,
            local_decls: local_decls_for_sig(&sig, span),
            blocks: IndexVec::new(),
            span,
            sig,
        }
    }

    fn into_mir(self) -> Mir<'tcx> {
        Mir::new(
            self.blocks,
            IndexVec::from_elem_n(
                VisibilityScopeData { span: self.span, parent_scope: None }, 1
            ),
            IndexVec::new(),
            self.sig.output(),
            None,
            self.local_decls,
            self.sig.inputs().len(),
            vec![],
            self.span
        )
    }

    fn source_info(&self) -> SourceInfo {
        SourceInfo { span: self.span, scope: ARGUMENT_VISIBILITY_SCOPE }
    }

    fn block(
        &mut self,
        statements: Vec<Statement<'tcx>>,
        kind: TerminatorKind<'tcx>,
        is_cleanup: bool
    ) -> BasicBlock {
        let source_info = self.source_info();
        self.blocks.push(BasicBlockData {
            statements,
            terminator: Some(Terminator { source_info, kind }),
            is_cleanup,
        })
    }

    fn make_statement(&self, kind: StatementKind<'tcx>) -> Statement<'tcx> {
        Statement {
            source_info: self.source_info(),
            kind,
        }
    }

    fn copy_shim(&mut self) {
        let rcvr = Lvalue::Local(Local::new(1+0)).deref();
        let ret_statement = self.make_statement(
            StatementKind::Assign(
                Lvalue::Local(RETURN_POINTER),
                Rvalue::Use(Operand::Consume(rcvr))
            )
        );
        self.block(vec![ret_statement], TerminatorKind::Return, false);
    }

    fn make_lvalue(&mut self, mutability: Mutability, ty: ty::Ty<'tcx>) -> Lvalue<'tcx> {
        let span = self.span;
        Lvalue::Local(
            self.local_decls.push(temp_decl(mutability, ty, span))
        )
    }

    fn make_clone_call(
        &mut self,
        ty: ty::Ty<'tcx>,
        rcvr_field: Lvalue<'tcx>,
        next: BasicBlock,
        cleanup: BasicBlock
    ) -> Lvalue<'tcx> {
        let tcx = self.tcx;

        let substs = Substs::for_item(
            tcx,
            self.def_id,
            |_, _| tcx.types.re_erased,
            |_, _| ty
        );

        // `func == Clone::clone(&ty) -> ty`
        let func = Operand::Constant(box Constant {
            span: self.span,
            ty: tcx.mk_fn_def(self.def_id, substs),
            literal: Literal::Value {
                value: ConstVal::Function(self.def_id, substs),
            },
        });

        let ref_loc = self.make_lvalue(
            Mutability::Not,
            tcx.mk_ref(tcx.types.re_erased, ty::TypeAndMut {
                ty,
                mutbl: hir::Mutability::MutImmutable,
            })
        );

        let loc = self.make_lvalue(Mutability::Not, ty);

        // `let ref_loc: &ty = &rcvr_field;`
        let statement = self.make_statement(
            StatementKind::Assign(
                ref_loc.clone(),
                Rvalue::Ref(tcx.types.re_erased, BorrowKind::Shared, rcvr_field)
            )
        );

        // `let loc = Clone::clone(ref_loc);`
        self.block(vec![statement], TerminatorKind::Call {
            func,
            args: vec![Operand::Consume(ref_loc)],
            destination: Some((loc.clone(), next)),
            cleanup: Some(cleanup),
        }, false);

        loc
    }

    fn loop_header(
        &mut self,
        beg: Lvalue<'tcx>,
        end: Lvalue<'tcx>,
        loop_body: BasicBlock,
        loop_end: BasicBlock,
        is_cleanup: bool
    ) {
        let tcx = self.tcx;

        let cond = self.make_lvalue(Mutability::Mut, tcx.types.bool);
        let compute_cond = self.make_statement(
            StatementKind::Assign(
                cond.clone(),
                Rvalue::BinaryOp(BinOp::Ne, Operand::Consume(end), Operand::Consume(beg))
            )
        );

        // `if end != beg { goto loop_body; } else { goto loop_end; }`
        self.block(
            vec![compute_cond],
            TerminatorKind::if_(tcx, Operand::Consume(cond), loop_body, loop_end),
            is_cleanup
        );
    }

    fn make_usize(&self, value: usize) -> Box<Constant<'tcx>> {
        let value = ConstUsize::new(value as u64, self.tcx.sess.target.uint_type).unwrap();
        box Constant {
            span: self.span,
            ty: self.tcx.types.usize,
            literal: Literal::Value {
                value: ConstVal::Integral(ConstInt::Usize(value))
            }
        }
    }

    fn array_shim(&mut self, ty: ty::Ty<'tcx>, len: usize) {
        let tcx = self.tcx;
        let span = self.span;
        let rcvr = Lvalue::Local(Local::new(1+0)).deref();

        let beg = self.local_decls.push(temp_decl(Mutability::Mut, tcx.types.usize, span));
        let end = self.make_lvalue(Mutability::Not, tcx.types.usize);
        let ret = self.make_lvalue(Mutability::Mut, tcx.mk_array(ty, len));

        // BB #0
        // `let mut beg = 0;`
        // `let end = len;`
        // `goto #1;`
        let inits = vec![
            self.make_statement(
                StatementKind::Assign(
                    Lvalue::Local(beg),
                    Rvalue::Use(Operand::Constant(self.make_usize(0)))
                )
            ),
            self.make_statement(
                StatementKind::Assign(
                    end.clone(),
                    Rvalue::Use(Operand::Constant(self.make_usize(len)))
                )
            )
        ];
        self.block(inits, TerminatorKind::Goto { target: BasicBlock::new(1) }, false);

        // BB #1: loop {
        //     BB #2;
        //     BB #3;
        // }
        // BB #4;
        self.loop_header(Lvalue::Local(beg), end, BasicBlock::new(2), BasicBlock::new(4), false);

        // BB #2
        // `let cloned = Clone::clone(rcvr[beg])`;
        // Goto #3 if ok, #5 if unwinding happens.
        let rcvr_field = rcvr.clone().index(beg);
        let cloned = self.make_clone_call(ty, rcvr_field, BasicBlock::new(3), BasicBlock::new(5));

        // BB #3
        // `ret[beg] = cloned;`
        // `beg = beg + 1;`
        // `goto #1`;
        let ret_field = ret.clone().index(beg);
        let statements = vec![
            self.make_statement(
                StatementKind::Assign(
                    ret_field,
                    Rvalue::Use(Operand::Consume(cloned))
                )
            ),
            self.make_statement(
                StatementKind::Assign(
                    Lvalue::Local(beg),
                    Rvalue::BinaryOp(
                        BinOp::Add,
                        Operand::Consume(Lvalue::Local(beg)),
                        Operand::Constant(self.make_usize(1))
                    )
                )
            )
        ];
        self.block(statements, TerminatorKind::Goto { target: BasicBlock::new(1) }, false);

        // BB #4
        // `return ret;`
        let ret_statement = self.make_statement(
            StatementKind::Assign(
                Lvalue::Local(RETURN_POINTER),
                Rvalue::Use(Operand::Consume(ret.clone())),
            )
        );
        self.block(vec![ret_statement], TerminatorKind::Return, false);

        // BB #5 (cleanup)
        // `let end = beg;`
        // `let mut beg = 0;`
        // goto #6;
        let end = beg;
        let beg = self.local_decls.push(temp_decl(Mutability::Mut, tcx.types.usize, span));
        let init = self.make_statement(
            StatementKind::Assign(
                Lvalue::Local(beg),
                Rvalue::Use(Operand::Constant(self.make_usize(0)))
            )
        );
        self.block(vec![init], TerminatorKind::Goto { target: BasicBlock::new(6) }, true);

        // BB #6 (cleanup): loop {
        //     BB #7;
        //     BB #8;
        // }
        // BB #9;
        self.loop_header(Lvalue::Local(beg), Lvalue::Local(end),
                         BasicBlock::new(7), BasicBlock::new(9), true);

        // BB #7 (cleanup)
        // `drop(ret[beg])`;
        self.block(vec![], TerminatorKind::Drop {
            location: ret.index(beg),
            target: BasicBlock::new(8),
            unwind: None,
        }, true);

        // BB #8 (cleanup)
        // `beg = beg + 1;`
        // `goto #6;`
        let statement = self.make_statement(
            StatementKind::Assign(
                Lvalue::Local(beg),
                Rvalue::BinaryOp(
                    BinOp::Add,
                    Operand::Consume(Lvalue::Local(beg)),
                    Operand::Constant(self.make_usize(1))
                )
            )
        );
        self.block(vec![statement], TerminatorKind::Goto { target: BasicBlock::new(6) }, true);

        // BB #9 (resume)
        self.block(vec![], TerminatorKind::Resume, true);
    }

    fn tuple_shim(&mut self, tys: &ty::Slice<ty::Ty<'tcx>>) {
        let rcvr = Lvalue::Local(Local::new(1+0)).deref();

        let mut returns = Vec::new();
        for (i, ity) in tys.iter().enumerate() {
            let rcvr_field = rcvr.clone().field(Field::new(i), *ity);

            // BB #(2i)
            // `returns[i] = Clone::clone(&rcvr.i);`
            // Goto #(2i + 2) if ok, #(2i + 1) if unwinding happens.
            returns.push(
                self.make_clone_call(
                    *ity,
                    rcvr_field,
                    BasicBlock::new(2 * i + 2),
                    BasicBlock::new(2 * i + 1),
                )
            );

            // BB #(2i + 1) (cleanup)
            if i == 0 {
                // Nothing to drop, just resume.
                self.block(vec![], TerminatorKind::Resume, true);
            } else {
                // Drop previous field and goto previous cleanup block.
                self.block(vec![], TerminatorKind::Drop {
                    location: returns[i - 1].clone(),
                    target: BasicBlock::new(2 * i - 1),
                    unwind: None,
                }, true);
            }
        }

        // `return (returns[0], returns[1], ..., returns[tys.len() - 1]);`
        let ret_statement = self.make_statement(
            StatementKind::Assign(
                Lvalue::Local(RETURN_POINTER),
                Rvalue::Aggregate(
                    box AggregateKind::Tuple,
                    returns.into_iter().map(Operand::Consume).collect()
                )
            )
        );
       self.block(vec![ret_statement], TerminatorKind::Return, false);
    }
}

/// Build a "call" shim for `def_id`. The shim calls the
/// function specified by `call_kind`, first adjusting its first
/// argument according to `rcvr_adjustment`.
///
/// If `untuple_args` is a vec of types, the second argument of the
/// function will be untupled as these types.
fn build_call_shim<'a, 'tcx>(tcx: ty::TyCtxt<'a, 'tcx, 'tcx>,
                             def_id: DefId,
                             rcvr_adjustment: Adjustment,
                             call_kind: CallKind,
                             untuple_args: Option<&[Ty<'tcx>]>)
                             -> Mir<'tcx>
{
    debug!("build_call_shim(def_id={:?}, rcvr_adjustment={:?}, \
            call_kind={:?}, untuple_args={:?})",
           def_id, rcvr_adjustment, call_kind, untuple_args);

    let sig = tcx.fn_sig(def_id);
    let sig = tcx.erase_late_bound_regions(&sig);
    let span = tcx.def_span(def_id);

    debug!("build_call_shim: sig={:?}", sig);

    let mut local_decls = local_decls_for_sig(&sig, span);
    let source_info = SourceInfo { span, scope: ARGUMENT_VISIBILITY_SCOPE };

    let rcvr_arg = Local::new(1+0);
    let rcvr_l = Lvalue::Local(rcvr_arg);
    let mut statements = vec![];

    let rcvr = match rcvr_adjustment {
        Adjustment::Identity => Operand::Consume(rcvr_l),
        Adjustment::Deref => Operand::Consume(rcvr_l.deref()),
        Adjustment::RefMut => {
            // let rcvr = &mut rcvr;
            let ref_rcvr = local_decls.push(temp_decl(
                Mutability::Not,
                tcx.mk_ref(tcx.types.re_erased, ty::TypeAndMut {
                    ty: sig.inputs()[0],
                    mutbl: hir::Mutability::MutMutable
                }),
                span
            ));
            statements.push(Statement {
                source_info,
                kind: StatementKind::Assign(
                    Lvalue::Local(ref_rcvr),
                    Rvalue::Ref(tcx.types.re_erased, BorrowKind::Mut, rcvr_l)
                )
            });
            Operand::Consume(Lvalue::Local(ref_rcvr))
        }
    };

    let (callee, mut args) = match call_kind {
        CallKind::Indirect => (rcvr, vec![]),
        CallKind::Direct(def_id) => (
            Operand::Constant(box Constant {
                span,
                ty: tcx.type_of(def_id),
                literal: Literal::Value {
                    value: ConstVal::Function(def_id,
                        Substs::identity_for_item(tcx, def_id)),
                },
            }),
            vec![rcvr]
        )
    };

    if let Some(untuple_args) = untuple_args {
        args.extend(untuple_args.iter().enumerate().map(|(i, ity)| {
            let arg_lv = Lvalue::Local(Local::new(1+1));
            Operand::Consume(arg_lv.field(Field::new(i), *ity))
        }));
    } else {
        args.extend((1..sig.inputs().len()).map(|i| {
            Operand::Consume(Lvalue::Local(Local::new(1+i)))
        }));
    }

    let mut blocks = IndexVec::new();
    let block = |blocks: &mut IndexVec<_, _>, statements, kind, is_cleanup| {
        blocks.push(BasicBlockData {
            statements,
            terminator: Some(Terminator { source_info, kind }),
            is_cleanup
        })
    };

    // BB #0
    block(&mut blocks, statements, TerminatorKind::Call {
        func: callee,
        args,
        destination: Some((Lvalue::Local(RETURN_POINTER),
                           BasicBlock::new(1))),
        cleanup: if let Adjustment::RefMut = rcvr_adjustment {
            Some(BasicBlock::new(3))
        } else {
            None
        }
    }, false);

    if let Adjustment::RefMut = rcvr_adjustment {
        // BB #1 - drop for Self
        block(&mut blocks, vec![], TerminatorKind::Drop {
            location: Lvalue::Local(rcvr_arg),
            target: BasicBlock::new(2),
            unwind: None
        }, false);
    }
    // BB #1/#2 - return
    block(&mut blocks, vec![], TerminatorKind::Return, false);
    if let Adjustment::RefMut = rcvr_adjustment {
        // BB #3 - drop if closure panics
        block(&mut blocks, vec![], TerminatorKind::Drop {
            location: Lvalue::Local(rcvr_arg),
            target: BasicBlock::new(4),
            unwind: None
        }, true);

        // BB #4 - resume
        block(&mut blocks, vec![], TerminatorKind::Resume, true);
    }

    let mut mir = Mir::new(
        blocks,
        IndexVec::from_elem_n(
            VisibilityScopeData { span: span, parent_scope: None }, 1
        ),
        IndexVec::new(),
        sig.output(),
        None,
        local_decls,
        sig.inputs().len(),
        vec![],
        span
    );
    if let Abi::RustCall = sig.abi {
        mir.spread_arg = Some(Local::new(sig.inputs().len()));
    }
    mir
}

pub fn build_adt_ctor<'a, 'gcx, 'tcx>(infcx: &infer::InferCtxt<'a, 'gcx, 'tcx>,
                                      ctor_id: ast::NodeId,
                                      fields: &[hir::StructField],
                                      span: Span)
                                      -> (Mir<'tcx>, MirSource)
{
    let tcx = infcx.tcx;
    let def_id = tcx.hir.local_def_id(ctor_id);
    let sig = tcx.no_late_bound_regions(&tcx.fn_sig(def_id))
        .expect("LBR in ADT constructor signature");
    let sig = tcx.erase_regions(&sig);

    let (adt_def, substs) = match sig.output().sty {
        ty::TyAdt(adt_def, substs) => (adt_def, substs),
        _ => bug!("unexpected type for ADT ctor {:?}", sig.output())
    };

    debug!("build_ctor: def_id={:?} sig={:?} fields={:?}", def_id, sig, fields);

    let local_decls = local_decls_for_sig(&sig, span);

    let source_info = SourceInfo {
        span,
        scope: ARGUMENT_VISIBILITY_SCOPE
    };

    let variant_no = if adt_def.is_enum() {
        adt_def.variant_index_with_id(def_id)
    } else {
        0
    };

    // return = ADT(arg0, arg1, ...); return
    let start_block = BasicBlockData {
        statements: vec![Statement {
            source_info,
            kind: StatementKind::Assign(
                Lvalue::Local(RETURN_POINTER),
                Rvalue::Aggregate(
                    box AggregateKind::Adt(adt_def, variant_no, substs, None),
                    (1..sig.inputs().len()+1).map(|i| {
                        Operand::Consume(Lvalue::Local(Local::new(i)))
                    }).collect()
                )
            )
        }],
        terminator: Some(Terminator {
            source_info,
            kind: TerminatorKind::Return,
        }),
        is_cleanup: false
    };

    let mir = Mir::new(
        IndexVec::from_elem_n(start_block, 1),
        IndexVec::from_elem_n(
            VisibilityScopeData { span: span, parent_scope: None }, 1
        ),
        IndexVec::new(),
        sig.output(),
        None,
        local_decls,
        sig.inputs().len(),
        vec![],
        span
    );
    (mir, MirSource::Fn(ctor_id))
}
