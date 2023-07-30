/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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


#include <absl/container/flat_hash_map.h>
#include <absl/container/flat_hash_set.h>
#include <absl/container/inlined_vector.h>
#include <absl/meta/type_traits.h>
#include <boost/container/flat_set.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
// IWYU pragma: no_include "ext/alloc_traits.h"
#include <algorithm>
#include <iterator>
#include <numeric>
#include <string_view>

#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/health_log_gen.h"
#include "mongo/db/catalog/health_log_interface.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/exec/docval_to_sbeval.h"
#include "mongo/db/exec/sbe/expressions/expression.h"
#include "mongo/db/exec/sbe/stages/branch.h"
#include "mongo/db/exec/sbe/stages/co_scan.h"
#include "mongo/db/exec/sbe/stages/hash_agg.h"
#include "mongo/db/exec/sbe/stages/limit_skip.h"
#include "mongo/db/exec/sbe/stages/loop_join.h"
#include "mongo/db/exec/sbe/stages/makeobj.h"
#include "mongo/db/exec/sbe/stages/project.h"
#include "mongo/db/exec/sbe/stages/scan.h"
#include "mongo/db/exec/sbe/stages/traverse.h"
#include "mongo/db/exec/sbe/stages/union.h"
#include "mongo/db/exec/sbe/stages/unwind.h"
#include "mongo/db/index/index_access_method.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/index/multikey_paths.h"
#include "mongo/db/matcher/matcher_type_set.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/pipeline/expression_dependencies.h"
#include "mongo/db/query/bson_typemask.h"
#include "mongo/db/query/projection.h"
#include "mongo/db/query/projection_ast.h"
#include "mongo/db/query/projection_ast_path_tracking_visitor.h"
#include "mongo/db/query/projection_ast_visitor.h"
#include "mongo/db/query/sbe_stage_builder.h"
#include "mongo/db/query/sbe_stage_builder_helpers.h"
#include "mongo/db/query/tree_walker.h"
#include "mongo/db/record_id.h"
#include "mongo/db/storage/execution_context.h"
#include "mongo/db/storage/record_data.h"
#include "mongo/db/storage/record_store.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/snapshot.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/log_options.h"
#include "mongo/util/debug_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/shared_buffer_fragment.h"
#include "mongo/util/stacktrace.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo::stage_builder {

std::unique_ptr<sbe::EExpression> makeUnaryOp(sbe::EPrimUnary::Op unaryOp,
                                              std::unique_ptr<sbe::EExpression> operand) {
    return sbe::makeE<sbe::EPrimUnary>(unaryOp, std::move(operand));
}

std::unique_ptr<sbe::EExpression> makeNot(std::unique_ptr<sbe::EExpression> e) {
    return makeUnaryOp(sbe::EPrimUnary::logicNot, std::move(e));
}

std::unique_ptr<sbe::EExpression> makeBinaryOp(sbe::EPrimBinary::Op binaryOp,
                                               std::unique_ptr<sbe::EExpression> lhs,
                                               std::unique_ptr<sbe::EExpression> rhs,
                                               std::unique_ptr<sbe::EExpression> collator) {
    using namespace std::literals;

    if (collator && sbe::EPrimBinary::isComparisonOp(binaryOp)) {
        return sbe::makeE<sbe::EPrimBinary>(
            binaryOp, std::move(lhs), std::move(rhs), std::move(collator));
    } else {
        return sbe::makeE<sbe::EPrimBinary>(binaryOp, std::move(lhs), std::move(rhs));
    }
}

std::unique_ptr<sbe::EExpression> makeBinaryOp(sbe::EPrimBinary::Op binaryOp,
                                               std::unique_ptr<sbe::EExpression> lhs,
                                               std::unique_ptr<sbe::EExpression> rhs,
                                               StageBuilderState& state) {
    auto collatorSlot = state.getCollatorSlot();
    auto collatorVar = collatorSlot ? sbe::makeE<sbe::EVariable>(*collatorSlot) : nullptr;

    return makeBinaryOp(binaryOp, std::move(lhs), std::move(rhs), std::move(collatorVar));
}

std::unique_ptr<sbe::EExpression> makeIsMember(std::unique_ptr<sbe::EExpression> input,
                                               std::unique_ptr<sbe::EExpression> arr,
                                               std::unique_ptr<sbe::EExpression> collator) {
    if (collator) {
        return makeFunction("collIsMember", std::move(collator), std::move(input), std::move(arr));
    } else {
        return makeFunction("isMember", std::move(input), std::move(arr));
    }
}

std::unique_ptr<sbe::EExpression> makeIsMember(std::unique_ptr<sbe::EExpression> input,
                                               std::unique_ptr<sbe::EExpression> arr,
                                               StageBuilderState& state) {
    auto collatorSlot = state.getCollatorSlot();
    auto collatorVar = collatorSlot ? sbe::makeE<sbe::EVariable>(*collatorSlot) : nullptr;

    return makeIsMember(std::move(input), std::move(arr), std::move(collatorVar));
}

std::unique_ptr<sbe::EExpression> generateNullOrMissingExpr(const sbe::EExpression& expr) {
    return makeBinaryOp(sbe::EPrimBinary::fillEmpty,
                        makeFunction("typeMatch",
                                     expr.clone(),
                                     makeConstant(sbe::value::TypeTags::NumberInt64,
                                                  sbe::value::bitcastFrom<int64_t>(
                                                      getBSONTypeMask(BSONType::jstNULL) |
                                                      getBSONTypeMask(BSONType::Undefined)))),
                        sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::Boolean,
                                                   sbe::value::bitcastFrom<bool>(true)));
}

std::unique_ptr<sbe::EExpression> generateNullOrMissing(const sbe::EVariable& var) {
    return generateNullOrMissingExpr(var);
}

std::unique_ptr<sbe::EExpression> generateNullOrMissing(const sbe::FrameId frameId,
                                                        const sbe::value::SlotId slotId) {
    sbe::EVariable var{frameId, slotId};
    return generateNullOrMissing(var);
}

std::unique_ptr<sbe::EExpression> generateNullOrMissing(std::unique_ptr<sbe::EExpression> arg) {
    return generateNullOrMissingExpr(*arg);
}

std::unique_ptr<sbe::EExpression> generateNullOrMissing(EvalExpr arg, StageBuilderState& state) {
    auto expr = arg.extractExpr(state.slotVarMap, state);
    return generateNullOrMissingExpr(*expr);
}

std::unique_ptr<sbe::EExpression> generateNonNumericCheck(const sbe::EVariable& var) {
    return makeNot(makeFunction("isNumber", var.clone()));
}

std::unique_ptr<sbe::EExpression> generateNonNumericCheck(EvalExpr expr, StageBuilderState& state) {
    return makeNot(makeFunction("isNumber", expr.extractExpr(state.slotVarMap, state)));
}

