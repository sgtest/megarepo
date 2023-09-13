//! A constant propagation optimization pass based on dataflow analysis.
//!
//! Currently, this pass only propagates scalar values.

use rustc_const_eval::const_eval::CheckAlignment;
use rustc_const_eval::interpret::{ImmTy, Immediate, InterpCx, OpTy, Projectable};
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::def::DefKind;
use rustc_middle::mir::interpret::{AllocId, ConstAllocation, ConstValue, InterpResult, Scalar};
use rustc_middle::mir::visit::{MutVisitor, PlaceContext, Visitor};
use rustc_middle::mir::*;
use rustc_middle::ty::layout::TyAndLayout;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_mir_dataflow::value_analysis::{
    Map, PlaceIndex, State, TrackElem, ValueAnalysis, ValueAnalysisWrapper, ValueOrPlace,
};
use rustc_mir_dataflow::{lattice::FlatSet, Analysis, Results, ResultsVisitor};
use rustc_span::def_id::DefId;
use rustc_span::DUMMY_SP;
use rustc_target::abi::{Align, FieldIdx, VariantIdx};

use crate::MirPass;

// These constants are somewhat random guesses and have not been optimized.
// If `tcx.sess.mir_opt_level() >= 4`, we ignore the limits (this can become very expensive).
const BLOCK_LIMIT: usize = 100;
const PLACE_LIMIT: usize = 100;

pub struct DataflowConstProp;

impl<'tcx> MirPass<'tcx> for DataflowConstProp {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.mir_opt_level() >= 3
    }

    #[instrument(skip_all level = "debug")]
    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        debug!(def_id = ?body.source.def_id());
        if tcx.sess.mir_opt_level() < 4 && body.basic_blocks.len() > BLOCK_LIMIT {
            debug!("aborted dataflow const prop due too many basic blocks");
            return;
        }

        // We want to have a somewhat linear runtime w.r.t. the number of statements/terminators.
        // Let's call this number `n`. Dataflow analysis has `O(h*n)` transfer function
        // applications, where `h` is the height of the lattice. Because the height of our lattice
        // is linear w.r.t. the number of tracked places, this is `O(tracked_places * n)`. However,
        // because every transfer function application could traverse the whole map, this becomes
        // `O(num_nodes * tracked_places * n)` in terms of time complexity. Since the number of
        // map nodes is strongly correlated to the number of tracked places, this becomes more or
        // less `O(n)` if we place a constant limit on the number of tracked places.
        let place_limit = if tcx.sess.mir_opt_level() < 4 { Some(PLACE_LIMIT) } else { None };

        // Decide which places to track during the analysis.
        let map = Map::new(tcx, body, place_limit);

        // Perform the actual dataflow analysis.
        let analysis = ConstAnalysis::new(tcx, body, map);
        let mut results = debug_span!("analyze")
            .in_scope(|| analysis.wrap().into_engine(tcx, body).iterate_to_fixpoint());

        // Collect results and patch the body afterwards.
        let mut visitor = Collector::new(tcx, &body.local_decls);
        debug_span!("collect").in_scope(|| results.visit_reachable_with(body, &mut visitor));
        let mut patch = visitor.patch;
        debug_span!("patch").in_scope(|| patch.visit_body_preserves_cfg(body));
    }
}

struct ConstAnalysis<'a, 'tcx> {
    map: Map,
    tcx: TyCtxt<'tcx>,
    local_decls: &'a LocalDecls<'tcx>,
    ecx: InterpCx<'tcx, 'tcx, DummyMachine>,
    param_env: ty::ParamEnv<'tcx>,
}

impl<'tcx> ValueAnalysis<'tcx> for ConstAnalysis<'_, 'tcx> {
    type Value = FlatSet<Scalar>;

    const NAME: &'static str = "ConstAnalysis";

    fn map(&self) -> &Map {
        &self.map
    }

