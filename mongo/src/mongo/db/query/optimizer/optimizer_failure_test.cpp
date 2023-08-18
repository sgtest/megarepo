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

#include <string>
#include <utility>

#include <absl/container/node_hash_map.h>
#include <boost/none.hpp>

#include "mongo/base/string_data.h"
#include "mongo/db/query/cost_model/cost_model_gen.h"
#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/optimizer/comparison_op.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/metadata.h"
#include "mongo/db/query/optimizer/metadata_factory.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/opt_phase_manager.h"
#include "mongo/db/query/optimizer/props.h"
#include "mongo/db/query/optimizer/rewrites/const_eval.h"
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"
#include "mongo/db/query/optimizer/utils/unit_test_abt_literals.h"
#include "mongo/db/query/optimizer/utils/unit_test_utils.h"
#include "mongo/db/query/optimizer/utils/utils.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/death_test.h"
#include "mongo/util/assert_util.h"


namespace mongo::optimizer {
namespace {
using namespace unit_test_abt_literals;

// Default selectivity of predicates used by HintedCE to force certain plans.
constexpr double kDefaultSelectivity = 0.1;

DEATH_TEST_REGEX(Optimizer, HitIterationLimitInrunStructuralPhases, "Tripwire assertion.*6808700") {
    auto prefixId = PrefixId::createForTests();

    ABT scanNode = make<ScanNode>("scanProjection", "testColl");
    ABT evalNode = make<EvaluationNode>("evalProj1", Constant::int64(5), std::move(scanNode));


    auto phaseManager =
        makePhaseManager({OptPhase::PathFuse, OptPhase::ConstEvalPre},
                         prefixId,
                         {{{"test1", createScanDef({}, {})}, {"test2", createScanDef({}, {})}}},
                         boost::none /*costModel*/,
                         DebugInfo(true, DebugInfo::kDefaultDebugLevelForTests, 0));

    ASSERT_THROWS_CODE(phaseManager.optimize(evalNode), DBException, 6808700);
}

DEATH_TEST_REGEX(Optimizer,
                 LogicalWriterFailedToRewriteFixPointMemSubPhase,
                 "Tripwire assertion.*6808702") {
    using namespace properties;
    auto prefixId = PrefixId::createForTests();

    ABT scanNode = make<ScanNode>("ptest", "test");
    ABT collationNode = make<CollationNode>(
        CollationRequirement({{"ptest", CollationOp::Ascending}}), std::move(scanNode));
    ABT evalNode =
        make<EvaluationNode>("P1",
                             make<EvalPath>(make<PathIdentity>(), make<Variable>("ptest")),
                             std::move(collationNode));
    ABT filterNode = make<FilterNode>(make<EvalFilter>(make<PathIdentity>(), make<Variable>("P1")),
                                      std::move(evalNode));

    ABT rootNode = make<RootNode>(properties::ProjectionRequirement{{}}, std::move(filterNode));


    auto phaseManager = makePhaseManager({OptPhase::MemoSubstitutionPhase},
                                         prefixId,
                                         {{{"test", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo(true, DebugInfo::kDefaultDebugLevelForTests, 0));

    ASSERT_THROWS_CODE(phaseManager.optimize(rootNode), DBException, 6808702);
}

DEATH_TEST_REGEX(Optimizer,
                 LogicalWriterFailedToRewriteFixPointMemExpPhase,
                 "Tripwire assertion.*6808702") {
    using namespace properties;
    auto prefixId = PrefixId::createForTests();

    ABT scanNode = make<ScanNode>("ptest", "test");
    ABT collationNode = make<CollationNode>(
        CollationRequirement({{"ptest", CollationOp::Ascending}}), std::move(scanNode));
    ABT evalNode =
        make<EvaluationNode>("P1",
                             make<EvalPath>(make<PathIdentity>(), make<Variable>("ptest")),
                             std::move(collationNode));
    ABT filterNode = make<FilterNode>(make<EvalFilter>(make<PathIdentity>(), make<Variable>("P1")),
                                      std::move(evalNode));

    ABT rootNode = make<RootNode>(properties::ProjectionRequirement{{}}, std::move(filterNode));


    auto phaseManager = makePhaseManager({OptPhase::MemoExplorationPhase},
                                         prefixId,
                                         {{{"test", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo(true, DebugInfo::kDefaultDebugLevelForTests, 0));

    ASSERT_THROWS_CODE(phaseManager.optimize(rootNode), DBException, 6808702);
}

DEATH_TEST_REGEX(Optimizer, BadGroupID, "Tripwire assertion.*6808704") {
    using namespace properties;
    auto prefixId = PrefixId::createForTests();

    ABT scanNode = make<ScanNode>("ptest", "test");
    ABT collationNode = make<CollationNode>(
        CollationRequirement({{"ptest", CollationOp::Ascending}}), std::move(scanNode));
    ABT evalNode =
        make<EvaluationNode>("P1",
                             make<EvalPath>(make<PathIdentity>(), make<Variable>("ptest")),
                             std::move(collationNode));
    ABT filterNode = make<FilterNode>(make<EvalFilter>(make<PathIdentity>(), make<Variable>("P1")),
                                      std::move(evalNode));

    ABT rootNode = make<RootNode>(properties::ProjectionRequirement{{}}, std::move(filterNode));


    auto phaseManager = makePhaseManager({OptPhase::MemoImplementationPhase},
                                         prefixId,
                                         {{{"test", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo(true, DebugInfo::kDefaultDebugLevelForTests, 0));

    ASSERT_THROWS_CODE(phaseManager.optimize(rootNode), DBException, 6808704);
}

DEATH_TEST_REGEX(Optimizer, EnvHasFreeVariables, "Tripwire assertion.*6808711") {
    using namespace properties;
    auto prefixId = PrefixId::createForTests();

    auto rootNode = NodeBuilder{}
                        .root("p1", "p2")
                        .eval("p2", _evalp(_id(), "p3"_var))
                        .finish(_scan("p1", "test"));

    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"test", createScanDef({}, {})}}},
        boost::none /*costModel*/,
        DebugInfo(true, DebugInfo::kDefaultDebugLevelForTests, DebugInfo::kIterationLimitForTests));

    ASSERT_THROWS_CODE(phaseManager.optimize(rootNode), DBException, 6808711);
}

DEATH_TEST_REGEX(Optimizer, RootHasNonexistentProjection, "Tripwire assertion.*7088003") {
    using namespace properties;
    auto prefixId = PrefixId::createForTests();

    auto rootNode = NodeBuilder{}
                        .root("p1", "p2", "p3")
                        .eval("p2", _evalp(_id(), "p1"_var))
                        .finish(_scan("p1", "test"));

    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"test", createScanDef({}, {})}}},
        boost::none /*costModel*/,
        DebugInfo(true, DebugInfo::kDefaultDebugLevelForTests, DebugInfo::kIterationLimitForTests));

    ASSERT_THROWS_CODE(phaseManager.optimize(rootNode), DBException, 7088003);
}

DEATH_TEST_REGEX(Optimizer, FailedToRetrieveRID, "Tripwire assertion.*6808705") {
    using namespace properties;
    auto prefixId = PrefixId::createForTests();

    ABT scanNode = make<ScanNode>("root", "c1");

    ABT projectionANode = make<EvaluationNode>(
        "pa",
        make<EvalPath>(make<PathGet>("a", make<PathIdentity>()), make<Variable>("root")),
        std::move(scanNode));

    ABT filterANode =
        make<FilterNode>(make<EvalFilter>(make<PathCompare>(Operations::Gt, Constant::int64(0)),
                                          make<Variable>("pa")),
                         std::move(projectionANode));

    ABT projectionBNode = make<EvaluationNode>(
        "pb",
        make<EvalPath>(make<PathGet>("b", make<PathIdentity>()), make<Variable>("root")),
        std::move(filterANode));

    ABT filterBNode =
        make<FilterNode>(make<EvalFilter>(make<PathCompare>(Operations::Gt, Constant::int64(1)),
                                          make<Variable>("pb")),
                         std::move(projectionBNode));

    ABT groupByNode = make<GroupByNode>(ProjectionNameVector{"pa"},
                                        ProjectionNameVector{"pc"},
                                        makeSeq(make<Variable>("pb")),
                                        std::move(filterBNode));

    ABT rootNode =
        make<RootNode>(ProjectionRequirement{ProjectionNameVector{"pc"}}, std::move(groupByNode));

    auto phaseManager = makePhaseManagerRequireRID(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1",
           createScanDef(
               {},
               {{"index1",
                 IndexDefinition{
                     {{makeNonMultikeyIndexPath("a"), CollationOp::Ascending}},
                     false /*isMultiKey*/,
                     {DistributionType::HashPartitioning, makeSeq(makeNonMultikeyIndexPath("a"))},
                     {}}}},
               ConstEval::constFold,
               {DistributionType::HashPartitioning, makeSeq(makeNonMultikeyIndexPath("b"))})}},
         5 /*numberOfPartitions*/},
        DebugInfo(true, DebugInfo::kDefaultDebugLevelForTests, DebugInfo::kIterationLimitForTests));

    ASSERT_THROWS_CODE(phaseManager.optimize(rootNode), DBException, 6808705);
}

}  // namespace
}  // namespace mongo::optimizer
