// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub use self::ImplOrTraitItemId::*;
pub use self::Variance::*;
pub use self::DtorKind::*;
pub use self::ImplOrTraitItemContainer::*;
pub use self::BorrowKind::*;
pub use self::ImplOrTraitItem::*;
pub use self::IntVarValue::*;
pub use self::LvaluePreference::*;
pub use self::fold::TypeFoldable;

use dep_graph::{self, DepNode};
use hir::map as ast_map;
use middle;
use middle::cstore::{self, LOCAL_CRATE};
use hir::def::{Def, PathResolution, ExportMap};
use hir::def_id::DefId;
use middle::lang_items::{FnTraitLangItem, FnMutTraitLangItem, FnOnceTraitLangItem};
use middle::region::{CodeExtent, ROOT_CODE_EXTENT};
use traits;
use ty;
use ty::subst::{Subst, Substs, VecPerParamSpace};
use ty::walk::TypeWalker;
use util::common::MemoizationMap;
use util::nodemap::NodeSet;
use util::nodemap::FnvHashMap;

use serialize::{Encodable, Encoder, Decodable, Decoder};
use std::borrow::Cow;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::iter;
use std::rc::Rc;
use std::slice;
use std::vec::IntoIter;
use syntax::ast::{self, CrateNum, Name, NodeId};
use syntax::attr::{self, AttrMetaMethods};
use syntax::parse::token::InternedString;
use syntax_pos::{DUMMY_SP, Span};

use rustc_const_math::ConstInt;

use hir;
use hir::{ItemImpl, ItemTrait, PatKind};
use hir::intravisit::Visitor;

pub use self::sty::{Binder, DebruijnIndex};
pub use self::sty::{BuiltinBound, BuiltinBounds, ExistentialBounds};
pub use self::sty::{BareFnTy, FnSig, PolyFnSig, FnOutput, PolyFnOutput};
pub use self::sty::{ClosureTy, InferTy, ParamTy, ProjectionTy, TraitTy};
pub use self::sty::{ClosureSubsts, TypeAndMut};
pub use self::sty::{TraitRef, TypeVariants, PolyTraitRef};
pub use self::sty::{BoundRegion, EarlyBoundRegion, FreeRegion, Region};
pub use self::sty::Issue32330;
pub use self::sty::{TyVid, IntVid, FloatVid, RegionVid, SkolemizedRegionVid};
pub use self::sty::BoundRegion::*;
pub use self::sty::FnOutput::*;
pub use self::sty::InferTy::*;
pub use self::sty::Region::*;
pub use self::sty::TypeVariants::*;

pub use self::sty::BuiltinBound::Send as BoundSend;
pub use self::sty::BuiltinBound::Sized as BoundSized;
pub use self::sty::BuiltinBound::Copy as BoundCopy;
pub use self::sty::BuiltinBound::Sync as BoundSync;

pub use self::contents::TypeContents;
pub use self::context::{TyCtxt, tls};
pub use self::context::{CtxtArenas, Lift, Tables};

pub use self::trait_def::{TraitDef, TraitFlags};

pub mod adjustment;
pub mod cast;
pub mod error;
pub mod fast_reject;
pub mod fold;
pub mod item_path;
pub mod layout;
pub mod _match;
pub mod maps;
pub mod outlives;
pub mod relate;
pub mod subst;
pub mod trait_def;
pub mod walk;
pub mod wf;
pub mod util;

mod contents;
mod context;
mod flags;
mod ivar;
mod structural_impls;
mod sty;

pub type Disr = ConstInt;

// Data types

/// The complete set of all analyses described in this module. This is
/// produced by the driver and fed to trans and later passes.
#[derive(Clone)]
pub struct CrateAnalysis<'a> {
    pub export_map: ExportMap,
    pub access_levels: middle::privacy::AccessLevels,
    pub reachable: NodeSet,
    pub name: &'a str,
    pub glob_map: Option<hir::GlobMap>,
}

#[derive(Copy, Clone)]
pub enum DtorKind {
    NoDtor,
    TraitDtor(bool)
}

impl DtorKind {
    pub fn is_present(&self) -> bool {
        match *self {
            TraitDtor(..) => true,
            _ => false
        }
    }

    pub fn has_drop_flag(&self) -> bool {
        match self {
            &NoDtor => false,
            &TraitDtor(flag) => flag
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImplOrTraitItemContainer {
    TraitContainer(DefId),
    ImplContainer(DefId),
}

impl ImplOrTraitItemContainer {
    pub fn id(&self) -> DefId {
        match *self {
            TraitContainer(id) => id,
            ImplContainer(id) => id,
        }
    }
}

/// The "header" of an impl is everything outside the body: a Self type, a trait
/// ref (in the case of a trait impl), and a set of predicates (from the
/// bounds/where clauses).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ImplHeader<'tcx> {
    pub impl_def_id: DefId,
    pub self_ty: Ty<'tcx>,
    pub trait_ref: Option<TraitRef<'tcx>>,
    pub predicates: Vec<Predicate<'tcx>>,
}

impl<'a, 'gcx, 'tcx> ImplHeader<'tcx> {
    pub fn with_fresh_ty_vars(selcx: &mut traits::SelectionContext<'a, 'gcx, 'tcx>,
                              impl_def_id: DefId)
                              -> ImplHeader<'tcx>
    {
        let tcx = selcx.tcx();
        let impl_generics = tcx.lookup_item_type(impl_def_id).generics;
        let impl_substs = selcx.infcx().fresh_substs_for_generics(DUMMY_SP, &impl_generics);

        let header = ImplHeader {
            impl_def_id: impl_def_id,
            self_ty: tcx.lookup_item_type(impl_def_id).ty,
            trait_ref: tcx.impl_trait_ref(impl_def_id),
            predicates: tcx.lookup_predicates(impl_def_id).predicates.into_vec(),
        }.subst(tcx, &impl_substs);

        let traits::Normalized { value: mut header, obligations } =
            traits::normalize(selcx, traits::ObligationCause::dummy(), &header);

        header.predicates.extend(obligations.into_iter().map(|o| o.predicate));
        header
    }
}

#[derive(Clone)]
pub enum ImplOrTraitItem<'tcx> {
    ConstTraitItem(Rc<AssociatedConst<'tcx>>),
    MethodTraitItem(Rc<Method<'tcx>>),
    TypeTraitItem(Rc<AssociatedType<'tcx>>),
}

impl<'tcx> ImplOrTraitItem<'tcx> {
    fn id(&self) -> ImplOrTraitItemId {
        match *self {
            ConstTraitItem(ref associated_const) => {
                ConstTraitItemId(associated_const.def_id)
            }
            MethodTraitItem(ref method) => MethodTraitItemId(method.def_id),
            TypeTraitItem(ref associated_type) => {
                TypeTraitItemId(associated_type.def_id)
            }
        }
    }

    pub fn def(&self) -> Def {
        match *self {
            ConstTraitItem(ref associated_const) => Def::AssociatedConst(associated_const.def_id),
            MethodTraitItem(ref method) => Def::Method(method.def_id),
            TypeTraitItem(ref ty) => Def::AssociatedTy(ty.container.id(), ty.def_id),
        }
    }

    pub fn def_id(&self) -> DefId {
        match *self {
            ConstTraitItem(ref associated_const) => associated_const.def_id,
            MethodTraitItem(ref method) => method.def_id,
            TypeTraitItem(ref associated_type) => associated_type.def_id,
        }
    }

    pub fn name(&self) -> Name {
        match *self {
            ConstTraitItem(ref associated_const) => associated_const.name,
            MethodTraitItem(ref method) => method.name,
            TypeTraitItem(ref associated_type) => associated_type.name,
        }
    }

    pub fn vis(&self) -> Visibility {
        match *self {
            ConstTraitItem(ref associated_const) => associated_const.vis,
            MethodTraitItem(ref method) => method.vis,
            TypeTraitItem(ref associated_type) => associated_type.vis,
        }
    }

    pub fn container(&self) -> ImplOrTraitItemContainer {
        match *self {
            ConstTraitItem(ref associated_const) => associated_const.container,
            MethodTraitItem(ref method) => method.container,
            TypeTraitItem(ref associated_type) => associated_type.container,
        }
    }

    pub fn as_opt_method(&self) -> Option<Rc<Method<'tcx>>> {
        match *self {
            MethodTraitItem(ref m) => Some((*m).clone()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ImplOrTraitItemId {
    ConstTraitItemId(DefId),
    MethodTraitItemId(DefId),
    TypeTraitItemId(DefId),
}

impl ImplOrTraitItemId {
    pub fn def_id(&self) -> DefId {
        match *self {
            ConstTraitItemId(def_id) => def_id,
            MethodTraitItemId(def_id) => def_id,
            TypeTraitItemId(def_id) => def_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum Visibility {
    /// Visible everywhere (including in other crates).
    Public,
    /// Visible only in the given crate-local module.
    Restricted(NodeId),
    /// Not visible anywhere in the local crate. This is the visibility of private external items.
    PrivateExternal,
}

pub trait NodeIdTree {
    fn is_descendant_of(&self, node: NodeId, ancestor: NodeId) -> bool;
}

impl<'a> NodeIdTree for ast_map::Map<'a> {
    fn is_descendant_of(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut node_ancestor = node;
        while node_ancestor != ancestor {
            let node_ancestor_parent = self.get_module_parent(node_ancestor);
            if node_ancestor_parent == node_ancestor {
                return false;
            }
            node_ancestor = node_ancestor_parent;
        }
        true
    }
}

impl Visibility {
    pub fn from_hir(visibility: &hir::Visibility, id: NodeId, tcx: TyCtxt) -> Self {
        match *visibility {
            hir::Public => Visibility::Public,
            hir::Visibility::Crate => Visibility::Restricted(ast::CRATE_NODE_ID),
            hir::Visibility::Restricted { id, .. } => match tcx.expect_def(id) {
                // If there is no resolution, `resolve` will have already reported an error, so
                // assume that the visibility is public to avoid reporting more privacy errors.
                Def::Err => Visibility::Public,
                def => Visibility::Restricted(tcx.map.as_local_node_id(def.def_id()).unwrap()),
            },
            hir::Inherited => Visibility::Restricted(tcx.map.get_module_parent(id)),
        }
    }

    /// Returns true if an item with this visibility is accessible from the given block.
    pub fn is_accessible_from<T: NodeIdTree>(self, block: NodeId, tree: &T) -> bool {
        let restriction = match self {
            // Public items are visible everywhere.
            Visibility::Public => return true,
            // Private items from other crates are visible nowhere.
            Visibility::PrivateExternal => return false,
            // Restricted items are visible in an arbitrary local module.
            Visibility::Restricted(module) => module,
        };

        tree.is_descendant_of(block, restriction)
    }

    /// Returns true if this visibility is at least as accessible as the given visibility
    pub fn is_at_least<T: NodeIdTree>(self, vis: Visibility, tree: &T) -> bool {
        let vis_restriction = match vis {
            Visibility::Public => return self == Visibility::Public,
            Visibility::PrivateExternal => return true,
            Visibility::Restricted(module) => module,
        };

        self.is_accessible_from(vis_restriction, tree)
    }
}

#[derive(Clone, Debug)]
pub struct Method<'tcx> {
    pub name: Name,
    pub generics: Generics<'tcx>,
    pub predicates: GenericPredicates<'tcx>,
    pub fty: &'tcx BareFnTy<'tcx>,
    pub explicit_self: ExplicitSelfCategory,
    pub vis: Visibility,
    pub defaultness: hir::Defaultness,
    pub def_id: DefId,
    pub container: ImplOrTraitItemContainer,
}

impl<'tcx> Method<'tcx> {
    pub fn new(name: Name,
               generics: ty::Generics<'tcx>,
               predicates: GenericPredicates<'tcx>,
               fty: &'tcx BareFnTy<'tcx>,
               explicit_self: ExplicitSelfCategory,
               vis: Visibility,
               defaultness: hir::Defaultness,
               def_id: DefId,
               container: ImplOrTraitItemContainer)
               -> Method<'tcx> {
        Method {
            name: name,
            generics: generics,
            predicates: predicates,
            fty: fty,
            explicit_self: explicit_self,
            vis: vis,
            defaultness: defaultness,
            def_id: def_id,
            container: container,
        }
    }

    pub fn container_id(&self) -> DefId {
        match self.container {
            TraitContainer(id) => id,
            ImplContainer(id) => id,
        }
    }
}

impl<'tcx> PartialEq for Method<'tcx> {
    #[inline]
    fn eq(&self, other: &Self) -> bool { self.def_id == other.def_id }
}

impl<'tcx> Eq for Method<'tcx> {}

impl<'tcx> Hash for Method<'tcx> {
    #[inline]
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.def_id.hash(s)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AssociatedConst<'tcx> {
    pub name: Name,
    pub ty: Ty<'tcx>,
    pub vis: Visibility,
    pub defaultness: hir::Defaultness,
    pub def_id: DefId,
    pub container: ImplOrTraitItemContainer,
    pub has_value: bool
}

#[derive(Clone, Copy, Debug)]
pub struct AssociatedType<'tcx> {
    pub name: Name,
    pub ty: Option<Ty<'tcx>>,
    pub vis: Visibility,
    pub defaultness: hir::Defaultness,
    pub def_id: DefId,
    pub container: ImplOrTraitItemContainer,
}

#[derive(Clone, PartialEq, RustcDecodable, RustcEncodable)]
pub struct ItemVariances {
    pub types: VecPerParamSpace<Variance>,
    pub regions: VecPerParamSpace<Variance>,
}

#[derive(Clone, PartialEq, RustcDecodable, RustcEncodable, Copy)]
pub enum Variance {
    Covariant,      // T<A> <: T<B> iff A <: B -- e.g., function return type
    Invariant,      // T<A> <: T<B> iff B == A -- e.g., type of mutable cell
    Contravariant,  // T<A> <: T<B> iff B <: A -- e.g., function param type
    Bivariant,      // T<A> <: T<B>            -- e.g., unused type parameter
}

#[derive(Clone, Copy, Debug)]
pub struct MethodCallee<'tcx> {
    /// Impl method ID, for inherent methods, or trait method ID, otherwise.
    pub def_id: DefId,
    pub ty: Ty<'tcx>,
    pub substs: &'tcx subst::Substs<'tcx>
}

/// With method calls, we store some extra information in
/// side tables (i.e method_map). We use
/// MethodCall as a key to index into these tables instead of
/// just directly using the expression's NodeId. The reason
/// for this being that we may apply adjustments (coercions)
/// with the resulting expression also needing to use the
/// side tables. The problem with this is that we don't
/// assign a separate NodeId to this new expression
/// and so it would clash with the base expression if both
/// needed to add to the side tables. Thus to disambiguate
/// we also keep track of whether there's an adjustment in
/// our key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MethodCall {
    pub expr_id: NodeId,
    pub autoderef: u32
}

impl MethodCall {
    pub fn expr(id: NodeId) -> MethodCall {
        MethodCall {
            expr_id: id,
            autoderef: 0
        }
    }

    pub fn autoderef(expr_id: NodeId, autoderef: u32) -> MethodCall {
        MethodCall {
            expr_id: expr_id,
            autoderef: 1 + autoderef
        }
    }
}

// maps from an expression id that corresponds to a method call to the details
// of the method to be invoked
pub type MethodMap<'tcx> = FnvHashMap<MethodCall, MethodCallee<'tcx>>;

// Contains information needed to resolve types and (in the future) look up
// the types of AST nodes.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct CReaderCacheKey {
    pub cnum: CrateNum,
    pub pos: usize,
}

/// Describes the fragment-state associated with a NodeId.
///
/// Currently only unfragmented paths have entries in the table,
/// but longer-term this enum is expected to expand to also
/// include data for fragmented paths.
#[derive(Copy, Clone, Debug)]
pub enum FragmentInfo {
    Moved { var: NodeId, move_expr: NodeId },
    Assigned { var: NodeId, assign_expr: NodeId, assignee_id: NodeId },
}

// Flags that we track on types. These flags are propagated upwards
// through the type during type construction, so that we can quickly
// check whether the type has various kinds of types in it without
// recursing over the type itself.
bitflags! {
    flags TypeFlags: u32 {
        const HAS_PARAMS         = 1 << 0,
        const HAS_SELF           = 1 << 1,
        const HAS_TY_INFER       = 1 << 2,
        const HAS_RE_INFER       = 1 << 3,
        const HAS_RE_SKOL        = 1 << 4,
        const HAS_RE_EARLY_BOUND = 1 << 5,
        const HAS_FREE_REGIONS   = 1 << 6,
        const HAS_TY_ERR         = 1 << 7,
        const HAS_PROJECTION     = 1 << 8,
        const HAS_TY_CLOSURE     = 1 << 9,

        // true if there are "names" of types and regions and so forth
        // that are local to a particular fn
        const HAS_LOCAL_NAMES    = 1 << 10,

        // Present if the type belongs in a local type context.
        // Only set for TyInfer other than Fresh.
        const KEEP_IN_LOCAL_TCX  = 1 << 11,

        const NEEDS_SUBST        = TypeFlags::HAS_PARAMS.bits |
                                   TypeFlags::HAS_SELF.bits |
                                   TypeFlags::HAS_RE_EARLY_BOUND.bits,

        // Flags representing the nominal content of a type,
        // computed by FlagsComputation. If you add a new nominal
        // flag, it should be added here too.
        const NOMINAL_FLAGS     = TypeFlags::HAS_PARAMS.bits |
                                  TypeFlags::HAS_SELF.bits |
                                  TypeFlags::HAS_TY_INFER.bits |
                                  TypeFlags::HAS_RE_INFER.bits |
                                  TypeFlags::HAS_RE_EARLY_BOUND.bits |
                                  TypeFlags::HAS_FREE_REGIONS.bits |
                                  TypeFlags::HAS_TY_ERR.bits |
                                  TypeFlags::HAS_PROJECTION.bits |
                                  TypeFlags::HAS_TY_CLOSURE.bits |
                                  TypeFlags::HAS_LOCAL_NAMES.bits |
                                  TypeFlags::KEEP_IN_LOCAL_TCX.bits,

        // Caches for type_is_sized, type_moves_by_default
        const SIZEDNESS_CACHED  = 1 << 16,
        const IS_SIZED          = 1 << 17,
        const MOVENESS_CACHED   = 1 << 18,
        const MOVES_BY_DEFAULT  = 1 << 19,
    }
}

pub struct TyS<'tcx> {
    pub sty: TypeVariants<'tcx>,
    pub flags: Cell<TypeFlags>,

    // the maximal depth of any bound regions appearing in this type.
    region_depth: u32,
}

impl<'tcx> PartialEq for TyS<'tcx> {
    #[inline]
    fn eq(&self, other: &TyS<'tcx>) -> bool {
        // (self as *const _) == (other as *const _)
        (self as *const TyS<'tcx>) == (other as *const TyS<'tcx>)
    }
}
impl<'tcx> Eq for TyS<'tcx> {}

impl<'tcx> Hash for TyS<'tcx> {
    fn hash<H: Hasher>(&self, s: &mut H) {
        (self as *const TyS).hash(s)
    }
}

pub type Ty<'tcx> = &'tcx TyS<'tcx>;

impl<'tcx> Encodable for Ty<'tcx> {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        cstore::tls::with_encoding_context(s, |ecx, rbml_w| {
            ecx.encode_ty(rbml_w, *self);
            Ok(())
        })
    }
}

impl<'tcx> Decodable for Ty<'tcx> {
    fn decode<D: Decoder>(d: &mut D) -> Result<Ty<'tcx>, D::Error> {
        cstore::tls::with_decoding_context(d, |dcx, rbml_r| {
            Ok(dcx.decode_ty(rbml_r))
        })
    }
}


/// Upvars do not get their own node-id. Instead, we use the pair of
/// the original var id (that is, the root variable that is referenced
/// by the upvar) and the id of the closure expression.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpvarId {
    pub var_id: NodeId,
    pub closure_expr_id: NodeId,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, RustcEncodable, RustcDecodable, Copy)]
