/**
 * Test that calls to read from telemetry store fail when feature flag is turned off and sampling
 * rate > 0.
 */
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

// Set sampling rate to -1.
let options = {
    setParameter: {internalQueryStatsRateLimit: -1},
};
const conn = MongoRunner.runMongod(options);
const testdb = conn.getDB('test');

// This test specifically tests error handling when the feature flag is not on.
// TODO SERVER-65800 This test can be deleted when the feature is on by default.
if (!conn || FeatureFlagUtil.isEnabled(testdb, "QueryStats")) {
    jsTestLog(`Skipping test since feature flag is disabled. conn: ${conn}`);
    if (conn) {
        MongoRunner.stopMongod(conn);
    }
    quit();
}

var coll = testdb[jsTestName()];
coll.drop();

// Bulk insert documents to reduces roundtrips and make timeout on a slow machine less likely.
const bulk = coll.initializeUnorderedBulkOp();
for (let i = 1; i <= 20; i++) {
    bulk.insert({foo: 0, bar: Math.floor(Math.random() * 3)});
}
assert.commandWorked(bulk.execute());

// Pipeline to read telemetry store should fail without feature flag turned on even though sampling
// rate is > 0.
assert.commandFailedWithCode(
    testdb.adminCommand({aggregate: 1, pipeline: [{$queryStats: {}}], cursor: {}}),
    ErrorCodes.QueryFeatureNotAllowed);

// Pipeline, with a filter, to read telemetry store fails without feature flag turned on even though
// sampling rate is > 0.
assert.commandFailedWithCode(testdb.adminCommand({
    aggregate: 1,
    pipeline: [{$queryStats: {}}, {$match: {"key.queryShape.find": {$eq: "###"}}}],
    cursor: {}
}),
                             ErrorCodes.QueryFeatureNotAllowed);

MongoRunner.stopMongod(conn);