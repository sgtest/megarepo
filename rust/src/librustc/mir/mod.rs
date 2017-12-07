// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! MIR datatypes and passes. See [the README](README.md) for details.

use graphviz::IntoCow;
use middle::const_val::ConstVal;
use middle::region;
use rustc_const_math::{ConstUsize, ConstInt, ConstMathErr};
use rustc_data_structures::indexed_vec::{IndexVec, Idx};
use rustc_data_structures::control_flow_graph::dominators::{Dominators, dominators};
use rustc_data_structures::control_flow_graph::{GraphPredecessors, GraphSuccessors};
use rustc_data_structures::control_flow_graph::ControlFlowGraph;
use rustc_serialize as serialize;
use hir::def::CtorKind;
use hir::def_id::DefId;
use ty::subst::{Subst, Substs};
use ty::{self, AdtDef, ClosureSubsts, Region, Ty, TyCtxt, GeneratorInterior};
use ty::fold::{TypeFoldable, TypeFolder, TypeVisitor};
use util::ppaux;
use std::slice;
use hir::{self, InlineAsm};
use std::ascii;
use std::borrow::{Cow};
use std::cell::Ref;
use std::fmt::{self, Debug, Formatter, Write};
use std::{iter, u32};
use std::ops::{Index, IndexMut};
use std::rc::Rc;
use std::vec::IntoIter;
use syntax::ast::{self, Name};
use syntax::symbol::InternedString;
use syntax_pos::Span;

mod cache;
pub mod tcx;
pub mod visit;
pub mod traversal;

/// Types for locals
type LocalDecls<'tcx> = IndexVec<Local, LocalDecl<'tcx>>;

pub trait HasLocalDecls<'tcx> {
    fn local_decls(&self) -> &LocalDecls<'tcx>;
}

impl<'tcx> HasLocalDecls<'tcx> for LocalDecls<'tcx> {
    fn local_decls(&self) -> &LocalDecls<'tcx> {
        self
    }
}

impl<'tcx> HasLocalDecls<'tcx> for Mir<'tcx> {
    fn local_decls(&self) -> &LocalDecls<'tcx> {
        &self.local_decls
    }
}

/// Lowered representation of a single function.
#[derive(Clone, RustcEncodable, RustcDecodable, Debug)]
pub struct Mir<'tcx> {
    /// List of basic blocks. References to basic block use a newtyped index type `BasicBlock`
    /// that indexes into this vector.
    basic_blocks: IndexVec<BasicBlock, BasicBlockData<'tcx>>,

    /// List of visibility (lexical) scopes; these are referenced by statements
    /// and used (eventually) for debuginfo. Indexed by a `VisibilityScope`.
    pub visibility_scopes: IndexVec<VisibilityScope, VisibilityScopeData>,

    /// Crate-local information for each visibility scope, that can't (and
    /// needn't) be tracked across crates.
    pub visibility_scope_info: ClearCrossCrate<IndexVec<VisibilityScope, VisibilityScopeInfo>>,

    /// Rvalues promoted from this function, such as borrows of constants.
    /// Each of them is the Mir of a constant with the fn's type parameters
    /// in scope, but a separate set of locals.
    pub promoted: IndexVec<Promoted, Mir<'tcx>>,

    /// Yield type of the function, if it is a generator.
    pub yield_ty: Option<Ty<'tcx>>,

    /// Generator drop glue
    pub generator_drop: Option<Box<Mir<'tcx>>>,

    /// The layout of a generator. Produced by the state transformation.
    pub generator_layout: Option<GeneratorLayout<'tcx>>,

    /// Declarations of locals.
    ///
    /// The first local is the return value pointer, followed by `arg_count`
    /// locals for the function arguments, followed by any user-declared
    /// variables and temporaries.
    pub local_decls: LocalDecls<'tcx>,

    /// Number of arguments this function takes.
    ///
    /// Starting at local 1, `arg_count` locals will be provided by the caller
    /// and can be assumed to be initialized.
    ///
    /// If this MIR was built for a constant, this will be 0.
    pub arg_count: usize,

    /// Names and capture modes of all the closure upvars, assuming
    /// the first argument is either the closure or a reference to it.
    pub upvar_decls: Vec<UpvarDecl>,

    /// Mark an argument local (which must be a tuple) as getting passed as
    /// its individual components at the LLVM level.
    ///
    /// This is used for the "rust-call" ABI.
    pub spread_arg: Option<Local>,

    /// A span representing this MIR, for error reporting
    pub span: Span,

    /// A cache for various calculations
    cache: cache::Cache
}

/// where execution begins
pub const START_BLOCK: BasicBlock = BasicBlock(0);

impl<'tcx> Mir<'tcx> {
    pub fn new(basic_blocks: IndexVec<BasicBlock, BasicBlockData<'tcx>>,
               visibility_scopes: IndexVec<VisibilityScope, VisibilityScopeData>,
               visibility_scope_info: ClearCrossCrate<IndexVec<VisibilityScope,
                                                               VisibilityScopeInfo>>,
               promoted: IndexVec<Promoted, Mir<'tcx>>,
               yield_ty: Option<Ty<'tcx>>,
               local_decls: IndexVec<Local, LocalDecl<'tcx>>,
               arg_count: usize,
               upvar_decls: Vec<UpvarDecl>,
               span: Span) -> Self
    {
        // We need `arg_count` locals, and one for the return place
        assert!(local_decls.len() >= arg_count + 1,
            "expected at least {} locals, got {}", arg_count + 1, local_decls.len());

        Mir {
            basic_blocks,
            visibility_scopes,
            visibility_scope_info,
            promoted,
            yield_ty,
            generator_drop: None,
            generator_layout: None,
            local_decls,
            arg_count,
            upvar_decls,
            spread_arg: None,
            span,
            cache: cache::Cache::new()
        }
    }

    #[inline]
    pub fn basic_blocks(&self) -> &IndexVec<BasicBlock, BasicBlockData<'tcx>> {
        &self.basic_blocks
    }

    #[inline]
    pub fn basic_blocks_mut(&mut self) -> &mut IndexVec<BasicBlock, BasicBlockData<'tcx>> {
        self.cache.invalidate();
        &mut self.basic_blocks
    }

    #[inline]
    pub fn basic_blocks_and_local_decls_mut(&mut self) -> (
        &mut IndexVec<BasicBlock, BasicBlockData<'tcx>>,
        &mut LocalDecls<'tcx>,
    ) {
        self.cache.invalidate();
        (&mut self.basic_blocks, &mut self.local_decls)
    }

    #[inline]
    pub fn predecessors(&self) -> Ref<IndexVec<BasicBlock, Vec<BasicBlock>>> {
        self.cache.predecessors(self)
    }

    #[inline]
    pub fn predecessors_for(&self, bb: BasicBlock) -> Ref<Vec<BasicBlock>> {
        Ref::map(self.predecessors(), |p| &p[bb])
    }

    #[inline]
    pub fn dominators(&self) -> Dominators<BasicBlock> {
        dominators(self)
    }

    #[inline]
    pub fn local_kind(&self, local: Local) -> LocalKind {
        let index = local.0 as usize;
        if index == 0 {
            debug_assert!(self.local_decls[local].mutability == Mutability::Mut,
                          "return place should be mutable");

            LocalKind::ReturnPointer
        } else if index < self.arg_count + 1 {
            LocalKind::Arg
        } else if self.local_decls[local].name.is_some() {
            LocalKind::Var
        } else {
            debug_assert!(self.local_decls[local].mutability == Mutability::Mut,
                          "temp should be mutable");

            LocalKind::Temp
        }
    }

    /// Returns an iterator over all temporaries.
    #[inline]
    pub fn temps_iter<'a>(&'a self) -> impl Iterator<Item=Local> + 'a {
        (self.arg_count+1..self.local_decls.len()).filter_map(move |index| {
            let local = Local::new(index);
            if self.local_decls[local].is_user_variable {
                None
            } else {
                Some(local)
            }
        })
    }

    /// Returns an iterator over all user-declared locals.
    #[inline]
    pub fn vars_iter<'a>(&'a self) -> impl Iterator<Item=Local> + 'a {
        (self.arg_count+1..self.local_decls.len()).filter_map(move |index| {
            let local = Local::new(index);
            if self.local_decls[local].is_user_variable {
                Some(local)
            } else {
                None
            }
        })
    }

    /// Returns an iterator over all function arguments.
    #[inline]
    pub fn args_iter(&self) -> impl Iterator<Item=Local> {
        let arg_count = self.arg_count;
        (1..arg_count+1).map(Local::new)
    }

    /// Returns an iterator over all user-defined variables and compiler-generated temporaries (all
    /// locals that are neither arguments nor the return place).
    #[inline]
    pub fn vars_and_temps_iter(&self) -> impl Iterator<Item=Local> {
        let arg_count = self.arg_count;
        let local_count = self.local_decls.len();
        (arg_count+1..local_count).map(Local::new)
    }

    /// Changes a statement to a nop. This is both faster than deleting instructions and avoids
    /// invalidating statement indices in `Location`s.
    pub fn make_statement_nop(&mut self, location: Location) {
        let block = &mut self[location.block];
        debug_assert!(location.statement_index < block.statements.len());
        block.statements[location.statement_index].make_nop()
    }

    /// Returns the source info associated with `location`.
    pub fn source_info(&self, location: Location) -> &SourceInfo {
        let block = &self[location.block];
        let stmts = &block.statements;
        let idx = location.statement_index;
        if idx < stmts.len() {
            &stmts[idx].source_info
        } else {
            assert!(idx == stmts.len());
            &block.terminator().source_info
        }
    }

    /// Return the return type, it always return first element from `local_decls` array
    pub fn return_ty(&self) -> Ty<'tcx> {
        self.local_decls[RETURN_PLACE].ty
    }
}

