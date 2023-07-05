/**
 * Tests scenario related to SERVER-12923.
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

const t = db.cqf_sort_match;
t.drop();

const bulk = t.initializeUnorderedBulkOp();
const nDocs = 1000;
for (let i = 0; i < nDocs; i++) {
    bulk.insert({a: i, b: i % 10});
}
assert.commandWorked(bulk.execute());

assert.commandWorked(t.createIndex({a: 1}));
assert.commandWorked(t.createIndex({b: 1}));

{
    const res = t.explain("executionStats").aggregate([{$sort: {b: 1}}, {$match: {a: {$eq: 0}}}]);
    assert.eq(1, res.executionStats.nReturned);

    // Index on "a" is preferred.
    const indexNode = navigateToPlanPath(res, "child.child.leftChild");
    assertValueOnPath("IndexScan", indexNode, "nodeType");
    assertValueOnPath("a_1", indexNode, "indexDefName");
}

{
    // Test we are inverting bounds correctly during lowering.
    const res =
        runWithParams([{key: "internalCascadesOptimizerFastIndexNullHandling", value: true}],
                      () => t.find().sort({a: -1}).hint({a: 1}).toArray());
    assert.eq(nDocs, res.length);

    // Assert we are sorted in reverse order on "a".
    let prev = -1;
    for (let v of res) {
        const current = v["a"];
        if (prev >= 0) {
            assert(current < prev);
        }
        prev = current;
    }
}