    fn handle_set_discriminant(
        &self,
        place: Place<'tcx>,
        variant_index: VariantIdx,
        state: &mut State<Self::Value>,
    ) {
        state.flood_discr(place.as_ref(), &self.map);
        if self.map.find_discr(place.as_ref()).is_some() {
            let enum_ty = place.ty(self.local_decls, self.tcx).ty;
            if let Some(discr) = self.eval_discriminant(enum_ty, variant_index) {
                state.assign_discr(
                    place.as_ref(),
                    ValueOrPlace::Value(FlatSet::Elem(discr)),
                    &self.map,
                );
            }
        }
    }

    fn handle_assign(
        &self,
        target: Place<'tcx>,
        rvalue: &Rvalue<'tcx>,
        state: &mut State<Self::Value>,
    ) {
        match rvalue {
            Rvalue::Use(operand) => {
                state.flood(target.as_ref(), self.map());
                if let Some(target) = self.map.find(target.as_ref()) {
                    self.assign_operand(state, target, operand);
                }
            }
            Rvalue::CopyForDeref(rhs) => {
                state.flood(target.as_ref(), self.map());
                if let Some(target) = self.map.find(target.as_ref()) {
                    self.assign_operand(state, target, &Operand::Copy(*rhs));
                }
            }
            Rvalue::Aggregate(kind, operands) => {
                // If we assign `target = Enum::Variant#0(operand)`,
                // we must make sure that all `target as Variant#i` are `Top`.
                state.flood(target.as_ref(), self.map());

                let Some(target_idx) = self.map().find(target.as_ref()) else { return };

                let (variant_target, variant_index) = match **kind {
                    AggregateKind::Tuple | AggregateKind::Closure(..) => (Some(target_idx), None),
                    AggregateKind::Adt(def_id, variant_index, ..) => {
                        match self.tcx.def_kind(def_id) {
                            DefKind::Struct => (Some(target_idx), None),
                            DefKind::Enum => (
                                self.map.apply(target_idx, TrackElem::Variant(variant_index)),
                                Some(variant_index),
                            ),
                            _ => return,
                        }
                    }
                    _ => return,
                };
                if let Some(variant_target_idx) = variant_target {
                    for (field_index, operand) in operands.iter().enumerate() {
                        if let Some(field) = self.map().apply(
                            variant_target_idx,
                            TrackElem::Field(FieldIdx::from_usize(field_index)),
                        ) {
                            self.assign_operand(state, field, operand);
                        }
                    }
                }
                if let Some(variant_index) = variant_index
                    && let Some(discr_idx) = self.map().apply(target_idx, TrackElem::Discriminant)
                {
                    // We are assigning the discriminant as part of an aggregate.
                    // This discriminant can only alias a variant field's value if the operand
                    // had an invalid value for that type.
                    // Using invalid values is UB, so we are allowed to perform the assignment
                    // without extra flooding.
                    let enum_ty = target.ty(self.local_decls, self.tcx).ty;
                    if let Some(discr_val) = self.eval_discriminant(enum_ty, variant_index) {
                        state.insert_value_idx(discr_idx, FlatSet::Elem(discr_val), &self.map);
                    }
                }
            }
            Rvalue::CheckedBinaryOp(op, box (left, right)) => {
                // Flood everything now, so we can use `insert_value_idx` directly later.
                state.flood(target.as_ref(), self.map());

                let Some(target) = self.map().find(target.as_ref()) else { return };

                let value_target = self.map().apply(target, TrackElem::Field(0_u32.into()));
                let overflow_target = self.map().apply(target, TrackElem::Field(1_u32.into()));

                if value_target.is_some() || overflow_target.is_some() {
                    let (val, overflow) = self.binary_op(state, *op, left, right);

                    if let Some(value_target) = value_target {
                        // We have flooded `target` earlier.
                        state.insert_value_idx(value_target, val, self.map());
                    }
                    if let Some(overflow_target) = overflow_target {
                        let overflow = match overflow {
                            FlatSet::Top => FlatSet::Top,
                            FlatSet::Elem(overflow) => FlatSet::Elem(Scalar::from_bool(overflow)),
                            FlatSet::Bottom => FlatSet::Bottom,
                        };
                        // We have flooded `target` earlier.
                        state.insert_value_idx(overflow_target, overflow, self.map());
                    }
                }
            }
            Rvalue::Cast(
                CastKind::PointerCoercion(ty::adjustment::PointerCoercion::Unsize),
                operand,
                _,
            ) => {
                let pointer = self.handle_operand(operand, state);
                state.assign(target.as_ref(), pointer, self.map());

                if let Some(target_len) = self.map().find_len(target.as_ref())
                    && let operand_ty = operand.ty(self.local_decls, self.tcx)
                    && let Some(operand_ty) = operand_ty.builtin_deref(true)
                    && let ty::Array(_, len) = operand_ty.ty.kind()
                    && let Some(len) = ConstantKind::Ty(*len).try_eval_scalar_int(self.tcx, self.param_env)
                {
                    state.insert_value_idx(target_len, FlatSet::Elem(len.into()), self.map());
                }
            }
            _ => self.super_assign(target, rvalue, state),
        }
    }

