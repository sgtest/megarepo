// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use hair::Hair;
use rustc_data_structures::fnv::FnvHashMap;
use std::fmt::{Debug, Formatter, Error};
use std::slice;
use std::u32;

/// Lowered representation of a single function.
pub struct Mir<H:Hair> {
    pub basic_blocks: Vec<BasicBlockData<H>>,

    // for every node id
    pub extents: FnvHashMap<H::CodeExtent, Vec<GraphExtent>>,

    pub var_decls: Vec<VarDecl<H>>,
    pub arg_decls: Vec<ArgDecl<H>>,
    pub temp_decls: Vec<TempDecl<H>>,
}

/// where execution begins
pub const START_BLOCK: BasicBlock = BasicBlock(0);

/// where execution ends, on normal return
pub const END_BLOCK: BasicBlock = BasicBlock(1);

/// where execution ends, on panic
pub const DIVERGE_BLOCK: BasicBlock = BasicBlock(2);

impl<H:Hair> Mir<H> {
    pub fn all_basic_blocks(&self) -> Vec<BasicBlock> {
        (0..self.basic_blocks.len())
            .map(|i| BasicBlock::new(i))
            .collect()
    }

    pub fn basic_block_data(&self, bb: BasicBlock) -> &BasicBlockData<H> {
        &self.basic_blocks[bb.index()]
    }

    pub fn basic_block_data_mut(&mut self, bb: BasicBlock) -> &mut BasicBlockData<H> {
        &mut self.basic_blocks[bb.index()]
    }
}

///////////////////////////////////////////////////////////////////////////
// Mutability and borrow kinds

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mutability {
    Mut,
    Not,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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
    Mut
}

///////////////////////////////////////////////////////////////////////////
// Variables and temps

// A "variable" is a binding declared by the user as part of the fn
// decl, a let, etc.
pub struct VarDecl<H:Hair> {
    pub mutability: Mutability,
    pub name: H::Ident,
    pub ty: H::Ty,
}

// A "temp" is a temporary that we place on the stack. They are
// anonymous, always mutable, and have only a type.
pub struct TempDecl<H:Hair> {
    pub ty: H::Ty,
}

// A "arg" is one of the function's formal arguments. These are
// anonymous and distinct from the bindings that the user declares.
//
// For example, in this function:
//
// ```
// fn foo((x, y): (i32, u32)) { ... }
// ```
//
// there is only one argument, of type `(i32, u32)`, but two bindings
// (`x` and `y`).
pub struct ArgDecl<H:Hair> {
    pub ty: H::Ty,
}

///////////////////////////////////////////////////////////////////////////
// Graph extents

/// A moment in the flow of execution. It corresponds to a point in
/// between two statements:
///
///    BB[block]:
///                          <--- if statement == 0
///        STMT[0]
///                          <--- if statement == 1
///        STMT[1]
///        ...
///                          <--- if statement == n-1
///        STMT[n-1]
///                          <--- if statement == n
///
/// where the block has `n` statements.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPoint {
    pub block: BasicBlock,
    pub statement: u32,
}

/// A single-entry-multiple-exit region in the graph. We build one of
/// these for every node-id during MIR construction. By construction
/// we are assured that the entry dominates all points within, and
/// that, for every interior point X, it is postdominated by some exit.
pub struct GraphExtent {
    pub entry: ExecutionPoint,
    pub exit: GraphExtentExit,
}

pub enum GraphExtentExit {
    /// `Statement(X)`: a very common special case covering a span
    /// that is local to a single block. It starts at the entry point
    /// and extends until the start of statement `X` (non-inclusive).
    Statement(u32),

    /// The more general case where the exits are a set of points.
    Points(Vec<ExecutionPoint>),
}

///////////////////////////////////////////////////////////////////////////
// BasicBlock

/// The index of a particular basic block. The index is into the `basic_blocks`
/// list of the `Mir`.
///
/// (We use a `u32` internally just to save memory.)
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct BasicBlock(u32);

impl BasicBlock {
    pub fn new(index: usize) -> BasicBlock {
        assert!(index < (u32::MAX as usize));
        BasicBlock(index as u32)
    }

    /// Extract the index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "BB({})", self.0)
    }
}

///////////////////////////////////////////////////////////////////////////
// BasicBlock and Terminator

#[derive(Debug)]
pub struct BasicBlockData<H:Hair> {
    pub statements: Vec<Statement<H>>,
    pub terminator: Terminator<H>,
}

pub enum Terminator<H:Hair> {
    /// block should have one successor in the graph; we jump there
    Goto { target: BasicBlock },

    /// block should initiate unwinding; should be one successor
    /// that does cleanup and branches to DIVERGE_BLOCK
    Panic { target: BasicBlock },