pub enum BorrowKind {
    /// Data must be immutable and is aliasable.
    ImmBorrow,

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
    UniqueImmBorrow,

    /// Data is mutable and not aliasable.
    MutBorrow
}

/// Information describing the capture of an upvar. This is computed
/// during `typeck`, specifically by `regionck`.
#[derive(PartialEq, Clone, Debug, Copy)]
pub enum UpvarCapture {
    /// Upvar is captured by value. This is always true when the
    /// closure is labeled `move`, but can also be true in other cases
    /// depending on inference.
    ByValue,

    /// Upvar is captured by reference.
    ByRef(UpvarBorrow),
}

#[derive(PartialEq, Clone, Copy)]
pub struct UpvarBorrow {
    /// The kind of borrow: by-ref upvars have access to shared
    /// immutable borrows, which are not part of the normal language
    /// syntax.
    pub kind: BorrowKind,

    /// Region of the resulting reference.
    pub region: ty::Region,
}

pub type UpvarCaptureMap = FnvHashMap<UpvarId, UpvarCapture>;

#[derive(Copy, Clone)]
pub struct ClosureUpvar<'tcx> {
    pub def: Def,
    pub span: Span,
    pub ty: Ty<'tcx>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum IntVarValue {
    IntType(ast::IntTy),
    UintType(ast::UintTy),
}

/// Default region to use for the bound of objects that are
/// supplied as the value for this type parameter. This is derived
/// from `T:'a` annotations appearing in the type definition.  If
/// this is `None`, then the default is inherited from the
/// surrounding context. See RFC #599 for details.
#[derive(Copy, Clone)]
pub enum ObjectLifetimeDefault {
    /// Require an explicit annotation. Occurs when multiple
    /// `T:'a` constraints are found.
    Ambiguous,

    /// Use the base default, typically 'static, but in a fn body it is a fresh variable
    BaseDefault,

    /// Use the given region as the default.
    Specific(Region),
}

#[derive(Clone)]
pub struct TypeParameterDef<'tcx> {
    pub name: Name,
    pub def_id: DefId,
    pub space: subst::ParamSpace,
    pub index: u32,
    pub default_def_id: DefId, // for use in error reporing about defaults
    pub default: Option<Ty<'tcx>>,
    pub object_lifetime_default: ObjectLifetimeDefault,
}

#[derive(Clone)]
pub struct RegionParameterDef {
    pub name: Name,
    pub def_id: DefId,
    pub space: subst::ParamSpace,
    pub index: u32,
    pub bounds: Vec<ty::Region>,
}

impl RegionParameterDef {
    pub fn to_early_bound_region(&self) -> ty::Region {
        ty::ReEarlyBound(ty::EarlyBoundRegion {
            space: self.space,
            index: self.index,
            name: self.name,
        })
    }
    pub fn to_bound_region(&self) -> ty::BoundRegion {
        // this is an early bound region, so unaffected by #32330
        ty::BoundRegion::BrNamed(self.def_id, self.name, Issue32330::WontChange)
    }
}

/// Information about the formal type/lifetime parameters associated
/// with an item or method. Analogous to hir::Generics.
#[derive(Clone, Debug)]
pub struct Generics<'tcx> {
    pub types: VecPerParamSpace<TypeParameterDef<'tcx>>,
    pub regions: VecPerParamSpace<RegionParameterDef>,
}

impl<'tcx> Generics<'tcx> {
    pub fn empty() -> Generics<'tcx> {
        Generics {
            types: VecPerParamSpace::empty(),
            regions: VecPerParamSpace::empty(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.regions.is_empty()
    }

    pub fn has_type_params(&self, space: subst::ParamSpace) -> bool {
        !self.types.is_empty_in(space)
    }

    pub fn has_region_params(&self, space: subst::ParamSpace) -> bool {
        !self.regions.is_empty_in(space)
    }
}

/// Bounds on generics.
#[derive(Clone)]
pub struct GenericPredicates<'tcx> {
    pub predicates: VecPerParamSpace<Predicate<'tcx>>,
}