    fn handle_rvalue(
        &self,
        rvalue: &Rvalue<'tcx>,
        state: &mut State<Self::Value>,
    ) -> ValueOrPlace<Self::Value> {
        let val = match rvalue {
            Rvalue::Len(place) => {
                let place_ty = place.ty(self.local_decls, self.tcx);
                if let ty::Array(_, len) = place_ty.ty.kind() {
                    ConstantKind::Ty(*len)
                        .try_eval_scalar(self.tcx, self.param_env)
                        .map_or(FlatSet::Top, FlatSet::Elem)
                } else if let [ProjectionElem::Deref] = place.projection[..] {
                    state.get_len(place.local.into(), self.map())
                } else {
                    FlatSet::Top
                }
            }
            Rvalue::Cast(CastKind::IntToInt | CastKind::IntToFloat, operand, ty) => {
                match self.eval_operand(operand, state) {
                    FlatSet::Elem(op) => self
                        .ecx
                        .int_to_int_or_float(&op, *ty)
                        .map_or(FlatSet::Top, |result| self.wrap_immediate(result)),
                    FlatSet::Bottom => FlatSet::Bottom,
                    FlatSet::Top => FlatSet::Top,
                }
            }
            Rvalue::Cast(CastKind::FloatToInt | CastKind::FloatToFloat, operand, ty) => {
                match self.eval_operand(operand, state) {
                    FlatSet::Elem(op) => self
                        .ecx
                        .float_to_float_or_int(&op, *ty)
                        .map_or(FlatSet::Top, |result| self.wrap_immediate(result)),
                    FlatSet::Bottom => FlatSet::Bottom,
                    FlatSet::Top => FlatSet::Top,
                }
            }
            Rvalue::Cast(CastKind::Transmute, operand, _) => {
                match self.eval_operand(operand, state) {
                    FlatSet::Elem(op) => self.wrap_immediate(*op),
                    FlatSet::Bottom => FlatSet::Bottom,
                    FlatSet::Top => FlatSet::Top,
                }
            }
            Rvalue::BinaryOp(op, box (left, right)) => {
                // Overflows must be ignored here.
                let (val, _overflow) = self.binary_op(state, *op, left, right);
                val
            }
            Rvalue::UnaryOp(op, operand) => match self.eval_operand(operand, state) {
                FlatSet::Elem(value) => self
                    .ecx
                    .unary_op(*op, &value)
                    .map_or(FlatSet::Top, |val| self.wrap_immediate(*val)),
                FlatSet::Bottom => FlatSet::Bottom,
                FlatSet::Top => FlatSet::Top,
            },
            Rvalue::NullaryOp(null_op, ty) => {
                let Ok(layout) = self.tcx.layout_of(self.param_env.and(*ty)) else {
                    return ValueOrPlace::Value(FlatSet::Top);
                };
                let val = match null_op {
                    NullOp::SizeOf if layout.is_sized() => layout.size.bytes(),
                    NullOp::AlignOf if layout.is_sized() => layout.align.abi.bytes(),
                    NullOp::OffsetOf(fields) => layout
                        .offset_of_subfield(&self.ecx, fields.iter().map(|f| f.index()))
                        .bytes(),
                    _ => return ValueOrPlace::Value(FlatSet::Top),
                };
                FlatSet::Elem(Scalar::from_target_usize(val, &self.tcx))
            }
            Rvalue::Discriminant(place) => state.get_discr(place.as_ref(), self.map()),
            _ => return self.super_rvalue(rvalue, state),
        };
        ValueOrPlace::Value(val)
    }

