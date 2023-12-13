import {
    assertValueOnPlanPath,
    checkCascadesOptimizerEnabled,
    removeUUIDsFromExplain,
    runWithFastPathsDisabled,
    runWithParams,
} from "jstests/libs/optimizer_utils.js";

if (!checkCascadesOptimizerEnabled(db)) {
    jsTestLog("Skipping test because the optimizer is not enabled");
    quit();
}

const t = db.cqf_index_hints;
t.drop();

assert.commandWorked(t.insert({_id: 0, b: 0, a: [1, 2, 3, 4]}));
assert.commandWorked(t.insert({_id: 1, b: 1, a: [2, 3, 4]}));
assert.commandWorked(t.insert({_id: 2, b: 2, a: [2]}));
assert.commandWorked(t.insert({_id: 3, b: 3, a: 2}));
assert.commandWorked(t.insert({_id: 4, b: 4, a: [1, 3]}));

runWithFastPathsDisabled(() => {
    // Empty hint is ignored when there are no relevant indexes.
    let res = t.explain("executionStats").find({a: 2}).hint({}).finish();
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

    res = t.explain("executionStats").aggregate([{$match: {a: 2}}], {hint: {}});
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");
});

assert.commandWorked(t.createIndex({a: 1}));
assert.commandWorked(t.createIndex({b: 1}));

// There are too few documents, and an index is not preferable.
runWithFastPathsDisabled(() => {
    let res = t.explain("executionStats").find({a: 2}).finish();
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");
});

runWithFastPathsDisabled(() => {
    // Empty hint is ignored when there are relevant indexes that are not preferable.
    let res = t.explain("executionStats").find({a: 2}).hint({}).finish();
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

    res = t.explain("executionStats").aggregate([{$match: {a: 2}}], {hint: {}});
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");
});

{
    let res = t.explain("executionStats").find({a: 2}).hint({a: 1}).finish();
    assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");
}

{
    let res = t.explain("executionStats").find({a: 2}).hint("a_1").finish();
    assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");
}

{
    let res = t.explain("executionStats").find({a: 2}).hint({$natural: 1}).finish();
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

    res = t.find({a: 2}).hint({$natural: 1}).toArray();
    assert.eq(res[0]._id, 0, res);
}

{
    let res = t.explain("executionStats").find({a: 2}).hint({$natural: -1}).finish();
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

    res = t.find({a: 2}).hint({$natural: -1}).toArray();
    assert.eq(res[0]._id, 3, res);
}

// Generate enough documents for index to be preferable.
for (let i = 0; i < 100; i++) {
    assert.commandWorked(t.insert({b: i + 5, a: i + 10}));
}

runWithFastPathsDisabled(() => {
    let res = t.explain("executionStats").find({a: 2}).finish();
    assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");
});

runWithFastPathsDisabled(() => {
    // Empty hint is ignored when there are relevant indexes that are preferable.
    let res = t.explain("executionStats").find({a: 2}).hint({}).finish();
    assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");

    res = t.explain("executionStats").aggregate([{$match: {a: 2}}], {hint: {}});
    assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");
});

{
    let res = t.explain("executionStats").find({a: 2}).hint({a: 1}).finish();
    assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");
}

{
    let res = t.explain("executionStats").find({a: 2}).hint("a_1").finish();
    assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");
}
{
    let res = t.explain("executionStats").find({a: 2}).hint({$natural: 1}).finish();
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

    res = t.find({a: 2}).hint({$natural: 1}).toArray();
    assert.eq(res[0]._id, 0, res);
}

{
    let res = t.explain("executionStats").find({a: 2}).hint({$natural: -1}).finish();
    assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

    res = t.find({a: 2}).hint({$natural: -1}).toArray();
    assert.eq(res[0]._id, 3, res);
}