std::unique_ptr<sbe::EExpression> generateLongLongMinCheck(const sbe::EVariable& var) {
    return makeBinaryOp(
        sbe::EPrimBinary::logicAnd,
        makeFunction("typeMatch",
                     var.clone(),
                     makeConstant(sbe::value::TypeTags::NumberInt64,
                                  sbe::value::bitcastFrom<int64_t>(
                                      MatcherTypeSet{BSONType::NumberLong}.getBSONTypeMask()))),
        makeBinaryOp(sbe::EPrimBinary::eq,
                     var.clone(),
                     sbe::makeE<sbe::EConstant>(
                         sbe::value::TypeTags::NumberInt64,
                         sbe::value::bitcastFrom<int64_t>(std::numeric_limits<int64_t>::min()))));
}

std::unique_ptr<sbe::EExpression> generateNaNCheck(const sbe::EVariable& var) {
    return makeFunction("isNaN", var.clone());
}

std::unique_ptr<sbe::EExpression> generateNaNCheck(EvalExpr expr, StageBuilderState& state) {
    return makeFunction("isNaN", expr.extractExpr(state.slotVarMap, state));
}

std::unique_ptr<sbe::EExpression> generateInfinityCheck(const sbe::EVariable& var) {
    return makeFunction("isInfinity"_sd, var.clone());
}

std::unique_ptr<sbe::EExpression> generateInfinityCheck(EvalExpr expr, StageBuilderState& state) {
    return makeFunction("isInfinity"_sd, expr.extractExpr(state.slotVarMap, state));
}

std::unique_ptr<sbe::EExpression> generateNonPositiveCheck(const sbe::EVariable& var) {
    return makeBinaryOp(sbe::EPrimBinary::EPrimBinary::lessEq,
                        var.clone(),
                        sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::NumberInt32,
                                                   sbe::value::bitcastFrom<int32_t>(0)));
}

std::unique_ptr<sbe::EExpression> generatePositiveCheck(const sbe::EExpression& expr) {
    return makeBinaryOp(sbe::EPrimBinary::EPrimBinary::greater,
                        expr.clone(),
                        sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::NumberInt32,
                                                   sbe::value::bitcastFrom<int32_t>(0)));
}

std::unique_ptr<sbe::EExpression> generateNegativeCheck(const sbe::EVariable& var) {
    return makeBinaryOp(sbe::EPrimBinary::EPrimBinary::less,
                        var.clone(),
                        sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::NumberInt32,
                                                   sbe::value::bitcastFrom<int32_t>(0)));
}

std::unique_ptr<sbe::EExpression> generateNonObjectCheck(const sbe::EVariable& var) {
    return makeNot(makeFunction("isObject", var.clone()));
}

std::unique_ptr<sbe::EExpression> generateNonStringCheck(const sbe::EExpression& expr) {
    return makeNot(makeFunction("isString", expr.clone()));
}

std::unique_ptr<sbe::EExpression> generateNullishOrNotRepresentableInt32Check(
    const sbe::EVariable& var) {
    auto numericConvert32 =
        sbe::makeE<sbe::ENumericConvert>(var.clone(), sbe::value::TypeTags::NumberInt32);
    return makeBinaryOp(sbe::EPrimBinary::logicOr,
                        generateNullOrMissing(var),
                        makeNot(makeFunction("exists", std::move(numericConvert32))));
}

std::unique_ptr<sbe::EExpression> generateNonTimestampCheck(const sbe::EVariable& var) {
    return makeNot(makeFunction("isTimestamp", var.clone()));
}

template <>
std::unique_ptr<sbe::EExpression> buildMultiBranchConditional(
    std::unique_ptr<sbe::EExpression> defaultCase) {
    return defaultCase;
}

std::unique_ptr<sbe::EExpression> buildMultiBranchConditionalFromCaseValuePairs(
    std::vector<CaseValuePair> caseValuePairs, std::unique_ptr<sbe::EExpression> defaultValue) {
    return std::accumulate(
        std::make_reverse_iterator(std::make_move_iterator(caseValuePairs.end())),
        std::make_reverse_iterator(std::make_move_iterator(caseValuePairs.begin())),
        std::move(defaultValue),
        [](auto&& expression, auto&& caseValuePair) {
            return buildMultiBranchConditional(std::move(caseValuePair), std::move(expression));
        });
}

std::unique_ptr<sbe::PlanStage> makeLimitTree(std::unique_ptr<sbe::PlanStage> inputStage,
                                              PlanNodeId planNodeId,
                                              long long limit) {
    return sbe::makeS<sbe::LimitSkipStage>(std::move(inputStage), limit, boost::none, planNodeId);
}

std::unique_ptr<sbe::PlanStage> makeLimitCoScanTree(PlanNodeId planNodeId, long long limit) {
    return sbe::makeS<sbe::LimitSkipStage>(
        sbe::makeS<sbe::CoScanStage>(planNodeId), limit, boost::none, planNodeId);
}

std::unique_ptr<sbe::EExpression> makeFillEmptyFalse(std::unique_ptr<sbe::EExpression> e) {
    using namespace std::literals;
    return makeBinaryOp(sbe::EPrimBinary::fillEmpty,
                        std::move(e),
                        sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::Boolean,
                                                   sbe::value::bitcastFrom<bool>(false)));
}

std::unique_ptr<sbe::EExpression> makeVariable(sbe::value::SlotId slotId) {
    return sbe::makeE<sbe::EVariable>(slotId);
}

std::unique_ptr<sbe::EExpression> makeVariable(sbe::FrameId frameId, sbe::value::SlotId slotId) {
    return sbe::makeE<sbe::EVariable>(frameId, slotId);
}

std::unique_ptr<sbe::EExpression> makeMoveVariable(sbe::FrameId frameId,
                                                   sbe::value::SlotId slotId) {
    return sbe::makeE<sbe::EVariable>(frameId, slotId, true);
}

std::unique_ptr<sbe::EExpression> makeFillEmptyNull(std::unique_ptr<sbe::EExpression> e) {
    using namespace std::literals;
    return makeBinaryOp(sbe::EPrimBinary::fillEmpty,
                        std::move(e),
                        sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::Null, 0));
}

std::unique_ptr<sbe::EExpression> makeFillEmptyUndefined(std::unique_ptr<sbe::EExpression> e) {
    using namespace std::literals;
    return makeBinaryOp(sbe::EPrimBinary::fillEmpty,
                        std::move(e),
                        sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::bsonUndefined, 0));
}