    fn handle_constant(
        &self,
        constant: &Constant<'tcx>,
        _state: &mut State<Self::Value>,
    ) -> Self::Value {
        constant
            .literal
            .try_eval_scalar(self.tcx, self.param_env)
            .map_or(FlatSet::Top, FlatSet::Elem)
    }

    fn handle_switch_int<'mir>(
        &self,
        discr: &'mir Operand<'tcx>,
        targets: &'mir SwitchTargets,
        state: &mut State<Self::Value>,
    ) -> TerminatorEdges<'mir, 'tcx> {
        let value = match self.handle_operand(discr, state) {
            ValueOrPlace::Value(value) => value,
            ValueOrPlace::Place(place) => state.get_idx(place, self.map()),
        };
        match value {
            // We are branching on uninitialized data, this is UB, treat it as unreachable.
            // This allows the set of visited edges to grow monotonically with the lattice.
            FlatSet::Bottom => TerminatorEdges::None,
            FlatSet::Elem(scalar) => {
                let choice = scalar.assert_bits(scalar.size());
                TerminatorEdges::Single(targets.target_for_value(choice))
            }
            FlatSet::Top => TerminatorEdges::SwitchInt { discr, targets },
        }
    }
}

impl<'a, 'tcx> ConstAnalysis<'a, 'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>, body: &'a Body<'tcx>, map: Map) -> Self {
        let param_env = tcx.param_env_reveal_all_normalized(body.source.def_id());
        Self {
            map,
            tcx,
            local_decls: &body.local_decls,
            ecx: InterpCx::new(tcx, DUMMY_SP, param_env, DummyMachine),
            param_env: param_env,
        }
    }

    /// The caller must have flooded `place`.
    fn assign_operand(
        &self,
        state: &mut State<FlatSet<Scalar>>,
        place: PlaceIndex,
        operand: &Operand<'tcx>,
    ) {
        match operand {
            Operand::Copy(rhs) | Operand::Move(rhs) => {
                if let Some(rhs) = self.map.find(rhs.as_ref()) {
                    state.insert_place_idx(place, rhs, &self.map);
                } else if rhs.projection.first() == Some(&PlaceElem::Deref)
                    && let FlatSet::Elem(pointer) = state.get(rhs.local.into(), &self.map)
                    && let rhs_ty = self.local_decls[rhs.local].ty
                    && let Ok(rhs_layout) = self.tcx.layout_of(self.param_env.and(rhs_ty))
                {
                    let op = ImmTy::from_scalar(pointer, rhs_layout).into();
                    self.assign_constant(state, place, op, &rhs.projection);
                }
            }
            Operand::Constant(box constant) => {
                if let Ok(constant) = self.ecx.eval_mir_constant(&constant.literal, None, None) {
                    self.assign_constant(state, place, constant, &[]);
                }
            }
        }
    }

    /// The caller must have flooded `place`.
    ///
    /// Perform: `place = operand.projection`.
    #[instrument(level = "trace", skip(self, state))]
    fn assign_constant(
        &self,
        state: &mut State<FlatSet<Scalar>>,
        place: PlaceIndex,
        mut operand: OpTy<'tcx>,
        projection: &[PlaceElem<'tcx>],
    ) -> Option<!> {
        for &(mut proj_elem) in projection {
            if let PlaceElem::Index(index) = proj_elem {
                if let FlatSet::Elem(index) = state.get(index.into(), &self.map)
                    && let Ok(offset) = index.to_target_usize(&self.tcx)
                    && let Some(min_length) = offset.checked_add(1)
                {
                    proj_elem = PlaceElem::ConstantIndex { offset, min_length, from_end: false };
                } else {
                    return None;
                }
            }
            operand = self.ecx.project(&operand, proj_elem).ok()?;
        }

        self.map.for_each_projection_value(
            place,
            operand,
            &mut |elem, op| match elem {
                TrackElem::Field(idx) => self.ecx.project_field(op, idx.as_usize()).ok(),
                TrackElem::Variant(idx) => self.ecx.project_downcast(op, idx).ok(),
                TrackElem::Discriminant => {
                    let variant = self.ecx.read_discriminant(op).ok()?;
                    let discr_value = self.ecx.discriminant_for_variant(op.layout, variant).ok()?;
                    Some(discr_value.into())
                }
                TrackElem::DerefLen => {
                    let op: OpTy<'_> = self.ecx.deref_pointer(op).ok()?.into();
                    let len_usize = op.len(&self.ecx).ok()?;
                    let layout =
                        self.tcx.layout_of(self.param_env.and(self.tcx.types.usize)).unwrap();
                    Some(ImmTy::from_uint(len_usize, layout).into())
                }
            },
            &mut |place, op| {
                if let Ok(imm) = self.ecx.read_immediate_raw(op)
                    && let Some(imm) = imm.right()
                {
                    let elem = self.wrap_immediate(*imm);
                    state.insert_value_idx(place, elem, &self.map);
                }
            },
        );

        None
    }

    fn binary_op(
        &self,
        state: &mut State<FlatSet<Scalar>>,
        op: BinOp,
        left: &Operand<'tcx>,
        right: &Operand<'tcx>,
    ) -> (FlatSet<Scalar>, FlatSet<bool>) {
        let left = self.eval_operand(left, state);
        let right = self.eval_operand(right, state);

        match (left, right) {
            (FlatSet::Bottom, _) | (_, FlatSet::Bottom) => (FlatSet::Bottom, FlatSet::Bottom),
            // Both sides are known, do the actual computation.
            (FlatSet::Elem(left), FlatSet::Elem(right)) => {
                match self.ecx.overflowing_binary_op(op, &left, &right) {
                    Ok((val, overflow, _)) => (FlatSet::Elem(val), FlatSet::Elem(overflow)),
                    _ => (FlatSet::Top, FlatSet::Top),
                }
            }
            // Exactly one side is known, attempt some algebraic simplifications.
            (FlatSet::Elem(const_arg), _) | (_, FlatSet::Elem(const_arg)) => {
                let layout = const_arg.layout;
                if !matches!(layout.abi, rustc_target::abi::Abi::Scalar(..)) {
                    return (FlatSet::Top, FlatSet::Top);
                }

                let arg_scalar = const_arg.to_scalar();
                let Ok(arg_value) = arg_scalar.to_bits(layout.size) else {
                    return (FlatSet::Top, FlatSet::Top);
                };

                match op {
                    BinOp::BitAnd if arg_value == 0 => (FlatSet::Elem(arg_scalar), FlatSet::Bottom),
                    BinOp::BitOr
                        if arg_value == layout.size.truncate(u128::MAX)
                            || (layout.ty.is_bool() && arg_value == 1) =>
                    {
                        (FlatSet::Elem(arg_scalar), FlatSet::Bottom)
                    }
                    BinOp::Mul if layout.ty.is_integral() && arg_value == 0 => {
                        (FlatSet::Elem(arg_scalar), FlatSet::Elem(false))
                    }
                    _ => (FlatSet::Top, FlatSet::Top),
                }
            }
            (FlatSet::Top, FlatSet::Top) => (FlatSet::Top, FlatSet::Top),
        }
    }

    fn eval_operand(
        &self,
        op: &Operand<'tcx>,
        state: &mut State<FlatSet<Scalar>>,
    ) -> FlatSet<ImmTy<'tcx>> {
        let value = match self.handle_operand(op, state) {
            ValueOrPlace::Value(value) => value,
            ValueOrPlace::Place(place) => state.get_idx(place, &self.map),
        };
        match value {
            FlatSet::Top => FlatSet::Top,
            FlatSet::Elem(scalar) => {
                let ty = op.ty(self.local_decls, self.tcx);
                self.tcx.layout_of(self.param_env.and(ty)).map_or(FlatSet::Top, |layout| {
                    FlatSet::Elem(ImmTy::from_scalar(scalar.into(), layout))
                })
            }
            FlatSet::Bottom => FlatSet::Bottom,
        }
    }

    fn eval_discriminant(&self, enum_ty: Ty<'tcx>, variant_index: VariantIdx) -> Option<Scalar> {
        if !enum_ty.is_enum() {
            return None;
        }
        let enum_ty_layout = self.tcx.layout_of(self.param_env.and(enum_ty)).ok()?;
        let discr_value = self.ecx.discriminant_for_variant(enum_ty_layout, variant_index).ok()?;
        Some(discr_value.to_scalar())
    }

    fn wrap_immediate(&self, imm: Immediate) -> FlatSet<Scalar> {
        match imm {
            Immediate::Scalar(scalar) => FlatSet::Elem(scalar),
            Immediate::Uninit => FlatSet::Bottom,
            _ => FlatSet::Top,
        }
    }
}

