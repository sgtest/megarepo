/**
 * Tests that inserts with a variety of skips in measurements (i.e, measurements that don't have
 * values for certain fields) are handled correctly in the time-series intermediate data check
 * (specifically, that we do not unexpectedly fail the test).
 * @tags: [
 *   featureFlagTimeseriesAlwaysUseCompressedBuckets,
 *   requires_fcv_80,
 * ]
 */

const conn = MongoRunner.runMongod();
const testDB = conn.getDB(jsTestName());
const coll = testDB.coll;
const bucketsColl = testDB.system.buckets.coll;
const time = ISODate("2024-01-16T20:48:39.448Z");

coll.drop();
assert.commandWorked(
    testDB.createCollection(coll.getName(), {timeseries: {timeField: "t", metaField: "m"}}));
assert.commandWorked(coll.insertMany([
    {t: time, m: 0, a: 1, b: 3, c: 4},  // Create Bucket 0
    {t: time, m: 1},                    // Create Bucket 1
    {t: time, m: 2}                     // Create Bucket 2
]));

assert.commandWorked(coll.insertMany([
    {t: time, m: 0, a: 1, b: 3, c: 4},              // Bucket 0
    {t: time, m: 0, a: 2, c: 4},                    // Bucket 0
    {t: time, m: 0, a: 1, b: 3},                    // Bucket 0
    {t: time, m: 0, d: 4, c: 4},                    // Bucket 0
    {t: time, m: 0},                                // Bucket 0
    {t: time, m: 0, a: 1, b: 3, c: 4, d: 3},        // Bucket 0
    {t: time, m: 1},                                // Bucket 1
    {t: time, m: 1, a: 2},                          // Bucket 1
    {t: time, m: 1, a: 1, c: 3},                    // Bucket 1
    {t: time, m: 1, a: 2},                          // Bucket 1
    {t: time, m: 1, a: 1, d: 3},                    // Bucket 1
    {t: time, m: 1, a: 2},                          // Bucket 1
    {t: time, m: 2},                                // Bucket 2
    {t: time, m: 2},                                // Bucket 2
    {t: time, m: 2},                                // Bucket 2
    {t: time, m: 2},                                // Bucket 2
    {t: time, m: 2},                                // Bucket 2
    {t: time, m: 2, a: 2, b: 2, c: 2, d: 2, e: 5},  // Bucket 2
]));

MongoRunner.stopMongod(conn);