std::unique_ptr<sbe::EExpression> generateShardKeyBinding(
    const sbe::MatchPath& keyPatternField,
    sbe::value::FrameIdGenerator& frameIdGenerator,
    std::unique_ptr<sbe::EExpression> inputExpr,
    int level) {
    invariant(level >= 0);

    auto makeGetFieldKeyPattern = [&](std::unique_ptr<sbe::EExpression> slot) {
        return makeFillEmptyNull(makeFunction(
            "getField"_sd, std::move(slot), sbe::makeE<sbe::EConstant>(keyPatternField[level])));
    };

    if (level == keyPatternField.numParts() - 1) {
        auto frameId = frameIdGenerator.generate();
        auto bindSlot = sbe::makeE<sbe::EVariable>(frameId, 0);
        return makeGetFieldKeyPattern(std::move(inputExpr));
    }

    auto frameId = frameIdGenerator.generate();
    auto nextSlot = sbe::makeE<sbe::EVariable>(frameId, 0);
    auto shardKeyBinding =
        generateShardKeyBinding(keyPatternField, frameIdGenerator, nextSlot->clone(), level + 1);

    return sbe::makeE<sbe::ELocalBind>(
        frameId,
        sbe::makeEs(makeGetFieldKeyPattern(std::move(inputExpr))),
        sbe::makeE<sbe::EIf>(makeFunction("isArray"_sd, nextSlot->clone()),
                             nextSlot->clone(),
                             std::move(shardKeyBinding)));
}

EvalStage makeLimitCoScanStage(PlanNodeId planNodeId, long long limit) {
    return {makeLimitCoScanTree(planNodeId, limit), sbe::makeSV()};
}

std::pair<sbe::value::SlotId, EvalStage> projectEvalExpr(
    EvalExpr expr,
    EvalStage stage,
    PlanNodeId planNodeId,
    sbe::value::SlotIdGenerator* slotIdGenerator,
    StageBuilderState& state) {
    // If expr's value is already in a slot, return the slot.
    if (expr.hasSlot()) {
        return {*expr.getSlot(), std::move(stage)};
    }

    // If expr's value is an expression, create a ProjectStage to evaluate the expression
    // into a slot.
    auto slot = slotIdGenerator->generate();
    stage =
        makeProject(std::move(stage), planNodeId, slot, expr.extractExpr(state.slotVarMap, state));
    return {slot, std::move(stage)};
}

EvalStage makeProject(EvalStage stage, sbe::SlotExprPairVector projects, PlanNodeId planNodeId) {
    auto outSlots = stage.extractOutSlots();
    for (auto& [slot, _] : projects) {
        outSlots.push_back(slot);
    }

    return {sbe::makeS<sbe::ProjectStage>(
                stage.extractStage(planNodeId), std::move(projects), planNodeId),
            std::move(outSlots)};
}

EvalStage makeLoopJoin(EvalStage left,
                       EvalStage right,
                       PlanNodeId planNodeId,
                       const sbe::value::SlotVector& lexicalEnvironment) {
    // If 'left' and 'right' are both null, we just return null. If one of 'left'/'right' is null
    // and the other is non-null, return whichever one is non-null.
    if (left.isNull()) {
        return right;
    } else if (right.isNull()) {
        return left;
    }

    auto outerProjects = left.getOutSlots();
    auto outerCorrelated = left.getOutSlots();

    outerCorrelated.insert(
        outerCorrelated.end(), lexicalEnvironment.begin(), lexicalEnvironment.end());

    auto outSlots = left.extractOutSlots();
    outSlots.insert(outSlots.end(), right.getOutSlots().begin(), right.getOutSlots().end());

    return {sbe::makeS<sbe::LoopJoinStage>(left.extractStage(planNodeId),
                                           right.extractStage(planNodeId),
                                           std::move(outerProjects),
                                           std::move(outerCorrelated),
                                           nullptr,
                                           planNodeId),
            std::move(outSlots)};
}

EvalStage makeUnwind(EvalStage inputEvalStage,
                     sbe::value::SlotIdGenerator* slotIdGenerator,
                     PlanNodeId planNodeId,
                     bool preserveNullAndEmptyArrays) {
    auto unwindSlot = slotIdGenerator->generate();
    auto unwindStage = sbe::makeS<sbe::UnwindStage>(inputEvalStage.extractStage(planNodeId),
                                                    inputEvalStage.getOutSlots().front(),
                                                    unwindSlot,
                                                    slotIdGenerator->generate(),
                                                    preserveNullAndEmptyArrays,
                                                    planNodeId);
    return {std::move(unwindStage), sbe::makeSV(unwindSlot)};
}

EvalStage makeBranch(EvalStage thenStage,
                     EvalStage elseStage,
                     std::unique_ptr<sbe::EExpression> ifExpr,
                     sbe::value::SlotVector thenVals,
                     sbe::value::SlotVector elseVals,
                     sbe::value::SlotVector outputVals,
                     PlanNodeId planNodeId) {
    auto branchStage = sbe::makeS<sbe::BranchStage>(thenStage.extractStage(planNodeId),
                                                    elseStage.extractStage(planNodeId),
                                                    std::move(ifExpr),
                                                    std::move(thenVals),
                                                    std::move(elseVals),
                                                    outputVals,
                                                    planNodeId);
    return {std::move(branchStage), std::move(outputVals)};
}

EvalStage makeTraverse(EvalStage outer,
                       EvalStage inner,
                       sbe::value::SlotId inField,
                       sbe::value::SlotId outField,
                       sbe::value::SlotId outFieldInner,
                       std::unique_ptr<sbe::EExpression> foldExpr,
                       std::unique_ptr<sbe::EExpression> finalExpr,
                       PlanNodeId planNodeId,
                       boost::optional<size_t> nestedArraysDepth,
                       const sbe::value::SlotVector& lexicalEnvironment) {
    sbe::value::SlotVector outerCorrelated = lexicalEnvironment;
    for (auto slot : outer.getOutSlots()) {
        if (slot != inField) {
            outerCorrelated.push_back(slot);
        }
    }

    auto outSlots = outer.extractOutSlots();
    outSlots.push_back(outField);

    return {sbe::makeS<sbe::TraverseStage>(outer.extractStage(planNodeId),
                                           inner.extractStage(planNodeId),
                                           inField,
                                           outField,
                                           outFieldInner,
                                           std::move(outerCorrelated),
                                           std::move(foldExpr),
                                           std::move(finalExpr),
                                           planNodeId,
                                           nestedArraysDepth),
            std::move(outSlots)};
}

EvalStage makeLimitSkip(EvalStage input,
                        PlanNodeId planNodeId,
                        boost::optional<long long> limit,
                        boost::optional<long long> skip) {
    return EvalStage{
        sbe::makeS<sbe::LimitSkipStage>(input.extractStage(planNodeId), limit, skip, planNodeId),
        input.extractOutSlots()};
}