pub(crate) struct Patch<'tcx> {
    tcx: TyCtxt<'tcx>,

    /// For a given MIR location, this stores the values of the operands used by that location. In
    /// particular, this is before the effect, such that the operands of `_1 = _1 + _2` are
    /// properly captured. (This may become UB soon, but it is currently emitted even by safe code.)
    pub(crate) before_effect: FxHashMap<(Location, Place<'tcx>), ConstantKind<'tcx>>,

    /// Stores the assigned values for assignments where the Rvalue is constant.
    pub(crate) assignments: FxHashMap<Location, ConstantKind<'tcx>>,
}

impl<'tcx> Patch<'tcx> {
    pub(crate) fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self { tcx, before_effect: FxHashMap::default(), assignments: FxHashMap::default() }
    }

    fn make_operand(&self, literal: ConstantKind<'tcx>) -> Operand<'tcx> {
        Operand::Constant(Box::new(Constant { span: DUMMY_SP, user_ty: None, literal }))
    }
}

struct Collector<'tcx, 'locals> {
    patch: Patch<'tcx>,
    local_decls: &'locals LocalDecls<'tcx>,
}

impl<'tcx, 'locals> Collector<'tcx, 'locals> {
    pub(crate) fn new(tcx: TyCtxt<'tcx>, local_decls: &'locals LocalDecls<'tcx>) -> Self {
        Self { patch: Patch::new(tcx), local_decls }
    }