#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct VisibilityScopeInfo {
    /// A NodeId with lint levels equivalent to this scope's lint levels.
    pub lint_root: ast::NodeId,
    /// The unsafe block that contains this node.
    pub safety: Safety,
}

#[derive(Copy, Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum Safety {
    Safe,
    /// Unsafe because of a PushUnsafeBlock
    BuiltinUnsafe,
    /// Unsafe because of an unsafe fn
    FnUnsafe,
    /// Unsafe because of an `unsafe` block
    ExplicitUnsafe(ast::NodeId)
}

impl_stable_hash_for!(struct Mir<'tcx> {
    basic_blocks,
    visibility_scopes,
    visibility_scope_info,
    promoted,
    yield_ty,
    generator_drop,
    generator_layout,
    local_decls,
    arg_count,
    upvar_decls,
    spread_arg,
    span,
    cache
});

impl<'tcx> Index<BasicBlock> for Mir<'tcx> {
    type Output = BasicBlockData<'tcx>;

    #[inline]
    fn index(&self, index: BasicBlock) -> &BasicBlockData<'tcx> {
        &self.basic_blocks()[index]
    }
}

impl<'tcx> IndexMut<BasicBlock> for Mir<'tcx> {
    #[inline]
    fn index_mut(&mut self, index: BasicBlock) -> &mut BasicBlockData<'tcx> {
        &mut self.basic_blocks_mut()[index]
    }
}

#[derive(Clone, Debug)]
pub enum ClearCrossCrate<T> {
    Clear,
    Set(T)
}

impl<T: serialize::Encodable> serialize::UseSpecializedEncodable for ClearCrossCrate<T> {}
impl<T: serialize::Decodable> serialize::UseSpecializedDecodable for ClearCrossCrate<T> {}

/// Grouped information about the source code origin of a MIR entity.
/// Intended to be inspected by diagnostics and debuginfo.
/// Most passes can work with it as a whole, within a single function.
#[derive(Copy, Clone, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable, Hash)]
pub struct SourceInfo {
    /// Source span for the AST pertaining to this MIR entity.
    pub span: Span,

    /// The lexical visibility scope, i.e. which bindings can be seen.
    pub scope: VisibilityScope
}

///////////////////////////////////////////////////////////////////////////
// Mutability and borrow kinds

#[derive(Copy, Clone, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum Mutability {
    Mut,
    Not,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum BorrowKind {
    /// Data must be immutable and is aliasable.
    Shared,

    /// Data must be immutable but not aliasable.  This kind of borrow
    /// cannot currently be expressed by the user and is used only in
    /// implicit closure bindings. It is needed when you the closure
    /// is borrowing or mutating a mutable referent, e.g.:
    ///
    ///    let x: &mut isize = ...;
    ///    let y = || *x += 5;
    ///
    /// If we were to try to translate this closure into a more explicit
    /// form, we'd encounter an error with the code as written:
    ///
    ///    struct Env { x: & &mut isize }
    ///    let x: &mut isize = ...;
    ///    let y = (&mut Env { &x }, fn_ptr);  // Closure is pair of env and fn
    ///    fn fn_ptr(env: &mut Env) { **env.x += 5; }
    ///
    /// This is then illegal because you cannot mutate a `&mut` found
    /// in an aliasable location. To solve, you'd have to translate with
    /// an `&mut` borrow:
    ///
    ///    struct Env { x: & &mut isize }
    ///    let x: &mut isize = ...;
    ///    let y = (&mut Env { &mut x }, fn_ptr); // changed from &x to &mut x
    ///    fn fn_ptr(env: &mut Env) { **env.x += 5; }
    ///
    /// Now the assignment to `**env.x` is legal, but creating a
    /// mutable pointer to `x` is not because `x` is not mutable. We
    /// could fix this by declaring `x` as `let mut x`. This is ok in
    /// user code, if awkward, but extra weird for closures, since the
    /// borrow is hidden.
    ///
    /// So we introduce a "unique imm" borrow -- the referent is
    /// immutable, but not aliasable. This solves the problem. For
    /// simplicity, we don't give users the way to express this
    /// borrow, it's just used when translating closures.
    Unique,

    /// Data is mutable and not aliasable.
    Mut,
}

///////////////////////////////////////////////////////////////////////////
// Variables and temps

newtype_index!(Local
    {
        DEBUG_FORMAT = "_{}",
        const RETURN_PLACE = 0,
    });

/// Classifies locals into categories. See `Mir::local_kind`.
#[derive(PartialEq, Eq, Debug)]
pub enum LocalKind {
    /// User-declared variable binding
    Var,
    /// Compiler-introduced temporary
    Temp,
    /// Function argument
    Arg,
    /// Location of function's return value
    ReturnPointer,
}

/// A MIR local.
///
/// This can be a binding declared by the user, a temporary inserted by the compiler, a function
/// argument, or the return place.
#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct LocalDecl<'tcx> {
    /// `let mut x` vs `let x`.
    ///
    /// Temporaries and the return place are always mutable.
    pub mutability: Mutability,

    /// True if this corresponds to a user-declared local variable.
    pub is_user_variable: bool,

    /// True if this is an internal local
    ///
    /// These locals are not based on types in the source code and are only used
    /// for a few desugarings at the moment.
    ///
    /// The generator transformation will sanity check the locals which are live
    /// across a suspension point against the type components of the generator
    /// which type checking knows are live across a suspension point. We need to
    /// flag drop flags to avoid triggering this check as they are introduced
    /// after typeck.
    ///
    /// Unsafety checking will also ignore dereferences of these locals,
    /// so they can be used for raw pointers only used in a desugaring.
    ///
    /// This should be sound because the drop flags are fully algebraic, and
    /// therefore don't affect the OIBIT or outlives properties of the
    /// generator.
    pub internal: bool,

    /// Type of this local.
    pub ty: Ty<'tcx>,

    /// Name of the local, used in debuginfo and pretty-printing.
    ///
    /// Note that function arguments can also have this set to `Some(_)`
    /// to generate better debuginfo.
    pub name: Option<Name>,

    /// Source info of the local.
    pub source_info: SourceInfo,

    /// The *lexical* visibility scope the local is defined
    /// in. If the local was defined in a let-statement, this
    /// is *within* the let-statement, rather than outside
    /// of it.
    pub lexical_scope: VisibilityScope,
}

impl<'tcx> LocalDecl<'tcx> {
    /// Create a new `LocalDecl` for a temporary.
    #[inline]
    pub fn new_temp(ty: Ty<'tcx>, span: Span) -> Self {
        LocalDecl {
            mutability: Mutability::Mut,
            ty,
            name: None,
            source_info: SourceInfo {
                span,
                scope: ARGUMENT_VISIBILITY_SCOPE
            },
            lexical_scope: ARGUMENT_VISIBILITY_SCOPE,
            internal: false,
            is_user_variable: false
        }
    }

    /// Create a new `LocalDecl` for a internal temporary.
    #[inline]
    pub fn new_internal(ty: Ty<'tcx>, span: Span) -> Self {
        LocalDecl {
            mutability: Mutability::Mut,
            ty,
            name: None,
            source_info: SourceInfo {
                span,
                scope: ARGUMENT_VISIBILITY_SCOPE
            },
            lexical_scope: ARGUMENT_VISIBILITY_SCOPE,
            internal: true,
            is_user_variable: false
        }
    }

    /// Builds a `LocalDecl` for the return place.
    ///
    /// This must be inserted into the `local_decls` list as the first local.
    #[inline]
    pub fn new_return_place(return_ty: Ty, span: Span) -> LocalDecl {
        LocalDecl {
            mutability: Mutability::Mut,
            ty: return_ty,
            source_info: SourceInfo {
                span,
                scope: ARGUMENT_VISIBILITY_SCOPE
            },
            lexical_scope: ARGUMENT_VISIBILITY_SCOPE,
            internal: false,
            name: None,     // FIXME maybe we do want some name here?
            is_user_variable: false
        }
    }
}

/// A closure capture, with its name and mode.
#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct UpvarDecl {
    pub debug_name: Name,

    /// If true, the capture is behind a reference.
    pub by_ref: bool,

    pub mutability: Mutability,
}

///////////////////////////////////////////////////////////////////////////
// BasicBlock

newtype_index!(BasicBlock { DEBUG_FORMAT = "bb{}" });

impl BasicBlock {
    pub fn start_location(self) -> Location {
        Location {
            block: self,
            statement_index: 0,
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// BasicBlockData and Terminator

#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct BasicBlockData<'tcx> {
    /// List of statements in this block.
    pub statements: Vec<Statement<'tcx>>,

    /// Terminator for this block.
    ///
    /// NB. This should generally ONLY be `None` during construction.
    /// Therefore, you should generally access it via the
    /// `terminator()` or `terminator_mut()` methods. The only
    /// exception is that certain passes, such as `simplify_cfg`, swap
    /// out the terminator temporarily with `None` while they continue
    /// to recurse over the set of basic blocks.
    pub terminator: Option<Terminator<'tcx>>,

    /// If true, this block lies on an unwind path. This is used
    /// during trans where distinct kinds of basic blocks may be
    /// generated (particularly for MSVC cleanup). Unwind blocks must
    /// only branch to other unwind blocks.
    pub is_cleanup: bool,
}

#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct Terminator<'tcx> {
    pub source_info: SourceInfo,
    pub kind: TerminatorKind<'tcx>
}

#[derive(Clone, RustcEncodable, RustcDecodable)]
pub enum TerminatorKind<'tcx> {
    /// block should have one successor in the graph; we jump there
    Goto {
        target: BasicBlock,
    },