EvalStage makeUnion(std::vector<EvalStage> inputStages,
                    std::vector<sbe::value::SlotVector> inputVals,
                    sbe::value::SlotVector outputVals,
                    PlanNodeId planNodeId) {
    sbe::PlanStage::Vector branches;
    branches.reserve(inputStages.size());
    for (auto& inputStage : inputStages) {
        branches.emplace_back(inputStage.extractStage(planNodeId));
    }
    return EvalStage{sbe::makeS<sbe::UnionStage>(
                         std::move(branches), std::move(inputVals), outputVals, planNodeId),
                     outputVals};
}

EvalStage makeHashAgg(EvalStage stage,
                      sbe::value::SlotVector gbs,
                      sbe::AggExprVector aggs,
                      boost::optional<sbe::value::SlotId> collatorSlot,
                      bool allowDiskUse,
                      sbe::SlotExprPairVector mergingExprs,
                      PlanNodeId planNodeId) {
    stage.setOutSlots(gbs);
    for (auto& [slot, _] : aggs) {
        stage.addOutSlot(slot);
    }

    // In debug builds or when we explicitly set the query knob, we artificially force frequent
    // spilling. This makes sure that our tests exercise the spilling algorithm and the associated
    // logic for merging partial aggregates which otherwise would require large data sizes to
    // exercise.
    const bool forceIncreasedSpilling = allowDiskUse &&
        (kDebugBuild || internalQuerySlotBasedExecutionHashAggForceIncreasedSpilling.load());
    stage.setStage(sbe::makeS<sbe::HashAggStage>(stage.extractStage(planNodeId),
                                                 std::move(gbs),
                                                 std::move(aggs),
                                                 sbe::makeSV(),
                                                 true /* optimized close */,
                                                 collatorSlot,
                                                 allowDiskUse,
                                                 std::move(mergingExprs),
                                                 planNodeId,
                                                 true /* participateInTrialRunTracking */,
                                                 forceIncreasedSpilling));
    return stage;
}

EvalStage makeMkBsonObj(EvalStage stage,
                        sbe::value::SlotId objSlot,
                        boost::optional<sbe::value::SlotId> rootSlot,
                        boost::optional<sbe::MakeObjFieldBehavior> fieldBehavior,
                        std::vector<std::string> fields,
                        std::vector<std::string> projectFields,
                        sbe::value::SlotVector projectVars,
                        bool forceNewObject,
                        bool returnOldObject,
                        PlanNodeId planNodeId) {
    stage.setStage(sbe::makeS<sbe::MakeBsonObjStage>(stage.extractStage(planNodeId),
                                                     objSlot,
                                                     rootSlot,
                                                     fieldBehavior,
                                                     std::move(fields),
                                                     std::move(projectFields),
                                                     std::move(projectVars),
                                                     forceNewObject,
                                                     returnOldObject,
                                                     planNodeId));
    stage.addOutSlot(objSlot);

    return stage;
}

std::unique_ptr<sbe::EExpression> makeIfNullExpr(
    std::vector<std::unique_ptr<sbe::EExpression>> values,
    sbe::value::FrameIdGenerator* frameIdGenerator) {
    tassert(6987503, "Expected 'values' to be non-empty", values.size() > 0);

    size_t idx = values.size() - 1;
    auto expr = std::move(values[idx]);

    while (idx > 0) {
        --idx;

        auto frameId = frameIdGenerator->generate();
        auto var = sbe::EVariable{frameId, 0};

        expr = sbe::makeE<sbe::ELocalBind>(frameId,
                                           sbe::makeEs(std::move(values[idx])),
                                           sbe::makeE<sbe::EIf>(makeNot(generateNullOrMissing(var)),
                                                                var.clone(),
                                                                std::move(expr)));
    }

    return expr;
}

std::pair<sbe::value::SlotId, std::unique_ptr<sbe::PlanStage>> generateVirtualScan(
    sbe::value::SlotIdGenerator* slotIdGenerator,
    sbe::value::TypeTags arrTag,
    sbe::value::Value arrVal,
    PlanYieldPolicy* yieldPolicy) {
    // The value passed in must be an array.
    invariant(sbe::value::isArray(arrTag));

    // Make an EConstant expression for the array.
    auto arrayExpression = sbe::makeE<sbe::EConstant>(arrTag, arrVal);

    // Build the unwind/project/limit/coscan subtree.
    auto projectSlot = slotIdGenerator->generate();
    auto unwindSlot = slotIdGenerator->generate();
    auto unwind = sbe::makeS<sbe::UnwindStage>(
        sbe::makeProjectStage(makeLimitCoScanTree(kEmptyPlanNodeId, 1),
                              kEmptyPlanNodeId,
                              projectSlot,
                              std::move(arrayExpression)),
        projectSlot,
        unwindSlot,
        slotIdGenerator->generate(),  // We don't need an index slot but must to provide it.
        false,                        // Don't preserve null and empty arrays.
        kEmptyPlanNodeId,
        yieldPolicy);

    // Return the UnwindStage and its output slot. The UnwindStage can be used as an input
    // to other PlanStages.
    return {unwindSlot, std::move(unwind)};
}

std::pair<sbe::value::SlotVector, std::unique_ptr<sbe::PlanStage>> generateVirtualScanMulti(
    sbe::value::SlotIdGenerator* slotIdGenerator,
    int numSlots,
    sbe::value::TypeTags arrTag,
    sbe::value::Value arrVal,
    PlanYieldPolicy* yieldPolicy) {
    using namespace std::literals;

    invariant(numSlots >= 1);

    // Generate a mock scan with a single output slot.
    auto [scanSlot, scanStage] = generateVirtualScan(slotIdGenerator, arrTag, arrVal, yieldPolicy);

    // Create a ProjectStage that will read the data from 'scanStage' and split it up
    // across multiple output slots.
    sbe::value::SlotVector projectSlots;
    sbe::SlotExprPairVector projections;
    for (int32_t i = 0; i < numSlots; ++i) {
        projectSlots.emplace_back(slotIdGenerator->generate());
        projections.emplace_back(
            projectSlots.back(),
            makeFunction("getElement"_sd,
                         sbe::makeE<sbe::EVariable>(scanSlot),
                         sbe::makeE<sbe::EConstant>(sbe::value::TypeTags::NumberInt32,
                                                    sbe::value::bitcastFrom<int32_t>(i))));
    }

    return {std::move(projectSlots),
            sbe::makeS<sbe::ProjectStage>(
                std::move(scanStage), std::move(projections), kEmptyPlanNodeId)};
}

std::pair<sbe::value::TypeTags, sbe::value::Value> makeValue(const BSONObj& bo) {
    return sbe::value::copyValue(sbe::value::TypeTags::bsonObject,
                                 sbe::value::bitcastFrom<const char*>(bo.objdata()));
}

std::pair<sbe::value::TypeTags, sbe::value::Value> makeValue(const BSONArray& ba) {
    return sbe::value::copyValue(sbe::value::TypeTags::bsonArray,
                                 sbe::value::bitcastFrom<const char*>(ba.objdata()));
}

