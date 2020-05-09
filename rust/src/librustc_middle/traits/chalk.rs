//! Types required for Chalk-related queries
//!
//! The primary purpose of this file is defining an implementation for the
//! `chalk_ir::interner::Interner` trait. The primary purpose of this trait, as
//! its name suggest, is to provide an abstraction boundary for creating
//! interned Chalk types.

use chalk_ir::{GoalData, Parameter};

use rustc_middle::mir::Mutability;
use rustc_middle::ty::fold::{TypeFoldable, TypeFolder, TypeVisitor};
use rustc_middle::ty::{self, Ty, TyCtxt};

use rustc_hir::def_id::DefId;

use smallvec::SmallVec;

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// Since Chalk doesn't have full support for all Rust builtin types yet, we
/// need to use an enum here, rather than just `DefId`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RustDefId {
    Adt(DefId),
    Str,
    Never,
    Slice,
    Array,
    Ref(Mutability),
    RawPtr,

    Trait(DefId),

    Impl(DefId),

    FnDef(DefId),

    AssocTy(DefId),
}

#[derive(Copy, Clone)]
pub struct RustInterner<'tcx> {
    pub tcx: TyCtxt<'tcx>,
}

/// We don't ever actually need this. It's only required for derives.
impl<'tcx> Hash for RustInterner<'tcx> {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

/// We don't ever actually need this. It's only required for derives.
impl<'tcx> Ord for RustInterner<'tcx> {
    fn cmp(&self, _other: &Self) -> Ordering {
        Ordering::Equal
    }
}

/// We don't ever actually need this. It's only required for derives.
impl<'tcx> PartialOrd for RustInterner<'tcx> {
    fn partial_cmp(&self, _other: &Self) -> Option<Ordering> {
        None
    }
}

/// We don't ever actually need this. It's only required for derives.
impl<'tcx> PartialEq for RustInterner<'tcx> {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

/// We don't ever actually need this. It's only required for derives.
impl<'tcx> Eq for RustInterner<'tcx> {}

impl fmt::Debug for RustInterner<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RustInterner")
    }
}

// Right now, there is no interning at all. I was running into problems with
// adding interning in `ty/context.rs` for Chalk types with
// `parallel-compiler = true`. -jackh726
impl<'tcx> chalk_ir::interner::Interner for RustInterner<'tcx> {
    type InternedType = Box<chalk_ir::TyData<Self>>;
    type InternedLifetime = Box<chalk_ir::LifetimeData<Self>>;
    type InternedParameter = Box<chalk_ir::ParameterData<Self>>;
    type InternedGoal = Box<chalk_ir::GoalData<Self>>;
    type InternedGoals = Vec<chalk_ir::Goal<Self>>;
    type InternedSubstitution = Vec<chalk_ir::Parameter<Self>>;
    type InternedProgramClause = Box<chalk_ir::ProgramClauseData<Self>>;
    type InternedProgramClauses = Vec<chalk_ir::ProgramClause<Self>>;
    type InternedQuantifiedWhereClauses = Vec<chalk_ir::QuantifiedWhereClause<Self>>;
    type InternedParameterKinds = Vec<chalk_ir::ParameterKind<()>>;
    type InternedCanonicalVarKinds = Vec<chalk_ir::ParameterKind<chalk_ir::UniverseIndex>>;
    type DefId = RustDefId;
    type Identifier = ();

    fn debug_program_clause_implication(
        pci: &chalk_ir::ProgramClauseImplication<Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        let mut write = || {
            write!(fmt, "{:?}", pci.consequence)?;

            let conditions = pci.conditions.interned();

            let conds = conditions.len();
            if conds == 0 {
                return Ok(());
            }

            write!(fmt, " :- ")?;
            for cond in &conditions[..conds - 1] {
                write!(fmt, "{:?}, ", cond)?;
            }
            write!(fmt, "{:?}", conditions[conds - 1])?;
            Ok(())
        };
        Some(write())
    }

    fn debug_application_ty(
        application_ty: &chalk_ir::ApplicationTy<Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        let chalk_ir::ApplicationTy { name, substitution } = application_ty;
        Some(write!(fmt, "{:?}{:?}", name, chalk_ir::debug::Angle(substitution.interned())))
    }

