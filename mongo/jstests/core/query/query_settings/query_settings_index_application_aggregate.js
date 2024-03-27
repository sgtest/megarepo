// Tests query settings are applied to aggregate queries regardless of the query engine (SBE or
// classic).
// @tags: [
//   # Explain on foreign sharded collections does not return used indexes.
//   assumes_unsharded_collection,
//   # $planCacheStats can not be run with specified read preferences/concerns.
//   assumes_read_preference_unchanged,
//   assumes_read_concern_unchanged,
//   # $planCacheStats can not be run in transactions.
//   does_not_support_transactions,
//   directly_against_shardsvrs_incompatible,
//   simulate_atlas_proxy_incompatible,
//   cqf_incompatible,
//   # 'planCacheClear' command is not allowed with the security token.
//   not_allowed_with_signed_security_token,
//   requires_fcv_80,
//   # Explain for tracked unsharded collections return IXSCAN as inputStage
//   # TODO SERVER-87164 re-enable the tests in suites with random migrations
//   assumes_balancer_off,
// ]
//

import {
    assertDropAndRecreateCollection,
    assertDropCollection
} from "jstests/libs/collection_drop_recreate.js";
import {QuerySettingsIndexHintsTests} from "jstests/libs/query_settings_index_hints_tests.js";
import {QuerySettingsUtils} from "jstests/libs/query_settings_utils.js";
import {checkSbeRestrictedOrFullyEnabled} from "jstests/libs/sbe_util.js";

const coll = assertDropAndRecreateCollection(db, jsTestName());
const viewName = "identityView";
assertDropCollection(db, viewName);
assert.commandWorked(db.createView(viewName, coll.getName(), []));
const mainNs = {
    db: db.getName(),
    coll: coll.getName()
};
const secondaryColl = assertDropAndRecreateCollection(db, "secondary");
const secondaryViewName = "secondaryIdentityView";
assertDropCollection(db, secondaryViewName);
assert.commandWorked(db.createView(secondaryViewName, secondaryColl.getName(), []));
const secondaryNs = {
    db: db.getName(),
    coll: secondaryColl.getName()
};

// Insert data into the collection.
assert.commandWorked(coll.insertMany([
    {a: 1, b: 5},
    {a: 2, b: 4},
    {a: 3, b: 3},
    {a: 4, b: 2},
    {a: 5, b: 1},
]));

assert.commandWorked(secondaryColl.insertMany([
    {a: 1, b: 5},
    {a: 1, b: 5},
    {a: 3, b: 1},
]));

function setIndexes(coll, indexList) {
    assert.commandWorked(coll.dropIndexes());
    assert.commandWorked(coll.createIndexes(indexList));
}

function testAggregateQuerySettingsApplicationWithoutSecondaryCollections(collOrViewName) {
    const qsutils = new QuerySettingsUtils(db, collOrViewName);
    const qstests = new QuerySettingsIndexHintsTests(qsutils);

    setIndexes(coll, [qstests.indexA, qstests.indexB, qstests.indexAB]);

    // Ensure that query settings cluster parameter is empty.
    qsutils.assertQueryShapeConfiguration([]);

    const aggregateCmd = qsutils.makeAggregateQueryInstance({
        pipeline: [{$match: {a: 1, b: 5}}],
        cursor: {},
    });
    qstests.assertQuerySettingsIndexApplication(aggregateCmd, mainNs);
    qstests.assertQuerySettingsIgnoreCursorHints(aggregateCmd, mainNs);
    qstests.assertQuerySettingsFallback(aggregateCmd, mainNs);
    qstests.assertQuerySettingsCommandValidation(aggregateCmd, mainNs);
}

