/**
 * This test verifies that the optimizer fast path is used for specific query patterns.
 * @tags: [
 *  requires_fcv_73,
 * ]
 */
import {isBonsaiFastPathPlan} from "jstests/libs/analyze_plan.js";
import {
    checkCascadesOptimizerEnabled,
    runWithParams,
} from "jstests/libs/optimizer_utils.js";

if (!checkCascadesOptimizerEnabled(db)) {
    jsTestLog("Skipping test because the Bonsai optimizer is not enabled.");
    quit();
}

const numRecords = 100;
const coll = db[jsTestName()];
const paramObj = [{key: "internalCascadesOptimizerDisableFastPath", value: 0}];
coll.drop();

assert.commandWorked(coll.insertMany([...Array(numRecords).keys()].map(i => {
    return {_id: i, a: 1};
})));

{
    // Empty find should use the fast path.
    // TODO SERVER-83578: Remove the runWithParams.
    const explain = runWithParams(
        paramObj, () => assert.commandWorked(coll.explain("executionStats").find().finish()));
    assert(isBonsaiFastPathPlan(db, explain));
    assert.eq(numRecords, explain.executionStats.nReturned);
}
{
    // Empty match should use fast path.
    const explain = runWithParams(
        paramObj,
        () => assert.commandWorked(coll.explain("executionStats").aggregate([{$match: {}}])));
    assert(isBonsaiFastPathPlan(db, explain));
    assert.eq(numRecords, explain.executionStats.nReturned);
}
{
    // Empty aggregate should use fast path.
    const explain = runWithParams(
        paramObj, () => assert.commandWorked(coll.explain("executionStats").aggregate([])));
    assert(isBonsaiFastPathPlan(db, explain));
    assert.eq(numRecords, explain.executionStats.nReturned);
}
{
    // Find with predicates should not use a fast path.
    const explain = runWithParams(
        paramObj,
        () => assert.commandWorked(coll.explain("executionStats").find({a: 1, b: 2}).finish()));
    assert(!isBonsaiFastPathPlan(db, explain));
    assert.eq(0, explain.executionStats.nReturned);
}
{
    // Agg with matches should not use a fast path.
    const explain =
        runWithParams(paramObj,
                      () => assert.commandWorked(
                          coll.explain("executionStats").aggregate([{$match: {a: 1, b: 2}}])));
    assert(!isBonsaiFastPathPlan(db, explain));
    assert.eq(0, explain.executionStats.nReturned);
}
{
    // Agg with matches should not use a fast path.
    const explain = runWithParams(
        paramObj,
        () => assert.commandWorked(
            coll.explain("executionStats").aggregate([{$match: {a: 1}}, {$match: {b: 2}}])));
    assert(!isBonsaiFastPathPlan(db, explain));
    assert.eq(0, explain.executionStats.nReturned);
}
{
    // Agg with an empty and a non-emtpy $match in the pipeline to ensure that the pattern matching
    // uses the full query and not only the first part of the pipeline.
    const explain = runWithParams(
        paramObj,
        () => assert.commandWorked(
            coll.explain("executionStats").aggregate([{$match: {}}, {$match: {b: 2}}])));
    assert(!isBonsaiFastPathPlan(db, explain));
    assert.eq(0, explain.executionStats.nReturned);
}