    fn try_make_constant(
        &self,
        place: Place<'tcx>,
        state: &State<FlatSet<Scalar>>,
        map: &Map,
    ) -> Option<ConstantKind<'tcx>> {
        let FlatSet::Elem(Scalar::Int(value)) = state.get(place.as_ref(), &map) else {
            return None;
        };
        let ty = place.ty(self.local_decls, self.patch.tcx).ty;
        Some(ConstantKind::Val(ConstValue::Scalar(value.into()), ty))
    }
}

impl<'mir, 'tcx>
    ResultsVisitor<'mir, 'tcx, Results<'tcx, ValueAnalysisWrapper<ConstAnalysis<'_, 'tcx>>>>
    for Collector<'tcx, '_>
{
    type FlowState = State<FlatSet<Scalar>>;

    fn visit_statement_before_primary_effect(
        &mut self,
        results: &mut Results<'tcx, ValueAnalysisWrapper<ConstAnalysis<'_, 'tcx>>>,
        state: &Self::FlowState,
        statement: &'mir Statement<'tcx>,
        location: Location,
    ) {
        match &statement.kind {
            StatementKind::Assign(box (_, rvalue)) => {
                OperandCollector { state, visitor: self, map: &results.analysis.0.map }
                    .visit_rvalue(rvalue, location);
            }
            _ => (),
        }
    }

    fn visit_statement_after_primary_effect(
        &mut self,
        results: &mut Results<'tcx, ValueAnalysisWrapper<ConstAnalysis<'_, 'tcx>>>,
        state: &Self::FlowState,
        statement: &'mir Statement<'tcx>,
        location: Location,
    ) {
        match statement.kind {
            StatementKind::Assign(box (_, Rvalue::Use(Operand::Constant(_)))) => {
                // Don't overwrite the assignment if it already uses a constant (to keep the span).
            }
            StatementKind::Assign(box (place, _)) => {
                if let Some(value) = self.try_make_constant(place, state, &results.analysis.0.map) {
                    self.patch.assignments.insert(location, value);
                }
            }
            _ => (),
        }
    }

    fn visit_terminator_before_primary_effect(
        &mut self,
        results: &mut Results<'tcx, ValueAnalysisWrapper<ConstAnalysis<'_, 'tcx>>>,
        state: &Self::FlowState,
        terminator: &'mir Terminator<'tcx>,
        location: Location,
    ) {
        OperandCollector { state, visitor: self, map: &results.analysis.0.map }
            .visit_terminator(terminator, location);
    }
}