    /// operand evaluates to an integer; jump depending on its value
    /// to one of the targets, and otherwise fallback to `otherwise`
    SwitchInt {
        /// discriminant value being tested
        discr: Operand<'tcx>,

        /// type of value being tested
        switch_ty: Ty<'tcx>,

        /// Possible values. The locations to branch to in each case
        /// are found in the corresponding indices from the `targets` vector.
        values: Cow<'tcx, [ConstInt]>,

        /// Possible branch sites. The last element of this vector is used
        /// for the otherwise branch, so targets.len() == values.len() + 1
        /// should hold.
        // This invariant is quite non-obvious and also could be improved.
        // One way to make this invariant is to have something like this instead:
        //
        // branches: Vec<(ConstInt, BasicBlock)>,
        // otherwise: Option<BasicBlock> // exhaustive if None
        //
        // However we’ve decided to keep this as-is until we figure a case
        // where some other approach seems to be strictly better than other.
        targets: Vec<BasicBlock>,
    },

    /// Indicates that the landing pad is finished and unwinding should
    /// continue. Emitted by build::scope::diverge_cleanup.
    Resume,

    /// Indicates a normal return. The return place should have
    /// been filled in by now. This should occur at most once.
    Return,

    /// Indicates a terminator that can never be reached.
    Unreachable,

    /// Drop the Place
    Drop {
        location: Place<'tcx>,
        target: BasicBlock,
        unwind: Option<BasicBlock>
    },

    /// Drop the Place and assign the new value over it. This ensures
    /// that the assignment to LV occurs *even if* the destructor for
    /// place unwinds. Its semantics are best explained by by the
    /// elaboration:
    ///
    /// ```
    /// BB0 {
    ///   DropAndReplace(LV <- RV, goto BB1, unwind BB2)
    /// }
    /// ```
    ///
    /// becomes
    ///
    /// ```
    /// BB0 {
    ///   Drop(LV, goto BB1, unwind BB2)
    /// }
    /// BB1 {
    ///   // LV is now unitialized
    ///   LV <- RV
    /// }
    /// BB2 {
    ///   // LV is now unitialized -- its dtor panicked
    ///   LV <- RV
    /// }
    /// ```
    DropAndReplace {
        location: Place<'tcx>,
        value: Operand<'tcx>,
        target: BasicBlock,
        unwind: Option<BasicBlock>,
    },

    /// Block ends with a call of a converging function
    Call {
        /// The function that’s being called
        func: Operand<'tcx>,
        /// Arguments the function is called with.
        /// These are owned by the callee, which is free to modify them.
        /// This allows the memory occupied by "by-value" arguments to be
        /// reused across function calls without duplicating the contents.
        args: Vec<Operand<'tcx>>,
        /// Destination for the return value. If some, the call is converging.
        destination: Option<(Place<'tcx>, BasicBlock)>,
        /// Cleanups to be done if the call unwinds.
        cleanup: Option<BasicBlock>
    },

    /// Jump to the target if the condition has the expected value,
    /// otherwise panic with a message and a cleanup target.
    Assert {
        cond: Operand<'tcx>,
        expected: bool,
        msg: AssertMessage<'tcx>,
        target: BasicBlock,
        cleanup: Option<BasicBlock>
    },

    /// A suspend point
    Yield {
        /// The value to return
        value: Operand<'tcx>,
        /// Where to resume to
        resume: BasicBlock,
        /// Cleanup to be done if the generator is dropped at this suspend point
        drop: Option<BasicBlock>,
    },

    /// Indicates the end of the dropping of a generator
    GeneratorDrop,

    FalseEdges {
        real_target: BasicBlock,
        imaginary_targets: Vec<BasicBlock>
    },
}

impl<'tcx> Terminator<'tcx> {
    pub fn successors(&self) -> Cow<[BasicBlock]> {
        self.kind.successors()
    }

    pub fn successors_mut(&mut self) -> Vec<&mut BasicBlock> {
        self.kind.successors_mut()
    }

    pub fn unwind_mut(&mut self) -> Option<&mut Option<BasicBlock>> {
        self.kind.unwind_mut()
    }
}

impl<'tcx> TerminatorKind<'tcx> {
    pub fn if_<'a, 'gcx>(tcx: TyCtxt<'a, 'gcx, 'tcx>, cond: Operand<'tcx>,
                         t: BasicBlock, f: BasicBlock) -> TerminatorKind<'tcx> {
        static BOOL_SWITCH_FALSE: &'static [ConstInt] = &[ConstInt::U8(0)];
        TerminatorKind::SwitchInt {
            discr: cond,
            switch_ty: tcx.types.bool,
            values: From::from(BOOL_SWITCH_FALSE),
            targets: vec![f, t],
        }
    }

    pub fn successors(&self) -> Cow<[BasicBlock]> {
        use self::TerminatorKind::*;
        match *self {
            Goto { target: ref b } => slice::from_ref(b).into_cow(),
            SwitchInt { targets: ref b, .. } => b[..].into_cow(),
            Resume | GeneratorDrop => (&[]).into_cow(),
            Return => (&[]).into_cow(),
            Unreachable => (&[]).into_cow(),
            Call { destination: Some((_, t)), cleanup: Some(c), .. } => vec![t, c].into_cow(),
            Call { destination: Some((_, ref t)), cleanup: None, .. } =>
                slice::from_ref(t).into_cow(),
            Call { destination: None, cleanup: Some(ref c), .. } => slice::from_ref(c).into_cow(),
            Call { destination: None, cleanup: None, .. } => (&[]).into_cow(),
            Yield { resume: t, drop: Some(c), .. } => vec![t, c].into_cow(),
            Yield { resume: ref t, drop: None, .. } => slice::from_ref(t).into_cow(),
            DropAndReplace { target, unwind: Some(unwind), .. } |
            Drop { target, unwind: Some(unwind), .. } => {
                vec![target, unwind].into_cow()
            }
            DropAndReplace { ref target, unwind: None, .. } |
            Drop { ref target, unwind: None, .. } => {
                slice::from_ref(target).into_cow()
            }
            Assert { target, cleanup: Some(unwind), .. } => vec![target, unwind].into_cow(),
            Assert { ref target, .. } => slice::from_ref(target).into_cow(),
            FalseEdges { ref real_target, ref imaginary_targets } => {
                let mut s = vec![*real_target];
                s.extend_from_slice(imaginary_targets);
                s.into_cow()
            }
        }
    }

    // FIXME: no mootable cow. I’m honestly not sure what a “cow” between `&mut [BasicBlock]` and
    // `Vec<&mut BasicBlock>` would look like in the first place.
    pub fn successors_mut(&mut self) -> Vec<&mut BasicBlock> {
        use self::TerminatorKind::*;
        match *self {
            Goto { target: ref mut b } => vec![b],
            SwitchInt { targets: ref mut b, .. } => b.iter_mut().collect(),
            Resume | GeneratorDrop => Vec::new(),
            Return => Vec::new(),
            Unreachable => Vec::new(),
            Call { destination: Some((_, ref mut t)), cleanup: Some(ref mut c), .. } => vec![t, c],
            Call { destination: Some((_, ref mut t)), cleanup: None, .. } => vec![t],
            Call { destination: None, cleanup: Some(ref mut c), .. } => vec![c],
            Call { destination: None, cleanup: None, .. } => vec![],
            Yield { resume: ref mut t, drop: Some(ref mut c), .. } => vec![t, c],
            Yield { resume: ref mut t, drop: None, .. } => vec![t],
            DropAndReplace { ref mut target, unwind: Some(ref mut unwind), .. } |
            Drop { ref mut target, unwind: Some(ref mut unwind), .. } => vec![target, unwind],
            DropAndReplace { ref mut target, unwind: None, .. } |
            Drop { ref mut target, unwind: None, .. } => {
                vec![target]
            }
            Assert { ref mut target, cleanup: Some(ref mut unwind), .. } => vec![target, unwind],
            Assert { ref mut target, .. } => vec![target],
            FalseEdges { ref mut real_target, ref mut imaginary_targets } => {
                let mut s = vec![real_target];
                s.extend(imaginary_targets.iter_mut());
                s
            }
        }
    }

    pub fn unwind_mut(&mut self) -> Option<&mut Option<BasicBlock>> {
        match *self {
            TerminatorKind::Goto { .. } |
            TerminatorKind::Resume |
            TerminatorKind::Return |
            TerminatorKind::Unreachable |
            TerminatorKind::GeneratorDrop |
            TerminatorKind::Yield { .. } |
            TerminatorKind::SwitchInt { .. } |
            TerminatorKind::FalseEdges { .. } => {
                None
            },
            TerminatorKind::Call { cleanup: ref mut unwind, .. } |
            TerminatorKind::Assert { cleanup: ref mut unwind, .. } |
            TerminatorKind::DropAndReplace { ref mut unwind, .. } |
            TerminatorKind::Drop { ref mut unwind, .. } => {
                Some(unwind)
            }
        }
    }
}

impl<'tcx> BasicBlockData<'tcx> {
    pub fn new(terminator: Option<Terminator<'tcx>>) -> BasicBlockData<'tcx> {
        BasicBlockData {
            statements: vec![],
            terminator,
            is_cleanup: false,
        }
    }

    /// Accessor for terminator.
    ///
    /// Terminator may not be None after construction of the basic block is complete. This accessor
    /// provides a convenience way to reach the terminator.
    pub fn terminator(&self) -> &Terminator<'tcx> {
        self.terminator.as_ref().expect("invalid terminator state")
    }