uint32_t dateTypeMask() {
    return (getBSONTypeMask(sbe::value::TypeTags::Date) |
            getBSONTypeMask(sbe::value::TypeTags::Timestamp) |
            getBSONTypeMask(sbe::value::TypeTags::ObjectId) |
            getBSONTypeMask(sbe::value::TypeTags::bsonObjectId));
}

sbe::value::SlotId StageBuilderState::getGlobalVariableSlot(Variables::Id variableId) {
    if (auto it = data->variableIdToSlotMap.find(variableId);
        it != data->variableIdToSlotMap.end()) {
        return it->second;
    }

    auto slotId =
        env->registerSlot(sbe::value::TypeTags::Nothing, 0, false /* owned */, slotIdGenerator);
    data->variableIdToSlotMap.emplace(variableId, slotId);
    return slotId;
}

/**
 * Callback function that logs a message and uasserts if it detects a corrupt index key. An index
 * key is considered corrupt if it has no corresponding Record.
 */
void indexKeyCorruptionCheckCallback(OperationContext* opCtx,
                                     sbe::value::SlotAccessor* snapshotIdAccessor,
                                     sbe::value::SlotAccessor* indexKeyAccessor,
                                     sbe::value::SlotAccessor* indexKeyPatternAccessor,
                                     const RecordId& rid,
                                     const NamespaceString& nss) {
    // Having a recordId but no record is only an issue when we are not ignoring prepare conflicts.
    if (opCtx->recoveryUnit()->getPrepareConflictBehavior() == PrepareConflictBehavior::kEnforce) {
        tassert(5113700, "Should have snapshot id accessor", snapshotIdAccessor);
        auto currentSnapshotId = opCtx->recoveryUnit()->getSnapshotId();
        auto [snapshotIdTag, snapshotIdVal] = snapshotIdAccessor->getViewOfValue();
        const auto msgSnapshotIdTag = snapshotIdTag;
        tassert(5113701,
                str::stream() << "SnapshotId is of wrong type: " << msgSnapshotIdTag,
                snapshotIdTag == sbe::value::TypeTags::NumberInt64);
        auto snapshotId = sbe::value::bitcastTo<uint64_t>(snapshotIdVal);

        // If we have a recordId but no corresponding record, this means that said record has been
        // deleted. This can occur during yield, in which case the snapshot id would be incremented.
        // If, on the other hand, the current snapshot id matches that of the recordId, this
        // indicates an error as no yield could have taken place.
        if (snapshotId == currentSnapshotId.toNumber()) {
            tassert(5113703, "Should have index key accessor", indexKeyAccessor);
            tassert(5113704, "Should have key pattern accessor", indexKeyPatternAccessor);

            auto [ksTag, ksVal] = indexKeyAccessor->getViewOfValue();
            auto [kpTag, kpVal] = indexKeyPatternAccessor->getViewOfValue();

            const auto msgKsTag = ksTag;
            tassert(5113706,
                    str::stream() << "KeyString is of wrong type: " << msgKsTag,
                    ksTag == sbe::value::TypeTags::ksValue);

            const auto msgKpTag = kpTag;
            tassert(5113707,
                    str::stream() << "Index key pattern is of wrong type: " << msgKpTag,
                    kpTag == sbe::value::TypeTags::bsonObject);

            auto keyString = sbe::value::getKeyStringView(ksVal);
            tassert(5113708, "KeyString does not exist", keyString);

            BSONObj bsonKeyPattern(sbe::value::bitcastTo<const char*>(kpVal));
            auto bsonKeyString = key_string::toBson(*keyString, Ordering::make(bsonKeyPattern));
            auto hydratedKey = IndexKeyEntry::rehydrateKey(bsonKeyPattern, bsonKeyString);

            HealthLogEntry entry;
            entry.setNss(nss);
            entry.setTimestamp(Date_t::now());
            entry.setSeverity(SeverityEnum::Error);
            entry.setScope(ScopeEnum::Index);
            entry.setOperation("Index scan");
            entry.setMsg("Erroneous index key found with reference to non-existent record id");

            BSONObjBuilder bob;
            bob.append("recordId", rid.toString());
            bob.append("indexKeyData", hydratedKey);
            bob.appendElements(getStackTrace().getBSONRepresentation());
            entry.setData(bob.obj());

            HealthLogInterface::get(opCtx)->log(entry);

            LOGV2_ERROR_OPTIONS(
                5113709,
                {logv2::UserAssertAfterLog(ErrorCodes::DataCorruptionDetected)},
                "Erroneous index key found with reference to non-existent record id. Consider "
                "dropping and then re-creating the index and then running the validate command "
                "on the collection.",
                logAttrs(nss),
                "recordId"_attr = rid,
                "indexKeyData"_attr = hydratedKey);
        }
    }
}

/**
 * Callback function that returns true if a given index key is valid, false otherwise. An index key
 * is valid if either the snapshot id of the underlying index scan matches the current snapshot id,
 * or that the index keys are still part of the underlying index.
 */