impl<'tcx> MutVisitor<'tcx> for Patch<'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn visit_statement(&mut self, statement: &mut Statement<'tcx>, location: Location) {
        if let Some(value) = self.assignments.get(&location) {
            match &mut statement.kind {
                StatementKind::Assign(box (_, rvalue)) => {
                    *rvalue = Rvalue::Use(self.make_operand(*value));
                }
                _ => bug!("found assignment info for non-assign statement"),
            }
        } else {
            self.super_statement(statement, location);
        }
    }

    fn visit_operand(&mut self, operand: &mut Operand<'tcx>, location: Location) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                if let Some(value) = self.before_effect.get(&(location, *place)) {
                    *operand = self.make_operand(*value);
                } else if !place.projection.is_empty() {
                    self.super_operand(operand, location)
                }
            }
            Operand::Constant(_) => {}
        }
    }

    fn process_projection_elem(
        &mut self,
        elem: PlaceElem<'tcx>,
        location: Location,
    ) -> Option<PlaceElem<'tcx>> {
        if let PlaceElem::Index(local) = elem {
            let offset = self.before_effect.get(&(location, local.into()))?;
            let offset = offset.try_to_scalar()?;
            let offset = offset.to_target_usize(&self.tcx).ok()?;
            let min_length = offset.checked_add(1)?;
            Some(PlaceElem::ConstantIndex { offset, min_length, from_end: false })
        } else {
            None
        }
    }
}

struct OperandCollector<'tcx, 'map, 'locals, 'a> {
    state: &'a State<FlatSet<Scalar>>,
    visitor: &'a mut Collector<'tcx, 'locals>,
    map: &'map Map,
}

impl<'tcx> Visitor<'tcx> for OperandCollector<'tcx, '_, '_, '_> {
    fn visit_projection_elem(
        &mut self,
        _: PlaceRef<'tcx>,
        elem: PlaceElem<'tcx>,
        _: PlaceContext,
        location: Location,
    ) {
        if let PlaceElem::Index(local) = elem
            && let Some(value) = self.visitor.try_make_constant(local.into(), self.state, self.map)
        {
            self.visitor.patch.before_effect.insert((location, local.into()), value);
        }
    }

    fn visit_operand(&mut self, operand: &Operand<'tcx>, location: Location) {
        if let Some(place) = operand.place() {
            if let Some(value) = self.visitor.try_make_constant(place, self.state, self.map) {
                self.visitor.patch.before_effect.insert((location, place), value);
            } else if !place.projection.is_empty() {
                // Try to propagate into `Index` projections.
                self.super_operand(operand, location)
            }
        }
    }
}

struct DummyMachine;

