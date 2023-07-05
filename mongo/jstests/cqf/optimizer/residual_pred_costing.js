/**
 * Tests scenario related to SERVER-21697.
 */
import {
    assertValueOnPath,
    checkCascadesOptimizerEnabled,
    navigateToPlanPath,
    runWithParams,
} from "jstests/libs/optimizer_utils.js";

if (!checkCascadesOptimizerEnabled(db)) {
    jsTestLog("Skipping test because the optimizer is not enabled");
    quit();
}

const t = db.cqf_residual_pred_costing;
t.drop();

const bulk = t.initializeUnorderedBulkOp();
const nDocs = 2000;
for (let i = 0; i < nDocs; i++) {
    bulk.insert({a: i % 10, b: i % 10, c: i % 10, d: i % 10});
}
assert.commandWorked(bulk.execute());

assert.commandWorked(t.createIndex({a: 1, b: 1, c: 1, d: 1}));
assert.commandWorked(t.createIndex({a: 1, b: 1, d: 1}));
assert.commandWorked(t.createIndex({a: 1, d: 1}));

const res = runWithParams(
    [{key: "internalCascadesOptimizerFastIndexNullHandling", value: true}],
    () => t.explain("executionStats")
              .aggregate([{$match: {a: {$eq: 0}, b: {$eq: 0}, c: {$eq: 0}}}, {$sort: {d: 1}}]));
assert.eq(nDocs * 0.1, res.executionStats.nReturned);

// Demonstrate we can pick the indexing covering most fields.
const indexNode = navigateToPlanPath(res, "child.leftChild");
assertValueOnPath("IndexScan", indexNode, "nodeType");
assertValueOnPath("a_1_b_1_c_1_d_1", indexNode, "indexDefName");