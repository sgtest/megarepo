/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include "mongo/db/query/optimizer/metadata_factory.h"
#include "mongo/db/query/optimizer/rewrites/const_eval.h"
#include "mongo/db/query/optimizer/utils/unit_test_abt_literals.h"
#include "mongo/db/query/optimizer/utils/unit_test_utils.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/unittest/inline_auto_update.h"

namespace mongo::optimizer {
namespace {

using namespace unit_test_abt_literals;
TEST(PhysRewriter, RemoveOrphansEnforcerMultipleCollections) {
    // Hypothetical MQL which could generate this ABT:
    //   db.c1.aggregate([{$unionWith: {coll: "c2", pipeline: [{$match: {}}]}}])
    ABT rootNode = NodeBuilder{}
                       .root("root")
                       .un(ProjectionNameVector{"root"},
                           {NodeHolder{NodeBuilder{}.finish(_scan("root", "c2"))}})
                       .finish(_scan("root", "c1"));

    auto prefixId = PrefixId::createForTests();

    auto scanDef1 =
        createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                      UUID::gen(),
                      ScanDefOptions{},
                      IndexDefinitions{},
                      MultikeynessTrie{},
                      ConstEval::constFold,
                      // Sharded on {a: 1}
                      DistributionAndPaths{DistributionType::Centralized},
                      true /*exists*/,
                      boost::none /*ce*/,
                      ShardingMetadata({{_get("a", _id())._n, CollationOp::Ascending}}, true));

    auto scanDef2 =
        createScanDef(DatabaseNameUtil::deserialize(boost::none, "test2"),
                      UUID::gen(),
                      ScanDefOptions{},
                      IndexDefinitions{},
                      MultikeynessTrie{},
                      ConstEval::constFold,
                      // Sharded on {b: 1}
                      DistributionAndPaths{DistributionType::Centralized},
                      true /*exists*/,
                      boost::none /*ce*/,
                      ShardingMetadata({{_get("b", _id())._n, CollationOp::Ascending}}, true));

    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1", scanDef1}, {"c2", scanDef2}}},
        boost::none /*costModel*/,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});

    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    // Note the evaluation node to project the shard key and filter node to perform shard filtering.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Union [{root}]\n"
        "|   Filter []\n"
        "|   |   FunctionCall [shardFilter]\n"
        "|   |   Variable [shardKey_3]\n"
        "|   PhysicalScan [{'<root>': root, 'b': shardKey_3}, c2]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   Variable [shardKey_1]\n"
        "PhysicalScan [{'<root>': root, 'a': shardKey_1}, c1]\n",
        optimized);
}

// Common setup function to construct optimizer metadata with no indexes and invoke optimization
// given a physical plan and sharding metadata. Returns the optimized plan.
static ABT optimizeABTWithShardingMetadataNoIndexes(ABT& rootNode,
                                                    ShardingMetadata shardingMetadata) {
    auto prefixId = PrefixId::createForTests();

    // Shard keys guarentee non-multikeyness of all their components. In some cases, there might not
    // be an index backing the shard key. So to make use of the multikeyness data of the shard key,
    // we populate the multikeyness trie.
    MultikeynessTrie trie;
    for (auto&& comp : shardingMetadata.shardKey()) {
        trie.add(comp._path);
    }

    auto scanDef = createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                                 UUID::gen(),
                                 ScanDefOptions{},
                                 IndexDefinitions{},
                                 std::move(trie),
                                 ConstEval::constFold,
                                 DistributionAndPaths{DistributionType::Centralized},
                                 true /*exists*/,
                                 boost::none /*ce*/,
                                 shardingMetadata);

    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1", scanDef}}},
        boost::none /*costModel*/,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});

    ABT optimized = rootNode;
    phaseManager.optimize(optimized);
    return optimized;
};

TEST(PhysRewriter, ScanNodeRemoveOrphansImplementerBasic) {
    ABT rootNode = NodeBuilder{}.root("root").finish(_scan("root", "c1"));

    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending},
                         {_get("b", _id())._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(rootNode, sm);
    // The fields of the shard key are extracted in the physical scan.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [shardKey_3]\n"
        "|   Variable [shardKey_2]\n"
        "PhysicalScan [{'<root>': root, 'a': shardKey_2, 'b': shardKey_3}, c1]\n",
        optimized);
}