    fn debug_substitution(
        substitution: &chalk_ir::Substitution<Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        Some(write!(fmt, "{:?}", substitution.interned()))
    }

    fn debug_separator_trait_ref(
        separator_trait_ref: &chalk_ir::SeparatorTraitRef<'_, Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        let substitution = &separator_trait_ref.trait_ref.substitution;
        let parameters = substitution.interned();
        Some(write!(
            fmt,
            "{:?}{}{:?}{:?}",
            parameters[0],
            separator_trait_ref.separator,
            separator_trait_ref.trait_ref.trait_id,
            chalk_ir::debug::Angle(&parameters[1..])
        ))
    }

    fn debug_quantified_where_clauses(
        clauses: &chalk_ir::QuantifiedWhereClauses<Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        Some(write!(fmt, "{:?}", clauses.interned()))
    }

    fn debug_alias(
        alias_ty: &chalk_ir::AliasTy<Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        match alias_ty {
            chalk_ir::AliasTy::Projection(projection_ty) => {
                Self::debug_projection_ty(projection_ty, fmt)
            }
            chalk_ir::AliasTy::Opaque(opaque_ty) => Self::debug_opaque_ty(opaque_ty, fmt),
        }
    }

    fn debug_projection_ty(
        projection_ty: &chalk_ir::ProjectionTy<Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        Some(write!(
            fmt,
            "projection: {:?} {:?}",
            projection_ty.associated_ty_id, projection_ty.substitution,
        ))
    }

    fn debug_opaque_ty(
        opaque_ty: &chalk_ir::OpaqueTy<Self>,
        fmt: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        Some(write!(fmt, "{:?}", opaque_ty.opaque_ty_id))
    }

    fn intern_ty(&self, ty: chalk_ir::TyData<Self>) -> Self::InternedType {
        Box::new(ty)
    }

