/**
 * Tests commands(e.g. aggregate, create) that use pipeline stages not supported in API Version 1.
 *
 * Tests which create views aren't expected to work when collections are implicitly sharded.
 * @tags: [
 *   assumes_read_concern_unchanged,
 *   assumes_read_preference_unchanged,
 *   assumes_unsharded_collection,
 *   uses_api_parameters,
 * ]
 */

const testDb = db.getSiblingDB(jsTestName());
const collName = "api_version_pipeline_stages";
const coll = testDb[collName];
coll.drop();
coll.insert({a: 1});

const unstablePipelines = [
    [{$collStats: {count: {}, latencyStats: {}}}],
    [{$currentOp: {}}],
    [{$indexStats: {}}],
    [{$listLocalSessions: {}}],
    [{$listSessions: {}}],
    [{$planCacheStats: {}}],
    [{$unionWith: {coll: "coll2", pipeline: [{$collStats: {latencyStats: {}}}]}}],
    [{$lookup: {from: "coll2", pipeline: [{$indexStats: {}}]}}],
    [{$lookup: {from: "coll2", _internalCollation: {locale: "simple"}}}],
    [{$facet: {field1: [], field2: [{$indexStats: {}}]}}],
];

function assertAggregateFailsWithAPIStrict(pipeline) {
    assert.commandFailedWithCode(testDb.runCommand({
        aggregate: collName,
        pipeline: pipeline,
        cursor: {},
        apiStrict: true,
        apiVersion: "1"
    }),
                                 ErrorCodes.APIStrictError);
}

for (let pipeline of unstablePipelines) {
    // Assert error thrown when running a pipeline with stages not in API Version 1.
    assertAggregateFailsWithAPIStrict(pipeline);

    // Assert error thrown when creating a view on a pipeline with stages not in API Version 1.
    assert.commandFailedWithCode(testDb.runCommand({
        create: 'api_version_pipeline_stages_should_fail',
        viewOn: collName,
        pipeline: pipeline,
        apiStrict: true,
        apiVersion: "1"
    }),
                                 ErrorCodes.APIStrictError);
}

// Test that $collStats is allowed in APIVersion 1, even with 'apiStrict: true', so long as the only
// parameter given is 'count'.
assertAggregateFailsWithAPIStrict([{$collStats: {latencyStats: {}}}]);
assertAggregateFailsWithAPIStrict([{$collStats: {latencyStats: {histograms: true}}}]);
assertAggregateFailsWithAPIStrict([{$collStats: {storageStats: {}}}]);
assertAggregateFailsWithAPIStrict([{$collStats: {queryExecStats: {}}}]);
assertAggregateFailsWithAPIStrict([{$collStats: {latencyStats: {}, queryExecStats: {}}}]);
assertAggregateFailsWithAPIStrict(
    [{$collStats: {latencyStats: {}, storageStats: {scale: 1024}, queryExecStats: {}}}]);

assert.commandWorked(testDb.runCommand({
    aggregate: collName,
    pipeline: [{$collStats: {}}],
    cursor: {},
    apiVersion: "1",
    apiStrict: true
}));
assert.commandWorked(testDb.runCommand({
    aggregate: collName,
    pipeline: [{$collStats: {count: {}}}],
    cursor: {},
    apiVersion: "1",
    apiStrict: true
}));

// Test that by running the aggregate command with $collStats + $group like our drivers do to
// compute the count, we get back a single result in the first batch - no getMore is required.
// This test is meant to mimic a drivers test and serve as a warning if we may be making a breaking
// change for the drivers.
const cmdResult = assert.commandWorked(testDb.runCommand({
    aggregate: collName,
    pipeline: [{$collStats: {count: {}}}, {$group: {_id: 1, count: {$sum: "$count"}}}],
    cursor: {},
    apiVersion: "1",
    apiStrict: true
}));

assert.eq(cmdResult.cursor.id, 0, cmdResult);
assert.eq(cmdResult.cursor.firstBatch.length, 1, cmdResult);
assert.eq(cmdResult.cursor.firstBatch[0].count, 1, cmdResult);