TEST(PhysRewriter, ScanNodeRemoveOrphansImplementerDottedBasic) {
    ABT rootNode = NodeBuilder{}.root("root").finish(_scan("root", "c1"));
    ShardingMetadata sm({{_get("a", _get("b", _id()))._n, CollationOp::Ascending},
                         {_get("c", _get("d", _id()))._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(rootNode, sm);
    // The top-level of each field's path is pushed down into the physical scan, and the rest of
    // the path is obtained with an evaluation node.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [shardKey_5]\n"
        "|   Variable [shardKey_4]\n"
        "Evaluation [{shardKey_5}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [shardKey_3]\n"
        "|   PathGet [d]\n"
        "|   PathIdentity []\n"
        "Evaluation [{shardKey_4}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [shardKey_2]\n"
        "|   PathGet [b]\n"
        "|   PathIdentity []\n"
        "PhysicalScan [{'<root>': root, 'a': shardKey_2, 'c': shardKey_3}, c1]\n",
        optimized);
}

TEST(PhysRewriter, ScanNodeRemoveOrphansImplementerDottedSharedPrefix) {
    ABT rootNode = NodeBuilder{}.root("root").finish(_scan("root", "c1"));
    ShardingMetadata sm({{_get("a", _get("b", _id()))._n, CollationOp::Ascending},
                         {_get("a", _get("c", _id()))._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(rootNode, sm);
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [shardKey_4]\n"
        "|   Variable [shardKey_3]\n"
        "Evaluation [{shardKey_4}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [shardKey_2]\n"
        "|   PathGet [c]\n"
        "|   PathIdentity []\n"
        "Evaluation [{shardKey_3}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [shardKey_2]\n"
        "|   PathGet [b]\n"
        "|   PathIdentity []\n"
        "PhysicalScan [{'<root>': root, 'a': shardKey_2}, c1]\n",
        optimized);
}

TEST(PhysRewriter, ScanNodeRemoveOrphansImplementerDottedDoubleSharedPrefix) {
    ABT rootNode = NodeBuilder{}.root("root").finish(_scan("root", "c1"));
    // Sharded on {a.b.c: 1, a.b.d:1}
    ShardingMetadata sm({{_get("a", _get("b", _get("c", _id())))._n, CollationOp::Ascending},
                         {_get("a", _get("b", _get("d", _id())))._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(rootNode, sm);
    // Only the top level of shared paths is currently pushed down into the physical scan.
    // TODO SERVER-79435: Factor out a shared path to the greatest extent possible (e.g. 'a.b'
    // rather than just 'a').
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [shardKey_4]\n"
        "|   Variable [shardKey_3]\n"
        "Evaluation [{shardKey_4}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [shardKey_2]\n"
        "|   PathGet [b]\n"
        "|   PathGet [d]\n"
        "|   PathIdentity []\n"
        "Evaluation [{shardKey_3}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [shardKey_2]\n"
        "|   PathGet [b]\n"
        "|   PathGet [c]\n"
        "|   PathIdentity []\n"
        "PhysicalScan [{'<root>': root, 'a': shardKey_2}, c1]\n",
        optimized);
}

TEST(PhysRewriter, ScanNodeRemoveOrphansImplementerSeekTargetBasic) {
    using namespace properties;

    ABT scanNode = make<ScanNode>("root", "c1");

    ABT filterNode = make<FilterNode>(
        make<EvalFilter>(make<PathGet>("a",
                                       make<PathTraverse>(
                                           PathTraverse::kSingleLevel,
                                           make<PathCompare>(Operations::Eq, Constant::int64(1)))),
                         make<Variable>("root")),
        std::move(scanNode));

    ABT rootNode =
        make<RootNode>(ProjectionRequirement{ProjectionNameVector{"root"}}, std::move(filterNode));

    ShardingMetadata sm({{_get("b", _id())._n, CollationOp::Ascending}}, true);

    auto scanDef = createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                                 UUID::gen(),
                                 {},
                                 {{"index1", makeIndexDefinition("a", CollationOp::Ascending)}},
                                 MultikeynessTrie{},
                                 ConstEval::constFold,
                                 DistributionAndPaths{DistributionType::Centralized},
                                 true,
                                 boost::none,
                                 sm);
    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1", scanDef}}},
        /*costModel*/ boost::none,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});
    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    // Note: we don't assert on the explain of the plan verbatim because there is non-determinism in
    // the order of rewrites that are applied which causes non-determinism in the projection names
    // that are generated.

    // Assert plan structure contains NLJ with in index scan on left and shard filter + seek on the
    // right.
    const BSONObj explainRoot = ExplainGenerator::explainBSONObj(optimized);
    ASSERT_BSON_PATH("\"NestedLoopJoin\"", explainRoot, "child.nodeType");
    ASSERT_BSON_PATH("\"IndexScan\"", explainRoot, "child.leftChild.nodeType");
    ASSERT_BSON_PATH("\"index1\"", explainRoot, "child.leftChild.indexDefName");
    ASSERT_BSON_PATH("\"Filter\"", explainRoot, "child.rightChild.nodeType");
    ASSERT_BSON_PATH("\"FunctionCall\"", explainRoot, "child.rightChild.filter.nodeType");
    ASSERT_BSON_PATH("\"shardFilter\"", explainRoot, "child.rightChild.filter.name");
    ASSERT_BSON_PATH("\"LimitSkip\"", explainRoot, "child.rightChild.child.nodeType");
    ASSERT_BSON_PATH("\"Seek\"", explainRoot, "child.rightChild.child.child.nodeType");

    // Assert that shard key {b: 1} projection was pushed down into the SeekNode.
    const auto shardKeyElem = dotted_path_support::extractElementAtPath(
        explainRoot, "child.rightChild.child.child.fieldProjectionMap.b");
    ASSERT_TRUE(shardKeyElem.ok());
    // Get projection to which the shard key is bound.
    const auto shardKeyProj = shardKeyElem.String();
    // Assert that the projection used in the 'shardFilter' function call is that of the shard key.
    ASSERT_EQ(shardKeyProj,
              dotted_path_support::extractElementAtPath(explainRoot,
                                                        "child.rightChild.filter.arguments.0.name")
                  .String());
}

TEST(PhysRewriter, ScanNodeRemoveOrphansImplementerSeekTargetDottedSharedPrefix) {
    ABT rootNode = NodeBuilder{}
                       .root("root")
                       .filter(_evalf(_get("e", _traverse1(_cmp("Eq", "3"_cint64))), "root"_var))
                       .finish(_scan("root", "c1"));
    // Sharded on {a.b.c: 1, a.b.d:1}
    ShardingMetadata sm({{_get("a", _get("b", _get("c", _id())))._n, CollationOp::Ascending},
                         {_get("a", _get("b", _get("d", _id())))._n, CollationOp::Ascending}},
                        true);
    auto shardScanDef =
        createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                      UUID::gen(),
                      ScanDefOptions{},
                      {{"index1", makeIndexDefinition("e", CollationOp::Ascending)}},
                      MultikeynessTrie{},
                      ConstEval::constFold,
                      DistributionAndPaths{DistributionType::Centralized},
                      true /*exists*/,
                      boost::none /*ce*/,
                      sm);

    auto prefixId = PrefixId::createForTests();

    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1", shardScanDef}}},
        /*costModel*/ boost::none,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});
    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    const BSONObj explainRoot = ExplainGenerator::explainBSONObj(optimized);
    ASSERT_BSON_PATH("\"NestedLoopJoin\"", explainRoot, "child.nodeType");
    ASSERT_BSON_PATH("\"IndexScan\"", explainRoot, "child.leftChild.nodeType");
    ASSERT_BSON_PATH("\"index1\"", explainRoot, "child.leftChild.indexDefName");
    ASSERT_BSON_PATH("\"Filter\"", explainRoot, "child.rightChild.nodeType");
    ASSERT_BSON_PATH("\"Evaluation\"", explainRoot, "child.rightChild.child.nodeType");
    ASSERT_BSON_PATH("\"Evaluation\"", explainRoot, "child.rightChild.child.child.nodeType");
    ASSERT_BSON_PATH("\"LimitSkip\"", explainRoot, "child.rightChild.child.child.child.nodeType");
    ASSERT_BSON_PATH("\"Seek\"", explainRoot, "child.rightChild.child.child.child.child.nodeType");
    // Assert top level field of shard key is pushed down into the SeekNode.
    ASSERT_TRUE(dotted_path_support::extractElementAtPath(
                    explainRoot, "child.rightChild.child.child.child.child.fieldProjectionMap.a")
                    .ok());
}