    /// jump to branch 0 if this lvalue evaluates to true
    If { cond: Operand<H>, targets: [BasicBlock; 2] },

    /// lvalue evaluates to some enum; jump depending on the branch
    Switch { discr: Lvalue<H>, targets: Vec<BasicBlock> },

    /// Indicates that the last statement in the block panics, aborts,
    /// etc. No successors. This terminator appears on exactly one
    /// basic block which we create in advance. However, during
    /// construction, we use this value as a sentinel for "terminator
    /// not yet assigned", and assert at the end that only the
    /// well-known diverging block actually diverges.
    Diverge,

    /// Indicates a normal return. The ReturnPointer lvalue should
    /// have been filled in by now. This should only occur in the
    /// `END_BLOCK`.
    Return,

    /// block ends with a call; it should have two successors. The
    /// first successor indicates normal return. The second indicates
    /// unwinding.
    Call { data: CallData<H>, targets: [BasicBlock; 2] },
}

impl<H:Hair> Terminator<H> {
    pub fn successors(&self) -> &[BasicBlock] {
        use self::Terminator::*;
        match *self {
            Goto { target: ref b } => slice::ref_slice(b),
            Panic { target: ref b } => slice::ref_slice(b),
            If { cond: _, targets: ref b } => b,
            Switch { discr: _, targets: ref b } => b,
            Diverge => &[],
            Return => &[],
            Call { data: _, targets: ref b } => b,
        }
    }
}

#[derive(Debug)]
pub struct CallData<H:Hair> {
    /// where the return value is written to
    pub destination: Lvalue<H>,

    /// the fn being called
    pub func: Lvalue<H>,

    /// the arguments
    pub args: Vec<Lvalue<H>>,
}

impl<H:Hair> BasicBlockData<H> {
    pub fn new(terminator: Terminator<H>) -> BasicBlockData<H> {
        BasicBlockData {
            statements: vec![],
            terminator: terminator,
        }
    }
}

impl<H:Hair> Debug for Terminator<H> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        use self::Terminator::*;
        match *self {
            Goto { target } =>
                write!(fmt, "goto -> {:?}", target),
            Panic { target } =>
                write!(fmt, "panic -> {:?}", target),
            If { cond: ref lv, ref targets } =>
                write!(fmt, "if({:?}) -> {:?}", lv, targets),
            Switch { discr: ref lv, ref targets } =>
                write!(fmt, "switch({:?}) -> {:?}", lv, targets),
            Diverge =>
                write!(fmt, "diverge"),
            Return =>
                write!(fmt, "return"),
            Call { data: ref c, targets } => {
                try!(write!(fmt, "{:?} = {:?}(", c.destination, c.func));
                for (index, arg) in c.args.iter().enumerate() {
                    if index > 0 { try!(write!(fmt, ", ")); }
                    try!(write!(fmt, "{:?}", arg));
                }
                write!(fmt, ") -> {:?}", targets)
            }
        }
    }
}


///////////////////////////////////////////////////////////////////////////
// Statements

pub struct Statement<H:Hair> {
    pub span: H::Span,
    pub kind: StatementKind<H>,
}

#[derive(Debug)]
pub enum StatementKind<H:Hair> {
    Assign(Lvalue<H>, Rvalue<H>),
    Drop(DropKind, Lvalue<H>),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DropKind {
    Shallow,
    Deep
}

impl<H:Hair> Debug for Statement<H> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        use self::StatementKind::*;
        match self.kind {
            Assign(ref lv, ref rv) => write!(fmt, "{:?} = {:?}", lv, rv),
            Drop(DropKind::Shallow, ref lv) => write!(fmt, "shallow_drop {:?}", lv),
            Drop(DropKind::Deep, ref lv) => write!(fmt, "drop {:?}", lv),
        }
    }
}
///////////////////////////////////////////////////////////////////////////
// Lvalues

/// A path to a value; something that can be evaluated without
/// changing or disturbing program state.
#[derive(Clone, PartialEq)]
pub enum Lvalue<H:Hair> {
    /// local variable declared by the user
    Var(u32),

    /// temporary introduced during lowering into MIR
    Temp(u32),

    /// formal parameter of the function; note that these are NOT the
    /// bindings that the user declares, which are vars
    Arg(u32),

    /// static or static mut variable
    Static(H::DefId),

    /// the return pointer of the fn
    ReturnPointer,

    /// projection out of an lvalue (access a field, deref a pointer, etc)
    Projection(Box<LvalueProjection<H>>)
}