    pub fn terminator_mut(&mut self) -> &mut Terminator<'tcx> {
        self.terminator.as_mut().expect("invalid terminator state")
    }

    pub fn retain_statements<F>(&mut self, mut f: F) where F: FnMut(&mut Statement) -> bool {
        for s in &mut self.statements {
            if !f(s) {
                s.kind = StatementKind::Nop;
            }
        }
    }
}

impl<'tcx> Debug for TerminatorKind<'tcx> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        self.fmt_head(fmt)?;
        let successors = self.successors();
        let labels = self.fmt_successor_labels();
        assert_eq!(successors.len(), labels.len());

        match successors.len() {
            0 => Ok(()),

            1 => write!(fmt, " -> {:?}", successors[0]),

            _ => {
                write!(fmt, " -> [")?;
                for (i, target) in successors.iter().enumerate() {
                    if i > 0 {
                        write!(fmt, ", ")?;
                    }
                    write!(fmt, "{}: {:?}", labels[i], target)?;
                }
                write!(fmt, "]")
            }

        }
    }
}

impl<'tcx> TerminatorKind<'tcx> {
    /// Write the "head" part of the terminator; that is, its name and the data it uses to pick the
    /// successor basic block, if any. The only information not included is the list of possible
    /// successors, which may be rendered differently between the text and the graphviz format.
    pub fn fmt_head<W: Write>(&self, fmt: &mut W) -> fmt::Result {
        use self::TerminatorKind::*;
        match *self {
            Goto { .. } => write!(fmt, "goto"),
            SwitchInt { discr: ref place, .. } => write!(fmt, "switchInt({:?})", place),
            Return => write!(fmt, "return"),
            GeneratorDrop => write!(fmt, "generator_drop"),
            Resume => write!(fmt, "resume"),
            Yield { ref value, .. } => write!(fmt, "_1 = suspend({:?})", value),
            Unreachable => write!(fmt, "unreachable"),
            Drop { ref location, .. } => write!(fmt, "drop({:?})", location),
            DropAndReplace { ref location, ref value, .. } =>
                write!(fmt, "replace({:?} <- {:?})", location, value),
            Call { ref func, ref args, ref destination, .. } => {
                if let Some((ref destination, _)) = *destination {
                    write!(fmt, "{:?} = ", destination)?;
                }
                write!(fmt, "{:?}(", func)?;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        write!(fmt, ", ")?;
                    }
                    write!(fmt, "{:?}", arg)?;
                }
                write!(fmt, ")")
            }
            Assert { ref cond, expected, ref msg, .. } => {
                write!(fmt, "assert(")?;
                if !expected {
                    write!(fmt, "!")?;
                }
                write!(fmt, "{:?}, ", cond)?;

                match *msg {
                    AssertMessage::BoundsCheck { ref len, ref index } => {
                        write!(fmt, "{:?}, {:?}, {:?}",
                               "index out of bounds: the len is {} but the index is {}",
                               len, index)?;
                    }
                    AssertMessage::Math(ref err) => {
                        write!(fmt, "{:?}", err.description())?;
                    }
                    AssertMessage::GeneratorResumedAfterReturn => {
                        write!(fmt, "{:?}", "generator resumed after completion")?;
                    }
                    AssertMessage::GeneratorResumedAfterPanic => {
                        write!(fmt, "{:?}", "generator resumed after panicking")?;
                    }
                }

                write!(fmt, ")")
            },
            FalseEdges { .. } => write!(fmt, "falseEdges")
        }
    }

    /// Return the list of labels for the edges to the successor basic blocks.
    pub fn fmt_successor_labels(&self) -> Vec<Cow<'static, str>> {
        use self::TerminatorKind::*;
        match *self {
            Return | Resume | Unreachable | GeneratorDrop => vec![],
            Goto { .. } => vec!["".into()],
            SwitchInt { ref values, .. } => {
                values.iter()
                      .map(|const_val| {
                          let mut buf = String::new();
                          fmt_const_val(&mut buf, &ConstVal::Integral(*const_val)).unwrap();
                          buf.into()
                      })
                      .chain(iter::once(String::from("otherwise").into()))
                      .collect()
            }
            Call { destination: Some(_), cleanup: Some(_), .. } =>
                vec!["return".into_cow(), "unwind".into_cow()],
            Call { destination: Some(_), cleanup: None, .. } => vec!["return".into_cow()],
            Call { destination: None, cleanup: Some(_), .. } => vec!["unwind".into_cow()],
            Call { destination: None, cleanup: None, .. } => vec![],
            Yield { drop: Some(_), .. } =>
                vec!["resume".into_cow(), "drop".into_cow()],
            Yield { drop: None, .. } => vec!["resume".into_cow()],
            DropAndReplace { unwind: None, .. } |
            Drop { unwind: None, .. } => vec!["return".into_cow()],
            DropAndReplace { unwind: Some(_), .. } |
            Drop { unwind: Some(_), .. } => {
                vec!["return".into_cow(), "unwind".into_cow()]
            }
            Assert { cleanup: None, .. } => vec!["".into()],
            Assert { .. } =>
                vec!["success".into_cow(), "unwind".into_cow()],
            FalseEdges { ref imaginary_targets, .. } => {
                let mut l = vec!["real".into()];
                l.resize(imaginary_targets.len() + 1, "imaginary".into());
                l
            }
        }
    }
}

#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum AssertMessage<'tcx> {
    BoundsCheck {
        len: Operand<'tcx>,
        index: Operand<'tcx>
    },
    Math(ConstMathErr),
    GeneratorResumedAfterReturn,
    GeneratorResumedAfterPanic,
}

///////////////////////////////////////////////////////////////////////////
// Statements

#[derive(Clone, RustcEncodable, RustcDecodable)]
pub struct Statement<'tcx> {
    pub source_info: SourceInfo,
    pub kind: StatementKind<'tcx>,
}

impl<'tcx> Statement<'tcx> {
    /// Changes a statement to a nop. This is both faster than deleting instructions and avoids
    /// invalidating statement indices in `Location`s.
    pub fn make_nop(&mut self) {
        self.kind = StatementKind::Nop
    }
}

#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum StatementKind<'tcx> {
    /// Write the RHS Rvalue to the LHS Place.
    Assign(Place<'tcx>, Rvalue<'tcx>),

    /// Write the discriminant for a variant to the enum Place.
    SetDiscriminant { place: Place<'tcx>, variant_index: usize },

    /// Start a live range for the storage of the local.
    StorageLive(Local),

    /// End the current live range for the storage of the local.
    StorageDead(Local),

    /// Execute a piece of inline Assembly.
    InlineAsm {
        asm: Box<InlineAsm>,
        outputs: Vec<Place<'tcx>>,
        inputs: Vec<Operand<'tcx>>
    },

    /// Assert the given places to be valid inhabitants of their type.  These statements are
    /// currently only interpreted by miri and only generated when "-Z mir-emit-validate" is passed.
    /// See <https://internals.rust-lang.org/t/types-as-contracts/5562/73> for more details.
    Validate(ValidationOp, Vec<ValidationOperand<'tcx, Place<'tcx>>>),

    /// Mark one terminating point of a region scope (i.e. static region).
    /// (The starting point(s) arise implicitly from borrows.)
    EndRegion(region::Scope),

    /// No-op. Useful for deleting instructions without affecting statement indices.
    Nop,
}

/// The `ValidationOp` describes what happens with each of the operands of a
/// `Validate` statement.
#[derive(Copy, Clone, RustcEncodable, RustcDecodable, PartialEq, Eq)]
pub enum ValidationOp {
    /// Recursively traverse the place following the type and validate that all type
    /// invariants are maintained.  Furthermore, acquire exclusive/read-only access to the
    /// memory reachable from the place.
    Acquire,
    /// Recursive traverse the *mutable* part of the type and relinquish all exclusive
    /// access.
    Release,
    /// Recursive traverse the *mutable* part of the type and relinquish all exclusive
    /// access *until* the given region ends.  Then, access will be recovered.
    Suspend(region::Scope),
}

impl Debug for ValidationOp {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        use self::ValidationOp::*;
        match *self {
            Acquire => write!(fmt, "Acquire"),
            Release => write!(fmt, "Release"),
            // (reuse lifetime rendering policy from ppaux.)
            Suspend(ref ce) => write!(fmt, "Suspend({})", ty::ReScope(*ce)),
        }
    }
}

// This is generic so that it can be reused by miri
#[derive(Clone, RustcEncodable, RustcDecodable)]
pub struct ValidationOperand<'tcx, T> {
    pub place: T,
    pub ty: Ty<'tcx>,
    pub re: Option<region::Scope>,
    pub mutbl: hir::Mutability,
}

impl<'tcx, T: Debug> Debug for ValidationOperand<'tcx, T> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        write!(fmt, "{:?}: {:?}", self.place, self.ty)?;
        if let Some(ce) = self.re {
            // (reuse lifetime rendering policy from ppaux.)
            write!(fmt, "/{}", ty::ReScope(ce))?;
        }
        if let hir::MutImmutable = self.mutbl {
            write!(fmt, " (imm)")?;
        }
        Ok(())
    }
}