TEST(PhysRewriter, RemoveOrphansSargableNodeComplete) {
    // Hypothetical MQL which could generate this ABT: {$match: {a: 1}}
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .finish(_scan("root", "c1"));
    // Shard key {a: 1, b: 1};
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending},
                         {_get("b", _id())._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // Projections on 'a' and 'b' pushed down into PhysicalScan and used as args to 'shardFilter()'.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [evalTemp_1]\n"
        "|   Variable [evalTemp_0]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_0]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_0, 'b': evalTemp_1}, c1]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphansSargableNodeCompleteDottedShardKey) {
    // {$match: {"a.b": {$gt: 1}}}
    ABT root =
        NodeBuilder{}
            .root("root")
            .filter(_evalf(_get("a", _traverse1(_get("b", _traverse1(_cmp("Gt", "1"_cint64))))),
                           "root"_var))
            .finish(_scan("root", "c1"));
    // Shard key {'a.b': 1}
    ShardingMetadata sm({{_get("a", _get("b", _id()))._n, CollationOp::Ascending}}, true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // Push down projection on 'a' into PhysicalScan and use that stream to project 'b' to use as
    // input to 'shardFilter()'. This avoids explicitly projecting 'a.b' from the root projection.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   Variable [shardKey_1]\n"
        "Evaluation [{shardKey_1}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [evalTemp_0]\n"
        "|   PathGet [b]\n"
        "|   PathIdentity []\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_0]\n"
        "|   PathGet [b]\n"
        "|   PathCompare [Gt]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_0}, c1]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphansSargableNodeIndex) {
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _cmp("Gt", "1"_cint64)), "root"_var))
                   .finish(_scan("root", "c1"));
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending}}, true);

    // Make predicates on PathGet[a] very selective to prefer IndexScan plan over collection scan.
    ce::PartialSchemaSelHints ceHints;
    ceHints.emplace(PartialSchemaKey{"root", _get("a", _id())._n}, SelectivityType{0.01});

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1",
           createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                         UUID::gen(),
                         {},
                         {{"index1", makeIndexDefinition("a", CollationOp::Ascending, false)}},
                         MultikeynessTrie{},
                         ConstEval::constFold,
                         DistributionAndPaths{DistributionType::Centralized},
                         true /*exists*/,
                         boost::none /*ce*/,
                         sm)}}},
        makeHintedCE(std::move(ceHints)),
        boost::none /*costModel*/,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});

    ABT optimized = root;
    phaseManager.optimize(optimized);

    ASSERT_BETWEEN(10, 16, phaseManager.getMemo().getStats()._physPlanExplorationCount);

    // The shard filter is performed on the index side of the NLJ and pushed the projection into the
    // index scan.
    const BSONObj explainRoot = ExplainGenerator::explainBSONObj(optimized);
    ASSERT_BSON_PATH("\"NestedLoopJoin\"", explainRoot, "child.nodeType");
    ASSERT_BSON_PATH("\"Filter\"", explainRoot, "child.leftChild.nodeType");
    ASSERT_BSON_PATH("\"FunctionCall\"", explainRoot, "child.leftChild.filter.nodeType");
    ASSERT_BSON_PATH("\"shardFilter\"", explainRoot, "child.leftChild.filter.name");
    ASSERT_BSON_PATH("\"IndexScan\"", explainRoot, "child.leftChild.child.nodeType");
    ASSERT_BSON_PATH("\"index1\"", explainRoot, "child.leftChild.child.indexDefName");
}