impl<'a, 'gcx, 'tcx> GenericPredicates<'tcx> {
    pub fn empty() -> GenericPredicates<'tcx> {
        GenericPredicates {
            predicates: VecPerParamSpace::empty(),
        }
    }

    pub fn instantiate(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>, substs: &Substs<'tcx>)
                       -> InstantiatedPredicates<'tcx> {
        InstantiatedPredicates {
            predicates: self.predicates.subst(tcx, substs),
        }
    }

    pub fn instantiate_supertrait(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                  poly_trait_ref: &ty::PolyTraitRef<'tcx>)
                                  -> InstantiatedPredicates<'tcx>
    {
        InstantiatedPredicates {
            predicates: self.predicates.map(|pred| {
                pred.subst_supertrait(tcx, poly_trait_ref)
            })
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Predicate<'tcx> {
    /// Corresponds to `where Foo : Bar<A,B,C>`. `Foo` here would be
    /// the `Self` type of the trait reference and `A`, `B`, and `C`
    /// would be the parameters in the `TypeSpace`.
    Trait(PolyTraitPredicate<'tcx>),

    /// A predicate created by RFC1592
    Rfc1592(Box<Predicate<'tcx>>),

    /// where `T1 == T2`.
    Equate(PolyEquatePredicate<'tcx>),

    /// where 'a : 'b
    RegionOutlives(PolyRegionOutlivesPredicate),

    /// where T : 'a
    TypeOutlives(PolyTypeOutlivesPredicate<'tcx>),

    /// where <T as TraitRef>::Name == X, approximately.
    /// See `ProjectionPredicate` struct for details.
    Projection(PolyProjectionPredicate<'tcx>),

    /// no syntax: T WF
    WellFormed(Ty<'tcx>),

    /// trait must be object-safe
    ObjectSafe(DefId),

    /// No direct syntax. May be thought of as `where T : FnFoo<...>` for some 'TypeSpace'
    /// substitutions `...` and T being a closure type.  Satisfied (or refuted) once we know the
    /// closure's kind.
    ClosureKind(DefId, ClosureKind),
}

impl<'a, 'gcx, 'tcx> Predicate<'tcx> {
    /// Performs a substitution suitable for going from a
    /// poly-trait-ref to supertraits that must hold if that
    /// poly-trait-ref holds. This is slightly different from a normal
    /// substitution in terms of what happens with bound regions.  See
    /// lengthy comment below for details.
    pub fn subst_supertrait(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>,
                            trait_ref: &ty::PolyTraitRef<'tcx>)
                            -> ty::Predicate<'tcx>
    {
        // The interaction between HRTB and supertraits is not entirely
        // obvious. Let me walk you (and myself) through an example.
        //
        // Let's start with an easy case. Consider two traits:
        //
        //     trait Foo<'a> : Bar<'a,'a> { }
        //     trait Bar<'b,'c> { }
        //
        // Now, if we have a trait reference `for<'x> T : Foo<'x>`, then
        // we can deduce that `for<'x> T : Bar<'x,'x>`. Basically, if we
        // knew that `Foo<'x>` (for any 'x) then we also know that
        // `Bar<'x,'x>` (for any 'x). This more-or-less falls out from
        // normal substitution.
        //
        // In terms of why this is sound, the idea is that whenever there
        // is an impl of `T:Foo<'a>`, it must show that `T:Bar<'a,'a>`
        // holds.  So if there is an impl of `T:Foo<'a>` that applies to
        // all `'a`, then we must know that `T:Bar<'a,'a>` holds for all
        // `'a`.
        //
        // Another example to be careful of is this:
        //
        //     trait Foo1<'a> : for<'b> Bar1<'a,'b> { }
        //     trait Bar1<'b,'c> { }
        //
        // Here, if we have `for<'x> T : Foo1<'x>`, then what do we know?
        // The answer is that we know `for<'x,'b> T : Bar1<'x,'b>`. The
        // reason is similar to the previous example: any impl of
        // `T:Foo1<'x>` must show that `for<'b> T : Bar1<'x, 'b>`.  So
        // basically we would want to collapse the bound lifetimes from
        // the input (`trait_ref`) and the supertraits.
        //
        // To achieve this in practice is fairly straightforward. Let's
        // consider the more complicated scenario:
        //
        // - We start out with `for<'x> T : Foo1<'x>`. In this case, `'x`
        //   has a De Bruijn index of 1. We want to produce `for<'x,'b> T : Bar1<'x,'b>`,
        //   where both `'x` and `'b` would have a DB index of 1.
        //   The substitution from the input trait-ref is therefore going to be
        //   `'a => 'x` (where `'x` has a DB index of 1).
        // - The super-trait-ref is `for<'b> Bar1<'a,'b>`, where `'a` is an
        //   early-bound parameter and `'b' is a late-bound parameter with a
        //   DB index of 1.
        // - If we replace `'a` with `'x` from the input, it too will have
        //   a DB index of 1, and thus we'll have `for<'x,'b> Bar1<'x,'b>`
        //   just as we wanted.
        //
        // There is only one catch. If we just apply the substitution `'a
        // => 'x` to `for<'b> Bar1<'a,'b>`, the substitution code will
        // adjust the DB index because we substituting into a binder (it
        // tries to be so smart...) resulting in `for<'x> for<'b>
        // Bar1<'x,'b>` (we have no syntax for this, so use your
        // imagination). Basically the 'x will have DB index of 2 and 'b
        // will have DB index of 1. Not quite what we want. So we apply
        // the substitution to the *contents* of the trait reference,
        // rather than the trait reference itself (put another way, the
        // substitution code expects equal binding levels in the values
        // from the substitution and the value being substituted into, and
        // this trick achieves that).

        let substs = &trait_ref.0.substs;
        match *self {
            Predicate::Trait(ty::Binder(ref data)) =>
                Predicate::Trait(ty::Binder(data.subst(tcx, substs))),
            Predicate::Rfc1592(ref pi) =>
                Predicate::Rfc1592(Box::new(pi.subst_supertrait(tcx, trait_ref))),
            Predicate::Equate(ty::Binder(ref data)) =>
                Predicate::Equate(ty::Binder(data.subst(tcx, substs))),
            Predicate::RegionOutlives(ty::Binder(ref data)) =>
                Predicate::RegionOutlives(ty::Binder(data.subst(tcx, substs))),
            Predicate::TypeOutlives(ty::Binder(ref data)) =>
                Predicate::TypeOutlives(ty::Binder(data.subst(tcx, substs))),
            Predicate::Projection(ty::Binder(ref data)) =>
                Predicate::Projection(ty::Binder(data.subst(tcx, substs))),
            Predicate::WellFormed(data) =>
                Predicate::WellFormed(data.subst(tcx, substs)),
            Predicate::ObjectSafe(trait_def_id) =>
                Predicate::ObjectSafe(trait_def_id),
            Predicate::ClosureKind(closure_def_id, kind) =>
                Predicate::ClosureKind(closure_def_id, kind),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TraitPredicate<'tcx> {
    pub trait_ref: TraitRef<'tcx>
}
pub type PolyTraitPredicate<'tcx> = ty::Binder<TraitPredicate<'tcx>>;

impl<'tcx> TraitPredicate<'tcx> {
    pub fn def_id(&self) -> DefId {
        self.trait_ref.def_id
    }

    /// Creates the dep-node for selecting/evaluating this trait reference.
    fn dep_node(&self) -> DepNode<DefId> {
        // Ideally, the dep-node would just have all the input types
        // in it.  But they are limited to including def-ids. So as an
        // approximation we include the def-ids for all nominal types
        // found somewhere. This means that we will e.g. conflate the
        // dep-nodes for `u32: SomeTrait` and `u64: SomeTrait`, but we
        // would have distinct dep-nodes for `Vec<u32>: SomeTrait`,
        // `Rc<u32>: SomeTrait`, and `(Vec<u32>, Rc<u32>): SomeTrait`.
        // Note that it's always sound to conflate dep-nodes, it just
        // leads to more recompilation.
        let def_ids: Vec<_> =
            self.input_types()
                .iter()
                .flat_map(|t| t.walk())
                .filter_map(|t| match t.sty {
                    ty::TyStruct(adt_def, _) |
                    ty::TyEnum(adt_def, _) =>
                        Some(adt_def.did),
                    _ =>
                        None
                })
                .collect();
        DepNode::TraitSelect(self.def_id(), def_ids)
    }

    pub fn input_types(&self) -> &[Ty<'tcx>] {
        self.trait_ref.substs.types.as_slice()
    }

    pub fn self_ty(&self) -> Ty<'tcx> {
        self.trait_ref.self_ty()
    }
}

impl<'tcx> PolyTraitPredicate<'tcx> {
    pub fn def_id(&self) -> DefId {
        // ok to skip binder since trait def-id does not care about regions
        self.0.def_id()
    }

    pub fn dep_node(&self) -> DepNode<DefId> {
        // ok to skip binder since depnode does not care about regions
        self.0.dep_node()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EquatePredicate<'tcx>(pub Ty<'tcx>, pub Ty<'tcx>); // `0 == 1`
pub type PolyEquatePredicate<'tcx> = ty::Binder<EquatePredicate<'tcx>>;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OutlivesPredicate<A,B>(pub A, pub B); // `A : B`
pub type PolyOutlivesPredicate<A,B> = ty::Binder<OutlivesPredicate<A,B>>;
pub type PolyRegionOutlivesPredicate = PolyOutlivesPredicate<ty::Region, ty::Region>;
pub type PolyTypeOutlivesPredicate<'tcx> = PolyOutlivesPredicate<Ty<'tcx>, ty::Region>;

/// This kind of predicate has no *direct* correspondent in the
/// syntax, but it roughly corresponds to the syntactic forms:
///
/// 1. `T : TraitRef<..., Item=Type>`
/// 2. `<T as TraitRef<...>>::Item == Type` (NYI)
///
/// In particular, form #1 is "desugared" to the combination of a
/// normal trait predicate (`T : TraitRef<...>`) and one of these
/// predicates. Form #2 is a broader form in that it also permits
/// equality between arbitrary types. Processing an instance of Form
/// #2 eventually yields one of these `ProjectionPredicate`
/// instances to normalize the LHS.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ProjectionPredicate<'tcx> {
    pub projection_ty: ProjectionTy<'tcx>,
    pub ty: Ty<'tcx>,
}

pub type PolyProjectionPredicate<'tcx> = Binder<ProjectionPredicate<'tcx>>;

impl<'tcx> PolyProjectionPredicate<'tcx> {
    pub fn item_name(&self) -> Name {
        self.0.projection_ty.item_name // safe to skip the binder to access a name
    }

    pub fn sort_key(&self) -> (DefId, Name) {
        self.0.projection_ty.sort_key()
    }
}

pub trait ToPolyTraitRef<'tcx> {
    fn to_poly_trait_ref(&self) -> PolyTraitRef<'tcx>;
}

impl<'tcx> ToPolyTraitRef<'tcx> for TraitRef<'tcx> {
    fn to_poly_trait_ref(&self) -> PolyTraitRef<'tcx> {
        assert!(!self.has_escaping_regions());
        ty::Binder(self.clone())
    }
}

impl<'tcx> ToPolyTraitRef<'tcx> for PolyTraitPredicate<'tcx> {
    fn to_poly_trait_ref(&self) -> PolyTraitRef<'tcx> {
        self.map_bound_ref(|trait_pred| trait_pred.trait_ref)
    }
}

impl<'tcx> ToPolyTraitRef<'tcx> for PolyProjectionPredicate<'tcx> {
    fn to_poly_trait_ref(&self) -> PolyTraitRef<'tcx> {
        // Note: unlike with TraitRef::to_poly_trait_ref(),
        // self.0.trait_ref is permitted to have escaping regions.
        // This is because here `self` has a `Binder` and so does our
        // return value, so we are preserving the number of binding
        // levels.
        ty::Binder(self.0.projection_ty.trait_ref)
    }
}

pub trait ToPredicate<'tcx> {
    fn to_predicate(&self) -> Predicate<'tcx>;
}

impl<'tcx> ToPredicate<'tcx> for TraitRef<'tcx> {
    fn to_predicate(&self) -> Predicate<'tcx> {
        // we're about to add a binder, so let's check that we don't
        // accidentally capture anything, or else that might be some
        // weird debruijn accounting.
        assert!(!self.has_escaping_regions());

        ty::Predicate::Trait(ty::Binder(ty::TraitPredicate {
            trait_ref: self.clone()
        }))
    }
}

impl<'tcx> ToPredicate<'tcx> for PolyTraitRef<'tcx> {
    fn to_predicate(&self) -> Predicate<'tcx> {
        ty::Predicate::Trait(self.to_poly_trait_predicate())
    }
}

impl<'tcx> ToPredicate<'tcx> for PolyEquatePredicate<'tcx> {
    fn to_predicate(&self) -> Predicate<'tcx> {
        Predicate::Equate(self.clone())
    }
}

impl<'tcx> ToPredicate<'tcx> for PolyRegionOutlivesPredicate {
    fn to_predicate(&self) -> Predicate<'tcx> {
        Predicate::RegionOutlives(self.clone())
    }
}

impl<'tcx> ToPredicate<'tcx> for PolyTypeOutlivesPredicate<'tcx> {
    fn to_predicate(&self) -> Predicate<'tcx> {
        Predicate::TypeOutlives(self.clone())
    }
}

impl<'tcx> ToPredicate<'tcx> for PolyProjectionPredicate<'tcx> {
    fn to_predicate(&self) -> Predicate<'tcx> {
        Predicate::Projection(self.clone())
    }
}

impl<'tcx> Predicate<'tcx> {
    /// Iterates over the types in this predicate. Note that in all
    /// cases this is skipping over a binder, so late-bound regions
    /// with depth 0 are bound by the predicate.
    pub fn walk_tys(&self) -> IntoIter<Ty<'tcx>> {
        let vec: Vec<_> = match *self {
            ty::Predicate::Trait(ref data) => {
                data.0.trait_ref.substs.types.as_slice().to_vec()
            }
            ty::Predicate::Rfc1592(ref data) => {
                return data.walk_tys()
            }
            ty::Predicate::Equate(ty::Binder(ref data)) => {
                vec![data.0, data.1]
            }
            ty::Predicate::TypeOutlives(ty::Binder(ref data)) => {
                vec![data.0]
            }
            ty::Predicate::RegionOutlives(..) => {
                vec![]
            }
            ty::Predicate::Projection(ref data) => {
                let trait_inputs = data.0.projection_ty.trait_ref.substs.types.as_slice();
                trait_inputs.iter()
                            .cloned()
                            .chain(Some(data.0.ty))
                            .collect()
            }
            ty::Predicate::WellFormed(data) => {
                vec![data]
            }
            ty::Predicate::ObjectSafe(_trait_def_id) => {
                vec![]
            }
            ty::Predicate::ClosureKind(_closure_def_id, _kind) => {
                vec![]
            }
        };

        // The only reason to collect into a vector here is that I was
        // too lazy to make the full (somewhat complicated) iterator
        // type that would be needed here. But I wanted this fn to
        // return an iterator conceptually, rather than a `Vec`, so as
        // to be closer to `Ty::walk`.
        vec.into_iter()
    }

    pub fn to_opt_poly_trait_ref(&self) -> Option<PolyTraitRef<'tcx>> {
        match *self {
            Predicate::Trait(ref t) => {
                Some(t.to_poly_trait_ref())
            }
            Predicate::Rfc1592(..) |
            Predicate::Projection(..) |
            Predicate::Equate(..) |
            Predicate::RegionOutlives(..) |
            Predicate::WellFormed(..) |
            Predicate::ObjectSafe(..) |
            Predicate::ClosureKind(..) |
            Predicate::TypeOutlives(..) => {
                None
            }
        }
    }
}

