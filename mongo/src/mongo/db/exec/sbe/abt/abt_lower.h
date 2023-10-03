/**
 *    Copyright (C) 2022-present MongoDB, Inc.
 *
 *    This program is free software: you can redistribute it and/or modify
 *    it under the terms of the Server Side Public License, version 1,
 *    as published by MongoDB, Inc.
 *
 *    This program is distributed in the hope that it will be useful,
 *    but WITHOUT ANY WARRANTY; without even the implied warranty of
 *    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *    Server Side Public License for more details.
 *
 *    You should have received a copy of the Server Side Public License
 *    along with this program. If not, see
 *    <http://www.mongodb.com/licensing/server-side-public-license>.
 *
 *    As a special exception, the copyright holders give permission to link the
 *    code of portions of this program with the OpenSSL library under certain
 *    conditions as described in each individual source file and distribute
 *    linked combinations including the program with the OpenSSL library. You
 *    must comply with the Server Side Public License in all respects for
 *    all of the code used other than as permitted herein. If you modify file(s)
 *    with this exception, you may extend this exception to your version of the
 *    file(s), but you are not obligated to do so. If you do not wish to do so,
 *    delete this exception statement from your version. If you delete this
 *    exception statement from all source files in the program, then also delete
 *    it in the license file.
 */

#pragma once

#include <boost/optional/optional.hpp>
#include <memory>
#include <string>
#include <vector>

#include "mongo/db/exec/sbe/abt/abt_lower_defs.h"
#include "mongo/db/exec/sbe/abt/slots_provider.h"
#include "mongo/db/exec/sbe/expressions/expression.h"
#include "mongo/db/exec/sbe/expressions/runtime_environment.h"
#include "mongo/db/exec/sbe/stages/stages.h"
#include "mongo/db/exec/sbe/values/slot.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/index_bounds.h"
#include "mongo/db/query/optimizer/metadata.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/node_defs.h"
#include "mongo/db/query/optimizer/reference_tracker.h"
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"
#include "mongo/db/query/optimizer/utils/utils.h"
#include "mongo/util/assert_util.h"

namespace mongo::optimizer {
constexpr mongo::StringData kshardFiltererSlotName = "shardFilterer"_sd;

class VarResolver {
public:
    using LowerFuncT = std::function<std::unique_ptr<sbe::EExpression>(const ProjectionName&)>;

    VarResolver(SlotVarMap& slotMap) : _slotMap(&slotMap) {}

    template <typename FuncT>
    VarResolver(FuncT lowerFn) : _lowerFn(std::move(lowerFn)) {}

    template <typename FuncT>
    VarResolver(SlotVarMap& slotMap, FuncT lowerFn)
        : _slotMap(&slotMap), _lowerFn(std::move(lowerFn)) {}

    std::unique_ptr<sbe::EExpression> operator()(const ProjectionName& name) const;

private:
    SlotVarMap* _slotMap = nullptr;
    LowerFuncT _lowerFn;
};

class SBEExpressionLowering {
public:
    SBEExpressionLowering(const VariableEnvironment& env,
                          VarResolver vr,
                          SlotsProvider& providedSlots,
                          sbe::value::SlotIdGenerator& ids,
                          sbe::InputParamToSlotMap& inputParamToSlotMap,
                          const Metadata* metadata = nullptr,
                          const NodeProps* np = nullptr)
        : _env(env),
          _varResolver(vr),
          _providedSlots(providedSlots),
          _slotIdGenerator(ids),
          _inputParamToSlotMap(inputParamToSlotMap),
          _metadata(metadata),
          _np(np) {}

    // The default noop transport.
    template <typename T, typename... Ts>
    std::unique_ptr<sbe::EExpression> transport(const T&, Ts&&...) {
        uasserted(6624237,
                  "ABT expression lowering encountered operator which cannot be directly lowered "
                  "to an SBE expression.");
        return nullptr;
    }