impl<'mir, 'tcx: 'mir> rustc_const_eval::interpret::Machine<'mir, 'tcx> for DummyMachine {
    rustc_const_eval::interpret::compile_time_machine!(<'mir, 'tcx>);
    type MemoryKind = !;
    const PANIC_ON_ALLOC_FAIL: bool = true;

    #[inline(always)]
    fn enforce_alignment(_ecx: &InterpCx<'mir, 'tcx, Self>) -> CheckAlignment {
        // We do not check for alignment to avoid having to carry an `Align`
        // in `ConstValue::ByRef`.
        CheckAlignment::No
    }

    fn enforce_validity(_ecx: &InterpCx<'mir, 'tcx, Self>, _layout: TyAndLayout<'tcx>) -> bool {
        unimplemented!()
    }
    fn alignment_check_failed(
        _ecx: &InterpCx<'mir, 'tcx, Self>,
        _has: Align,
        _required: Align,
        _check: CheckAlignment,
    ) -> interpret::InterpResult<'tcx, ()> {
        unimplemented!()
    }

    fn before_access_global(
        _tcx: TyCtxt<'tcx>,
        _machine: &Self,
        _alloc_id: AllocId,
        alloc: ConstAllocation<'tcx>,
        _static_def_id: Option<DefId>,
        is_write: bool,
    ) -> InterpResult<'tcx> {
        if is_write {
            crate::const_prop::throw_machine_stop_str!("can't write to global");
        }

        // If the static allocation is mutable, then we can't const prop it as its content
        // might be different at runtime.
        if alloc.inner().mutability.is_mut() {
            crate::const_prop::throw_machine_stop_str!("can't access mutable globals in ConstProp");
        }

        Ok(())
    }

    fn find_mir_or_eval_fn(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _instance: ty::Instance<'tcx>,
        _abi: rustc_target::spec::abi::Abi,
        _args: &[rustc_const_eval::interpret::FnArg<'tcx, Self::Provenance>],
        _destination: &rustc_const_eval::interpret::PlaceTy<'tcx, Self::Provenance>,
        _target: Option<BasicBlock>,
        _unwind: UnwindAction,
    ) -> interpret::InterpResult<'tcx, Option<(&'mir Body<'tcx>, ty::Instance<'tcx>)>> {
        unimplemented!()
    }

    fn panic_nounwind(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _msg: &str,
    ) -> interpret::InterpResult<'tcx> {
        unimplemented!()
    }

    fn call_intrinsic(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _instance: ty::Instance<'tcx>,
        _args: &[rustc_const_eval::interpret::OpTy<'tcx, Self::Provenance>],
        _destination: &rustc_const_eval::interpret::PlaceTy<'tcx, Self::Provenance>,
        _target: Option<BasicBlock>,
        _unwind: UnwindAction,
    ) -> interpret::InterpResult<'tcx> {
        unimplemented!()
    }

    fn assert_panic(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _msg: &rustc_middle::mir::AssertMessage<'tcx>,
        _unwind: UnwindAction,
    ) -> interpret::InterpResult<'tcx> {
        unimplemented!()
    }

    fn binary_ptr_op(
        _ecx: &InterpCx<'mir, 'tcx, Self>,
        _bin_op: BinOp,
        _left: &rustc_const_eval::interpret::ImmTy<'tcx, Self::Provenance>,
        _right: &rustc_const_eval::interpret::ImmTy<'tcx, Self::Provenance>,
    ) -> interpret::InterpResult<'tcx, (Scalar<Self::Provenance>, bool, Ty<'tcx>)> {
        throw_unsup!(Unsupported("".into()))
    }

    fn expose_ptr(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _ptr: interpret::Pointer<Self::Provenance>,
    ) -> interpret::InterpResult<'tcx> {
        unimplemented!()
    }

    fn init_frame_extra(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _frame: rustc_const_eval::interpret::Frame<'mir, 'tcx, Self::Provenance>,
    ) -> interpret::InterpResult<
        'tcx,
        rustc_const_eval::interpret::Frame<'mir, 'tcx, Self::Provenance, Self::FrameExtra>,
    > {
        unimplemented!()
    }

    fn stack<'a>(
        _ecx: &'a InterpCx<'mir, 'tcx, Self>,
    ) -> &'a [rustc_const_eval::interpret::Frame<'mir, 'tcx, Self::Provenance, Self::FrameExtra>]
    {
        // Return an empty stack instead of panicking, as `cur_span` uses it to evaluate constants.
        &[]
    }

    fn stack_mut<'a>(
        _ecx: &'a mut InterpCx<'mir, 'tcx, Self>,
    ) -> &'a mut Vec<
        rustc_const_eval::interpret::Frame<'mir, 'tcx, Self::Provenance, Self::FrameExtra>,
    > {
        unimplemented!()
    }
}