TEST(PhysRewriter, RemoveOrphansCovered) {
    ABT root = NodeBuilder{}
                   .root("pa")
                   .eval("pa", _evalp(_get("a", _id()), "root"_var))
                   .filter(_evalf(_get("a", _cmp("Gt", "1"_cint64)), "root"_var))
                   .finish(_scan("root", "c1"));
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending}}, true);

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1",
           createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                         UUID::gen(),
                         {},
                         {{"index1", makeIndexDefinition("a", CollationOp::Ascending, false)}},
                         MultikeynessTrie::fromIndexPath(_get("a", _id())._n),
                         ConstEval::constFold,
                         DistributionAndPaths{DistributionType::Centralized},
                         true /*exists*/,
                         boost::none /*ce*/,
                         sm)}}},
        boost::none /*costModel*/,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});

    ABT optimized = root;
    phaseManager.optimize(optimized);

    ASSERT_BETWEEN_AUTO(  // NOLINT
        5,
        15,
        phaseManager.getMemo().getStats()._physPlanExplorationCount);

    // No seek required.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{pa}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   Variable [pa]\n"
        "IndexScan [{'<indexKey> 0': pa}, scanDefName: c1, indexDefName: index1, interval: "
        "{>Const [1]}]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphansIndexDoesntCoverShardKey) {
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _cmp("Gt", "1"_cint64)), "root"_var))
                   .finish(_scan("root", "c1"));
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending},
                         {_get("b", _id())._n, CollationOp::Ascending}},
                        true);

    // Make predicates on PathGet[a] very selective to prefer IndexScan plan over collection scan.
    ce::PartialSchemaSelHints ceHints;
    ceHints.emplace(PartialSchemaKey{"root", _get("a", _id())._n}, SelectivityType{0.01});

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1",
           createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                         UUID::gen(),
                         {},
                         {{"index1", makeIndexDefinition("a", CollationOp::Ascending, false)}},
                         MultikeynessTrie{},
                         ConstEval::constFold,
                         DistributionAndPaths{DistributionType::Centralized},
                         true /*exists*/,
                         boost::none /*ce*/,
                         sm)}}},
        makeHintedCE(std::move(ceHints)),
        boost::none /*costModel*/,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});

    ABT optimized = root;
    phaseManager.optimize(optimized);

    ASSERT_BETWEEN(8, 14, phaseManager.getMemo().getStats()._physPlanExplorationCount);

    // Shard key {a: 1, b: 1} and index on {a: 1} means that shard filtering must occur on the seek
    // side.
    const BSONObj explainRoot = ExplainGenerator::explainBSONObj(optimized);
    ASSERT_BSON_PATH("\"NestedLoopJoin\"", explainRoot, "child.nodeType");
    ASSERT_BSON_PATH("\"IndexScan\"", explainRoot, "child.leftChild.nodeType");
    ASSERT_BSON_PATH("\"Filter\"", explainRoot, "child.rightChild.nodeType");
    ASSERT_BSON_PATH("\"FunctionCall\"", explainRoot, "child.rightChild.filter.nodeType");
    ASSERT_BSON_PATH("\"shardFilter\"", explainRoot, "child.rightChild.filter.name");
    ASSERT_BSON_PATH("\"LimitSkip\"", explainRoot, "child.rightChild.child.nodeType");
    ASSERT_BSON_PATH("\"Seek\"", explainRoot, "child.rightChild.child.child.nodeType");
}