/// Represents the bounds declared on a particular set of type
/// parameters.  Should eventually be generalized into a flag list of
/// where clauses.  You can obtain a `InstantiatedPredicates` list from a
/// `GenericPredicates` by using the `instantiate` method. Note that this method
/// reflects an important semantic invariant of `InstantiatedPredicates`: while
/// the `GenericPredicates` are expressed in terms of the bound type
/// parameters of the impl/trait/whatever, an `InstantiatedPredicates` instance
/// represented a set of bounds for some particular instantiation,
/// meaning that the generic parameters have been substituted with
/// their values.
///
/// Example:
///
///     struct Foo<T,U:Bar<T>> { ... }
///
/// Here, the `GenericPredicates` for `Foo` would contain a list of bounds like
/// `[[], [U:Bar<T>]]`.  Now if there were some particular reference
/// like `Foo<isize,usize>`, then the `InstantiatedPredicates` would be `[[],
/// [usize:Bar<isize>]]`.
#[derive(Clone)]
pub struct InstantiatedPredicates<'tcx> {
    pub predicates: VecPerParamSpace<Predicate<'tcx>>,
}

impl<'tcx> InstantiatedPredicates<'tcx> {
    pub fn empty() -> InstantiatedPredicates<'tcx> {
        InstantiatedPredicates { predicates: VecPerParamSpace::empty() }
    }

    pub fn is_empty(&self) -> bool {
        self.predicates.is_empty()
    }
}

impl<'tcx> TraitRef<'tcx> {
    pub fn new(def_id: DefId, substs: &'tcx Substs<'tcx>) -> TraitRef<'tcx> {
        TraitRef { def_id: def_id, substs: substs }
    }

    pub fn self_ty(&self) -> Ty<'tcx> {
        self.substs.self_ty().unwrap()
    }

    pub fn input_types(&self) -> &[Ty<'tcx>] {
        // Select only the "input types" from a trait-reference. For
        // now this is all the types that appear in the
        // trait-reference, but it should eventually exclude
        // associated types.
        self.substs.types.as_slice()
    }
}

/// When type checking, we use the `ParameterEnvironment` to track
/// details about the type/lifetime parameters that are in scope.
/// It primarily stores the bounds information.
///
/// Note: This information might seem to be redundant with the data in
/// `tcx.ty_param_defs`, but it is not. That table contains the
/// parameter definitions from an "outside" perspective, but this
/// struct will contain the bounds for a parameter as seen from inside
/// the function body. Currently the only real distinction is that
/// bound lifetime parameters are replaced with free ones, but in the
/// future I hope to refine the representation of types so as to make
/// more distinctions clearer.
#[derive(Clone)]
pub struct ParameterEnvironment<'tcx> {
    /// See `construct_free_substs` for details.
    pub free_substs: &'tcx Substs<'tcx>,

    /// Each type parameter has an implicit region bound that
    /// indicates it must outlive at least the function body (the user
    /// may specify stronger requirements). This field indicates the
    /// region of the callee.
    pub implicit_region_bound: ty::Region,

    /// Obligations that the caller must satisfy. This is basically
    /// the set of bounds on the in-scope type parameters, translated
    /// into Obligations, and elaborated and normalized.
    pub caller_bounds: Vec<ty::Predicate<'tcx>>,

    /// Scope that is attached to free regions for this scope. This
    /// is usually the id of the fn body, but for more abstract scopes
    /// like structs we often use the node-id of the struct.
    ///
    /// FIXME(#3696). It would be nice to refactor so that free
    /// regions don't have this implicit scope and instead introduce
    /// relationships in the environment.
    pub free_id_outlive: CodeExtent,
}

impl<'a, 'tcx> ParameterEnvironment<'tcx> {
    pub fn with_caller_bounds(&self,
                              caller_bounds: Vec<ty::Predicate<'tcx>>)
                              -> ParameterEnvironment<'tcx>
    {
        ParameterEnvironment {
            free_substs: self.free_substs,
            implicit_region_bound: self.implicit_region_bound,
            caller_bounds: caller_bounds,
            free_id_outlive: self.free_id_outlive,
        }
    }

    /// Construct a parameter environment given an item, impl item, or trait item
    pub fn for_item(tcx: TyCtxt<'a, 'tcx, 'tcx>, id: NodeId)
                    -> ParameterEnvironment<'tcx> {
        match tcx.map.find(id) {
            Some(ast_map::NodeImplItem(ref impl_item)) => {
                match impl_item.node {
                    hir::ImplItemKind::Type(_) | hir::ImplItemKind::Const(_, _) => {
                        // associated types don't have their own entry (for some reason),
                        // so for now just grab environment for the impl
                        let impl_id = tcx.map.get_parent(id);
                        let impl_def_id = tcx.map.local_def_id(impl_id);
                        let scheme = tcx.lookup_item_type(impl_def_id);
                        let predicates = tcx.lookup_predicates(impl_def_id);
                        tcx.construct_parameter_environment(impl_item.span,
                                                            &scheme.generics,
                                                            &predicates,
                                                            tcx.region_maps.item_extent(id))
                    }
                    hir::ImplItemKind::Method(_, ref body) => {
                        let method_def_id = tcx.map.local_def_id(id);
                        match tcx.impl_or_trait_item(method_def_id) {
                            MethodTraitItem(ref method_ty) => {
                                let method_generics = &method_ty.generics;
                                let method_bounds = &method_ty.predicates;
                                tcx.construct_parameter_environment(
                                    impl_item.span,
                                    method_generics,
                                    method_bounds,
                                    tcx.region_maps.call_site_extent(id, body.id))
                            }
                            _ => {
                                bug!("ParameterEnvironment::for_item(): \
                                      got non-method item from impl method?!")
                            }
                        }
                    }
                }
            }
            Some(ast_map::NodeTraitItem(trait_item)) => {
                match trait_item.node {
                    hir::TypeTraitItem(..) | hir::ConstTraitItem(..) => {
                        // associated types don't have their own entry (for some reason),
                        // so for now just grab environment for the trait
                        let trait_id = tcx.map.get_parent(id);
                        let trait_def_id = tcx.map.local_def_id(trait_id);
                        let trait_def = tcx.lookup_trait_def(trait_def_id);
                        let predicates = tcx.lookup_predicates(trait_def_id);
                        tcx.construct_parameter_environment(trait_item.span,
                                                            &trait_def.generics,
                                                            &predicates,
                                                            tcx.region_maps.item_extent(id))
                    }
                    hir::MethodTraitItem(_, ref body) => {
                        // Use call-site for extent (unless this is a
                        // trait method with no default; then fallback
                        // to the method id).
                        let method_def_id = tcx.map.local_def_id(id);
                        match tcx.impl_or_trait_item(method_def_id) {
                            MethodTraitItem(ref method_ty) => {
                                let method_generics = &method_ty.generics;
                                let method_bounds = &method_ty.predicates;
                                let extent = if let Some(ref body) = *body {
                                    // default impl: use call_site extent as free_id_outlive bound.
                                    tcx.region_maps.call_site_extent(id, body.id)
                                } else {
                                    // no default impl: use item extent as free_id_outlive bound.
                                    tcx.region_maps.item_extent(id)
                                };
                                tcx.construct_parameter_environment(
                                    trait_item.span,
                                    method_generics,
                                    method_bounds,
                                    extent)
                            }
                            _ => {
                                bug!("ParameterEnvironment::for_item(): \
                                      got non-method item from provided \
                                      method?!")
                            }
                        }
                    }
                }
            }
            Some(ast_map::NodeItem(item)) => {
                match item.node {
                    hir::ItemFn(_, _, _, _, _, ref body) => {
                        // We assume this is a function.
                        let fn_def_id = tcx.map.local_def_id(id);
                        let fn_scheme = tcx.lookup_item_type(fn_def_id);
                        let fn_predicates = tcx.lookup_predicates(fn_def_id);

                        tcx.construct_parameter_environment(
                            item.span,
                            &fn_scheme.generics,
                            &fn_predicates,
                            tcx.region_maps.call_site_extent(id, body.id))
                    }
                    hir::ItemEnum(..) |
                    hir::ItemStruct(..) |
                    hir::ItemTy(..) |
                    hir::ItemImpl(..) |
                    hir::ItemConst(..) |
                    hir::ItemStatic(..) => {
                        let def_id = tcx.map.local_def_id(id);
                        let scheme = tcx.lookup_item_type(def_id);
                        let predicates = tcx.lookup_predicates(def_id);
                        tcx.construct_parameter_environment(item.span,
                                                            &scheme.generics,
                                                            &predicates,
                                                            tcx.region_maps.item_extent(id))
                    }
                    hir::ItemTrait(..) => {
                        let def_id = tcx.map.local_def_id(id);
                        let trait_def = tcx.lookup_trait_def(def_id);
                        let predicates = tcx.lookup_predicates(def_id);
                        tcx.construct_parameter_environment(item.span,
                                                            &trait_def.generics,
                                                            &predicates,
                                                            tcx.region_maps.item_extent(id))
                    }
                    _ => {
                        span_bug!(item.span,
                                  "ParameterEnvironment::for_item():
                                   can't create a parameter \
                                   environment for this kind of item")
                    }
                }
            }
            Some(ast_map::NodeExpr(..)) => {
                // This is a convenience to allow closures to work.
                ParameterEnvironment::for_item(tcx, tcx.map.get_parent(id))
            }
            Some(ast_map::NodeForeignItem(item)) => {
                let def_id = tcx.map.local_def_id(id);
                let scheme = tcx.lookup_item_type(def_id);
                let predicates = tcx.lookup_predicates(def_id);
                tcx.construct_parameter_environment(item.span,
                                                    &scheme.generics,
                                                    &predicates,
                                                    ROOT_CODE_EXTENT)
            }
            _ => {
                bug!("ParameterEnvironment::from_item(): \
                      `{}` is not an item",
                     tcx.map.node_to_string(id))
            }
        }
    }
}

/// A "type scheme", in ML terminology, is a type combined with some
/// set of generic types that the type is, well, generic over. In Rust
/// terms, it is the "type" of a fn item or struct -- this type will
/// include various generic parameters that must be substituted when
/// the item/struct is referenced. That is called converting the type
/// scheme to a monotype.
///
/// - `generics`: the set of type parameters and their bounds
/// - `ty`: the base types, which may reference the parameters defined
///   in `generics`
///
/// Note that TypeSchemes are also sometimes called "polytypes" (and
/// in fact this struct used to carry that name, so you may find some
/// stray references in a comment or something). We try to reserve the
/// "poly" prefix to refer to higher-ranked things, as in
/// `PolyTraitRef`.
///
/// Note that each item also comes with predicates, see
/// `lookup_predicates`.
#[derive(Clone, Debug)]
pub struct TypeScheme<'tcx> {
    pub generics: Generics<'tcx>,
    pub ty: Ty<'tcx>,
}

bitflags! {
    flags AdtFlags: u32 {
        const NO_ADT_FLAGS        = 0,
        const IS_ENUM             = 1 << 0,
        const IS_DTORCK           = 1 << 1, // is this a dtorck type?
        const IS_DTORCK_VALID     = 1 << 2,
        const IS_PHANTOM_DATA     = 1 << 3,
        const IS_SIMD             = 1 << 4,
        const IS_FUNDAMENTAL      = 1 << 5,
        const IS_NO_DROP_FLAG     = 1 << 6,
    }
}

pub type AdtDef<'tcx> = &'tcx AdtDefData<'tcx, 'static>;
pub type VariantDef<'tcx> = &'tcx VariantDefData<'tcx, 'static>;
pub type FieldDef<'tcx> = &'tcx FieldDefData<'tcx, 'static>;

// See comment on AdtDefData for explanation
pub type AdtDefMaster<'tcx> = &'tcx AdtDefData<'tcx, 'tcx>;
pub type VariantDefMaster<'tcx> = &'tcx VariantDefData<'tcx, 'tcx>;
pub type FieldDefMaster<'tcx> = &'tcx FieldDefData<'tcx, 'tcx>;

pub struct VariantDefData<'tcx, 'container: 'tcx> {
    /// The variant's DefId. If this is a tuple-like struct,
    /// this is the DefId of the struct's ctor.
    pub did: DefId,
    pub name: Name, // struct's name if this is a struct
    pub disr_val: Disr,
    pub fields: Vec<FieldDefData<'tcx, 'container>>,
    pub kind: VariantKind,
}

pub struct FieldDefData<'tcx, 'container: 'tcx> {
    /// The field's DefId. NOTE: the fields of tuple-like enum variants
    /// are not real items, and don't have entries in tcache etc.
    pub did: DefId,
    pub name: Name,
    pub vis: Visibility,
    /// TyIVar is used here to allow for variance (see the doc at
    /// AdtDefData).
    ///
    /// Note: direct accesses to `ty` must also add dep edges.
    ty: ivar::TyIVar<'tcx, 'container>
}

