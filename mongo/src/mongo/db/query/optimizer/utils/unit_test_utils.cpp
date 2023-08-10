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

#include "mongo/db/query/optimizer/utils/unit_test_utils.h"

#include <absl/container/node_hash_map.h>
#include <cstddef>
#include <fstream>  // IWYU pragma: keep
#include <iostream>
#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/db/pipeline/abt/utils.h"
#include "mongo/db/query/ce/heuristic_estimator.h"
#include "mongo/db/query/ce/hinted_estimator.h"
#include "mongo/db/query/cost_model/cost_estimator_impl.h"
#include "mongo/db/query/cost_model/cost_model_manager.h"
#include "mongo/db/query/optimizer/cascades/memo.h"
#include "mongo/db/query/optimizer/explain.h"
#include "mongo/db/query/optimizer/metadata.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/rewrites/const_eval.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/utils/const_fold_interface.h"


namespace mongo::optimizer {

static constexpr bool kDebugAsserts = false;


void maybePrintABT(const ABT& abt) {
    // Always print using the supported versions to make sure we don't crash.
    const std::string strV1 = ExplainGenerator::explain(abt);
    const std::string strV2 = ExplainGenerator::explainV2(abt);
    const std::string strV2Compact = ExplainGenerator::explainV2Compact(abt);
    const std::string strBSON = ExplainGenerator::explainBSONStr(abt);

    if constexpr (kDebugAsserts) {
        std::cout << "V1: " << strV1 << "\n";
        std::cout << "V2: " << strV2 << "\n";
        std::cout << "V2Compact: " << strV2Compact << "\n";
        std::cout << "BSON: " << strBSON << "\n";
    }
}

std::string getPropsStrForExplain(const OptPhaseManager& phaseManager) {
    return ExplainGenerator::explainV2(
        make<MemoPhysicalDelegatorNode>(phaseManager.getPhysicalNodeId()),
        true /*displayPhysicalProperties*/,
        &phaseManager.getMemo());
}


ABT makeIndexPath(FieldPathType fieldPath, bool isMultiKey) {
    ABT result = make<PathIdentity>();

    for (size_t i = fieldPath.size(); i-- > 0;) {
        if (isMultiKey) {
            result = make<PathTraverse>(PathTraverse::kSingleLevel, std::move(result));
        }
        result = make<PathGet>(std::move(fieldPath.at(i)), std::move(result));
    }

    return result;
}

ABT makeIndexPath(FieldNameType fieldName) {
    return makeIndexPath(FieldPathType{std::move(fieldName)});
}

ABT makeNonMultikeyIndexPath(FieldNameType fieldName) {
    return makeIndexPath(FieldPathType{std::move(fieldName)}, false /*isMultiKey*/);
}

IndexDefinition makeIndexDefinition(FieldNameType fieldName, CollationOp op, bool isMultiKey) {
    IndexCollationSpec idxCollSpec{
        IndexCollationEntry((isMultiKey ? makeIndexPath(std::move(fieldName))
                                        : makeNonMultikeyIndexPath(std::move(fieldName))),
                            op)};
    return IndexDefinition{std::move(idxCollSpec), isMultiKey};
}

IndexDefinition makeCompositeIndexDefinition(std::vector<TestIndexField> indexFields,
                                             bool isMultiKey) {
    IndexCollationSpec idxCollSpec;
    for (auto& idxField : indexFields) {
        idxCollSpec.emplace_back((idxField.isMultiKey
                                      ? makeIndexPath(std::move(idxField.fieldName))
                                      : makeNonMultikeyIndexPath(std::move(idxField.fieldName))),
                                 idxField.op);
    }
    return IndexDefinition{std::move(idxCollSpec), isMultiKey};
}

std::unique_ptr<CardinalityEstimator> makeHeuristicCE() {
    return std::make_unique<ce::HeuristicEstimator>();
}

std::unique_ptr<CardinalityEstimator> makeHintedCE(
    ce::PartialSchemaSelHints hints, ce::PartialSchemaIntervalSelHints intervalHints) {
    return std::make_unique<ce::HintedEstimator>(std::move(hints), std::move(intervalHints));
}

cost_model::CostModelCoefficients getTestCostModel() {
    return cost_model::CostModelManager::getDefaultCoefficients();
}

std::unique_ptr<CostEstimator> makeCostEstimator() {
    return makeCostEstimator(getTestCostModel());
}

std::unique_ptr<CostEstimator> makeCostEstimator(
    const cost_model::CostModelCoefficients& costModel) {
    return std::make_unique<cost_model::CostEstimatorImpl>(costModel);
}


OptPhaseManager makePhaseManager(
    OptPhaseManager::PhaseSet phaseSet,
    PrefixId& prefixId,
    Metadata metadata,
    const boost::optional<cost_model::CostModelCoefficients>& costModel,
    DebugInfo debugInfo,
    QueryHints queryHints) {
    return OptPhaseManager{std::move(phaseSet),
                           prefixId,
                           false /*requireRID*/,
                           std::move(metadata),
                           makeHeuristicCE(),  // primary CE
                           makeHeuristicCE(),  // substitution phase CE, same as primary
                           makeCostEstimator(costModel ? *costModel : getTestCostModel()),
                           defaultConvertPathToInterval,
                           ConstEval::constFold,
                           std::move(debugInfo),
                           std::move(queryHints)};
}

OptPhaseManager makePhaseManager(
    OptPhaseManager::PhaseSet phaseSet,
    PrefixId& prefixId,
    Metadata metadata,
    std::unique_ptr<CardinalityEstimator> ce,
    const boost::optional<cost_model::CostModelCoefficients>& costModel,
    DebugInfo debugInfo,
    QueryHints queryHints) {
    return OptPhaseManager{std::move(phaseSet),
                           prefixId,
                           false /*requireRID*/,
                           std::move(metadata),
                           std::move(ce),      // primary CE
                           makeHeuristicCE(),  // substitution phase CE
                           makeCostEstimator(costModel ? *costModel : getTestCostModel()),
                           defaultConvertPathToInterval,
                           ConstEval::constFold,
                           std::move(debugInfo),
                           std::move(queryHints)};
}


OptPhaseManager makePhaseManagerRequireRID(OptPhaseManager::PhaseSet phaseSet,
                                           PrefixId& prefixId,
                                           Metadata metadata,
                                           DebugInfo debugInfo,
                                           QueryHints queryHints) {
    return OptPhaseManager{std::move(phaseSet),
                           prefixId,
                           true /*requireRID*/,
                           std::move(metadata),
                           makeHeuristicCE(),  // primary CE
                           makeHeuristicCE(),  // substitution phase CE, same as primary
                           makeCostEstimator(),
                           defaultConvertPathToInterval,
                           ConstEval::constFold,
                           std::move(debugInfo),
                           std::move(queryHints)};
}

bool planComparator(const PlanAndProps& e1, const PlanAndProps& e2) {
    // Sort plans by estimated cost. If costs are equal, sort lexicographically by plan explain.
    // This allows us to break ties if costs are equal.
    const auto c1 = e1.getRootAnnotation()._cost;
    const auto c2 = e2.getRootAnnotation()._cost;
    if (c1 < c2) {
        return true;
    }
    if (c2 < c1) {
        return false;
    }

    const auto explain1 = ExplainGenerator::explainV2(e1._node);
    const auto explain2 = ExplainGenerator::explainV2(e2._node);
    return explain1 < explain2;
}
}  // namespace mongo::optimizer