function testAggregateQuerySettingsApplicationWithLookupEquiJoin(
    collOrViewName, secondaryCollOrViewName, isSecondaryCollAView) {
    const qsutils = new QuerySettingsUtils(db, collOrViewName);
    const qstests = new QuerySettingsIndexHintsTests(qsutils);

    // Set indexes on both collections.
    setIndexes(coll, [qstests.indexA, qstests.indexB, qstests.indexAB]);
    setIndexes(secondaryColl, [qstests.indexA, qstests.indexB, qstests.indexAB]);

    // Ensure that query settings cluster parameter is empty.
    qsutils.assertQueryShapeConfiguration([]);

    const aggregateCmd = qsutils.makeAggregateQueryInstance({
    pipeline: [
      { $match: { a: 1, b: 5 } },
      {
        $lookup:
          { from: secondaryCollOrViewName, localField: "a", foreignField: "a", as: "output" }
      }
    ],
    cursor: {},
  });

    // Ensure query settings index application for 'mainNs', 'secondaryNs' and both.
    qstests.assertQuerySettingsIndexApplication(aggregateCmd, mainNs);
    qstests.assertQuerySettingsLookupJoinIndexApplication(
        aggregateCmd, secondaryNs, isSecondaryCollAView);
    qstests.assertQuerySettingsIndexAndLookupJoinApplications(
        aggregateCmd, mainNs, secondaryNs, isSecondaryCollAView);

    // Ensure query settings ignore cursor hints when being set on main collection.
    qstests.assertQuerySettingsIgnoreCursorHints(aggregateCmd, mainNs);
    if (checkSbeRestrictedOrFullyEnabled(db) && !isSecondaryCollAView) {
        // The aggregation stage will get pushed down to SBE, and index hints will get applied to
        // secondary collections. This prevents cursor hints from also being applied.
        qstests.assertQuerySettingsIgnoreCursorHints(aggregateCmd, secondaryNs);
    } else {
        // No SBE push down happens. The $lookup will get executed as a separate pipeline, so we
        // expect cursor hints to be applied on the main collection, while query settings will get
        // applied on the secondary collection.
        qstests.assertQuerySettingsWithCursorHints(aggregateCmd, mainNs, secondaryNs);
    }

    // Ensure that providing query settings with an invalid index result in the same plan as no
    // query settings being set.
    // NOTE: The fallback is not tested when hinting secondary collections, as instead of fallback,
    // hash join or nlj will be used.
    // TODO: SERVER-86400 Add support for $natural hints on secondary collections.
    qstests.assertQuerySettingsFallback(aggregateCmd, mainNs);

    qstests.assertQuerySettingsCommandValidation(aggregateCmd, mainNs);
    qstests.assertQuerySettingsCommandValidation(aggregateCmd, secondaryNs);
}

function testAggregateQuerySettingsApplicationWithLookupPipeline(collOrViewName,
                                                                 secondaryCollOrViewName) {
    const qsutils = new QuerySettingsUtils(db, collOrViewName);
    const qstests = new QuerySettingsIndexHintsTests(qsutils);

    // Set indexes on both collections.
    setIndexes(coll, [qstests.indexA, qstests.indexB, qstests.indexAB]);
    setIndexes(secondaryColl, [qstests.indexA, qstests.indexB, qstests.indexAB]);

    // Ensure that query settings cluster parameter is empty.
    qsutils.assertQueryShapeConfiguration([]);

    const aggregateCmd = qsutils.makeAggregateQueryInstance({
    pipeline: [
      { $match: { a: 1, b: 5 } },
      {
        $lookup:
          { from: secondaryCollOrViewName, pipeline: [{ $match: { a: 1, b: 5 } }], as: "output" }
      }
    ],
    cursor: {},
  });

    // Ensure query settings index application for 'mainNs', 'secondaryNs' and both.
    qstests.assertQuerySettingsIndexApplication(aggregateCmd, mainNs);
    qstests.assertQuerySettingsLookupPipelineIndexApplication(aggregateCmd, secondaryNs);
    qstests.assertQuerySettingsIndexAndLookupPipelineApplications(
        aggregateCmd, mainNs, secondaryNs);

    // Ensure query settings ignore cursor hints when being set on main collection.
    qstests.assertQuerySettingsIgnoreCursorHints(aggregateCmd, mainNs);

    // Ensure both cursor hints and query settings are applied, since they are specified on
    // different pipelines.
    qstests.assertQuerySettingsWithCursorHints(aggregateCmd, mainNs, secondaryNs);

    qstests.assertQuerySettingsFallback(aggregateCmd, mainNs);
    qstests.assertQuerySettingsFallback(aggregateCmd, secondaryNs);

    qstests.assertQuerySettingsCommandValidation(aggregateCmd, mainNs);
    qstests.assertQuerySettingsCommandValidation(aggregateCmd, secondaryNs);
}