/// The definition of an abstract data type - a struct or enum.
///
/// These are all interned (by intern_adt_def) into the adt_defs
/// table.
///
/// Because of the possibility of nested tcx-s, this type
/// needs 2 lifetimes: the traditional variant lifetime ('tcx)
/// bounding the lifetime of the inner types is of course necessary.
/// However, it is not sufficient - types from a child tcx must
/// not be leaked into the master tcx by being stored in an AdtDefData.
///
/// The 'container lifetime ensures that by outliving the container
/// tcx and preventing shorter-lived types from being inserted. When
/// write access is not needed, the 'container lifetime can be
/// erased to 'static, which can be done by the AdtDef wrapper.
pub struct AdtDefData<'tcx, 'container: 'tcx> {
    pub did: DefId,
    pub variants: Vec<VariantDefData<'tcx, 'container>>,
    destructor: Cell<Option<DefId>>,
    flags: Cell<AdtFlags>,
    sized_constraint: ivar::TyIVar<'tcx, 'container>,
}

impl<'tcx, 'container> PartialEq for AdtDefData<'tcx, 'container> {
    // AdtDefData are always interned and this is part of TyS equality
    #[inline]
    fn eq(&self, other: &Self) -> bool { self as *const _ == other as *const _ }
}

impl<'tcx, 'container> Eq for AdtDefData<'tcx, 'container> {}

impl<'tcx, 'container> Hash for AdtDefData<'tcx, 'container> {
    #[inline]
    fn hash<H: Hasher>(&self, s: &mut H) {
        (self as *const AdtDefData).hash(s)
    }
}

impl<'tcx> Encodable for AdtDef<'tcx> {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        self.did.encode(s)
    }
}

impl<'tcx> Decodable for AdtDef<'tcx> {
    fn decode<D: Decoder>(d: &mut D) -> Result<AdtDef<'tcx>, D::Error> {
        let def_id: DefId = Decodable::decode(d)?;

        cstore::tls::with_decoding_context(d, |dcx, _| {
            let def_id = dcx.translate_def_id(def_id);
            Ok(dcx.tcx().lookup_adt_def(def_id))
        })
    }
}


#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AdtKind { Struct, Enum }

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, RustcEncodable, RustcDecodable)]
pub enum VariantKind { Struct, Tuple, Unit }

impl VariantKind {
    pub fn from_variant_data(vdata: &hir::VariantData) -> Self {
        match *vdata {
            hir::VariantData::Struct(..) => VariantKind::Struct,
            hir::VariantData::Tuple(..) => VariantKind::Tuple,
            hir::VariantData::Unit(..) => VariantKind::Unit,
        }
    }
}

impl<'a, 'gcx, 'tcx, 'container> AdtDefData<'gcx, 'container> {
    fn new(tcx: TyCtxt<'a, 'gcx, 'tcx>,
           did: DefId,
           kind: AdtKind,
           variants: Vec<VariantDefData<'gcx, 'container>>) -> Self {
        let mut flags = AdtFlags::NO_ADT_FLAGS;
        let attrs = tcx.get_attrs(did);
        if attr::contains_name(&attrs, "fundamental") {
            flags = flags | AdtFlags::IS_FUNDAMENTAL;
        }
        if attr::contains_name(&attrs, "unsafe_no_drop_flag") {
            flags = flags | AdtFlags::IS_NO_DROP_FLAG;
        }
        if tcx.lookup_simd(did) {
            flags = flags | AdtFlags::IS_SIMD;
        }
        if Some(did) == tcx.lang_items.phantom_data() {
            flags = flags | AdtFlags::IS_PHANTOM_DATA;
        }
        if let AdtKind::Enum = kind {
            flags = flags | AdtFlags::IS_ENUM;
        }
        AdtDefData {
            did: did,
            variants: variants,
            flags: Cell::new(flags),
            destructor: Cell::new(None),
            sized_constraint: ivar::TyIVar::new(),
        }
    }

    fn calculate_dtorck(&'gcx self, tcx: TyCtxt) {
        if tcx.is_adt_dtorck(self) {
            self.flags.set(self.flags.get() | AdtFlags::IS_DTORCK);
        }
        self.flags.set(self.flags.get() | AdtFlags::IS_DTORCK_VALID)
    }

    /// Returns the kind of the ADT - Struct or Enum.
    #[inline]
    pub fn adt_kind(&self) -> AdtKind {
        if self.flags.get().intersects(AdtFlags::IS_ENUM) {
            AdtKind::Enum
        } else {
            AdtKind::Struct
        }
    }

    /// Returns whether this is a dtorck type. If this returns
    /// true, this type being safe for destruction requires it to be
    /// alive; Otherwise, only the contents are required to be.
    #[inline]
    pub fn is_dtorck(&'gcx self, tcx: TyCtxt) -> bool {
        if !self.flags.get().intersects(AdtFlags::IS_DTORCK_VALID) {
            self.calculate_dtorck(tcx)
        }
        self.flags.get().intersects(AdtFlags::IS_DTORCK)
    }

    /// Returns whether this type is #[fundamental] for the purposes
    /// of coherence checking.
    #[inline]
    pub fn is_fundamental(&self) -> bool {
        self.flags.get().intersects(AdtFlags::IS_FUNDAMENTAL)
    }

    #[inline]
    pub fn is_simd(&self) -> bool {
        self.flags.get().intersects(AdtFlags::IS_SIMD)
    }

    /// Returns true if this is PhantomData<T>.
    #[inline]
    pub fn is_phantom_data(&self) -> bool {
        self.flags.get().intersects(AdtFlags::IS_PHANTOM_DATA)
    }

    /// Returns whether this type has a destructor.
    pub fn has_dtor(&self) -> bool {
        match self.dtor_kind() {
            NoDtor => false,
            TraitDtor(..) => true
        }
    }

    /// Asserts this is a struct and returns the struct's unique
    /// variant.
    pub fn struct_variant(&self) -> &VariantDefData<'gcx, 'container> {
        assert_eq!(self.adt_kind(), AdtKind::Struct);
        &self.variants[0]
    }

    #[inline]
    pub fn type_scheme(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>) -> TypeScheme<'gcx> {
        tcx.lookup_item_type(self.did)
    }

    #[inline]
    pub fn predicates(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>) -> GenericPredicates<'gcx> {
        tcx.lookup_predicates(self.did)
    }

    /// Returns an iterator over all fields contained
    /// by this ADT.
    #[inline]
    pub fn all_fields(&self) ->
            iter::FlatMap<
                slice::Iter<VariantDefData<'gcx, 'container>>,
                slice::Iter<FieldDefData<'gcx, 'container>>,
                for<'s> fn(&'s VariantDefData<'gcx, 'container>)
                    -> slice::Iter<'s, FieldDefData<'gcx, 'container>>
            > {
        self.variants.iter().flat_map(VariantDefData::fields_iter)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }

    #[inline]
    pub fn is_univariant(&self) -> bool {
        self.variants.len() == 1
    }

    pub fn is_payloadfree(&self) -> bool {
        !self.variants.is_empty() &&
            self.variants.iter().all(|v| v.fields.is_empty())
    }

    pub fn variant_with_id(&self, vid: DefId) -> &VariantDefData<'gcx, 'container> {
        self.variants
            .iter()
            .find(|v| v.did == vid)
            .expect("variant_with_id: unknown variant")
    }

    pub fn variant_index_with_id(&self, vid: DefId) -> usize {
        self.variants
            .iter()
            .position(|v| v.did == vid)
            .expect("variant_index_with_id: unknown variant")
    }

    pub fn variant_of_def(&self, def: Def) -> &VariantDefData<'gcx, 'container> {
        match def {
            Def::Variant(_, vid) => self.variant_with_id(vid),
            Def::Struct(..) | Def::TyAlias(..) => self.struct_variant(),
            _ => bug!("unexpected def {:?} in variant_of_def", def)
        }
    }

    pub fn destructor(&self) -> Option<DefId> {
        self.destructor.get()
    }

    pub fn set_destructor(&self, dtor: DefId) {
        self.destructor.set(Some(dtor));
    }

    pub fn dtor_kind(&self) -> DtorKind {
        match self.destructor.get() {
            Some(_) => {
                TraitDtor(!self.flags.get().intersects(AdtFlags::IS_NO_DROP_FLAG))
            }
            None => NoDtor,
        }
    }
}

impl<'a, 'gcx, 'tcx, 'container> AdtDefData<'tcx, 'container> {
    /// Returns a simpler type such that `Self: Sized` if and only
    /// if that type is Sized, or `TyErr` if this type is recursive.
    ///
    /// HACK: instead of returning a list of types, this function can
    /// return a tuple. In that case, the result is Sized only if
    /// all elements of the tuple are Sized.
    ///
    /// This is generally the `struct_tail` if this is a struct, or a
    /// tuple of them if this is an enum.
    ///
    /// Oddly enough, checking that the sized-constraint is Sized is
    /// actually more expressive than checking all members:
    /// the Sized trait is inductive, so an associated type that references
    /// Self would prevent its containing ADT from being Sized.
    ///
    /// Due to normalization being eager, this applies even if
    /// the associated type is behind a pointer, e.g. issue #31299.
    pub fn sized_constraint(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>) -> Ty<'tcx> {
        let dep_node = DepNode::SizedConstraint(self.did);
        match self.sized_constraint.get(dep_node) {
            None => {
                let global_tcx = tcx.global_tcx();
                let this = global_tcx.lookup_adt_def_master(self.did);
                this.calculate_sized_constraint_inner(global_tcx, &mut Vec::new());
                self.sized_constraint(tcx)
            }
            Some(ty) => ty
        }
    }
}

impl<'a, 'tcx> AdtDefData<'tcx, 'tcx> {
    /// Calculates the Sized-constraint.
    ///
    /// As the Sized-constraint of enums can be a *set* of types,
    /// the Sized-constraint may need to be a set also. Because introducing
    /// a new type of IVar is currently a complex affair, the Sized-constraint
    /// may be a tuple.
    ///
    /// In fact, there are only a few options for the constraint:
    ///     - `bool`, if the type is always Sized
    ///     - an obviously-unsized type
    ///     - a type parameter or projection whose Sizedness can't be known
    ///     - a tuple of type parameters or projections, if there are multiple
    ///       such.
    ///     - a TyError, if a type contained itself. The representability
    ///       check should catch this case.
    fn calculate_sized_constraint_inner(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                        stack: &mut Vec<AdtDefMaster<'tcx>>)
    {

        let dep_node = || DepNode::SizedConstraint(self.did);
        if self.sized_constraint.get(dep_node()).is_some() {
            return;
        }

        if stack.contains(&self) {
            debug!("calculate_sized_constraint: {:?} is recursive", self);
            // This should be reported as an error by `check_representable`.
            //
            // Consider the type as Sized in the meanwhile to avoid
            // further errors.
            self.sized_constraint.fulfill(dep_node(), tcx.types.err);
            return;
        }

        stack.push(self);

        let tys : Vec<_> =
            self.variants.iter().flat_map(|v| {
                v.fields.last()
            }).flat_map(|f| {
                self.sized_constraint_for_ty(tcx, stack, f.unsubst_ty())
            }).collect();

        let self_ = stack.pop().unwrap();
        assert_eq!(self_, self);

        let ty = match tys.len() {
            _ if tys.references_error() => tcx.types.err,
            0 => tcx.types.bool,
            1 => tys[0],
            _ => tcx.mk_tup(tys)
        };

        match self.sized_constraint.get(dep_node()) {
            Some(old_ty) => {
                debug!("calculate_sized_constraint: {:?} recurred", self);
                assert_eq!(old_ty, tcx.types.err)
            }
            None => {
                debug!("calculate_sized_constraint: {:?} => {:?}", self, ty);
                self.sized_constraint.fulfill(dep_node(), ty)
            }
        }
    }

    fn sized_constraint_for_ty(
        &'tcx self,
        tcx: TyCtxt<'a, 'tcx, 'tcx>,
        stack: &mut Vec<AdtDefMaster<'tcx>>,
        ty: Ty<'tcx>
    ) -> Vec<Ty<'tcx>> {
        let result = match ty.sty {
            TyBool | TyChar | TyInt(..) | TyUint(..) | TyFloat(..) |
            TyBox(..) | TyRawPtr(..) | TyRef(..) | TyFnDef(..) | TyFnPtr(_) |
            TyArray(..) | TyClosure(..) => {
                vec![]
            }

            TyStr | TyTrait(..) | TySlice(_) | TyError => {
                // these are never sized - return the target type
                vec![ty]
            }

            TyTuple(ref tys) => {
                // FIXME(#33242) we only need to constrain the last field
                tys.iter().flat_map(|ty| {
                    self.sized_constraint_for_ty(tcx, stack, ty)
                }).collect()
            }

            TyEnum(adt, substs) | TyStruct(adt, substs) => {
                // recursive case
                let adt = tcx.lookup_adt_def_master(adt.did);
                adt.calculate_sized_constraint_inner(tcx, stack);
                let adt_ty =
                    adt.sized_constraint
                    .unwrap(DepNode::SizedConstraint(adt.did))
                    .subst(tcx, substs);
                debug!("sized_constraint_for_ty({:?}) intermediate = {:?}",
                       ty, adt_ty);
                if let ty::TyTuple(ref tys) = adt_ty.sty {
                    tys.iter().flat_map(|ty| {
                        self.sized_constraint_for_ty(tcx, stack, ty)
                    }).collect()
                } else {
                    self.sized_constraint_for_ty(tcx, stack, adt_ty)
                }
            }

            TyProjection(..) => {
                // must calculate explicitly.
                // FIXME: consider special-casing always-Sized projections
                vec![ty]
            }

            TyParam(..) => {
                // perf hack: if there is a `T: Sized` bound, then
                // we know that `T` is Sized and do not need to check
                // it on the impl.

                let sized_trait = match tcx.lang_items.sized_trait() {
                    Some(x) => x,
                    _ => return vec![ty]
                };
                let sized_predicate = Binder(TraitRef {
                    def_id: sized_trait,
                    substs: tcx.mk_substs(Substs::new_trait(
                        vec![], vec![], ty
                    ))
                }).to_predicate();
                let predicates = tcx.lookup_predicates(self.did).predicates;
                if predicates.into_iter().any(|p| p == sized_predicate) {
                    vec![]
                } else {
                    vec![ty]
                }
            }

            TyInfer(..) => {
                bug!("unexpected type `{:?}` in sized_constraint_for_ty",
                     ty)
            }
        };
        debug!("sized_constraint_for_ty({:?}) = {:?}", ty, result);
        result
    }
}