impl<'tcx> Debug for Statement<'tcx> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        use self::StatementKind::*;
        match self.kind {
            Assign(ref place, ref rv) => write!(fmt, "{:?} = {:?}", place, rv),
            // (reuse lifetime rendering policy from ppaux.)
            EndRegion(ref ce) => write!(fmt, "EndRegion({})", ty::ReScope(*ce)),
            Validate(ref op, ref places) => write!(fmt, "Validate({:?}, {:?})", op, places),
            StorageLive(ref place) => write!(fmt, "StorageLive({:?})", place),
            StorageDead(ref place) => write!(fmt, "StorageDead({:?})", place),
            SetDiscriminant { ref place, variant_index } => {
                write!(fmt, "discriminant({:?}) = {:?}", place, variant_index)
            },
            InlineAsm { ref asm, ref outputs, ref inputs } => {
                write!(fmt, "asm!({:?} : {:?} : {:?})", asm, outputs, inputs)
            },
            Nop => write!(fmt, "nop"),
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Places

/// A path to a value; something that can be evaluated without
/// changing or disturbing program state.
#[derive(Clone, PartialEq, RustcEncodable, RustcDecodable)]
pub enum Place<'tcx> {
    /// local variable
    Local(Local),

    /// static or static mut variable
    Static(Box<Static<'tcx>>),

    /// projection out of a place (access a field, deref a pointer, etc)
    Projection(Box<PlaceProjection<'tcx>>),
}

/// The def-id of a static, along with its normalized type (which is
/// stored to avoid requiring normalization when reading MIR).
#[derive(Clone, PartialEq, RustcEncodable, RustcDecodable)]
pub struct Static<'tcx> {
    pub def_id: DefId,
    pub ty: Ty<'tcx>,
}

impl_stable_hash_for!(struct Static<'tcx> {
    def_id,
    ty
});

/// The `Projection` data structure defines things of the form `B.x`
/// or `*B` or `B[index]`. Note that it is parameterized because it is
/// shared between `Constant` and `Place`. See the aliases
/// `PlaceProjection` etc below.
#[derive(Clone, Debug, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable)]
pub struct Projection<'tcx, B, V, T> {
    pub base: B,
    pub elem: ProjectionElem<'tcx, V, T>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable)]
pub enum ProjectionElem<'tcx, V, T> {
    Deref,
    Field(Field, T),
    Index(V),

    /// These indices are generated by slice patterns. Easiest to explain
    /// by example:
    ///
    /// ```
    /// [X, _, .._, _, _] => { offset: 0, min_length: 4, from_end: false },
    /// [_, X, .._, _, _] => { offset: 1, min_length: 4, from_end: false },
    /// [_, _, .._, X, _] => { offset: 2, min_length: 4, from_end: true },
    /// [_, _, .._, _, X] => { offset: 1, min_length: 4, from_end: true },
    /// ```
    ConstantIndex {
        /// index or -index (in Python terms), depending on from_end
        offset: u32,
        /// thing being indexed must be at least this long
        min_length: u32,
        /// counting backwards from end?
        from_end: bool,
    },

    /// These indices are generated by slice patterns.
    ///
    /// slice[from:-to] in Python terms.
    Subslice {
        from: u32,
        to: u32,
    },

    /// "Downcast" to a variant of an ADT. Currently, we only introduce
    /// this for ADTs with more than one variant. It may be better to
    /// just introduce it always, or always for enums.
    Downcast(&'tcx AdtDef, usize),
}

/// Alias for projections as they appear in places, where the base is a place
/// and the index is a local.
pub type PlaceProjection<'tcx> = Projection<'tcx, Place<'tcx>, Local, Ty<'tcx>>;

/// Alias for projections as they appear in places, where the base is a place
/// and the index is a local.
pub type PlaceElem<'tcx> = ProjectionElem<'tcx, Local, Ty<'tcx>>;

newtype_index!(Field { DEBUG_FORMAT = "field[{}]" });

impl<'tcx> Place<'tcx> {
    pub fn field(self, f: Field, ty: Ty<'tcx>) -> Place<'tcx> {
        self.elem(ProjectionElem::Field(f, ty))
    }

    pub fn deref(self) -> Place<'tcx> {
        self.elem(ProjectionElem::Deref)
    }

    pub fn downcast(self, adt_def: &'tcx AdtDef, variant_index: usize) -> Place<'tcx> {
        self.elem(ProjectionElem::Downcast(adt_def, variant_index))
    }

    pub fn index(self, index: Local) -> Place<'tcx> {
        self.elem(ProjectionElem::Index(index))
    }

    pub fn elem(self, elem: PlaceElem<'tcx>) -> Place<'tcx> {
        Place::Projection(Box::new(PlaceProjection {
            base: self,
            elem,
        }))
    }
}

impl<'tcx> Debug for Place<'tcx> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        use self::Place::*;

        match *self {
            Local(id) => write!(fmt, "{:?}", id),
            Static(box self::Static { def_id, ty }) =>
                write!(fmt, "({}: {:?})", ty::tls::with(|tcx| tcx.item_path_str(def_id)), ty),
            Projection(ref data) =>
                match data.elem {
                    ProjectionElem::Downcast(ref adt_def, index) =>
                        write!(fmt, "({:?} as {})", data.base, adt_def.variants[index].name),
                    ProjectionElem::Deref =>
                        write!(fmt, "(*{:?})", data.base),
                    ProjectionElem::Field(field, ty) =>
                        write!(fmt, "({:?}.{:?}: {:?})", data.base, field.index(), ty),
                    ProjectionElem::Index(ref index) =>
                        write!(fmt, "{:?}[{:?}]", data.base, index),
                    ProjectionElem::ConstantIndex { offset, min_length, from_end: false } =>
                        write!(fmt, "{:?}[{:?} of {:?}]", data.base, offset, min_length),
                    ProjectionElem::ConstantIndex { offset, min_length, from_end: true } =>
                        write!(fmt, "{:?}[-{:?} of {:?}]", data.base, offset, min_length),
                    ProjectionElem::Subslice { from, to } if to == 0 =>
                        write!(fmt, "{:?}[{:?}:]", data.base, from),
                    ProjectionElem::Subslice { from, to } if from == 0 =>
                        write!(fmt, "{:?}[:-{:?}]", data.base, to),
                    ProjectionElem::Subslice { from, to } =>
                        write!(fmt, "{:?}[{:?}:-{:?}]", data.base,
                               from, to),

                },
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Scopes

newtype_index!(VisibilityScope
    {
        DEBUG_FORMAT = "scope[{}]",
        const ARGUMENT_VISIBILITY_SCOPE = 0,
    });

#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct VisibilityScopeData {
    pub span: Span,
    pub parent_scope: Option<VisibilityScope>,
}

///////////////////////////////////////////////////////////////////////////
// Operands

/// These are values that can appear inside an rvalue (or an index
/// place). They are intentionally limited to prevent rvalues from
/// being nested in one another.
#[derive(Clone, PartialEq, RustcEncodable, RustcDecodable)]
pub enum Operand<'tcx> {
    /// Copy: The value must be available for use afterwards.
    ///
    /// This implies that the type of the place must be `Copy`; this is true
    /// by construction during build, but also checked by the MIR type checker.
    Copy(Place<'tcx>),
    /// Move: The value (including old borrows of it) will not be used again.
    ///
    /// Safe for values of all types (modulo future developments towards `?Move`).
    /// Correct usage patterns are enforced by the borrow checker for safe code.
    /// `Copy` may be converted to `Move` to enable "last-use" optimizations.
    Move(Place<'tcx>),
    Constant(Box<Constant<'tcx>>),
}

impl<'tcx> Debug for Operand<'tcx> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        use self::Operand::*;
        match *self {
            Constant(ref a) => write!(fmt, "{:?}", a),
            Copy(ref place) => write!(fmt, "{:?}", place),
            Move(ref place) => write!(fmt, "move {:?}", place),
        }
    }
}

impl<'tcx> Operand<'tcx> {
    pub fn function_handle<'a>(
        tcx: TyCtxt<'a, 'tcx, 'tcx>,
        def_id: DefId,
        substs: &'tcx Substs<'tcx>,
        span: Span,
    ) -> Self {
        let ty = tcx.type_of(def_id).subst(tcx, substs);
        Operand::Constant(box Constant {
            span,
            ty,
            literal: Literal::Value {
                value: tcx.mk_const(ty::Const {
                    val: ConstVal::Function(def_id, substs),
                    ty
                })
            },
        })
    }

    pub fn to_copy(&self) -> Self {
        match *self {
            Operand::Copy(_) | Operand::Constant(_) => self.clone(),
            Operand::Move(ref place) => Operand::Copy(place.clone())
        }
    }
}

///////////////////////////////////////////////////////////////////////////
/// Rvalues