TEST(PhysRewriter, RemoveOrphansDottedPathIndex) {
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _get("b", _cmp("Gt", "1"_cint64))), "root"_var))
                   .finish(_scan("root", "c1"));
    ShardingMetadata sm({{_get("a", _get("b", _id()))._n, CollationOp::Ascending}}, true);

    // Make predicates on PathGet[a] PathGet [b] very selective to prefer IndexScan plan over
    // collection scan.
    ce::PartialSchemaSelHints ceHints;
    ceHints.emplace(PartialSchemaKey{"root", _get("a", _get("b", _id()))._n},
                    SelectivityType{0.01});

    auto prefixId = PrefixId::createForTests();
    IndexCollationSpec indexSpec{
        IndexCollationEntry(_get("a", _get("b", _id()))._n, CollationOp::Ascending),
        IndexCollationEntry(_get("a", _get("c", _id()))._n, CollationOp::Ascending)};
    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1",
           createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                         UUID::gen(),
                         {},
                         {{"index1", {indexSpec, false}}},
                         MultikeynessTrie{},
                         ConstEval::constFold,
                         DistributionAndPaths{DistributionType::Centralized},
                         true /*exists*/,
                         boost::none /*ce*/,
                         sm)}}},
        makeHintedCE(std::move(ceHints)),
        boost::none /*costModel*/,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});

    ABT optimized = root;
    phaseManager.optimize(optimized);

    ASSERT_BETWEEN(10, 16, phaseManager.getMemo().getStats()._physPlanExplorationCount);

    // Shard key {"a.b": 1} and index on {"a.b": 1, "a.c": 1}
    // The index scan produces the projections for "a.b" to perform shard filtering.
    const BSONObj explainRoot = ExplainGenerator::explainBSONObj(optimized);
    ASSERT_BSON_PATH("\"NestedLoopJoin\"", explainRoot, "child.nodeType");
    ASSERT_BSON_PATH("\"Filter\"", explainRoot, "child.leftChild.nodeType");
    ASSERT_BSON_PATH("\"FunctionCall\"", explainRoot, "child.leftChild.filter.nodeType");
    ASSERT_BSON_PATH("\"shardFilter\"", explainRoot, "child.leftChild.filter.name");
    ASSERT_BSON_PATH("\"IndexScan\"", explainRoot, "child.leftChild.child.nodeType");
    ASSERT_BSON_PATH("\"index1\"", explainRoot, "child.leftChild.child.indexDefName");
}