impl<'tcx, 'container> VariantDefData<'tcx, 'container> {
    #[inline]
    fn fields_iter(&self) -> slice::Iter<FieldDefData<'tcx, 'container>> {
        self.fields.iter()
    }

    pub fn kind(&self) -> VariantKind {
        self.kind
    }

    pub fn is_tuple_struct(&self) -> bool {
        self.kind() == VariantKind::Tuple
    }

    #[inline]
    pub fn find_field_named(&self,
                            name: ast::Name)
                            -> Option<&FieldDefData<'tcx, 'container>> {
        self.fields.iter().find(|f| f.name == name)
    }

    #[inline]
    pub fn index_of_field_named(&self,
                                name: ast::Name)
                                -> Option<usize> {
        self.fields.iter().position(|f| f.name == name)
    }

    #[inline]
    pub fn field_named(&self, name: ast::Name) -> &FieldDefData<'tcx, 'container> {
        self.find_field_named(name).unwrap()
    }
}

impl<'a, 'gcx, 'tcx, 'container> FieldDefData<'tcx, 'container> {
    pub fn new(did: DefId,
               name: Name,
               vis: Visibility) -> Self {
        FieldDefData {
            did: did,
            name: name,
            vis: vis,
            ty: ivar::TyIVar::new()
        }
    }

    pub fn ty(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>, subst: &Substs<'tcx>) -> Ty<'tcx> {
        self.unsubst_ty().subst(tcx, subst)
    }

    pub fn unsubst_ty(&self) -> Ty<'tcx> {
        self.ty.unwrap(DepNode::FieldTy(self.did))
    }

    pub fn fulfill_ty(&self, ty: Ty<'container>) {
        self.ty.fulfill(DepNode::FieldTy(self.did), ty);
    }
}

/// Records the substitutions used to translate the polytype for an
/// item into the monotype of an item reference.
#[derive(Clone)]
pub struct ItemSubsts<'tcx> {
    pub substs: &'tcx Substs<'tcx>,
}

#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash, Debug, RustcEncodable, RustcDecodable)]
pub enum ClosureKind {
    // Warning: Ordering is significant here! The ordering is chosen
    // because the trait Fn is a subtrait of FnMut and so in turn, and
    // hence we order it so that Fn < FnMut < FnOnce.
    Fn,
    FnMut,
    FnOnce,
}

impl<'a, 'tcx> ClosureKind {
    pub fn trait_did(&self, tcx: TyCtxt<'a, 'tcx, 'tcx>) -> DefId {
        let result = match *self {
            ClosureKind::Fn => tcx.lang_items.require(FnTraitLangItem),
            ClosureKind::FnMut => {
                tcx.lang_items.require(FnMutTraitLangItem)
            }
            ClosureKind::FnOnce => {
                tcx.lang_items.require(FnOnceTraitLangItem)
            }
        };
        match result {
            Ok(trait_did) => trait_did,
            Err(err) => tcx.sess.fatal(&err[..]),
        }
    }

    /// True if this a type that impls this closure kind
    /// must also implement `other`.
    pub fn extends(self, other: ty::ClosureKind) -> bool {
        match (self, other) {
            (ClosureKind::Fn, ClosureKind::Fn) => true,
            (ClosureKind::Fn, ClosureKind::FnMut) => true,
            (ClosureKind::Fn, ClosureKind::FnOnce) => true,
            (ClosureKind::FnMut, ClosureKind::FnMut) => true,
            (ClosureKind::FnMut, ClosureKind::FnOnce) => true,
            (ClosureKind::FnOnce, ClosureKind::FnOnce) => true,
            _ => false,
        }
    }
}

impl<'tcx> TyS<'tcx> {
    /// Iterator that walks `self` and any types reachable from
    /// `self`, in depth-first order. Note that just walks the types
    /// that appear in `self`, it does not descend into the fields of
    /// structs or variants. For example:
    ///
    /// ```notrust
    /// isize => { isize }
    /// Foo<Bar<isize>> => { Foo<Bar<isize>>, Bar<isize>, isize }
    /// [isize] => { [isize], isize }
    /// ```
    pub fn walk(&'tcx self) -> TypeWalker<'tcx> {
        TypeWalker::new(self)
    }

    /// Iterator that walks the immediate children of `self`.  Hence
    /// `Foo<Bar<i32>, u32>` yields the sequence `[Bar<i32>, u32]`
    /// (but not `i32`, like `walk`).
    pub fn walk_shallow(&'tcx self) -> IntoIter<Ty<'tcx>> {
        walk::walk_shallow(self)
    }

    /// Walks `ty` and any types appearing within `ty`, invoking the
    /// callback `f` on each type. If the callback returns false, then the
    /// children of the current type are ignored.
    ///
    /// Note: prefer `ty.walk()` where possible.
    pub fn maybe_walk<F>(&'tcx self, mut f: F)
        where F : FnMut(Ty<'tcx>) -> bool
    {
        let mut walker = self.walk();
        while let Some(ty) = walker.next() {
            if !f(ty) {
                walker.skip_current_subtree();
            }
        }
    }
}

impl<'tcx> ItemSubsts<'tcx> {
    pub fn is_noop(&self) -> bool {
        self.substs.is_noop()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LvaluePreference {
    PreferMutLvalue,
    NoPreference
}

impl LvaluePreference {
    pub fn from_mutbl(m: hir::Mutability) -> Self {
        match m {
            hir::MutMutable => PreferMutLvalue,
            hir::MutImmutable => NoPreference,
        }
    }
}

/// Helper for looking things up in the various maps that are populated during
/// typeck::collect (e.g., `tcx.impl_or_trait_items`, `tcx.tcache`, etc).  All of
/// these share the pattern that if the id is local, it should have been loaded
/// into the map by the `typeck::collect` phase.  If the def-id is external,
/// then we have to go consult the crate loading code (and cache the result for
/// the future).
fn lookup_locally_or_in_crate_store<M, F>(descr: &str,
                                          def_id: DefId,
                                          map: &M,
                                          load_external: F)
                                          -> M::Value where
    M: MemoizationMap<Key=DefId>,
    F: FnOnce() -> M::Value,
{
    map.memoize(def_id, || {
        if def_id.is_local() {
            bug!("No def'n found for {:?} in tcx.{}", def_id, descr);
        }
        load_external()
    })
}

impl BorrowKind {
    pub fn from_mutbl(m: hir::Mutability) -> BorrowKind {
        match m {
            hir::MutMutable => MutBorrow,
            hir::MutImmutable => ImmBorrow,
        }
    }

    /// Returns a mutability `m` such that an `&m T` pointer could be used to obtain this borrow
    /// kind. Because borrow kinds are richer than mutabilities, we sometimes have to pick a
    /// mutability that is stronger than necessary so that it at least *would permit* the borrow in
    /// question.
    pub fn to_mutbl_lossy(self) -> hir::Mutability {
        match self {
            MutBorrow => hir::MutMutable,
            ImmBorrow => hir::MutImmutable,

            // We have no type corresponding to a unique imm borrow, so
            // use `&mut`. It gives all the capabilities of an `&uniq`
            // and hence is a safe "over approximation".
            UniqueImmBorrow => hir::MutMutable,
        }
    }

    pub fn to_user_str(&self) -> &'static str {
        match *self {
            MutBorrow => "mutable",
            ImmBorrow => "immutable",
            UniqueImmBorrow => "uniquely immutable",
        }
    }
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    pub fn node_id_to_type(self, id: NodeId) -> Ty<'gcx> {
        match self.node_id_to_type_opt(id) {
           Some(ty) => ty,
           None => bug!("node_id_to_type: no type for node `{}`",
                        self.map.node_to_string(id))
        }
    }

    pub fn node_id_to_type_opt(self, id: NodeId) -> Option<Ty<'gcx>> {
        self.tables.borrow().node_types.get(&id).cloned()
    }

    pub fn node_id_item_substs(self, id: NodeId) -> ItemSubsts<'gcx> {
        match self.tables.borrow().item_substs.get(&id) {
            None => ItemSubsts {
                substs: self.global_tcx().mk_substs(Substs::empty())
            },
            Some(ts) => ts.clone(),
        }
    }

    // Returns the type of a pattern as a monotype. Like @expr_ty, this function
    // doesn't provide type parameter substitutions.
    pub fn pat_ty(self, pat: &hir::Pat) -> Ty<'gcx> {
        self.node_id_to_type(pat.id)
    }
    pub fn pat_ty_opt(self, pat: &hir::Pat) -> Option<Ty<'gcx>> {
        self.node_id_to_type_opt(pat.id)
    }