// Use index {a:1} multikeyness info, Cannot eliminate PathTraverse.
{
    const res = runWithParams(
        [
            {key: 'internalCascadesOptimizerExplainVersion', value: "v2"},
            {key: "internalCascadesOptimizerUseDescriptiveVarNames", value: true},
            // TODO SERVER-83441: Implement top-level fields pushdown from Filter into CollScan.
            {key: "internalCascadesOptimizerDisableSargableWhenNoIndexes", value: false}
        ],
        () => t.explain("executionStats").find({a: 2}).hint({$natural: -1}).finish());

    const expectedStr =
        `Root [{scan_0}]
Filter []
|   EvalFilter []
|   |   Variable [evalTemp_0]
|   PathTraverse [1]
|   PathCompare [Eq]
|   Const [2]
PhysicalScan [{'<root>': scan_0, 'a': evalTemp_0}, cqf_index_hints_, Reverse]
`;

    const actualStr = removeUUIDsFromExplain(db, res);
    assert.eq(expectedStr, actualStr);
}

// Hint collection scan to disable indexes. Check that index {b: 1} multikeyness info can eliminate
// PathTraverse.
{
    const res = runWithParams(
        [
            {key: 'internalCascadesOptimizerExplainVersion', value: "v2"},
            {key: "internalCascadesOptimizerUseDescriptiveVarNames", value: true},
            // TODO SERVER-83441: Implement top-level fields pushdown from Filter into CollScan.
            {key: "internalCascadesOptimizerDisableSargableWhenNoIndexes", value: false}
        ],
        () => t.explain("executionStats").find({b: 2}).hint({$natural: -1}).finish());

    const expectedStr =
        `Root [{scan_0}]
Filter []
|   EvalFilter []
|   |   Variable [evalTemp_0]
|   PathCompare [Eq]
|   Const [2]
PhysicalScan [{'<root>': scan_0, 'b': evalTemp_0}, cqf_index_hints_, Reverse]
`;

    const actualStr = removeUUIDsFromExplain(db, res);
    assert.eq(expectedStr, actualStr);
}

// Hint index {a: 1} to disable index {b:1}. Check that index {b: 1} multikeyness info can eliminate
// PathTraverse.
{
    const res = runWithParams(
        [
            {key: 'internalCascadesOptimizerExplainVersion', value: "v2"},
            {key: "internalCascadesOptimizerUseDescriptiveVarNames", value: true}
        ],
        () => t.explain("executionStats").find({a: {$gt: 0}, b: 2}).hint("a_1").finish());

    const expectedStr =
        `Root [{scan_0}]
NestedLoopJoin [joinType: Inner, {rid_1}]
|   |   Const [true]
|   Filter []
|   |   EvalFilter []
|   |   |   Variable [evalTemp_4]
|   |   PathCompare [Eq]
|   |   Const [2]
|   LimitSkip [limit: 1, skip: 0]
|   Seek [ridProjection: rid_1, {'<root>': scan_0, 'b': evalTemp_4}, cqf_index_hints_]
Unique [{rid_1}]
IndexScan [{'<rid>': rid_1}, scanDefName: cqf_index_hints_, indexDefName: a_1, interval: {(Const [0], Const [""])}]
`;

    const actualStr = removeUUIDsFromExplain(db, res);
    assert.eq(expectedStr, actualStr);
}

// Hint collection scan to disable indexes. Check that multikeyness info from a partial index
// (currently unsupported) does not eliminate PathTraverse.
{
    t.hideIndex({a: 1});
    t.hideIndex({b: 1});
    t.createIndex({b: 1}, {partialFilterExpression: {a: 2}});

    const res = runWithParams(
        [
            {key: 'internalCascadesOptimizerExplainVersion', value: "v2"},
            {key: "internalCascadesOptimizerUseDescriptiveVarNames", value: true},
            // TODO SERVER-83441: Implement top-level fields pushdown from Filter into CollScan.
            {key: "internalCascadesOptimizerDisableSargableWhenNoIndexes", value: false}
        ],
        () => t.explain("executionStats").find({b: 2}).hint({$natural: -1}).finish());

    const expectedStr =
        `Root [{scan_0}]
Filter []
|   EvalFilter []
|   |   Variable [evalTemp_0]
|   PathTraverse [1]
|   PathCompare [Eq]
|   Const [2]
PhysicalScan [{'<root>': scan_0, 'b': evalTemp_0}, cqf_index_hints_, Reverse]
`;

    const actualStr = removeUUIDsFromExplain(db, res);
    assert.eq(expectedStr, actualStr);
}