bool indexKeyConsistencyCheckCallback(OperationContext* opCtx,
                                      StringMap<const IndexCatalogEntry*>& entryMap,
                                      sbe::value::SlotAccessor* snapshotIdAccessor,
                                      sbe::value::SlotAccessor* indexIdentAccessor,
                                      sbe::value::SlotAccessor* indexKeyAccessor,
                                      const CollectionPtr& collection,
                                      const Record& nextRecord) {
    // The index consistency check is only performed when 'snapshotIdAccessor' is set.
    if (snapshotIdAccessor) {
        auto currentSnapshotId = opCtx->recoveryUnit()->getSnapshotId();
        auto [snapshotIdTag, snapshotIdVal] = snapshotIdAccessor->getViewOfValue();
        const auto msgSnapshotIdTag = snapshotIdTag;
        tassert(5290704,
                str::stream() << "SnapshotId is of wrong type: " << msgSnapshotIdTag,
                snapshotIdTag == sbe::value::TypeTags::NumberInt64);

        auto snapshotId = sbe::value::bitcastTo<uint64_t>(snapshotIdVal);
        if (currentSnapshotId.toNumber() != snapshotId) {
            tassert(5290707, "Should have index key accessor", indexKeyAccessor);
            tassert(5290714, "Should have index ident accessor", indexIdentAccessor);

            auto [identTag, identVal] = indexIdentAccessor->getViewOfValue();
            auto [ksTag, ksVal] = indexKeyAccessor->getViewOfValue();

            const auto msgIdentTag = identTag;
            tassert(5290708,
                    str::stream() << "Index name is of wrong type: " << msgIdentTag,
                    sbe::value::isString(identTag));

            const auto msgKsTag = ksTag;
            tassert(5290710,
                    str::stream() << "KeyString is of wrong type: " << msgKsTag,
                    ksTag == sbe::value::TypeTags::ksValue);

            auto keyString = sbe::value::getKeyStringView(ksVal);
            auto indexIdent = sbe::value::getStringView(identTag, identVal);
            tassert(5290712, "KeyString does not exist", keyString);

            auto it = entryMap.find(indexIdent);

            // If 'entryMap' doesn't contain an entry for 'indexIdent', create one.
            if (it == entryMap.end()) {
                auto indexCatalog = collection->getIndexCatalog();
                auto indexDesc = indexCatalog->findIndexByIdent(opCtx, indexIdent);
                auto entry = indexDesc ? indexDesc->getEntry() : nullptr;

                // Throw an error if we can't get the IndexDescriptor or the IndexCatalogEntry
                // (or if the index is dropped).
                uassert(ErrorCodes::QueryPlanKilled,
                        str::stream() << "query plan killed :: index dropped: " << indexIdent,
                        indexDesc && entry && !entry->isDropped());

                auto [newIt, _] = entryMap.emplace(indexIdent, entry);

                it = newIt;
            }

            auto entry = it->second;
            auto iam = entry->accessMethod()->asSortedData();
            tassert(5290709,
                    str::stream() << "Expected to find SortedDataIndexAccessMethod for index: "
                                  << indexIdent,
                    iam);

            auto& executionCtx = StorageExecutionContext::get(opCtx);
            auto keys = executionCtx.keys();
            SharedBufferFragmentBuilder pooledBuilder(
                key_string::HeapBuilder::kHeapAllocatorDefaultBytes);

            // There's no need to compute the prefixes of the indexed fields that cause the
            // index to be multikey when ensuring the keyData is still valid.
            KeyStringSet* multikeyMetadataKeys = nullptr;
            MultikeyPaths* multikeyPaths = nullptr;

            iam->getKeys(opCtx,
                         collection,
                         entry,
                         pooledBuilder,
                         nextRecord.data.toBson(),
                         InsertDeleteOptions::ConstraintEnforcementMode::kEnforceConstraints,
                         SortedDataIndexAccessMethod::GetKeysContext::kValidatingKeys,
                         keys.get(),
                         multikeyMetadataKeys,
                         multikeyPaths,
                         nextRecord.id);

            return keys->count(*keyString);
        }
    }

    return true;
}

std::unique_ptr<sbe::PlanStage> makeLoopJoinForFetch(std::unique_ptr<sbe::PlanStage> inputStage,
                                                     sbe::value::SlotId resultSlot,
                                                     sbe::value::SlotId recordIdSlot,
                                                     std::vector<std::string> fields,
                                                     sbe::value::SlotVector fieldSlots,
                                                     sbe::value::SlotId seekRecordIdSlot,
                                                     sbe::value::SlotId snapshotIdSlot,
                                                     sbe::value::SlotId indexIdentSlot,
                                                     sbe::value::SlotId indexKeySlot,
                                                     sbe::value::SlotId indexKeyPatternSlot,
                                                     const CollectionPtr& collToFetch,
                                                     PlanNodeId planNodeId,
                                                     sbe::value::SlotVector slotsToForward) {
    // It is assumed that we are generating a fetch loop join over the main collection. If we are
    // generating a fetch over a secondary collection, it is the responsibility of a parent node
    // in the QSN tree to indicate which collection we are fetching over.
    tassert(6355301, "Cannot fetch from a collection that doesn't exist", collToFetch);

    sbe::ScanCallbacks callbacks(indexKeyCorruptionCheckCallback, indexKeyConsistencyCheckCallback);

    // Scan the collection in the range [seekRecordIdSlot, Inf).
    auto scanStage = sbe::makeS<sbe::ScanStage>(collToFetch->uuid(),
                                                resultSlot,
                                                recordIdSlot,
                                                snapshotIdSlot,
                                                indexIdentSlot,
                                                indexKeySlot,
                                                indexKeyPatternSlot,
                                                boost::none,
                                                std::move(fields),
                                                std::move(fieldSlots),
                                                seekRecordIdSlot,
                                                boost::none /* minRecordIdSlot */,
                                                boost::none /* maxRecordIdSlot */,
                                                true /* forward */,
                                                nullptr,
                                                planNodeId,
                                                std::move(callbacks));

    // Get the recordIdSlot from the outer side (e.g., IXSCAN) and feed it to the inner side,
    // limiting the result set to 1 row.
    return sbe::makeS<sbe::LoopJoinStage>(
        std::move(inputStage),
        sbe::makeS<sbe::LimitSkipStage>(std::move(scanStage), 1, boost::none, planNodeId),
        std::move(slotsToForward),
        sbe::makeSV(
            seekRecordIdSlot, snapshotIdSlot, indexIdentSlot, indexKeySlot, indexKeyPatternSlot),
        nullptr,
        planNodeId);
}

sbe::value::SlotId StageBuilderState::registerInputParamSlot(
    MatchExpression::InputParamId paramId) {
    auto it = data->inputParamToSlotMap.find(paramId);
    if (it != data->inputParamToSlotMap.end()) {
        // This input parameter id has already been tied to a particular runtime environment slot.
        // Just return that slot to the caller. This can happen if a query planning optimization or
        // rewrite chose to clone one of the input expressions from the user's query.
        return it->second;
    }

    auto slotId =
        env->registerSlot(sbe::value::TypeTags::Nothing, 0, false /* owned */, slotIdGenerator);
    data->inputParamToSlotMap.emplace(paramId, slotId);
    return slotId;
}

boost::optional<sbe::value::SlotId> StageBuilderState::getTimeZoneDBSlot() {
    auto slotId = env->getSlotIfExists("timeZoneDB"_sd);

    if (!slotId) {
        return env->registerSlot(
            "timeZoneDB"_sd,
            sbe::value::TypeTags::timeZoneDB,
            sbe::value::bitcastFrom<const TimeZoneDatabase*>(getTimeZoneDatabase(opCtx)),
            false,
            slotIdGenerator);
    }

    return slotId;
}

boost::optional<sbe::value::SlotId> StageBuilderState::getCollatorSlot() {
    auto slotId = env->getSlotIfExists("collator"_sd);

    if (!slotId && data != nullptr) {
        if (auto coll = data->queryCollator.get()) {
            return env->registerSlot("collator"_sd,
                                     sbe::value::TypeTags::collator,
                                     sbe::value::bitcastFrom<const CollatorInterface*>(coll),
                                     false,
                                     slotIdGenerator);
        }
    }

    return slotId;
}

boost::optional<sbe::value::SlotId> StageBuilderState::getOplogTsSlot() {
    auto slotId = env->getSlotIfExists("oplogTs"_sd);

    if (!slotId) {
        return env->registerSlot(
            "oplogTs"_sd, sbe::value::TypeTags::Nothing, 0, false, slotIdGenerator);
    }

    return slotId;
}