    // Returns the type of an expression as a monotype.
    //
    // NB (1): This is the PRE-ADJUSTMENT TYPE for the expression.  That is, in
    // some cases, we insert `AutoAdjustment` annotations such as auto-deref or
    // auto-ref.  The type returned by this function does not consider such
    // adjustments.  See `expr_ty_adjusted()` instead.
    //
    // NB (2): This type doesn't provide type parameter substitutions; e.g. if you
    // ask for the type of "id" in "id(3)", it will return "fn(&isize) -> isize"
    // instead of "fn(ty) -> T with T = isize".
    pub fn expr_ty(self, expr: &hir::Expr) -> Ty<'gcx> {
        self.node_id_to_type(expr.id)
    }

    pub fn expr_ty_opt(self, expr: &hir::Expr) -> Option<Ty<'gcx>> {
        self.node_id_to_type_opt(expr.id)
    }

    /// Returns the type of `expr`, considering any `AutoAdjustment`
    /// entry recorded for that expression.
    ///
    /// It would almost certainly be better to store the adjusted ty in with
    /// the `AutoAdjustment`, but I opted not to do this because it would
    /// require serializing and deserializing the type and, although that's not
    /// hard to do, I just hate that code so much I didn't want to touch it
    /// unless it was to fix it properly, which seemed a distraction from the
    /// thread at hand! -nmatsakis
    pub fn expr_ty_adjusted(self, expr: &hir::Expr) -> Ty<'gcx> {
        self.expr_ty(expr)
            .adjust(self.global_tcx(), expr.span, expr.id,
                    self.tables.borrow().adjustments.get(&expr.id),
                    |method_call| {
            self.tables.borrow().method_map.get(&method_call).map(|method| method.ty)
        })
    }

    pub fn expr_ty_adjusted_opt(self, expr: &hir::Expr) -> Option<Ty<'gcx>> {
        self.expr_ty_opt(expr).map(|t| t.adjust(self.global_tcx(),
                                                expr.span,
                                                expr.id,
                                                self.tables.borrow().adjustments.get(&expr.id),
                                                |method_call| {
            self.tables.borrow().method_map.get(&method_call).map(|method| method.ty)
        }))
    }

    pub fn expr_span(self, id: NodeId) -> Span {
        match self.map.find(id) {
            Some(ast_map::NodeExpr(e)) => {
                e.span
            }
            Some(f) => {
                bug!("Node id {} is not an expr: {:?}", id, f);
            }
            None => {
                bug!("Node id {} is not present in the node map", id);
            }
        }
    }

    pub fn local_var_name_str(self, id: NodeId) -> InternedString {
        match self.map.find(id) {
            Some(ast_map::NodeLocal(pat)) => {
                match pat.node {
                    PatKind::Binding(_, ref path1, _) => path1.node.as_str(),
                    _ => {
                        bug!("Variable id {} maps to {:?}, not local", id, pat);
                    },
                }
            },
            r => bug!("Variable id {} maps to {:?}, not local", id, r),
        }
    }

    pub fn expr_is_lval(self, expr: &hir::Expr) -> bool {
         match expr.node {
            hir::ExprPath(..) => {
                // This function can be used during type checking when not all paths are
                // fully resolved. Partially resolved paths in expressions can only legally
                // refer to associated items which are always rvalues.
                match self.expect_resolution(expr.id).base_def {
                    Def::Local(..) | Def::Upvar(..) | Def::Static(..) | Def::Err => true,
                    _ => false,
                }
            }

            hir::ExprType(ref e, _) => {
                self.expr_is_lval(e)
            }

            hir::ExprUnary(hir::UnDeref, _) |
            hir::ExprField(..) |
            hir::ExprTupField(..) |
            hir::ExprIndex(..) => {
                true
            }

            hir::ExprCall(..) |
            hir::ExprMethodCall(..) |
            hir::ExprStruct(..) |
            hir::ExprTup(..) |
            hir::ExprIf(..) |
            hir::ExprMatch(..) |
            hir::ExprClosure(..) |
            hir::ExprBlock(..) |
            hir::ExprRepeat(..) |
            hir::ExprVec(..) |
            hir::ExprBreak(..) |
            hir::ExprAgain(..) |
            hir::ExprRet(..) |
            hir::ExprWhile(..) |
            hir::ExprLoop(..) |
            hir::ExprAssign(..) |
            hir::ExprInlineAsm(..) |
            hir::ExprAssignOp(..) |
            hir::ExprLit(_) |
            hir::ExprUnary(..) |
            hir::ExprBox(..) |
            hir::ExprAddrOf(..) |
            hir::ExprBinary(..) |
            hir::ExprCast(..) => {
                false
            }
        }
    }

    pub fn provided_trait_methods(self, id: DefId) -> Vec<Rc<Method<'gcx>>> {
        if let Some(id) = self.map.as_local_node_id(id) {
            if let ItemTrait(_, _, _, ref ms) = self.map.expect_item(id).node {
                ms.iter().filter_map(|ti| {
                    if let hir::MethodTraitItem(_, Some(_)) = ti.node {
                        match self.impl_or_trait_item(self.map.local_def_id(ti.id)) {
                            MethodTraitItem(m) => Some(m),
                            _ => {
                                bug!("provided_trait_methods(): \
                                      non-method item found from \
                                      looking up provided method?!")
                            }
                        }
                    } else {
                        None
                    }
                }).collect()
            } else {
                bug!("provided_trait_methods: `{:?}` is not a trait", id)
            }
        } else {
            self.sess.cstore.provided_trait_methods(self.global_tcx(), id)
        }
    }

    pub fn associated_consts(self, id: DefId) -> Vec<Rc<AssociatedConst<'gcx>>> {
        if let Some(id) = self.map.as_local_node_id(id) {
            match self.map.expect_item(id).node {
                ItemTrait(_, _, _, ref tis) => {
                    tis.iter().filter_map(|ti| {
                        if let hir::ConstTraitItem(_, _) = ti.node {
                            match self.impl_or_trait_item(self.map.local_def_id(ti.id)) {
                                ConstTraitItem(ac) => Some(ac),
                                _ => {
                                    bug!("associated_consts(): \
                                          non-const item found from \
                                          looking up a constant?!")
                                }
                            }
                        } else {
                            None
                        }
                    }).collect()
                }
                ItemImpl(_, _, _, _, _, ref iis) => {
                    iis.iter().filter_map(|ii| {
                        if let hir::ImplItemKind::Const(_, _) = ii.node {
                            match self.impl_or_trait_item(self.map.local_def_id(ii.id)) {
                                ConstTraitItem(ac) => Some(ac),
                                _ => {
                                    bug!("associated_consts(): \
                                          non-const item found from \
                                          looking up a constant?!")
                                }
                            }
                        } else {
                            None
                        }
                    }).collect()
                }
                _ => {
                    bug!("associated_consts: `{:?}` is not a trait or impl", id)
                }
            }
        } else {
            self.sess.cstore.associated_consts(self.global_tcx(), id)
        }
    }

    pub fn trait_impl_polarity(self, id: DefId) -> Option<hir::ImplPolarity> {
        if let Some(id) = self.map.as_local_node_id(id) {
            match self.map.find(id) {
                Some(ast_map::NodeItem(item)) => {
                    match item.node {
                        hir::ItemImpl(_, polarity, _, _, _, _) => Some(polarity),
                        _ => None
                    }
                }
                _ => None
            }
        } else {
            self.sess.cstore.impl_polarity(id)
        }
    }

    pub fn custom_coerce_unsized_kind(self, did: DefId) -> adjustment::CustomCoerceUnsized {
        self.custom_coerce_unsized_kinds.memoize(did, || {
            let (kind, src) = if did.krate != LOCAL_CRATE {
                (self.sess.cstore.custom_coerce_unsized_kind(did), "external")
            } else {
                (None, "local")
            };

            match kind {
                Some(kind) => kind,
                None => {
                    bug!("custom_coerce_unsized_kind: \
                          {} impl `{}` is missing its kind",
                          src, self.item_path_str(did));
                }
            }
        })
    }

    pub fn impl_or_trait_item(self, id: DefId) -> ImplOrTraitItem<'gcx> {
        lookup_locally_or_in_crate_store(
            "impl_or_trait_items", id, &self.impl_or_trait_items,
            || self.sess.cstore.impl_or_trait_item(self.global_tcx(), id)
                   .expect("missing ImplOrTraitItem in metadata"))
    }

    pub fn trait_item_def_ids(self, id: DefId) -> Rc<Vec<ImplOrTraitItemId>> {
        lookup_locally_or_in_crate_store(
            "trait_item_def_ids", id, &self.trait_item_def_ids,
            || Rc::new(self.sess.cstore.trait_item_def_ids(id)))
    }

    /// Returns the trait-ref corresponding to a given impl, or None if it is
    /// an inherent impl.
    pub fn impl_trait_ref(self, id: DefId) -> Option<TraitRef<'gcx>> {
        lookup_locally_or_in_crate_store(
            "impl_trait_refs", id, &self.impl_trait_refs,
            || self.sess.cstore.impl_trait_ref(self.global_tcx(), id))
    }

    /// Returns whether this DefId refers to an impl
    pub fn is_impl(self, id: DefId) -> bool {
        if let Some(id) = self.map.as_local_node_id(id) {
            if let Some(ast_map::NodeItem(
                &hir::Item { node: hir::ItemImpl(..), .. })) = self.map.find(id) {
                true
            } else {
                false
            }
        } else {
            self.sess.cstore.is_impl(id)
        }
    }

    /// Returns a path resolution for node id if it exists, panics otherwise.
    pub fn expect_resolution(self, id: NodeId) -> PathResolution {
        *self.def_map.borrow().get(&id).expect("no def-map entry for node id")
    }

    /// Returns a fully resolved definition for node id if it exists, panics otherwise.
    pub fn expect_def(self, id: NodeId) -> Def {
        self.expect_resolution(id).full_def()
    }

    /// Returns a fully resolved definition for node id if it exists, or none if no
    /// definition exists, panics on partial resolutions to catch errors.
    pub fn expect_def_or_none(self, id: NodeId) -> Option<Def> {
        self.def_map.borrow().get(&id).map(|resolution| resolution.full_def())
    }

    pub fn def_key(self, id: DefId) -> ast_map::DefKey {
        if id.is_local() {
            self.map.def_key(id)
        } else {
            self.sess.cstore.def_key(id)
        }
    }

    /// Returns the `DefPath` of an item. Note that if `id` is not
    /// local to this crate -- or is inlined into this crate -- the
    /// result will be a non-local `DefPath`.
    pub fn def_path(self, id: DefId) -> ast_map::DefPath {
        if id.is_local() {
            self.map.def_path(id)
        } else {
            self.sess.cstore.relative_def_path(id)
        }
    }

    pub fn item_name(self, id: DefId) -> ast::Name {
        if let Some(id) = self.map.as_local_node_id(id) {
            self.map.name(id)
        } else {
            self.sess.cstore.item_name(id)
        }
    }

    // Register a given item type
    pub fn register_item_type(self, did: DefId, ty: TypeScheme<'gcx>) {
        self.tcache.borrow_mut().insert(did, ty);
    }

    // If the given item is in an external crate, looks up its type and adds it to
    // the type cache. Returns the type parameters and type.
    pub fn lookup_item_type(self, did: DefId) -> TypeScheme<'gcx> {
        lookup_locally_or_in_crate_store(
            "tcache", did, &self.tcache,
            || self.sess.cstore.item_type(self.global_tcx(), did))
    }

    pub fn opt_lookup_item_type(self, did: DefId) -> Option<TypeScheme<'gcx>> {
        if let Some(scheme) = self.tcache.borrow_mut().get(&did) {
            return Some(scheme.clone());
        }

        if did.krate == LOCAL_CRATE {
            None
        } else {
            Some(self.sess.cstore.item_type(self.global_tcx(), did))
        }
    }

    /// Given the did of a trait, returns its canonical trait ref.
    pub fn lookup_trait_def(self, did: DefId) -> &'gcx TraitDef<'gcx> {
        lookup_locally_or_in_crate_store(
            "trait_defs", did, &self.trait_defs,
            || self.alloc_trait_def(self.sess.cstore.trait_def(self.global_tcx(), did))
        )
    }

    /// Given the did of an ADT, return a master reference to its
    /// definition. Unless you are planning on fulfilling the ADT's fields,
    /// use lookup_adt_def instead.
    pub fn lookup_adt_def_master(self, did: DefId) -> AdtDefMaster<'gcx> {
        lookup_locally_or_in_crate_store(
            "adt_defs", did, &self.adt_defs,
            || self.sess.cstore.adt_def(self.global_tcx(), did)
        )
    }

    /// Given the did of an ADT, return a reference to its definition.
    pub fn lookup_adt_def(self, did: DefId) -> AdtDef<'gcx> {
        // when reverse-variance goes away, a transmute::<AdtDefMaster,AdtDef>
        // would be needed here.
        self.lookup_adt_def_master(did)
    }

    /// Given the did of an item, returns its full set of predicates.
    pub fn lookup_predicates(self, did: DefId) -> GenericPredicates<'gcx> {
        lookup_locally_or_in_crate_store(
            "predicates", did, &self.predicates,
            || self.sess.cstore.item_predicates(self.global_tcx(), did))
    }

    /// Given the did of a trait, returns its superpredicates.
    pub fn lookup_super_predicates(self, did: DefId) -> GenericPredicates<'gcx> {
        lookup_locally_or_in_crate_store(
            "super_predicates", did, &self.super_predicates,
            || self.sess.cstore.item_super_predicates(self.global_tcx(), did))
    }

    /// If `type_needs_drop` returns true, then `ty` is definitely
    /// non-copy and *might* have a destructor attached; if it returns
    /// false, then `ty` definitely has no destructor (i.e. no drop glue).
    ///
    /// (Note that this implies that if `ty` has a destructor attached,
    /// then `type_needs_drop` will definitely return `true` for `ty`.)
    pub fn type_needs_drop_given_env(self,
                                     ty: Ty<'gcx>,
                                     param_env: &ty::ParameterEnvironment<'gcx>) -> bool {
        // Issue #22536: We first query type_moves_by_default.  It sees a
        // normalized version of the type, and therefore will definitely
        // know whether the type implements Copy (and thus needs no
        // cleanup/drop/zeroing) ...
        let tcx = self.global_tcx();
        let implements_copy = !ty.moves_by_default(tcx, param_env, DUMMY_SP);

        if implements_copy { return false; }

        // ... (issue #22536 continued) but as an optimization, still use
        // prior logic of asking if the `needs_drop` bit is set; we need
        // not zero non-Copy types if they have no destructor.

        // FIXME(#22815): Note that calling `ty::type_contents` is a
        // conservative heuristic; it may report that `needs_drop` is set
        // when actual type does not actually have a destructor associated
        // with it. But since `ty` absolutely did not have the `Copy`
        // bound attached (see above), it is sound to treat it as having a
        // destructor (e.g. zero its memory on move).

        let contents = ty.type_contents(tcx);
        debug!("type_needs_drop ty={:?} contents={:?}", ty, contents);
        contents.needs_drop(tcx)
    }

    /// Get the attributes of a definition.
    pub fn get_attrs(self, did: DefId) -> Cow<'gcx, [ast::Attribute]> {
        if let Some(id) = self.map.as_local_node_id(did) {
            Cow::Borrowed(self.map.attrs(id))
        } else {
            Cow::Owned(self.sess.cstore.item_attrs(did))
        }
    }

    /// Determine whether an item is annotated with an attribute
    pub fn has_attr(self, did: DefId, attr: &str) -> bool {
        self.get_attrs(did).iter().any(|item| item.check_name(attr))
    }

    /// Determine whether an item is annotated with `#[repr(packed)]`
    pub fn lookup_packed(self, did: DefId) -> bool {
        self.lookup_repr_hints(did).contains(&attr::ReprPacked)
    }

    /// Determine whether an item is annotated with `#[simd]`
    pub fn lookup_simd(self, did: DefId) -> bool {
        self.has_attr(did, "simd")
            || self.lookup_repr_hints(did).contains(&attr::ReprSimd)
    }

    pub fn item_variances(self, item_id: DefId) -> Rc<ItemVariances> {
        lookup_locally_or_in_crate_store(
            "item_variance_map", item_id, &self.item_variance_map,
            || Rc::new(self.sess.cstore.item_variances(item_id)))
    }

    pub fn trait_has_default_impl(self, trait_def_id: DefId) -> bool {
        self.populate_implementations_for_trait_if_necessary(trait_def_id);

        let def = self.lookup_trait_def(trait_def_id);
        def.flags.get().intersects(TraitFlags::HAS_DEFAULT_IMPL)
    }

    /// Records a trait-to-implementation mapping.
    pub fn record_trait_has_default_impl(self, trait_def_id: DefId) {
        let def = self.lookup_trait_def(trait_def_id);
        def.flags.set(def.flags.get() | TraitFlags::HAS_DEFAULT_IMPL)
    }

    /// Load primitive inherent implementations if necessary
    pub fn populate_implementations_for_primitive_if_necessary(self,
                                                               primitive_def_id: DefId) {
        if primitive_def_id.is_local() {
            return
        }

        // The primitive is not local, hence we are reading this out
        // of metadata.
        let _ignore = self.dep_graph.in_ignore();

        if self.populated_external_primitive_impls.borrow().contains(&primitive_def_id) {
            return
        }

        debug!("populate_implementations_for_primitive_if_necessary: searching for {:?}",
               primitive_def_id);

        let impl_items = self.sess.cstore.impl_items(primitive_def_id);

        // Store the implementation info.
        self.impl_items.borrow_mut().insert(primitive_def_id, impl_items);
        self.populated_external_primitive_impls.borrow_mut().insert(primitive_def_id);
    }

    /// Populates the type context with all the inherent implementations for
    /// the given type if necessary.
    pub fn populate_inherent_implementations_for_type_if_necessary(self,
                                                                   type_id: DefId) {
        if type_id.is_local() {
            return
        }

        // The type is not local, hence we are reading this out of
        // metadata and don't need to track edges.
        let _ignore = self.dep_graph.in_ignore();

        if self.populated_external_types.borrow().contains(&type_id) {
            return
        }

        debug!("populate_inherent_implementations_for_type_if_necessary: searching for {:?}",
               type_id);

        let inherent_impls = self.sess.cstore.inherent_implementations_for_type(type_id);
        for &impl_def_id in &inherent_impls {
            // Store the implementation info.
            let impl_items = self.sess.cstore.impl_items(impl_def_id);
            self.impl_items.borrow_mut().insert(impl_def_id, impl_items);
        }

        self.inherent_impls.borrow_mut().insert(type_id, Rc::new(inherent_impls));
        self.populated_external_types.borrow_mut().insert(type_id);
    }

    /// Populates the type context with all the implementations for the given
    /// trait if necessary.
    pub fn populate_implementations_for_trait_if_necessary(self, trait_id: DefId) {
        if trait_id.is_local() {
            return
        }

        // The type is not local, hence we are reading this out of
        // metadata and don't need to track edges.
        let _ignore = self.dep_graph.in_ignore();

        let def = self.lookup_trait_def(trait_id);
        if def.flags.get().intersects(TraitFlags::IMPLS_VALID) {
            return;
        }

        debug!("populate_implementations_for_trait_if_necessary: searching for {:?}", def);

        if self.sess.cstore.is_defaulted_trait(trait_id) {
            self.record_trait_has_default_impl(trait_id);
        }

        for impl_def_id in self.sess.cstore.implementations_of_trait(trait_id) {
            let impl_items = self.sess.cstore.impl_items(impl_def_id);
            let trait_ref = self.impl_trait_ref(impl_def_id).unwrap();

            // Record the trait->implementation mapping.
            if let Some(parent) = self.sess.cstore.impl_parent(impl_def_id) {
                def.record_remote_impl(self, impl_def_id, trait_ref, parent);
            } else {
                def.record_remote_impl(self, impl_def_id, trait_ref, trait_id);
            }

            // For any methods that use a default implementation, add them to
            // the map. This is a bit unfortunate.
            for impl_item_def_id in &impl_items {
                let method_def_id = impl_item_def_id.def_id();
                // load impl items eagerly for convenience
                // FIXME: we may want to load these lazily
                self.impl_or_trait_item(method_def_id);
            }

            // Store the implementation info.
            self.impl_items.borrow_mut().insert(impl_def_id, impl_items);
        }

        def.flags.set(def.flags.get() | TraitFlags::IMPLS_VALID);
    }

    pub fn closure_kind(self, def_id: DefId) -> ty::ClosureKind {
        // If this is a local def-id, it should be inserted into the
        // tables by typeck; else, it will be retreived from
        // the external crate metadata.
        if let Some(&kind) = self.tables.borrow().closure_kinds.get(&def_id) {
            return kind;
        }

        let kind = self.sess.cstore.closure_kind(def_id);
        self.tables.borrow_mut().closure_kinds.insert(def_id, kind);
        kind
    }

    pub fn closure_type(self,
                        def_id: DefId,
                        substs: ClosureSubsts<'tcx>)
                        -> ty::ClosureTy<'tcx>
    {
        // If this is a local def-id, it should be inserted into the
        // tables by typeck; else, it will be retreived from
        // the external crate metadata.
        if let Some(ty) = self.tables.borrow().closure_tys.get(&def_id) {
            return ty.subst(self, substs.func_substs);
        }

        let ty = self.sess.cstore.closure_ty(self.global_tcx(), def_id);
        self.tables.borrow_mut().closure_tys.insert(def_id, ty.clone());
        ty.subst(self, substs.func_substs)
    }

    /// Given the def_id of an impl, return the def_id of the trait it implements.
    /// If it implements no trait, return `None`.
    pub fn trait_id_of_impl(self, def_id: DefId) -> Option<DefId> {
        self.impl_trait_ref(def_id).map(|tr| tr.def_id)
    }

    /// If the given def ID describes a method belonging to an impl, return the
    /// ID of the impl that the method belongs to. Otherwise, return `None`.
    pub fn impl_of_method(self, def_id: DefId) -> Option<DefId> {
        if def_id.krate != LOCAL_CRATE {
            return self.sess.cstore.impl_or_trait_item(self.global_tcx(), def_id)
                       .and_then(|item| {
                match item.container() {
                    TraitContainer(_) => None,
                    ImplContainer(def_id) => Some(def_id),
                }
            });
        }
        match self.impl_or_trait_items.borrow().get(&def_id).cloned() {
            Some(trait_item) => {
                match trait_item.container() {
                    TraitContainer(_) => None,
                    ImplContainer(def_id) => Some(def_id),
                }
            }
            None => None
        }
    }

    /// If the given def ID describes an item belonging to a trait (either a
    /// default method or an implementation of a trait method), return the ID of
    /// the trait that the method belongs to. Otherwise, return `None`.
    pub fn trait_of_item(self, def_id: DefId) -> Option<DefId> {
        if def_id.krate != LOCAL_CRATE {
            return self.sess.cstore.trait_of_item(self.global_tcx(), def_id);
        }
        match self.impl_or_trait_items.borrow().get(&def_id).cloned() {
            Some(impl_or_trait_item) => {
                match impl_or_trait_item.container() {
                    TraitContainer(def_id) => Some(def_id),
                    ImplContainer(def_id) => self.trait_id_of_impl(def_id),
                }
            }
            None => None
        }
    }

    /// If the given def ID describes an item belonging to a trait, (either a
    /// default method or an implementation of a trait method), return the ID of
    /// the method inside trait definition (this means that if the given def ID
    /// is already that of the original trait method, then the return value is
    /// the same).
    /// Otherwise, return `None`.
    pub fn trait_item_of_item(self, def_id: DefId) -> Option<ImplOrTraitItemId> {
        let impl_item = match self.impl_or_trait_items.borrow().get(&def_id) {
            Some(m) => m.clone(),
            None => return None,
        };
        let name = impl_item.name();
        match self.trait_of_item(def_id) {
            Some(trait_did) => {
                self.trait_items(trait_did).iter()
                    .find(|item| item.name() == name)
                    .map(|item| item.id())
            }
            None => None
        }
    }

    /// Construct a parameter environment suitable for static contexts or other contexts where there
    /// are no free type/lifetime parameters in scope.
    pub fn empty_parameter_environment(self) -> ParameterEnvironment<'tcx> {

        // for an empty parameter environment, there ARE no free
        // regions, so it shouldn't matter what we use for the free id
        let free_id_outlive = self.region_maps.node_extent(ast::DUMMY_NODE_ID);
        ty::ParameterEnvironment {
            free_substs: self.mk_substs(Substs::empty()),
            caller_bounds: Vec::new(),
            implicit_region_bound: ty::ReEmpty,
            free_id_outlive: free_id_outlive
        }
    }

    /// Constructs and returns a substitution that can be applied to move from
    /// the "outer" view of a type or method to the "inner" view.
    /// In general, this means converting from bound parameters to
    /// free parameters. Since we currently represent bound/free type
    /// parameters in the same way, this only has an effect on regions.
    pub fn construct_free_substs(self, generics: &Generics<'gcx>,
                                 free_id_outlive: CodeExtent) -> Substs<'gcx> {
        // map T => T
        let mut types = VecPerParamSpace::empty();
        for def in generics.types.as_slice() {
            debug!("construct_parameter_environment(): push_types_from_defs: def={:?}",
                    def);
            types.push(def.space, self.global_tcx().mk_param_from_def(def));
        }

        // map bound 'a => free 'a
        let mut regions = VecPerParamSpace::empty();
        for def in generics.regions.as_slice() {
            let region =
                ReFree(FreeRegion { scope: free_id_outlive,
                                    bound_region: def.to_bound_region() });
            debug!("push_region_params {:?}", region);
            regions.push(def.space, region);
        }

        Substs {
            types: types,
            regions: regions,
        }
    }

    /// See `ParameterEnvironment` struct def'n for details.
    /// If you were using `free_id: NodeId`, you might try `self.region_maps.item_extent(free_id)`
    /// for the `free_id_outlive` parameter. (But note that that is not always quite right.)
    pub fn construct_parameter_environment(self,
                                           span: Span,
                                           generics: &ty::Generics<'gcx>,
                                           generic_predicates: &ty::GenericPredicates<'gcx>,
                                           free_id_outlive: CodeExtent)
                                           -> ParameterEnvironment<'gcx>
    {
        //
        // Construct the free substs.
        //

        let free_substs = self.construct_free_substs(generics, free_id_outlive);

        //
        // Compute the bounds on Self and the type parameters.
        //

        let tcx = self.global_tcx();
        let bounds = generic_predicates.instantiate(tcx, &free_substs);
        let bounds = tcx.liberate_late_bound_regions(free_id_outlive, &ty::Binder(bounds));
        let predicates = bounds.predicates.into_vec();

        // Finally, we have to normalize the bounds in the environment, in
        // case they contain any associated type projections. This process
        // can yield errors if the put in illegal associated types, like
        // `<i32 as Foo>::Bar` where `i32` does not implement `Foo`. We
        // report these errors right here; this doesn't actually feel
        // right to me, because constructing the environment feels like a
        // kind of a "idempotent" action, but I'm not sure where would be
        // a better place. In practice, we construct environments for
        // every fn once during type checking, and we'll abort if there
        // are any errors at that point, so after type checking you can be
        // sure that this will succeed without errors anyway.
        //

        let unnormalized_env = ty::ParameterEnvironment {
            free_substs: tcx.mk_substs(free_substs),
            implicit_region_bound: ty::ReScope(free_id_outlive),
            caller_bounds: predicates,
            free_id_outlive: free_id_outlive,
        };

        let cause = traits::ObligationCause::misc(span, free_id_outlive.node_id(&self.region_maps));
        traits::normalize_param_env_or_error(tcx, unnormalized_env, cause)
    }

    pub fn is_method_call(self, expr_id: NodeId) -> bool {
        self.tables.borrow().method_map.contains_key(&MethodCall::expr(expr_id))
    }

    pub fn is_overloaded_autoderef(self, expr_id: NodeId, autoderefs: u32) -> bool {
        self.tables.borrow().method_map.contains_key(&MethodCall::autoderef(expr_id,
                                                                            autoderefs))
    }

    pub fn upvar_capture(self, upvar_id: ty::UpvarId) -> Option<ty::UpvarCapture> {
        Some(self.tables.borrow().upvar_capture_map.get(&upvar_id).unwrap().clone())
    }

    pub fn visit_all_items_in_krate<V,F>(self,
                                         dep_node_fn: F,
                                         visitor: &mut V)
        where F: FnMut(DefId) -> DepNode<DefId>, V: Visitor<'gcx>
    {
        dep_graph::visit_all_items_in_krate(self.global_tcx(), dep_node_fn, visitor);
    }

    /// Looks up the span of `impl_did` if the impl is local; otherwise returns `Err`
    /// with the name of the crate containing the impl.
    pub fn span_of_impl(self, impl_did: DefId) -> Result<Span, InternedString> {
        if impl_did.is_local() {
            let node_id = self.map.as_local_node_id(impl_did).unwrap();
            Ok(self.map.span(node_id))
        } else {
            Err(self.sess.cstore.crate_name(impl_did.krate))
        }
    }
}

/// The category of explicit self.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum ExplicitSelfCategory {
    Static,
    ByValue,
    ByReference(Region, hir::Mutability),
    ByBox,
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    pub fn with_freevars<T, F>(self, fid: NodeId, f: F) -> T where
        F: FnOnce(&[hir::Freevar]) -> T,
    {
        match self.freevars.borrow().get(&fid) {
            None => f(&[]),
            Some(d) => f(&d[..])
        }
    }
}