function testAggregateQuerySettingsApplicationWithGraphLookup(collOrViewName,
                                                              secondaryCollOrViewName) {
    const qsutils = new QuerySettingsUtils(db, collOrViewName);
    const qstests = new QuerySettingsIndexHintsTests(qsutils);

    // Set indexes on both collections.
    setIndexes(coll, [qstests.indexA, qstests.indexB, qstests.indexAB]);
    setIndexes(secondaryColl, [qstests.indexA, qstests.indexB, qstests.indexAB]);

    // Ensure that query settings cluster parameter is empty.
    qsutils.assertQueryShapeConfiguration([]);

    const filter = {a: {$ne: "Bond"}, b: {$ne: "James"}};
    const pipeline = [{
        $match: filter
        }, {
        $graphLookup: {
        from: secondaryCollOrViewName,
        startWith: "$a",
        connectFromField: "b",
        connectToField: "a",
        as: "children",
        maxDepth: 4,
        depthField: "depth",
        restrictSearchWithMatch: filter
        }
    }];
    const aggregateCmd = qsutils.makeAggregateQueryInstance({pipeline});

    // Ensure query settings index application for 'mainNs'.
    // TODO SERVER-88561: Ensure query settings index application for 'secondaryNs' after
    // 'indexesUsed' is added to the 'explain' command output for the $graphLookup operation.
    qstests.assertQuerySettingsIndexApplication(aggregateCmd, mainNs);
    qstests.assertGraphLookupQuerySettingsInCache(aggregateCmd, secondaryNs);

    // Ensure query settings ignore cursor hints when being set on main collection.
    qstests.assertQuerySettingsIgnoreCursorHints(aggregateCmd, mainNs);

    qstests.assertQuerySettingsFallback(aggregateCmd, mainNs);

    qstests.assertQuerySettingsCommandValidation(aggregateCmd, mainNs);
    qstests.assertQuerySettingsCommandValidation(aggregateCmd, secondaryNs);
}

testAggregateQuerySettingsApplicationWithoutSecondaryCollections(coll.getName());
testAggregateQuerySettingsApplicationWithoutSecondaryCollections(viewName);

testAggregateQuerySettingsApplicationWithGraphLookup(coll.getName(), secondaryColl.getName());
testAggregateQuerySettingsApplicationWithGraphLookup(viewName, secondaryColl.getName());
testAggregateQuerySettingsApplicationWithGraphLookup(coll.getName(), secondaryViewName);
testAggregateQuerySettingsApplicationWithGraphLookup(viewName, secondaryViewName);

testAggregateQuerySettingsApplicationWithLookupEquiJoin(
    coll.getName(), secondaryColl.getName(), false);
testAggregateQuerySettingsApplicationWithLookupEquiJoin(viewName, secondaryColl.getName(), false);
testAggregateQuerySettingsApplicationWithLookupEquiJoin(coll.getName(), secondaryViewName, true);
testAggregateQuerySettingsApplicationWithLookupEquiJoin(viewName, secondaryViewName, true);

testAggregateQuerySettingsApplicationWithLookupPipeline(coll.getName(), secondaryColl.getName());
testAggregateQuerySettingsApplicationWithLookupPipeline(viewName, secondaryColl.getName());
testAggregateQuerySettingsApplicationWithLookupPipeline(coll.getName(), secondaryViewName);
testAggregateQuerySettingsApplicationWithLookupPipeline(viewName, secondaryViewName);