boost::optional<sbe::value::SlotId> StageBuilderState::getBuiltinVarSlot(Variables::Id id) {
    if (id == Variables::kRootId || id == Variables::kRemoveId) {
        return boost::none;
    }

    auto it = Variables::kIdToBuiltinVarName.find(id);
    tassert(1234567, "Expected 'id' to be in map", it != Variables::kIdToBuiltinVarName.end());

    auto& name = it->second;
    auto slotId = env->getSlotIfExists(name);
    if (!slotId) {
        if (variables.hasValue(id)) {
            auto [tag, val] = sbe::value::makeValue(variables.getValue(id));
            return env->registerSlot(name, tag, val, true, slotIdGenerator);
        } else if (id == Variables::kSearchMetaId) {
            // Normally, $search is responsible for setting a value for SEARCH_META, in which case
            // we will bind the value to a slot above. However, in the event of a query that does
            // not use $search, but references SEARCH_META, we need to bind a value of 'missing' to
            // a slot so that the plan can run correctly.
            return env->registerSlot(
                name, sbe::value::TypeTags::Nothing, 0, false, slotIdGenerator);
        }
    }

    return slotId;
}

/**
 * Given a key pattern and an array of slots of equal size, builds a SlotTreeNode representing the
 * mapping between key pattern component and slot.
 *
 * Note that this will "short circuit" in cases where the index key pattern contains two components
 * where one is a subpath of the other. For example with the key pattern {a:1, a.b: 1}, the "a.b"
 * component will not be represented in the output tree. For the purpose of rehydrating index keys,
 * this is fine (and actually preferable).
 */
std::unique_ptr<SlotTreeNode> buildKeyPatternTree(const BSONObj& keyPattern,
                                                  const sbe::value::SlotVector& slots) {
    std::vector<StringData> paths;
    for (auto&& elem : keyPattern) {
        paths.emplace_back(elem.fieldNameStringData());
    }

    return buildPathTree<boost::optional<sbe::value::SlotId>>(
        paths, slots.begin(), slots.end(), BuildPathTreeMode::RemoveConflictingPaths);
}

/**
 * Given a root SlotTreeNode, this function will construct an SBE expression for producing a partial
 * object from an index key.
 *
 * Example: Given the key pattern {a.b: 1, x: 1, a.c: 1} and the index key {"": 1, "": 2, "": 3},
 * the SBE expression returned by this function would produce the object {a: {b: 1, c: 3}, x: 2}.
 */
std::unique_ptr<sbe::EExpression> buildNewObjExpr(const SlotTreeNode* kpTree) {
    sbe::EExpression::Vector args;

    for (auto&& node : kpTree->children) {
        auto& fieldName = node->name;

        args.emplace_back(makeConstant(fieldName));
        if (node->value) {
            args.emplace_back(makeVariable(*node->value));
        } else {
            // The reason this is in an else branch is that in the case where we have an index key
            // like {a.b: ..., a: ...}, we've already made the logic for reconstructing the 'a'
            // portion, so the 'a.b' subtree can be skipped.
            args.push_back(buildNewObjExpr(node.get()));
        }
    }

    return sbe::makeE<sbe::EFunction>("newObj", std::move(args));
}

/**
 * Given a stage, and index key pattern a corresponding array of slot IDs, this function
 * add a ProjectStage to the tree which rehydrates the index key and stores the result in
 * 'resultSlot.'
 */
std::unique_ptr<sbe::PlanStage> rehydrateIndexKey(std::unique_ptr<sbe::PlanStage> stage,
                                                  const BSONObj& indexKeyPattern,
                                                  PlanNodeId nodeId,
                                                  const sbe::value::SlotVector& indexKeySlots,
                                                  sbe::value::SlotId resultSlot) {
    auto kpTree = buildKeyPatternTree(indexKeyPattern, indexKeySlots);
    auto keyExpr = buildNewObjExpr(kpTree.get());

    return sbe::makeProjectStage(std::move(stage), nodeId, resultSlot, std::move(keyExpr));
}

namespace {
struct GetProjectionNodesData {
    projection_ast::ProjectType projectType = projection_ast::ProjectType::kInclusion;
    std::vector<std::string> paths;
    std::vector<ProjectionNode> nodes;
};
using GetProjectionNodesContext =
    projection_ast::PathTrackingVisitorContext<GetProjectionNodesData>;

class GetProjectionNodesVisitor final : public projection_ast::ProjectionASTConstVisitor {
public:
    explicit GetProjectionNodesVisitor(GetProjectionNodesContext* context) : _context{context} {}

    void visit(const projection_ast::BooleanConstantASTNode* node) final {
        bool isInclusion = _context->data().projectType == projection_ast::ProjectType::kInclusion;
        auto path = getCurrentPath();

        // For inclusion projections, if we encounter "{_id: 0}" we ignore it. Likewise, for
        // exclusion projections, if we encounter "{_id: 1}" we ignore it. ("_id" is the only
        // field that gets special treatment by the projection parser, so it's the only field
        // where this check is necessary.)
        if (isInclusion != node->value() && path == "_id") {
            return;
        }

        _context->data().paths.emplace_back(std::move(path));
        _context->data().nodes.emplace_back(node);
    }
    void visit(const projection_ast::ExpressionASTNode* node) final {
        _context->data().paths.emplace_back(getCurrentPath());
        _context->data().nodes.emplace_back(node);
    }
    void visit(const projection_ast::ProjectionSliceASTNode* node) final {
        _context->data().paths.emplace_back(getCurrentPath());
        _context->data().nodes.emplace_back(node);
    }
    void visit(const projection_ast::ProjectionPositionalASTNode* node) final {
        tasserted(7580705, "Positional projections are not supported in SBE");
    }
    void visit(const projection_ast::ProjectionElemMatchASTNode* node) final {
        tasserted(7580706, "ElemMatch projections are not supported in SBE");
    }
    void visit(const projection_ast::ProjectionPathASTNode* node) final {}
    void visit(const projection_ast::MatchExpressionASTNode* node) final {}

private:
    std::string getCurrentPath() {
        return _context->fullPath().fullPath();
    }

    GetProjectionNodesContext* _context;
};
}  // namespace

std::pair<std::vector<std::string>, std::vector<ProjectionNode>> getProjectionNodes(
    const projection_ast::Projection& projection) {
    GetProjectionNodesContext ctx{{projection.type(), {}, {}}};
    GetProjectionNodesVisitor visitor(&ctx);

    projection_ast::PathTrackingConstWalker<GetProjectionNodesData> walker{&ctx, {}, {&visitor}};

    tree_walker::walk<true, projection_ast::ASTNode>(projection.root(), &walker);

    return {std::move(ctx.data().paths), std::move(ctx.data().nodes)};
}