TEST(PhysRewriter, RemoveOrphanedMultikeyIndex) {
    // Shard key: {a: 1}
    // Index: {a: 1, b: 1} -> multikey on b
    // Query: {$match: {a: {$gt: 2}, b: {$gt: 3}}}
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _cmp("Gt", "2"_cint64)), "root"_var))
                   .filter(_evalf(_get("b", _cmp("Gt", "3"_cint64)), "root"_var))
                   .finish(_scan("root", "c1"));
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending}}, true);

    ce::PartialSchemaSelHints ceHints;
    ceHints.emplace(PartialSchemaKey{"root", _get("a", _id())._n}, SelectivityType{0.01});
    ceHints.emplace(PartialSchemaKey{"root", _get("b", _id())._n}, SelectivityType{0.01});

    auto prefixId = PrefixId::createForTests();
    ABT indexPath0 = _get("a", _id())._n;
    ABT indexPath1 = _get("b", _id())._n;
    IndexCollationSpec indexSpec{IndexCollationEntry(indexPath0, CollationOp::Ascending),
                                 IndexCollationEntry(indexPath1, CollationOp::Ascending)};
    auto multikeyTrie = MultikeynessTrie::fromIndexPath(indexPath0);
    multikeyTrie.add(indexPath1);
    auto phaseManager = makePhaseManager(
        {OptPhase::MemoSubstitutionPhase,
         OptPhase::MemoExplorationPhase,
         OptPhase::MemoImplementationPhase},
        prefixId,
        {{{"c1",
           createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                         UUID::gen(),
                         {},
                         {{"index1", {indexSpec, false}}},
                         std::move(multikeyTrie),
                         ConstEval::constFold,
                         DistributionAndPaths{DistributionType::Centralized},
                         true /*exists*/,
                         boost::none /*ce*/,
                         sm)}}},
        makeHintedCE(std::move(ceHints)),
        boost::none /*costModel*/,
        {true /*debugMode*/, 2 /*debugLevel*/, DebugInfo::kIterationLimitForTests});

    ABT optimized = root;
    phaseManager.optimize(optimized);

    ASSERT_BETWEEN(24, 30, phaseManager.getMemo().getStats()._physPlanExplorationCount);

    // Ensure that we perform the shard filter using a projection from the index scan.
    const BSONObj explainRoot = ExplainGenerator::explainBSONObj(optimized);
    ASSERT_BSON_PATH("\"NestedLoopJoin\"", explainRoot, "child.nodeType");
    ASSERT_BSON_PATH("\"Filter\"", explainRoot, "child.leftChild.nodeType");
    ASSERT_BSON_PATH("\"FunctionCall\"", explainRoot, "child.leftChild.filter.nodeType");
    ASSERT_BSON_PATH("\"shardFilter\"", explainRoot, "child.leftChild.filter.name");
    ASSERT_BSON_PATH("\"IndexScan\"", explainRoot, "child.leftChild.child.child.nodeType");
    ASSERT_BSON_PATH("\"index1\"", explainRoot, "child.leftChild.child.child.indexDefName");
}