    std::unique_ptr<sbe::EExpression> transport(const Constant&);
    std::unique_ptr<sbe::EExpression> transport(const Variable& var);
    std::unique_ptr<sbe::EExpression> transport(const Source&);
    std::unique_ptr<sbe::EExpression> transport(const BinaryOp& op,
                                                std::unique_ptr<sbe::EExpression> lhs,
                                                std::unique_ptr<sbe::EExpression> rhs);
    std::unique_ptr<sbe::EExpression> transport(const UnaryOp& op,
                                                std::unique_ptr<sbe::EExpression> arg);
    std::unique_ptr<sbe::EExpression> transport(const If&,
                                                std::unique_ptr<sbe::EExpression> cond,
                                                std::unique_ptr<sbe::EExpression> thenBranch,
                                                std::unique_ptr<sbe::EExpression> elseBranch);

    void prepare(const Let& let);
    std::unique_ptr<sbe::EExpression> transport(const Let& let,
                                                std::unique_ptr<sbe::EExpression> bind,
                                                std::unique_ptr<sbe::EExpression> in);
    void prepare(const LambdaAbstraction& lam);
    std::unique_ptr<sbe::EExpression> transport(const LambdaAbstraction& lam,
                                                std::unique_ptr<sbe::EExpression> body);
    std::unique_ptr<sbe::EExpression> transport(const LambdaApplication&,
                                                std::unique_ptr<sbe::EExpression> lam,
                                                std::unique_ptr<sbe::EExpression> arg);
    std::unique_ptr<sbe::EExpression> transport(
        const FunctionCall& fn, std::vector<std::unique_ptr<sbe::EExpression>> args);

    std::unique_ptr<sbe::EExpression> optimize(const ABT& n);

private:
    std::unique_ptr<sbe::EExpression> handleShardFilterFunctionCall(
        const FunctionCall& fn,
        std::vector<std::unique_ptr<sbe::EExpression>>& args,
        std::string name);

    const VariableEnvironment& _env;
    VarResolver _varResolver;
    SlotsProvider& _providedSlots;
    sbe::value::SlotIdGenerator& _slotIdGenerator;

    // Map to record newly allocated slots and the parameter ids they were generated from.
    // For more details see PlanStageStaticData::inputParamToSlotMap
    sbe::InputParamToSlotMap& _inputParamToSlotMap;
    const Metadata* _metadata;
    const NodeProps* _np;

    sbe::FrameId _frameCounter{100};
    stdx::unordered_map<const Let*, sbe::FrameId> _letMap;
    stdx::unordered_map<const LambdaAbstraction*, sbe::FrameId> _lambdaMap;
};

enum class ScanOrder {
    Forward,
    Reverse,
    Random  // Uses a random cursor.
};

class SBENodeLowering {
public:
    SBENodeLowering(const VariableEnvironment& env,
                    SlotsProvider& providedSlots,
                    sbe::value::SlotIdGenerator& ids,
                    sbe::InputParamToSlotMap& inputParamToSlotMap,
                    const Metadata& metadata,
                    const NodeToGroupPropsMap& nodeToGroupPropsMap,
                    const ScanOrder scanOrder,
                    PlanYieldPolicy* yieldPolicy = nullptr)
        : _env(env),
          _providedSlots(providedSlots),
          _slotIdGenerator(ids),
          _inputParamToSlotMap(inputParamToSlotMap),
          _metadata(metadata),
          _nodeToGroupPropsMap(nodeToGroupPropsMap),
          _scanOrder(scanOrder),
          _yieldPolicy(yieldPolicy) {}

    // The default noop transport.
    template <typename T, typename... Ts>
    std::unique_ptr<sbe::PlanStage> walk(const T&,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         Ts&&...) {
        // We should not be seeing a physical delegator node here.
        static_assert(!canBePhysicalNode<T>() || std::is_same_v<MemoPhysicalDelegatorNode, T>,
                      "Physical nodes need to implement lowering");

        uasserted(6624238, "Unexpected node type.");
        return nullptr;
    }

    std::unique_ptr<sbe::PlanStage> walk(const RootNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& refs);
    std::unique_ptr<sbe::PlanStage> walk(const EvaluationNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& binds);