/// The `Projection` data structure defines things of the form `B.x`
/// or `*B` or `B[index]`. Note that it is parameterized because it is
/// shared between `Constant` and `Lvalue`. See the aliases
/// `LvalueProjection` etc below.
#[derive(Clone, Debug, PartialEq)]
pub struct Projection<H:Hair,B,V> {
    pub base: B,
    pub elem: ProjectionElem<H,V>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectionElem<H:Hair,V> {
    Deref,
    Field(Field<H>),
    Index(V),

    // These indices are generated by slice patterns. Easiest to explain
    // by example:
    //
    // ```
    // [X, _, .._, _, _] => { offset: 0, min_length: 4, from_end: false },
    // [_, X, .._, _, _] => { offset: 1, min_length: 4, from_end: false },
    // [_, _, .._, X, _] => { offset: 2, min_length: 4, from_end: true },
    // [_, _, .._, _, X] => { offset: 1, min_length: 4, from_end: true },
    // ```
    ConstantIndex {
        offset: u32,      // index or -index (in Python terms), depending on from_end
        min_length: u32,  // thing being indexed must be at least this long
        from_end: bool,   // counting backwards from end?
    },

    // "Downcast" to a variant of an ADT. Currently, we only introduce
    // this for ADTs with more than one variant. It may be better to
    // just introduce it always, or always for enums.
    Downcast(H::AdtDef, usize),
}

/// Alias for projections as they appear in lvalues, where the base is an lvalue
/// and the index is an operand.
pub type LvalueProjection<H> =
    Projection<H,Lvalue<H>,Operand<H>>;

/// Alias for projections as they appear in lvalues, where the base is an lvalue
/// and the index is an operand.
pub type LvalueElem<H> =
    ProjectionElem<H,Operand<H>>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field<H:Hair> {
    Named(H::Name),
    Indexed(usize),
}

impl<H:Hair> Lvalue<H> {
    pub fn field(self, f: Field<H>) -> Lvalue<H> {
        self.elem(ProjectionElem::Field(f))
    }

    pub fn deref(self) -> Lvalue<H> {
        self.elem(ProjectionElem::Deref)
    }

    pub fn index(self, index: Operand<H>) -> Lvalue<H> {
        self.elem(ProjectionElem::Index(index))
    }

    pub fn elem(self, elem: LvalueElem<H>) -> Lvalue<H> {
        Lvalue::Projection(Box::new(LvalueProjection { base: self, elem: elem }))
    }
}

impl<H:Hair> Debug for Lvalue<H> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        use self::Lvalue::*;