#[derive(Clone, RustcEncodable, RustcDecodable)]
pub enum Rvalue<'tcx> {
    /// x (either a move or copy, depending on type of x)
    Use(Operand<'tcx>),

    /// [x; 32]
    Repeat(Operand<'tcx>, ConstUsize),

    /// &x or &mut x
    Ref(Region<'tcx>, BorrowKind, Place<'tcx>),

    /// length of a [X] or [X;n] value
    Len(Place<'tcx>),

    Cast(CastKind, Operand<'tcx>, Ty<'tcx>),

    BinaryOp(BinOp, Operand<'tcx>, Operand<'tcx>),
    CheckedBinaryOp(BinOp, Operand<'tcx>, Operand<'tcx>),

    NullaryOp(NullOp, Ty<'tcx>),
    UnaryOp(UnOp, Operand<'tcx>),

    /// Read the discriminant of an ADT.
    ///
    /// Undefined (i.e. no effort is made to make it defined, but there’s no reason why it cannot
    /// be defined to return, say, a 0) if ADT is not an enum.
    Discriminant(Place<'tcx>),

    /// Create an aggregate value, like a tuple or struct.  This is
    /// only needed because we want to distinguish `dest = Foo { x:
    /// ..., y: ... }` from `dest.x = ...; dest.y = ...;` in the case
    /// that `Foo` has a destructor. These rvalues can be optimized
    /// away after type-checking and before lowering.
    Aggregate(Box<AggregateKind<'tcx>>, Vec<Operand<'tcx>>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum CastKind {
    Misc,

    /// Convert unique, zero-sized type for a fn to fn()
    ReifyFnPointer,

    /// Convert non capturing closure to fn()
    ClosureFnPointer,

    /// Convert safe fn() to unsafe fn()
    UnsafeFnPointer,

    /// "Unsize" -- convert a thin-or-fat pointer to a fat pointer.
    /// trans must figure out the details once full monomorphization
    /// is known. For example, this could be used to cast from a
    /// `&[i32;N]` to a `&[i32]`, or a `Box<T>` to a `Box<Trait>`
    /// (presuming `T: Trait`).
    Unsize,
}

#[derive(Clone, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum AggregateKind<'tcx> {
    /// The type is of the element
    Array(Ty<'tcx>),
    Tuple,

    /// The second field is variant number (discriminant), it's equal
    /// to 0 for struct and union expressions. The fourth field is
    /// active field number and is present only for union expressions
    /// -- e.g. for a union expression `SomeUnion { c: .. }`, the
    /// active field index would identity the field `c`
    Adt(&'tcx AdtDef, usize, &'tcx Substs<'tcx>, Option<usize>),

    Closure(DefId, ClosureSubsts<'tcx>),
    Generator(DefId, ClosureSubsts<'tcx>, GeneratorInterior<'tcx>),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum BinOp {
    /// The `+` operator (addition)
    Add,
    /// The `-` operator (subtraction)
    Sub,
    /// The `*` operator (multiplication)
    Mul,
    /// The `/` operator (division)
    Div,
    /// The `%` operator (modulus)
    Rem,
    /// The `^` operator (bitwise xor)
    BitXor,
    /// The `&` operator (bitwise and)
    BitAnd,
    /// The `|` operator (bitwise or)
    BitOr,
    /// The `<<` operator (shift left)
    Shl,
    /// The `>>` operator (shift right)
    Shr,
    /// The `==` operator (equality)
    Eq,
    /// The `<` operator (less than)
    Lt,
    /// The `<=` operator (less than or equal to)
    Le,
    /// The `!=` operator (not equal to)
    Ne,
    /// The `>=` operator (greater than or equal to)
    Ge,
    /// The `>` operator (greater than)
    Gt,
    /// The `ptr.offset` operator
    Offset,
}

impl BinOp {
    pub fn is_checkable(self) -> bool {
        use self::BinOp::*;
        match self {
            Add | Sub | Mul | Shl | Shr => true,
            _ => false
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum NullOp {
    /// Return the size of a value of that type
    SizeOf,
    /// Create a new uninitialized box for a value of that type
    Box,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum UnOp {
    /// The `!` operator for logical inversion
    Not,
    /// The `-` operator for negation
    Neg,
}

impl<'tcx> Debug for Rvalue<'tcx> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        use self::Rvalue::*;

        match *self {
            Use(ref place) => write!(fmt, "{:?}", place),
            Repeat(ref a, ref b) => write!(fmt, "[{:?}; {:?}]", a, b),
            Len(ref a) => write!(fmt, "Len({:?})", a),
            Cast(ref kind, ref place, ref ty) => {
                write!(fmt, "{:?} as {:?} ({:?})", place, ty, kind)
            }
            BinaryOp(ref op, ref a, ref b) => write!(fmt, "{:?}({:?}, {:?})", op, a, b),
            CheckedBinaryOp(ref op, ref a, ref b) => {
                write!(fmt, "Checked{:?}({:?}, {:?})", op, a, b)
            }
            UnaryOp(ref op, ref a) => write!(fmt, "{:?}({:?})", op, a),
            Discriminant(ref place) => write!(fmt, "discriminant({:?})", place),
            NullaryOp(ref op, ref t) => write!(fmt, "{:?}({:?})", op, t),
            Ref(region, borrow_kind, ref place) => {
                let kind_str = match borrow_kind {
                    BorrowKind::Shared => "",
                    BorrowKind::Mut | BorrowKind::Unique => "mut ",
                };

                // When printing regions, add trailing space if necessary.
                let region = if ppaux::verbose() || ppaux::identify_regions() {
                    let mut region = format!("{}", region);
                    if region.len() > 0 { region.push(' '); }
                    region
                } else {
                    // Do not even print 'static
                    "".to_owned()
                };
                write!(fmt, "&{}{}{:?}", region, kind_str, place)
            }

            Aggregate(ref kind, ref places) => {
                fn fmt_tuple(fmt: &mut Formatter, places: &[Operand]) -> fmt::Result {
                    let mut tuple_fmt = fmt.debug_tuple("");
                    for place in places {
                        tuple_fmt.field(place);
                    }
                    tuple_fmt.finish()
                }

                match **kind {
                    AggregateKind::Array(_) => write!(fmt, "{:?}", places),

                    AggregateKind::Tuple => {
                        match places.len() {
                            0 => write!(fmt, "()"),
                            1 => write!(fmt, "({:?},)", places[0]),
                            _ => fmt_tuple(fmt, places),
                        }
                    }

                    AggregateKind::Adt(adt_def, variant, substs, _) => {
                        let variant_def = &adt_def.variants[variant];

                        ppaux::parameterized(fmt, substs, variant_def.did, &[])?;

                        match variant_def.ctor_kind {
                            CtorKind::Const => Ok(()),
                            CtorKind::Fn => fmt_tuple(fmt, places),
                            CtorKind::Fictive => {
                                let mut struct_fmt = fmt.debug_struct("");
                                for (field, place) in variant_def.fields.iter().zip(places) {
                                    struct_fmt.field(&field.name.as_str(), place);
                                }
                                struct_fmt.finish()
                            }
                        }
                    }

                    AggregateKind::Closure(def_id, _) => ty::tls::with(|tcx| {
                        if let Some(node_id) = tcx.hir.as_local_node_id(def_id) {
                            let name = if tcx.sess.opts.debugging_opts.span_free_formats {
                                format!("[closure@{:?}]", node_id)
                            } else {
                                format!("[closure@{:?}]", tcx.hir.span(node_id))
                            };
                            let mut struct_fmt = fmt.debug_struct(&name);

                            tcx.with_freevars(node_id, |freevars| {
                                for (freevar, place) in freevars.iter().zip(places) {
                                    let var_name = tcx.hir.name(freevar.var_id());
                                    struct_fmt.field(&var_name.as_str(), place);
                                }
                            });

                            struct_fmt.finish()
                        } else {
                            write!(fmt, "[closure]")
                        }
                    }),

                    AggregateKind::Generator(def_id, _, _) => ty::tls::with(|tcx| {
                        if let Some(node_id) = tcx.hir.as_local_node_id(def_id) {
                            let name = format!("[generator@{:?}]", tcx.hir.span(node_id));
                            let mut struct_fmt = fmt.debug_struct(&name);

                            tcx.with_freevars(node_id, |freevars| {
                                for (freevar, place) in freevars.iter().zip(places) {
                                    let var_name = tcx.hir.name(freevar.var_id());
                                    struct_fmt.field(&var_name.as_str(), place);
                                }
                                struct_fmt.field("$state", &places[freevars.len()]);
                                for i in (freevars.len() + 1)..places.len() {
                                    struct_fmt.field(&format!("${}", i - freevars.len() - 1),
                                                     &places[i]);
                                }
                            });

                            struct_fmt.finish()
                        } else {
                            write!(fmt, "[generator]")
                        }
                    }),
                }
            }
        }
    }
}

///////////////////////////////////////////////////////////////////////////
/// Constants
///
/// Two constants are equal if they are the same constant. Note that
/// this does not necessarily mean that they are "==" in Rust -- in
/// particular one must be wary of `NaN`!

#[derive(Clone, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable)]
pub struct Constant<'tcx> {
    pub span: Span,
    pub ty: Ty<'tcx>,
    pub literal: Literal<'tcx>,
}

newtype_index!(Promoted { DEBUG_FORMAT = "promoted[{}]" });


#[derive(Clone, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable)]
pub enum Literal<'tcx> {
    Value {
        value: &'tcx ty::Const<'tcx>,
    },
    Promoted {
        // Index into the `promoted` vector of `Mir`.
        index: Promoted
    },
}

impl<'tcx> Debug for Constant<'tcx> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        write!(fmt, "{:?}", self.literal)
    }
}

impl<'tcx> Debug for Literal<'tcx> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        use self::Literal::*;
        match *self {
            Value { value } => {
                write!(fmt, "const ")?;
                fmt_const_val(fmt, &value.val)
            }
            Promoted { index } => {
                write!(fmt, "{:?}", index)
            }
        }
    }
}

/// Write a `ConstVal` in a way closer to the original source code than the `Debug` output.
fn fmt_const_val<W: Write>(fmt: &mut W, const_val: &ConstVal) -> fmt::Result {
    use middle::const_val::ConstVal::*;
    match *const_val {
        Float(f) => write!(fmt, "{:?}", f),
        Integral(n) => write!(fmt, "{}", n),
        Str(s) => write!(fmt, "{:?}", s),
        ByteStr(bytes) => {
            let escaped: String = bytes.data
                .iter()
                .flat_map(|&ch| ascii::escape_default(ch).map(|c| c as char))
                .collect();
            write!(fmt, "b\"{}\"", escaped)
        }
        Bool(b) => write!(fmt, "{:?}", b),
        Char(c) => write!(fmt, "{:?}", c),
        Variant(def_id) |
        Function(def_id, _) => write!(fmt, "{}", item_path_str(def_id)),
        Aggregate(_) => bug!("`ConstVal::{:?}` should not be in MIR", const_val),
        Unevaluated(..) => write!(fmt, "{:?}", const_val)
    }
}

fn item_path_str(def_id: DefId) -> String {
    ty::tls::with(|tcx| tcx.item_path_str(def_id))
}

impl<'tcx> ControlFlowGraph for Mir<'tcx> {

    type Node = BasicBlock;

    fn num_nodes(&self) -> usize { self.basic_blocks.len() }

    fn start_node(&self) -> Self::Node { START_BLOCK }

    fn predecessors<'graph>(&'graph self, node: Self::Node)
                            -> <Self as GraphPredecessors<'graph>>::Iter
    {
        self.predecessors_for(node).clone().into_iter()
    }
    fn successors<'graph>(&'graph self, node: Self::Node)
                          -> <Self as GraphSuccessors<'graph>>::Iter
    {
        self.basic_blocks[node].terminator().successors().into_owned().into_iter()
    }
}

impl<'a, 'b> GraphPredecessors<'b> for Mir<'a> {
    type Item = BasicBlock;
    type Iter = IntoIter<BasicBlock>;
}

impl<'a, 'b>  GraphSuccessors<'b> for Mir<'a> {
    type Item = BasicBlock;
    type Iter = IntoIter<BasicBlock>;
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Location {
    /// the location is within this block
    pub block: BasicBlock,

    /// the location is the start of the this statement; or, if `statement_index`
    /// == num-statements, then the start of the terminator.
    pub statement_index: usize,
}

impl fmt::Debug for Location {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{:?}[{}]", self.block, self.statement_index)
    }
}

impl Location {
    /// Returns the location immediately after this one within the enclosing block.
    ///
    /// Note that if this location represents a terminator, then the
    /// resulting location would be out of bounds and invalid.
    pub fn successor_within_block(&self) -> Location {
        Location { block: self.block, statement_index: self.statement_index + 1 }
    }

    pub fn dominates(&self, other: &Location, dominators: &Dominators<BasicBlock>) -> bool {
        if self.block == other.block {
            self.statement_index <= other.statement_index
        } else {
            dominators.is_dominated_by(other.block, self.block)
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable)]
pub enum UnsafetyViolationKind {
    General,
    ExternStatic(ast::NodeId),
    BorrowPacked(ast::NodeId),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable)]
pub struct UnsafetyViolation {
    pub source_info: SourceInfo,
    pub description: InternedString,
    pub kind: UnsafetyViolationKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable)]
pub struct UnsafetyCheckResult {
    /// Violations that are propagated *upwards* from this function
    pub violations: Rc<[UnsafetyViolation]>,
    /// unsafe blocks in this function, along with whether they are used. This is
    /// used for the "unused_unsafe" lint.
    pub unsafe_blocks: Rc<[(ast::NodeId, bool)]>,
}

/// The layout of generator state
#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct GeneratorLayout<'tcx> {
    pub fields: Vec<LocalDecl<'tcx>>,
}

/// After we borrow check a closure, we are left with various
/// requirements that we have inferred between the free regions that
/// appear in the closure's signature or on its field types.  These
/// requirements are then verified and proved by the closure's
/// creating function. This struct encodes those requirements.
///
/// The requirements are listed as being between various
/// `RegionVid`. The 0th region refers to `'static`; subsequent region
/// vids refer to the free regions that appear in the closure (or
/// generator's) type, in order of appearance. (This numbering is
/// actually defined by the `UniversalRegions` struct in the NLL
/// region checker. See for example
/// `UniversalRegions::closure_mapping`.) Note that we treat the free
/// regions in the closure's type "as if" they were erased, so their
/// precise identity is not important, only their position.
///
/// Example: If type check produces a closure with the closure substs:
///
/// ```
/// ClosureSubsts = [
///     i8,                                  // the "closure kind"
///     for<'x> fn(&'a &'x u32) -> &'x u32,  // the "closure signature"
///     &'a String,                          // some upvar
/// ]
/// ```
///
/// here, there is one unique free region (`'a`) but it appears
/// twice. We would "renumber" each occurence to a unique vid, as follows:
///
/// ```
/// ClosureSubsts = [
///     i8,                                  // the "closure kind"
///     for<'x> fn(&'1 &'x u32) -> &'x u32,  // the "closure signature"
///     &'2 String,                          // some upvar
/// ]
/// ```
///
/// Now the code might impose a requirement like `'1: '2`. When an
/// instance of the closure is created, the corresponding free regions
/// can be extracted from its type and constrained to have the given
/// outlives relationship.
#[derive(Clone, Debug)]
pub struct ClosureRegionRequirements {
    /// The number of external regions defined on the closure.  In our
    /// example above, it would be 3 -- one for `'static`, then `'1`
    /// and `'2`. This is just used for a sanity check later on, to
    /// make sure that the number of regions we see at the callsite
    /// matches.
    pub num_external_vids: usize,

    /// Requirements between the various free regions defined in
    /// indices.
    pub outlives_requirements: Vec<ClosureOutlivesRequirement>,
}

/// Indicates an outlives constraint between two free-regions declared
/// on the closure.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosureOutlivesRequirement {
    // This region ...
    pub free_region: ty::RegionVid,

    // .. must outlive this one.
    pub outlived_free_region: ty::RegionVid,

    // If not, report an error here.
    pub blame_span: Span,
}

/*
 * TypeFoldable implementations for MIR types
 */

impl<'tcx> TypeFoldable<'tcx> for Mir<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        Mir {
            basic_blocks: self.basic_blocks.fold_with(folder),
            visibility_scopes: self.visibility_scopes.clone(),
            visibility_scope_info: self.visibility_scope_info.clone(),
            promoted: self.promoted.fold_with(folder),
            yield_ty: self.yield_ty.fold_with(folder),
            generator_drop: self.generator_drop.fold_with(folder),
            generator_layout: self.generator_layout.fold_with(folder),
            local_decls: self.local_decls.fold_with(folder),
            arg_count: self.arg_count,
            upvar_decls: self.upvar_decls.clone(),
            spread_arg: self.spread_arg,
            span: self.span,
            cache: cache::Cache::new()
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        self.basic_blocks.visit_with(visitor) ||
        self.generator_drop.visit_with(visitor) ||
        self.generator_layout.visit_with(visitor) ||
        self.yield_ty.visit_with(visitor) ||
        self.promoted.visit_with(visitor)     ||
        self.local_decls.visit_with(visitor)
    }
}

