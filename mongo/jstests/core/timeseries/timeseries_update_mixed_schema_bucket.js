/**
 * Tests directly updating a time-series bucket to contain mixed schema.
 *
 * @tags: [
 *   # $listCatalog does not include the tenant prefix in its results.
 *   command_not_supported_in_serverless,
 *   requires_timeseries,
 *   requires_fcv_80,
 * ]
 */

const testDB = db.getSiblingDB(jsTestName());
const collName = "ts";

assert.commandWorked(testDB.runCommand({drop: collName}));
assert.commandWorked(
    testDB.createCollection(collName, {timeseries: {timeField: "t", metaField: "m"}}));
const coll = testDB[collName];
const bucketsColl = testDB["system.buckets." + collName];

const timeseriesBucketsMayHaveMixedSchemaData = function() {
    return bucketsColl.aggregate([{$listCatalog: {}}])
        .toArray()[0]
        .md.timeseriesBucketsMayHaveMixedSchemaData;
};

const bucket = {
    _id: ObjectId("65a6eb806ffc9fa4280ecac4"),
    control: {
        version: NumberInt(1),
        min: {
            _id: ObjectId("65a6eba7e6d2e848e08c3750"),
            t: ISODate("2024-01-16T20:48:00Z"),
            a: 0,
        },
        max: {
            _id: ObjectId("65a6eba7e6d2e848e08c3751"),
            t: ISODate("2024-01-16T20:48:39.448Z"),
            a: 1,
        },
    },
    meta: 0,
    data: {
        _id: {
            0: ObjectId("65a6eba7e6d2e848e08c3750"),
            1: ObjectId("65a6eba7e6d2e848e08c3751"),
        },
        t: {
            0: ISODate("2024-01-16T20:48:39.448Z"),
            1: ISODate("2024-01-16T20:48:39.448Z"),
        },
        a: {
            0: 0,
            1: 1,
        },
    }
};

const update = function() {
    return bucketsColl.update({_id: bucket._id},
                              {$set: {"control.min.a": 1, "control.max.a": "a", "data.a.0": "a"}});
};

assert.commandWorked(bucketsColl.insert(bucket));
assert.commandFailedWithCode(update(), ErrorCodes.CannotInsertTimeseriesBucketsWithMixedSchema);
assert.eq(timeseriesBucketsMayHaveMixedSchemaData(), false);
assert.commandWorked(
    testDB.runCommand({collMod: collName, timeseriesBucketsMayHaveMixedSchemaData: true}));
assert.eq(timeseriesBucketsMayHaveMixedSchemaData(), true);
assert.commandWorked(update());
assert.commandFailedWithCode(
    testDB.runCommand({collMod: collName, timeseriesBucketsMayHaveMixedSchemaData: false}),
    ErrorCodes.InvalidOptions);
assert.commandWorked(bucketsColl.deleteOne({_id: bucket._id}));
assert.eq(timeseriesBucketsMayHaveMixedSchemaData(), true);