TEST(PhysRewriter, RemoveOrphanEqualityOnSimpleShardKey) {
    // Query: {$match: {a: 1, b: 1}}
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .filter(_evalf(_get("b", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .finish(_scan("root", "c1"));
    // Shard key {a: 1}
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending}}, true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // No shard filter in the plan.
    ASSERT_EXPLAIN_V2_AUTO(  // NOLINT
        "Root [{root}]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_3]\n"
        "|   PathTraverse [1]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_2]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_2, 'b': evalTemp_3}, c1]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphanEqualityWithComplexPSR) {
    // Query: {$match: {a: 1, b: 1}}
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_composem(_get("a", _traverse1(_cmp("Eq", "1"_cint64))),
                                            _get("b", _traverse1(_cmp("Eq", "1"_cint64)))),
                                  "root"_var))
                   .finish(_scan("root", "c1"));
    // Shard key {a: 1}
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending}}, true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // No shard filter in the plan.
    ASSERT_EXPLAIN_V2_AUTO(  // NOLINT
        "Root [{root}]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_3]\n"
        "|   PathTraverse [1]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_2]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_2, 'b': evalTemp_3}, c1]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphanEqualityOnCompoundShardKey) {
    // Query: {$match: {a: 1, b: 1}}
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .filter(_evalf(_get("b", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .finish(_scan("root", "c1"));
    // Shard key {a: 1, b: 1}
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending},
                         {_get("b", _id())._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // No shard filter in the plan.
    ASSERT_EXPLAIN_V2_AUTO(  // NOLINT
        "Root [{root}]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_3]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_2]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_2, 'b': evalTemp_3}, c1]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphanNoEqualityOnCompoundShardKey) {
    // Query: {$match: {a: 1}}
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .finish(_scan("root", "c1"));
    // Shard key {a: 1, b: 1}
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending},
                         {_get("b", _id())._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // These is a shard filter in the plan.
    ASSERT_EXPLAIN_V2_AUTO(  // NOLINT
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [evalTemp_1]\n"
        "|   Variable [evalTemp_0]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_0]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_0, 'b': evalTemp_1}, c1]\n",
        optimized);
    ;
}

TEST(PhysRewriter, RemoveOrphanEqualityDottedPathInShardKey) {
    // Query: {$match: {"a.b": 1, "a.c": 1, "a.d": {$gt: 1}}}
    ABT root =
        NodeBuilder{}
            .root("root")
            .filter(_evalf(_get("a", _traverse1(_get("b", _traverse1(_cmp("Eq", "1"_cint64))))),
                           "root"_var))
            .filter(_evalf(_get("a", _traverse1(_get("c", _traverse1(_cmp("Eq", "1"_cint64))))),
                           "root"_var))
            .filter(_evalf(_get("a", _traverse1(_get("d", _traverse1(_cmp("Gt", "1"_cint64))))),
                           "root"_var))
            .finish(_scan("root", "c1"));
    // Shard key {"a.b": 1, "a.c": 1}
    ShardingMetadata sm({{_get("a", _get("b", _id()))._n, CollationOp::Ascending},
                         {_get("a", _get("c", _id()))._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // No shard filter in the plan.
    ASSERT_EXPLAIN_V2_AUTO(  // NOLINT
        "Root [{root}]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [d]\n"
        "|   PathTraverse [1]\n"
        "|   PathCompare [Gt]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [c]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [b]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_4}, c1]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphanNoEqualityDottedPathInShardKey) {
    // Query: {$match: {"a.b": 1, "a.c": {$gt: 1}, "a.d": 1}}
    ABT root =
        NodeBuilder{}
            .root("root")
            .filter(_evalf(_get("a", _traverse1(_get("b", _traverse1(_cmp("Eq", "1"_cint64))))),
                           "root"_var))
            .filter(_evalf(_get("a", _traverse1(_get("c", _traverse1(_cmp("Gt", "1"_cint64))))),
                           "root"_var))
            .filter(_evalf(_get("a", _traverse1(_get("d", _traverse1(_cmp("Eq", "1"_cint64))))),
                           "root"_var))
            .finish(_scan("root", "c1"));
    // Shard key {"a.b": 1, "a.c": 1}
    ShardingMetadata sm({{_get("a", _get("b", _id()))._n, CollationOp::Ascending},
                         {_get("a", _get("c", _id()))._n, CollationOp::Ascending}},
                        true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // There is shard filter in the plan.
    ASSERT_EXPLAIN_V2_AUTO(  // NOLINT
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [shardKey_3]\n"
        "|   Variable [shardKey_2]\n"
        "Evaluation [{shardKey_3}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [c]\n"
        "|   PathIdentity []\n"
        "Evaluation [{shardKey_2}]\n"
        "|   EvalPath []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [b]\n"
        "|   PathIdentity []\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [c]\n"
        "|   PathCompare [Gt]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [d]\n"
        "|   PathTraverse [1]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_4]\n"
        "|   PathGet [b]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_4}, c1]\n",
        optimized);
}

TEST(PhysRewriter, RemoveOrphanEqualityHashedShardKey) {
    // Query: {$match: {a: 1, b: 1}}
    ABT root = NodeBuilder{}
                   .root("root")
                   .filter(_evalf(_get("a", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .filter(_evalf(_get("b", _traverse1(_cmp("Eq", "1"_cint64))), "root"_var))
                   .finish(_scan("root", "c1"));
    // Shard key {a: 'hashed'}
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Clustered}}, true);
    const ABT optimized = optimizeABTWithShardingMetadataNoIndexes(root, sm);

    // No shard filter in the plan.
    ASSERT_EXPLAIN_V2_AUTO(  // NOLINT
        "Root [{root}]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_3]\n"
        "|   PathTraverse [1]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [evalTemp_2]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': root, 'a': evalTemp_2, 'b': evalTemp_3}, c1]\n",
        optimized);
}

// TODO SERVER-78507: Examine the physical alternatives in the memo, rather than the logical nodes,
// to check that the children of the RIDIntersect have physical alternatives with both combinations
// of RemoveOrphansRequirement.
TEST(PhysRewriter, RIDIntersectRemoveOrphansImplementer) {
    using namespace properties;

    ABT scanNode = make<ScanNode>("root", "c1");

    ABT filterNode = make<FilterNode>(
        make<EvalFilter>(make<PathGet>("a",
                                       make<PathTraverse>(
                                           PathTraverse::kSingleLevel,
                                           make<PathCompare>(Operations::Eq, Constant::int64(1)))),
                         make<Variable>("root")),
        std::move(scanNode));

    ABT rootNode =
        make<RootNode>(ProjectionRequirement{ProjectionNameVector{"root"}}, std::move(filterNode));

    {
        auto prefixId = PrefixId::createForTests();
        ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Ascending}}, true);
        auto phaseManager = makePhaseManager(
            {OptPhase::MemoSubstitutionPhase,
             OptPhase::MemoExplorationPhase,
             OptPhase::MemoImplementationPhase},
            prefixId,
            {{{"c1",
               createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                             UUID::gen(),
                             {},
                             {{"index1", makeIndexDefinition("a", CollationOp::Ascending)}},
                             MultikeynessTrie{},
                             ConstEval::constFold,
                             DistributionAndPaths{DistributionType::Centralized},
                             true /*exists*/,
                             boost::none /*ce*/,
                             sm)}}},
            boost::none /*costModel*/,
            {true /*debugMode*/, 3 /*debugLevel*/, DebugInfo::kIterationLimitForTests},
            {});

        ABT optimized = rootNode;
        phaseManager.optimize(optimized);

        /*
            Examine the RIDintersectNode in the memo to make sure that it meets the following
           conditions:
            1. The right-delegated group needs to have logial node '0' as a scan, and needs to have
           physical alternatives with RemoveOrphansRequirement both true and false.
            2. The left-delegated group needs to have logical node '0' as a Sargable [Index] with
           a=1 and should also have physical alternatives with RemoveOrphansRequirmeent both true
           and false.
        */

        const auto& memo = phaseManager.getMemo();

        const RIDIntersectNode* ridIntersectNode = nullptr;
        for (int groupId = 0; (size_t)groupId < memo.getGroupCount() && !ridIntersectNode;
             groupId++) {
            for (auto& node : memo.getLogicalNodes(groupId)) {
                if (ridIntersectNode = node.cast<RIDIntersectNode>(); ridIntersectNode) {
                    break;
                }
            }
        }
        ASSERT(ridIntersectNode);
        const auto* left = ridIntersectNode->getLeftChild().cast<MemoLogicalDelegatorNode>();
        const auto* right = ridIntersectNode->getRightChild().cast<MemoLogicalDelegatorNode>();
        ASSERT(left);
        ASSERT(right);

        // Given a groupId, checks that the corresponding group contains at least one physical
        // alternative alternative with RemoveOrphansRequirement 'true' and one with 'false'.
        // We don't care whether the optimizer found a plan for any of these physical
        // alternatives; we only care that it attempted all of them.
        auto containsMustRemoveTrueAndFalse = [&](GroupIdType groupId) {
            bool containsRemoveOrphansTrueAlternative = false,
                 containsRemoveOrphansFalseAlternative = false;
            for (const auto& node : memo.getPhysicalNodes(groupId)) {
                const PhysProps& props = node->_physProps;
                ASSERT(hasProperty<RemoveOrphansRequirement>(props));
                bool result = getPropertyConst<RemoveOrphansRequirement>(props).mustRemove();
                containsRemoveOrphansTrueAlternative =
                    containsRemoveOrphansTrueAlternative || result;
                containsRemoveOrphansFalseAlternative =
                    containsRemoveOrphansFalseAlternative || !result;
                if (containsRemoveOrphansTrueAlternative && containsRemoveOrphansFalseAlternative) {
                    return true;
                }
            }
            return false;
        };

        // Examine the left delegator.
        ASSERT(containsMustRemoveTrueAndFalse(left->getGroupId()));

        // Examine the right delegator.
        ASSERT(containsMustRemoveTrueAndFalse(right->getGroupId()));
    }
}

TEST(PhysRewriter, HashedShardKey) {
    ABT rootNode = NodeBuilder{}.root("root").finish(_scan("root", "c1"));
    // Sharded on {a: "hashed", b: 1}
    ShardingMetadata sm({{_get("a", _id())._n, CollationOp::Clustered},
                         {_get("b", _id())._n, CollationOp::Ascending}},
                        true);
    ABT optimized = optimizeABTWithShardingMetadataNoIndexes(rootNode, sm);
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{root}]\n"
        "Filter []\n"
        "|   FunctionCall [shardFilter]\n"
        "|   |   Variable [shardKey_3]\n"
        "|   FunctionCall [shardHash]\n"
        "|   Variable [shardKey_2]\n"
        "PhysicalScan [{'<root>': root, 'a': shardKey_2, 'b': shardKey_3}, c1]\n",
        optimized);
}

}  // namespace
}  // namespace mongo::optimizer