impl<'tcx> TypeFoldable<'tcx> for GeneratorLayout<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        GeneratorLayout {
            fields: self.fields.fold_with(folder),
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        self.fields.visit_with(visitor)
    }
}

impl<'tcx> TypeFoldable<'tcx> for LocalDecl<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        LocalDecl {
            ty: self.ty.fold_with(folder),
            ..self.clone()
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        self.ty.visit_with(visitor)
    }
}

impl<'tcx> TypeFoldable<'tcx> for BasicBlockData<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        BasicBlockData {
            statements: self.statements.fold_with(folder),
            terminator: self.terminator.fold_with(folder),
            is_cleanup: self.is_cleanup
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        self.statements.visit_with(visitor) || self.terminator.visit_with(visitor)
    }
}

impl<'tcx> TypeFoldable<'tcx> for ValidationOperand<'tcx, Place<'tcx>> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        ValidationOperand {
            place: self.place.fold_with(folder),
            ty: self.ty.fold_with(folder),
            re: self.re,
            mutbl: self.mutbl,
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        self.place.visit_with(visitor) || self.ty.visit_with(visitor)
    }
}

impl<'tcx> TypeFoldable<'tcx> for Statement<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        use mir::StatementKind::*;

        let kind = match self.kind {
            Assign(ref place, ref rval) => Assign(place.fold_with(folder), rval.fold_with(folder)),
            SetDiscriminant { ref place, variant_index } => SetDiscriminant {
                place: place.fold_with(folder),
                variant_index,
            },
            StorageLive(ref local) => StorageLive(local.fold_with(folder)),
            StorageDead(ref local) => StorageDead(local.fold_with(folder)),
            InlineAsm { ref asm, ref outputs, ref inputs } => InlineAsm {
                asm: asm.clone(),
                outputs: outputs.fold_with(folder),
                inputs: inputs.fold_with(folder)
            },

            // Note for future: If we want to expose the region scopes
            // during the fold, we need to either generalize EndRegion
            // to carry `[ty::Region]`, or extend the `TypeFolder`
            // trait with a `fn fold_scope`.
            EndRegion(ref region_scope) => EndRegion(region_scope.clone()),

            Validate(ref op, ref places) =>
                Validate(op.clone(),
                         places.iter().map(|operand| operand.fold_with(folder)).collect()),

            Nop => Nop,
        };
        Statement {
            source_info: self.source_info,
            kind,
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        use mir::StatementKind::*;

        match self.kind {
            Assign(ref place, ref rval) => { place.visit_with(visitor) || rval.visit_with(visitor) }
            SetDiscriminant { ref place, .. } => place.visit_with(visitor),
            StorageLive(ref local) |
            StorageDead(ref local) => local.visit_with(visitor),
            InlineAsm { ref outputs, ref inputs, .. } =>
                outputs.visit_with(visitor) || inputs.visit_with(visitor),

            // Note for future: If we want to expose the region scopes
            // during the visit, we need to either generalize EndRegion
            // to carry `[ty::Region]`, or extend the `TypeVisitor`
            // trait with a `fn visit_scope`.
            EndRegion(ref _scope) => false,

            Validate(ref _op, ref places) =>
                places.iter().any(|ty_and_place| ty_and_place.visit_with(visitor)),

            Nop => false,
        }
    }
}