    std::unique_ptr<sbe::PlanStage> walk(const FilterNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& filter);

    std::unique_ptr<sbe::PlanStage> walk(const LimitSkipNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child);
    std::unique_ptr<sbe::PlanStage> walk(const ExchangeNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& refs);
    std::unique_ptr<sbe::PlanStage> walk(const CollationNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& refs);

    std::unique_ptr<sbe::PlanStage> walk(const UniqueNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& refs);

    std::unique_ptr<sbe::PlanStage> walk(const SpoolProducerNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& filter,
                                         const ABT& binder,
                                         const ABT& refs);
    std::unique_ptr<sbe::PlanStage> walk(const SpoolConsumerNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& binder);

    std::unique_ptr<sbe::PlanStage> walk(const GroupByNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& aggBinds,
                                         const ABT& aggRefs,
                                         const ABT& gbBind,
                                         const ABT& gbRefs);

    std::unique_ptr<sbe::PlanStage> walk(const NestedLoopJoinNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& leftChild,
                                         const ABT& rightChild,
                                         const ABT& filter);
    std::unique_ptr<sbe::PlanStage> walk(const HashJoinNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& leftChild,
                                         const ABT& rightChild,
                                         const ABT& refs);
    std::unique_ptr<sbe::PlanStage> walk(const MergeJoinNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& leftChild,
                                         const ABT& rightChild,
                                         const ABT& refs);

    std::unique_ptr<sbe::PlanStage> walk(const SortedMergeNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABTVector& children,
                                         const ABT& binder,
                                         const ABT& refs);

    std::unique_ptr<sbe::PlanStage> walk(const UnionNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABTVector& children,
                                         const ABT& binder,
                                         const ABT& refs);

    std::unique_ptr<sbe::PlanStage> walk(const UnwindNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& child,
                                         const ABT& pidBind,
                                         const ABT& refs);

    std::unique_ptr<sbe::PlanStage> walk(const PhysicalScanNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& /*binds*/);
    std::unique_ptr<sbe::PlanStage> walk(const CoScanNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot);

    std::unique_ptr<sbe::PlanStage> walk(const IndexScanNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& /*binds*/);
    std::unique_ptr<sbe::PlanStage> walk(const SeekNode& n,
                                         SlotVarMap& slotMap,
                                         boost::optional<sbe::value::SlotId>& ridSlot,
                                         const ABT& /*binds*/,
                                         const ABT& /*refs*/);

    std::unique_ptr<sbe::PlanStage> optimize(const ABT& n,
                                             SlotVarMap& slotMap,
                                             boost::optional<sbe::value::SlotId>& ridSlot);

private:
    void generateSlots(SlotVarMap& slotMap,
                       const FieldProjectionMap& fieldProjectionMap,
                       boost::optional<sbe::value::SlotId>& ridSlot,
                       boost::optional<sbe::value::SlotId>& rootSlot,
                       std::vector<std::string>& fields,
                       sbe::value::SlotVector& vars);

    /**
     * Convert a vector of ProjectionNames to slot IDs from the projections that have already been
     * bound to slots.
     *
     * Preserves the order, and therefore preserves duplicates and the result .size() == the input
     * .size().
     *
     * Even when 'projectionNames' is free of duplicates, the output may have duplicates because two
     * projections can map to the same slot.
     */
    sbe::value::SlotVector convertProjectionsToSlots(const SlotVarMap& slotMap,
                                                     const ProjectionNameVector& projectionNames);

    /**
     * During Cascades, projections that a node is required to propagate up the tree are added to
     * the RequiredProjections node property. This function pulls out those projection names and
     * looks up the relevant slot IDs they are bound to. The optional toExclude vector can prevent
     * some slots from being added to the output vector.
     *
     * The output is free of duplicates.
     *
     * Does not guarantee any output order.
     */
    sbe::value::SlotVector convertRequiredProjectionsToSlots(
        const SlotVarMap& slotMap,
        const NodeProps& props,
        const sbe::value::SlotVector& toExclude = {});

