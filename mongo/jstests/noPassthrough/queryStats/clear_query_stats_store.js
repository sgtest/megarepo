/**
 * Test that the query stats store can be cleared when the cache size is reset to 0.
 * @tags: [featureFlagQueryStats]
 */
import {getQueryStats} from "jstests/libs/query_stats_utils.js";

// Turn on the collecting of telemetry metrics.
let options = {
    setParameter: {internalQueryStatsRateLimit: -1, internalQueryStatsCacheSize: "10MB"},
};

const conn = MongoRunner.runMongod(options);
const testDB = conn.getDB('test');
var coll = testDB[jsTestName()];
coll.drop();

let query = {};
for (var j = 0; j < 10; ++j) {
    query["foo.field.xyz." + j] = 1;
    query["bar.field.xyz." + j] = 2;
    query["baz.field.xyz." + j] = 3;
    coll.aggregate([{$match: query}]).itcount();
}

// Confirm number of entries in the store and that none have been evicted.
let res = getQueryStats(conn);
assert.eq(res.length, 10, res);
assert.eq(testDB.serverStatus().metrics.queryStats.numEvicted, 0);
assert.gt(testDB.serverStatus().metrics.queryStats.queryStatsStoreSizeEstimateBytes, 0);

// Command to clear the cache.
assert.commandWorked(testDB.adminCommand({setParameter: 1, internalQueryStatsCacheSize: "0MB"}));

// 10 regular queries plus the $queryStats query, means 11 entries evicted when the cache is
// cleared.
assert.eq(testDB.serverStatus().metrics.queryStats.numEvicted, 11);
assert.eq(testDB.serverStatus().metrics.queryStats.queryStatsStoreSizeEstimateBytes, 0);

// Calling $queryStats should fail when the query stats store size is 0 bytes.
assert.throwsWithCode(() => testDB.getSiblingDB("admin").aggregate([{$queryStats: {}}]), 6579000);
MongoRunner.stopMongod(conn);