impl<'tcx> TypeFoldable<'tcx> for Terminator<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        use mir::TerminatorKind::*;

        let kind = match self.kind {
            Goto { target } => Goto { target: target },
            SwitchInt { ref discr, switch_ty, ref values, ref targets } => SwitchInt {
                discr: discr.fold_with(folder),
                switch_ty: switch_ty.fold_with(folder),
                values: values.clone(),
                targets: targets.clone()
            },
            Drop { ref location, target, unwind } => Drop {
                location: location.fold_with(folder),
                target,
                unwind,
            },
            DropAndReplace { ref location, ref value, target, unwind } => DropAndReplace {
                location: location.fold_with(folder),
                value: value.fold_with(folder),
                target,
                unwind,
            },
            Yield { ref value, resume, drop } => Yield {
                value: value.fold_with(folder),
                resume: resume,
                drop: drop,
            },
            Call { ref func, ref args, ref destination, cleanup } => {
                let dest = destination.as_ref().map(|&(ref loc, dest)| {
                    (loc.fold_with(folder), dest)
                });

                Call {
                    func: func.fold_with(folder),
                    args: args.fold_with(folder),
                    destination: dest,
                    cleanup,
                }
            },
            Assert { ref cond, expected, ref msg, target, cleanup } => {
                let msg = if let AssertMessage::BoundsCheck { ref len, ref index } = *msg {
                    AssertMessage::BoundsCheck {
                        len: len.fold_with(folder),
                        index: index.fold_with(folder),
                    }
                } else {
                    msg.clone()
                };
                Assert {
                    cond: cond.fold_with(folder),
                    expected,
                    msg,
                    target,
                    cleanup,
                }
            },
            GeneratorDrop => GeneratorDrop,
            Resume => Resume,
            Return => Return,
            Unreachable => Unreachable,
            FalseEdges { real_target, ref imaginary_targets } =>
                FalseEdges { real_target, imaginary_targets: imaginary_targets.clone() }
        };
        Terminator {
            source_info: self.source_info,
            kind,
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        use mir::TerminatorKind::*;

        match self.kind {
            SwitchInt { ref discr, switch_ty, .. } =>
                discr.visit_with(visitor) || switch_ty.visit_with(visitor),
            Drop { ref location, ..} => location.visit_with(visitor),
            DropAndReplace { ref location, ref value, ..} =>
                location.visit_with(visitor) || value.visit_with(visitor),
            Yield { ref value, ..} =>
                value.visit_with(visitor),
            Call { ref func, ref args, ref destination, .. } => {
                let dest = if let Some((ref loc, _)) = *destination {
                    loc.visit_with(visitor)
                } else { false };
                dest || func.visit_with(visitor) || args.visit_with(visitor)
            },
            Assert { ref cond, ref msg, .. } => {
                if cond.visit_with(visitor) {
                    if let AssertMessage::BoundsCheck { ref len, ref index } = *msg {
                        len.visit_with(visitor) || index.visit_with(visitor)
                    } else {
                        false
                    }
                } else {
                    false
                }
            },
            Goto { .. } |
            Resume |
            Return |
            GeneratorDrop |
            Unreachable |
            FalseEdges { .. } => false
        }
    }
}

impl<'tcx> TypeFoldable<'tcx> for Place<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        match self {
            &Place::Projection(ref p) => Place::Projection(p.fold_with(folder)),
            _ => self.clone()
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        if let &Place::Projection(ref p) = self {
            p.visit_with(visitor)
        } else {
            false
        }
    }
}

impl<'tcx> TypeFoldable<'tcx> for Rvalue<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        use mir::Rvalue::*;
        match *self {
            Use(ref op) => Use(op.fold_with(folder)),
            Repeat(ref op, len) => Repeat(op.fold_with(folder), len),
            Ref(region, bk, ref place) =>
                Ref(region.fold_with(folder), bk, place.fold_with(folder)),
            Len(ref place) => Len(place.fold_with(folder)),
            Cast(kind, ref op, ty) => Cast(kind, op.fold_with(folder), ty.fold_with(folder)),
            BinaryOp(op, ref rhs, ref lhs) =>
                BinaryOp(op, rhs.fold_with(folder), lhs.fold_with(folder)),
            CheckedBinaryOp(op, ref rhs, ref lhs) =>
                CheckedBinaryOp(op, rhs.fold_with(folder), lhs.fold_with(folder)),
            UnaryOp(op, ref val) => UnaryOp(op, val.fold_with(folder)),
            Discriminant(ref place) => Discriminant(place.fold_with(folder)),
            NullaryOp(op, ty) => NullaryOp(op, ty.fold_with(folder)),
            Aggregate(ref kind, ref fields) => {
                let kind = box match **kind {
                    AggregateKind::Array(ty) => AggregateKind::Array(ty.fold_with(folder)),
                    AggregateKind::Tuple => AggregateKind::Tuple,
                    AggregateKind::Adt(def, v, substs, n) =>
                        AggregateKind::Adt(def, v, substs.fold_with(folder), n),
                    AggregateKind::Closure(id, substs) =>
                        AggregateKind::Closure(id, substs.fold_with(folder)),
                    AggregateKind::Generator(id, substs, interior) =>
                        AggregateKind::Generator(id,
                                                 substs.fold_with(folder),
                                                 interior.fold_with(folder)),
                };
                Aggregate(kind, fields.fold_with(folder))
            }
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        use mir::Rvalue::*;
        match *self {
            Use(ref op) => op.visit_with(visitor),
            Repeat(ref op, _) => op.visit_with(visitor),
            Ref(region, _, ref place) => region.visit_with(visitor) || place.visit_with(visitor),
            Len(ref place) => place.visit_with(visitor),
            Cast(_, ref op, ty) => op.visit_with(visitor) || ty.visit_with(visitor),
            BinaryOp(_, ref rhs, ref lhs) |
            CheckedBinaryOp(_, ref rhs, ref lhs) =>
                rhs.visit_with(visitor) || lhs.visit_with(visitor),
            UnaryOp(_, ref val) => val.visit_with(visitor),
            Discriminant(ref place) => place.visit_with(visitor),
            NullaryOp(_, ty) => ty.visit_with(visitor),
            Aggregate(ref kind, ref fields) => {
                (match **kind {
                    AggregateKind::Array(ty) => ty.visit_with(visitor),
                    AggregateKind::Tuple => false,
                    AggregateKind::Adt(_, _, substs, _) => substs.visit_with(visitor),
                    AggregateKind::Closure(_, substs) => substs.visit_with(visitor),
                    AggregateKind::Generator(_, substs, interior) => substs.visit_with(visitor) ||
                        interior.visit_with(visitor),
                }) || fields.visit_with(visitor)
            }
        }
    }
}

impl<'tcx> TypeFoldable<'tcx> for Operand<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        match *self {
            Operand::Copy(ref place) => Operand::Copy(place.fold_with(folder)),
            Operand::Move(ref place) => Operand::Move(place.fold_with(folder)),
            Operand::Constant(ref c) => Operand::Constant(c.fold_with(folder)),
        }
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        match *self {
            Operand::Copy(ref place) |
            Operand::Move(ref place) => place.visit_with(visitor),
            Operand::Constant(ref c) => c.visit_with(visitor)
        }
    }
}

impl<'tcx, B, V, T> TypeFoldable<'tcx> for Projection<'tcx, B, V, T>
    where B: TypeFoldable<'tcx>, V: TypeFoldable<'tcx>, T: TypeFoldable<'tcx>
{
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        use mir::ProjectionElem::*;

        let base = self.base.fold_with(folder);
        let elem = match self.elem {
            Deref => Deref,
            Field(f, ref ty) => Field(f, ty.fold_with(folder)),
            Index(ref v) => Index(v.fold_with(folder)),
            ref elem => elem.clone()
        };

        Projection {
            base,
            elem,
        }
    }

    fn super_visit_with<Vs: TypeVisitor<'tcx>>(&self, visitor: &mut Vs) -> bool {
        use mir::ProjectionElem::*;

        self.base.visit_with(visitor) ||
            match self.elem {
                Field(_, ref ty) => ty.visit_with(visitor),
                Index(ref v) => v.visit_with(visitor),
                _ => false
            }
    }
}

impl<'tcx> TypeFoldable<'tcx> for Constant<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        Constant {
            span: self.span.clone(),
            ty: self.ty.fold_with(folder),
            literal: self.literal.fold_with(folder)
        }
    }
    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        self.ty.visit_with(visitor) || self.literal.visit_with(visitor)
    }
}

impl<'tcx> TypeFoldable<'tcx> for Literal<'tcx> {
    fn super_fold_with<'gcx: 'tcx, F: TypeFolder<'gcx, 'tcx>>(&self, folder: &mut F) -> Self {
        match *self {
            Literal::Value { value } => Literal::Value {
                value: value.fold_with(folder)
            },
            Literal::Promoted { index } => Literal::Promoted { index }
        }
    }
    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        match *self {
            Literal::Value { value } => value.visit_with(visitor),
            Literal::Promoted { .. } => false
        }
    }
}
