/**
 * Tests that direct updates to a timeseries bucket collection close the bucket, preventing further
 * inserts to land in that bucket, including the case where a concurrent catalog write causes
 * a write conflict.
 */
import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {funWithArgs} from "jstests/libs/parallel_shell_helpers.js";

const conn = MongoRunner.runMongod();

const dbName = jsTestName();
const testDB = conn.getDB(dbName);
assert.commandWorked(testDB.dropDatabase());

const collName = 'test';

const timeFieldName = 'time';
const times = [
    ISODate('2021-01-01T01:00:00Z'),
    ISODate('2021-01-01T01:10:00Z'),
    ISODate('2021-01-01T01:20:00Z')
];
let docs = [
    {_id: 0, [timeFieldName]: times[0]},
    {_id: 1, [timeFieldName]: times[1]},
    {_id: 2, [timeFieldName]: times[2]}
];

const coll = testDB.getCollection(collName);
const bucketsColl = testDB.getCollection('system.buckets.' + coll.getName());
coll.drop();

assert.commandWorked(
    testDB.createCollection(coll.getName(), {timeseries: {timeField: timeFieldName}}));
assert.contains(bucketsColl.getName(), testDB.getCollectionNames());

assert.commandWorked(coll.insert(docs[0]));
assert.docEq(docs.slice(0, 1), coll.find().sort({_id: 1}).toArray());

let buckets = bucketsColl.find().sort({_id: 1}).toArray();
assert.eq(buckets.length, 1);
assert.eq(buckets[0].control.min[timeFieldName], times[0]);
assert.eq(buckets[0].control.max[timeFieldName], times[0]);

const fpInsert = configureFailPoint(conn, "hangTimeseriesInsertBeforeWrite");
const awaitInsert = startParallelShell(
    funWithArgs(function(dbName, collName, doc) {
        assert.commandWorked(
            db.getSiblingDB(dbName).getCollection(collName).insert(doc, {ordered: false}));
    }, dbName, coll.getName(), docs[1]), conn.port);
fpInsert.wait();

const modified = buckets[0];
modified.control.closed = true;

const fpUpdate = configureFailPoint(conn, "hangTimeseriesDirectModificationBeforeWriteConflict");
const awaitUpdate = startParallelShell(
    funWithArgs(function(dbName, collName, update) {
        const updateResult = assert.commandWorked(db.getSiblingDB(dbName)
                                                      .getCollection('system.buckets.' + collName)
                                                      .update({_id: update._id}, update));
        assert.eq(updateResult.nMatched, 1);
        assert.eq(updateResult.nModified, 1);
    }, dbName, coll.getName(), modified), conn.port);
fpUpdate.wait();

fpUpdate.off();
fpInsert.off();
awaitUpdate();
awaitInsert();

// The expected ordering is that the insert finished, then the update overwrote the bucket document,
// so there should be one document, and a closed flag.

assert.docEq(docs.slice(0, 1), coll.find().sort({_id: 1}).toArray());

buckets = bucketsColl.find().sort({_id: 1}).toArray();
assert.eq(buckets.length, 1);
assert.eq(buckets[0].control.min[timeFieldName], times[0]);
assert.eq(buckets[0].control.max[timeFieldName], times[0]);
assert(buckets[0].control.closed);

// Now another insert should generate a new bucket.

assert.commandWorked(coll.insert(docs[2]));
assert.docEq([docs[0], docs[2]], coll.find().sort({_id: 1}).toArray());

buckets = bucketsColl.find().sort({_id: 1}).toArray();
assert.eq(buckets.length, 2);
assert.eq(buckets[0].control.min[timeFieldName], times[0]);
assert.eq(buckets[0].control.max[timeFieldName], times[0]);
assert.eq(buckets[1].control.min[timeFieldName], times[2]);
assert.eq(buckets[1].control.max[timeFieldName], times[2]);

MongoRunner.stopMongod(conn);