    std::unique_ptr<sbe::EExpression> convertBoundsToExpr(SlotVarMap& slotMap,
                                                          bool isLower,
                                                          bool reversed,
                                                          const IndexDefinition& indexDef,
                                                          const CompoundBoundRequirement& bound);

    std::unique_ptr<sbe::PlanStage> generateInternal(const ABT& n,
                                                     SlotVarMap& slotMap,
                                                     boost::optional<sbe::value::SlotId>& ridSlot);

    /**
     * Maps a projection name to a slot by updating slotMap field.
     * By default it will tassert rather than overwrite an existing entry--it's the caller's
     * responsibility not to call this twice with the same projName. With 'canOverwrite = true' it
     * is allowed to overwrite an existing entry. This is useful for nodes that intentionally use
     * the same projName for two different values. For example, two independent index scans could
     * both use the same projName for RID. Or, Unwind uses the same projName both for the original
     * array, and the unwound elements.
     */
    void mapProjToSlot(SlotVarMap& slotMap,
                       const ProjectionName& projName,
                       sbe::value::SlotId slot,
                       bool canOverwrite = false);


    /**
     * Instantiate an expression lowering transporter for use in node lowering.
     */
    SBEExpressionLowering getExpressionLowering(SlotVarMap& slotMap,
                                                const NodeProps* np = nullptr) {
        return SBEExpressionLowering{
            _env, slotMap, _providedSlots, _slotIdGenerator, _inputParamToSlotMap, &_metadata, np};
    }

    std::unique_ptr<sbe::EExpression> lowerExpression(const ABT& e,
                                                      SlotVarMap& slotMap,
                                                      const NodeProps* np = nullptr) {
        return getExpressionLowering(slotMap, np).optimize(e);
    }
    const VariableEnvironment& _env;
    SlotsProvider& _providedSlots;

    sbe::value::SlotIdGenerator& _slotIdGenerator;

    sbe::InputParamToSlotMap& _inputParamToSlotMap;

    const Metadata& _metadata;
    const NodeToGroupPropsMap& _nodeToGroupPropsMap;

    // Specifies the order for any ScanStages. Currently only supported for single-threaded
    // (non parallel-scanned) mongod collections.
    // TODO SERVER-73010: handle cases where we have more than one collection scan.
    const ScanOrder _scanOrder;

    // Specifies the yielding policy to initialize the corresponding PlanStages with.
    PlanYieldPolicy* _yieldPolicy;
};

inline sbe::EPrimUnary::Op getEPrimUnaryOp(optimizer::Operations op) {
    switch (op) {
        case Operations::Neg:
            return sbe::EPrimUnary::negate;
        case Operations::Not:
            return sbe::EPrimUnary::logicNot;
        default:
            MONGO_UNREACHABLE;
    }
}

inline sbe::EPrimBinary::Op getEPrimBinaryOp(optimizer::Operations op) {
    switch (op) {
        case Operations::Eq:
            return sbe::EPrimBinary::eq;
        case Operations::Neq:
            return sbe::EPrimBinary::neq;
        case Operations::Gt:
            return sbe::EPrimBinary::greater;
        case Operations::Gte:
            return sbe::EPrimBinary::greaterEq;
        case Operations::Lt:
            return sbe::EPrimBinary::less;
        case Operations::Lte:
            return sbe::EPrimBinary::lessEq;
        case Operations::Add:
            return sbe::EPrimBinary::add;
        case Operations::Sub:
            return sbe::EPrimBinary::sub;
        case Operations::FillEmpty:
            return sbe::EPrimBinary::fillEmpty;
        case Operations::And:
            return sbe::EPrimBinary::logicAnd;
        case Operations::Or:
            return sbe::EPrimBinary::logicOr;
        case Operations::Cmp3w:
            return sbe::EPrimBinary::cmp3w;
        case Operations::Div:
            return sbe::EPrimBinary::div;
        case Operations::Mult:
            return sbe::EPrimBinary::mul;
        default:
            MONGO_UNREACHABLE;
    }
}
}  // namespace mongo::optimizer
