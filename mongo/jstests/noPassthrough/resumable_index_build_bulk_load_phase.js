/**
 * Tests that resumable index builds in the bulk load phase write their state to disk upon clean
 * shutdown and are resumed from the same phase to completion when the node is started back up.
 *
 * @tags: [
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   requires_replication,
 * ]
 */
load("jstests/noPassthrough/libs/index_build.js");
import {checkSBEEnabled} from "jstests/libs/sbe_util.js";
import {setUpServerForColumnStoreIndexTest} from "jstests/libs/columnstore_util.js";

const dbName = "test";

const rst = new ReplSetTest({nodes: 1});
rst.startSet();
rst.initiate();

const columnstoreEnabled =
    checkSBEEnabled(rst.getPrimary().getDB(dbName), ["featureFlagColumnstoreIndexes"], true) &&
    setUpServerForColumnStoreIndexTest(rst.getPrimary().getDB(dbName));

const runTests = function(docs, indexSpecsFlat, collNameSuffix) {
    const coll = rst.getPrimary().getDB(dbName).getCollection(jsTestName() + collNameSuffix);
    assert.commandWorked(coll.insert(docs));

    const runTest = function(indexSpecs, iteration) {
        ResumableIndexBuildTest.run(
            rst,
            dbName,
            coll.getName(),
            indexSpecs,
            [{name: "hangIndexBuildDuringBulkLoadPhase", logIdWithIndexName: 4924400}],
            iteration,
            ["bulk load"],
            [{skippedPhaseLogID: 20391}]);
    };

    runTest([[indexSpecsFlat[0]]], 0);
    runTest([[indexSpecsFlat[0]]], 1);
    runTest([[indexSpecsFlat[0]], [indexSpecsFlat[1]]], 0);
    runTest([[indexSpecsFlat[0]], [indexSpecsFlat[1]]], 1);
    runTest([indexSpecsFlat], 0);
    runTest([indexSpecsFlat], 1);
};

runTests([{a: 1, b: 1}, {a: 2, b: 2}], [{a: 1}, {b: 1}], "");
runTests([{a: [1, 2], b: [1, 2]}, {a: 2, b: 2}], [{a: 1}, {b: 1}], "_multikey_first");
runTests([{a: 1, b: 1}, {a: [1, 2], b: [1, 2]}], [{a: 1}, {b: 1}], "_multikey_last");
runTests([{a: [1, 2], b: 1}, {a: 2, b: [1, 2]}], [{a: 1}, {b: 1}], "_multikey_mixed");
runTests([{a: [1, 2], b: {c: [3, 4]}, d: ""}, {e: "", f: [[]], g: null, h: 8}],
         [{"$**": 1}, {h: 1}],
         "_wildcard");

if (columnstoreEnabled) {
    const testDocs = [
        {foo: 1, b: 10},
        {foo: 2, b: 11},
        {foo: 3, b: 12},
        {foo: 5, b: 13},
        {foo: 7, b: 14},
    ];
    runTests(testDocs, [{"$**": "columnstore"}, {b: 1}], "_columnstore");
}

rst.stopSet();