    fn ty_data<'a>(&self, ty: &'a Self::InternedType) -> &'a chalk_ir::TyData<Self> {
        ty
    }

    fn intern_lifetime(&self, lifetime: chalk_ir::LifetimeData<Self>) -> Self::InternedLifetime {
        Box::new(lifetime)
    }

    fn lifetime_data<'a>(
        &self,
        lifetime: &'a Self::InternedLifetime,
    ) -> &'a chalk_ir::LifetimeData<Self> {
        &lifetime
    }

    fn intern_parameter(
        &self,
        parameter: chalk_ir::ParameterData<Self>,
    ) -> Self::InternedParameter {
        Box::new(parameter)
    }

    fn parameter_data<'a>(
        &self,
        parameter: &'a Self::InternedParameter,
    ) -> &'a chalk_ir::ParameterData<Self> {
        &parameter
    }

    fn intern_goal(&self, goal: GoalData<Self>) -> Self::InternedGoal {
        Box::new(goal)
    }

    fn goal_data<'a>(&self, goal: &'a Self::InternedGoal) -> &'a GoalData<Self> {
        &goal
    }

    fn intern_goals<E>(
        &self,
        data: impl IntoIterator<Item = Result<chalk_ir::Goal<Self>, E>>,
    ) -> Result<Self::InternedGoals, E> {
        data.into_iter().collect::<Result<Vec<_>, _>>()
    }

    fn goals_data<'a>(&self, goals: &'a Self::InternedGoals) -> &'a [chalk_ir::Goal<Self>] {
        goals
    }

    fn intern_substitution<E>(
        &self,
        data: impl IntoIterator<Item = Result<chalk_ir::Parameter<Self>, E>>,
    ) -> Result<Self::InternedSubstitution, E> {
        data.into_iter().collect::<Result<Vec<_>, _>>()
    }

    fn substitution_data<'a>(
        &self,
        substitution: &'a Self::InternedSubstitution,
    ) -> &'a [Parameter<Self>] {
        substitution
    }

    fn intern_program_clause(
        &self,
        data: chalk_ir::ProgramClauseData<Self>,
    ) -> Self::InternedProgramClause {
        Box::new(data)
    }

    fn program_clause_data<'a>(
        &self,
        clause: &'a Self::InternedProgramClause,
    ) -> &'a chalk_ir::ProgramClauseData<Self> {
        &clause
    }

    fn intern_program_clauses<E>(
        &self,
        data: impl IntoIterator<Item = Result<chalk_ir::ProgramClause<Self>, E>>,
    ) -> Result<Self::InternedProgramClauses, E> {
        data.into_iter().collect::<Result<Vec<_>, _>>()
    }

    fn program_clauses_data<'a>(
        &self,
        clauses: &'a Self::InternedProgramClauses,
    ) -> &'a [chalk_ir::ProgramClause<Self>] {
        clauses
    }

    fn intern_quantified_where_clauses<E>(
        &self,
        data: impl IntoIterator<Item = Result<chalk_ir::QuantifiedWhereClause<Self>, E>>,
    ) -> Result<Self::InternedQuantifiedWhereClauses, E> {
        data.into_iter().collect::<Result<Vec<_>, _>>()
    }

    fn quantified_where_clauses_data<'a>(
        &self,
        clauses: &'a Self::InternedQuantifiedWhereClauses,
    ) -> &'a [chalk_ir::QuantifiedWhereClause<Self>] {
        clauses
    }

    fn intern_parameter_kinds<E>(
        &self,
        data: impl IntoIterator<Item = Result<chalk_ir::ParameterKind<()>, E>>,
    ) -> Result<Self::InternedParameterKinds, E> {
        data.into_iter().collect::<Result<Vec<_>, _>>()
    }

    fn parameter_kinds_data<'a>(
        &self,
        parameter_kinds: &'a Self::InternedParameterKinds,
    ) -> &'a [chalk_ir::ParameterKind<()>] {
        parameter_kinds
    }

    fn intern_canonical_var_kinds<E>(
        &self,
        data: impl IntoIterator<Item = Result<chalk_ir::ParameterKind<chalk_ir::UniverseIndex>, E>>,
    ) -> Result<Self::InternedCanonicalVarKinds, E> {
        data.into_iter().collect::<Result<Vec<_>, _>>()
    }

    fn canonical_var_kinds_data<'a>(
        &self,
        canonical_var_kinds: &'a Self::InternedCanonicalVarKinds,
    ) -> &'a [chalk_ir::ParameterKind<chalk_ir::UniverseIndex>] {
        canonical_var_kinds
    }
}

impl<'tcx> chalk_ir::interner::HasInterner for RustInterner<'tcx> {
    type Interner = Self;
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, HashStable, TypeFoldable)]
pub enum ChalkEnvironmentClause<'tcx> {
    /// A normal rust `ty::Predicate` in the environment.
    Predicate(ty::Predicate<'tcx>),
    /// A special clause in the environment that gets lowered to
    /// `chalk_ir::FromEnv::Ty`.
    TypeFromEnv(Ty<'tcx>),
}

impl<'tcx> TypeFoldable<'tcx> for &'tcx ty::List<ChalkEnvironmentClause<'tcx>> {
    fn super_fold_with<F: TypeFolder<'tcx>>(&self, folder: &mut F) -> Self {
        let v = self.iter().map(|t| t.fold_with(folder)).collect::<SmallVec<[_; 8]>>();
        folder.tcx().intern_chalk_environment_clause_list(&v)
    }

    fn super_visit_with<V: TypeVisitor<'tcx>>(&self, visitor: &mut V) -> bool {
        self.iter().any(|t| t.visit_with(visitor))
    }
}
/// We have to elaborate the environment of a chalk goal *before*
/// canonicalization. This type wraps the predicate and the elaborated
/// environment.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, HashStable, TypeFoldable)]
pub struct ChalkEnvironmentAndGoal<'tcx> {
    pub environment: &'tcx ty::List<ChalkEnvironmentClause<'tcx>>,
    pub goal: ty::Predicate<'tcx>,
}

impl<'tcx> fmt::Display for ChalkEnvironmentAndGoal<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "environment: {:?}, goal: {}", self.environment, self.goal)
    }
}