        match *self {
            Var(id) =>
                write!(fmt,"Var({:?})", id),
            Arg(id) =>
                write!(fmt,"Arg({:?})", id),
            Temp(id) =>
                write!(fmt,"Temp({:?})", id),
            Static(id) =>
                write!(fmt,"Static({:?})", id),
            ReturnPointer =>
                write!(fmt,"ReturnPointer"),
            Projection(ref data) =>
                match data.elem {
                    ProjectionElem::Downcast(_, variant_index) =>
                        write!(fmt,"({:?} as {:?})", data.base, variant_index),
                    ProjectionElem::Deref =>
                        write!(fmt,"(*{:?})", data.base),
                    ProjectionElem::Field(Field::Named(name)) =>
                        write!(fmt,"{:?}.{:?}", data.base, name),
                    ProjectionElem::Field(Field::Indexed(index)) =>
                        write!(fmt,"{:?}.{:?}", data.base, index),
                    ProjectionElem::Index(ref index) =>
                        write!(fmt,"{:?}[{:?}]", data.base, index),
                    ProjectionElem::ConstantIndex { offset, min_length, from_end: false } =>
                        write!(fmt,"{:?}[{:?} of {:?}]", data.base, offset, min_length),
                    ProjectionElem::ConstantIndex { offset, min_length, from_end: true } =>
                        write!(fmt,"{:?}[-{:?} of {:?}]", data.base, offset, min_length),
                },
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Operands
//
// These are values that can appear inside an rvalue (or an index
// lvalue). They are intentionally limited to prevent rvalues from
// being nested in one another.

#[derive(Clone, PartialEq)]
pub enum Operand<H:Hair> {
    Consume(Lvalue<H>),
    Constant(Constant<H>),
}

impl<H:Hair> Debug for Operand<H> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        use self::Operand::*;
        match *self {
            Constant(ref a) => write!(fmt, "{:?}", a),
            Consume(ref lv) => write!(fmt, "{:?}", lv),
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Rvalues

#[derive(Clone)]
pub enum Rvalue<H:Hair> {
    // x (either a move or copy, depending on type of x)
    Use(Operand<H>),

    // [x; 32]
    Repeat(Operand<H>, Operand<H>),

    // &x or &mut x
    Ref(H::Region, BorrowKind, Lvalue<H>),

    // length of a [X] or [X;n] value
    Len(Lvalue<H>),

    Cast(CastKind, Operand<H>, H::Ty),

    BinaryOp(BinOp, Operand<H>, Operand<H>),

    UnaryOp(UnOp, Operand<H>),

    // Creates an *uninitialized* Box
    Box(H::Ty),

    // Create an aggregate value, like a tuple or struct.  This is
    // only needed because we want to distinguish `dest = Foo { x:
    // ..., y: ... }` from `dest.x = ...; dest.y = ...;` in the case
    // that `Foo` has a destructor. These rvalues can be optimized
    // away after type-checking and before lowering.
    Aggregate(AggregateKind<H>, Vec<Operand<H>>),

    // Generates a slice of the form `&input[from_start..L-from_end]`
    // where `L` is the length of the slice. This is only created by
    // slice pattern matching, so e.g. a pattern of the form `[x, y,
    // .., z]` might create a slice with `from_start=2` and
    // `from_end=1`.
    Slice {
        input: Lvalue<H>,
        from_start: usize,
        from_end: usize,
    },

    InlineAsm(H::InlineAsm),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastKind {
    Misc,

    /// Convert unique, zero-sized type for a fn to fn()
    ReifyFnPointer,

    /// Convert safe fn() to unsafe fn()
    UnsafeFnPointer,

    /// "Unsize" -- convert a thin-or-fat pointer to a fat pointer.
    /// trans must figure out the details once full monomorphization
    /// is known. For example, this could be used to cast from a
    /// `&[i32;N]` to a `&[i32]`, or a `Box<T>` to a `Box<Trait>`
    /// (presuming `T: Trait`).
    Unsize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregateKind<H:Hair> {
    Vec,
    Tuple,
    Adt(H::AdtDef, usize, H::Substs),
    Closure(H::DefId, H::ClosureSubsts),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UnOp {
    /// The `!` operator for logical inversion
    Not,
    /// The `-` operator for negation
    Neg
}

impl<H:Hair> Debug for Rvalue<H> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        use self::Rvalue::*;

        match *self {
            Use(ref lvalue) => write!(fmt, "{:?}", lvalue),
            Repeat(ref a, ref b) => write!(fmt, "[{:?}; {:?}]", a, b),
            Ref(ref a, bk, ref b) => write!(fmt, "&{:?} {:?} {:?}", a, bk, b),
            Len(ref a) => write!(fmt, "LEN({:?})", a),
            Cast(ref kind, ref lv, ref ty) => write!(fmt, "{:?} as {:?} ({:?}", lv, ty, kind),
            BinaryOp(ref op, ref a, ref b) => write!(fmt, "{:?}({:?},{:?})", op, a, b),
            UnaryOp(ref op, ref a) => write!(fmt, "{:?}({:?})", op, a),
            Box(ref t) => write!(fmt, "Box {:?}", t),
            Aggregate(ref kind, ref lvs) => write!(fmt, "Aggregate<{:?}>({:?})", kind, lvs),
            InlineAsm(ref asm) => write!(fmt, "InlineAsm({:?})", asm),
            Slice { ref input, from_start, from_end } => write!(fmt, "{:?}[{:?}..-{:?}]",
                                                                input, from_start, from_end),
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Constants

#[derive(Clone, Debug, PartialEq)]
pub struct Constant<H:Hair> {
    pub span: H::Span,
    pub kind: ConstantKind<H>
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstantKind<H:Hair> {
    Literal(Literal<H>),
    Aggregate(AggregateKind<H>, Vec<Constant<H>>),
    Call(Box<Constant<H>>, Vec<Constant<H>>),
    Cast(Box<Constant<H>>, H::Ty),
    Repeat(Box<Constant<H>>, Box<Constant<H>>),
    Ref(BorrowKind, Box<Constant<H>>),
    BinaryOp(BinOp, Box<Constant<H>>, Box<Constant<H>>),
    UnaryOp(UnOp, Box<Constant<H>>),
    Projection(Box<ConstantProjection<H>>)
}

pub type ConstantProjection<H> =
    Projection<H,Constant<H>,Constant<H>>;

#[derive(Clone, Debug, PartialEq)]
pub enum Literal<H:Hair> {
    Item { def_id: H::DefId, substs: H::Substs },
    Projection { projection: H::Projection },
    Int { bits: IntegralBits, value: i64 },
    Uint { bits: IntegralBits, value: u64 },
    Float { bits: FloatBits, value: f64 },
    Char { c: char },
    Bool { value: bool },
    Bytes { value: H::Bytes },
    String { value: H::InternedString },
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum IntegralBits {
    B8, B16, B32, B64, BSize
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum FloatBits {
    F32, F64
}