std::pair<std::unique_ptr<sbe::PlanStage>, sbe::value::SlotVector> projectFieldsToSlots(
    std::unique_ptr<sbe::PlanStage> stage,
    const std::vector<std::string>& fields,
    sbe::value::SlotId resultSlot,
    PlanNodeId nodeId,
    sbe::value::SlotIdGenerator* slotIdGenerator,
    StageBuilderState& state,
    const PlanStageSlots* slots) {
    // 'outputSlots' will match the order of 'fields'. Bail out early if 'fields' is empty.
    auto outputSlots = sbe::makeSV();
    if (fields.empty()) {
        return {std::move(stage), std::move(outputSlots)};
    }

    // Handle the case where 'fields' contains only top-level fields.
    const bool topLevelFieldsOnly = std::all_of(
        fields.begin(), fields.end(), [](auto&& s) { return s.find('.') == std::string::npos; });
    if (topLevelFieldsOnly) {
        sbe::SlotExprPairVector projects;
        for (size_t i = 0; i < fields.size(); ++i) {
            auto name = std::make_pair(PlanStageSlots::kField, StringData(fields[i]));
            auto fieldSlot = slots != nullptr ? slots->getIfExists(name) : boost::none;
            if (fieldSlot) {
                outputSlots.emplace_back(*fieldSlot);
            } else {
                auto slot = slotIdGenerator->generate();
                auto getFieldExpr =
                    makeFunction("getField"_sd, makeVariable(resultSlot), makeConstant(fields[i]));
                outputSlots.emplace_back(slot);
                projects.emplace_back(slot, std::move(getFieldExpr));
            }
        }
        if (!projects.empty()) {
            stage = sbe::makeS<sbe::ProjectStage>(std::move(stage), std::move(projects), nodeId);
        }

        return {std::move(stage), std::move(outputSlots)};
    }

    // Handle the case where 'fields' contains at least one dotted path. We begin by creating a
    // path tree from 'fields'.
    using Node = PathTreeNode<EvalExpr>;
    auto treeRoot = buildPathTree<EvalExpr>(fields, BuildPathTreeMode::AllowConflictingPaths);

    std::vector<Node*> fieldNodes;
    for (const auto& field : fields) {
        auto fieldRef = sbe::MatchPath{field};
        fieldNodes.emplace_back(treeRoot->findNode(fieldRef));
    }

    auto fieldNodesSet = absl::flat_hash_set<Node*>{fieldNodes.begin(), fieldNodes.end()};

    std::vector<Node*> roots;
    treeRoot->value = resultSlot;
    roots.emplace_back(treeRoot.get());

    // If 'slots' is not null, then we perform a DFS traversal over the path tree to get it set up.
    if (slots != nullptr) {
        auto hasNodesToVisit = [&](const Node::ChildrenVector& v) {
            return std::any_of(v.begin(), v.end(), [](auto&& c) { return !c->value; });
        };
        auto preVisit = [&](Node* node, const std::string& path) {
            auto name = std::make_pair(PlanStageSlots::kField, StringData(path));
            // Look for a kField slot that corresponds to node's path.
            if (auto slot = slots->getIfExists(name); slot) {
                // We found a kField slot. Assign it to 'node->value' and mark 'node' as "visited",
                // and add 'node' to 'roots'.
                node->value = *slot;
                roots.emplace_back(node);
            }
        };
        auto postVisit = [&](Node* node) {
            // When 'node' hasn't been visited and it's not in 'fieldNodesSet' and when all of
            // node's children have already been visited, mark 'node' as having been "visited".
            // (The specific value we assign to 'node->value' doesn't actually matter.)
            if (!node->value && !fieldNodesSet.count(node) && !hasNodesToVisit(node->children)) {
                node->value = sbe::value::SlotId{-1};
            }
        };
        visitPathTreeNodes(treeRoot.get(), preVisit, postVisit);
    }

    std::vector<sbe::SlotExprPairVector> stackOfProjects;
    using DfsState = std::vector<std::pair<Node*, size_t>>;
    size_t depth = 0;

    for (auto&& root : roots) {
        // For each node in 'roots' we perform a DFS traversal, taking care to avoid visiting nodes
        // that are marked as having been "visited" already during the previous phase.
        visitPathTreeNodes(
            root,
            [&](Node* node, const DfsState& dfs) {
                // If node->value is initialized, that means that 'node' and its descendants
                // have already been visited.
                if (node->value) {
                    return false;
                }
                // visitRootNode is false, so we should be guaranteed that that there are at least
                // two entries in the DfsState: an entry for 'node' and an entry for node's parent.
                tassert(7182002, "Expected DfsState to have at least 2 entries", dfs.size() >= 2);

                auto parent = dfs[dfs.size() - 2].first;
                auto getFieldExpr = makeFunction(
                    "getField"_sd,
                    parent->value.hasSlot() ? makeVariable(*parent->value.getSlot())
                                            : parent->value.extractExpr(state.slotVarMap, state),
                    makeConstant(node->name));

                auto hasOneChildToVisit = [&] {
                    size_t count = 0;
                    auto it = node->children.begin();
                    for (; it != node->children.end() && count <= 1; ++it) {
                        count += !(*it)->value;
                    }
                    return count == 1;
                };

                if (!fieldNodesSet.count(node) && hasOneChildToVisit()) {
                    // If 'fieldNodesSet.count(node)' is false and 'node' doesn't have multiple
                    // children that need to be visited, then we don't need to project value to
                    // a slot. Store 'getExprvalue' into 'node->value' and return.
                    node->value = std::move(getFieldExpr);
                    return true;
                }

                // We need to project 'getFieldExpr' to a slot.
                auto slot = slotIdGenerator->generate();
                node->value = slot;
                // Grow 'stackOfProjects' if needed so that 'stackOfProjects[depth]' is valid.
                if (depth >= stackOfProjects.size()) {
                    stackOfProjects.resize(depth + 1);
                }
                // Add the projection to the appropriate level of 'stackOfProjects'.
                auto& projects = stackOfProjects[depth];
                projects.emplace_back(slot, std::move(getFieldExpr));
                // Increment the depth while we visit node's descendents.
                ++depth;

                return true;
            },
            [&](Node* node) {
                // If 'node->value' holds a slot, that means the previsit phase incremented 'depth'.
                // Now that we are done visiting node's descendents, we decrement 'depth'.
                if (node->value.hasSlot()) {
                    --depth;
                }
            });
    }

    // Generate a ProjectStage for each level of 'stackOfProjects'.
    for (auto&& projects : stackOfProjects) {
        if (!projects.empty()) {
            stage = sbe::makeS<sbe::ProjectStage>(std::move(stage), std::move(projects), nodeId);
        }
    }

    for (auto* node : fieldNodes) {
        outputSlots.emplace_back(*node->value.getSlot());
    }

    return {std::move(stage), std::move(outputSlots)};
}

}  // namespace mongo::stage_builder
