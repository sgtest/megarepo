// Tests the undocumented 'showMigrationEvents' option for change streams.
//
// @tags: [
//   requires_majority_read_concern,
//   uses_change_streams,
// ]
import {assertErrorCode} from "jstests/aggregation/extras/utils.js";

function checkEvents(changeStream, expectedEvents) {
    expectedEvents.forEach((event) => {
        assert.soon(() => changeStream.hasNext());
        let next = changeStream.next();
        assert.eq(next.operationType, event["operationType"]);
        assert.eq(next.documentKey, {_id: event["_id"]});
    });
}

function makeEvent(docId, opType) {
    assert(typeof docId === 'number');
    assert(typeof opType === 'string' && (opType === 'insert' || opType === 'delete'));
    return ({_id: docId, operationType: opType});
}

// TODO WT-3864: Re-enable test for LSM once transaction visibility bug in LSM is resolved.
if (jsTest.options().wiredTigerCollectionConfigString === "type=lsm") {
    jsTestLog("Skipping test because we're running with WiredTiger's LSM tree.");
    quit();
}

const rsNodeOptions = {
    // Use a higher frequency for periodic noops to speed up the test.
    setParameter: {periodicNoopIntervalSecs: 1, writePeriodicNoops: true}
};
const st =
    new ShardingTest({shards: 2, mongos: 1, rs: {nodes: 1}, other: {rsOptions: rsNodeOptions}});

const mongos = st.s;
const mongosColl = mongos.getCollection('test.chunk_mig');
const mongosDB = mongos.getDB("test");

// Enable sharding to inform mongos of the database, allowing us to open a cursor. All chunks start
// on shard0
assert.commandWorked(
    mongos.adminCommand({enableSharding: mongosDB.getName(), primaryShard: st.shard0.shardName}));

// Open a change stream cursor before the collection is sharded.
const changeStreamShardZero = st.shard0.getCollection('test.chunk_mig').aggregate([
    {$changeStream: {showMigrationEvents: true}}
]);
const changeStreamShardOne = st.shard1.getCollection('test.chunk_mig').aggregate([
    {$changeStream: {showMigrationEvents: true}}
]);

// Change streams opened on mongos do not allow showMigrationEvents to be set to true.
assertErrorCode(mongosColl, [{$changeStream: {showMigrationEvents: true}}], 31123);

assert(!changeStreamShardZero.hasNext(), "Do not expect any results yet");
assert(!changeStreamShardOne.hasNext(), "Do not expect any results yet");

jsTestLog("Sharding collection");
// Once we have a cursor, actually shard the collection.
assert.commandWorked(
    mongos.adminCommand({shardCollection: mongosColl.getFullName(), key: {_id: 1}}));

// Insert two documents.
assert.commandWorked(mongosColl.insert({_id: 0}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 20}, {writeConcern: {w: "majority"}}));

// Split the collection into two chunks: [MinKey, 10) and [10, MaxKey].
assert.commandWorked(mongos.adminCommand({split: mongosColl.getFullName(), middle: {_id: 10}}));

jsTestLog("Migrating [10, MaxKey] chunk to shard1.");
assert.commandWorked(mongos.adminCommand({
    moveChunk: mongosColl.getFullName(),
    find: {_id: 20},
    to: st.shard1.shardName,
    _waitForDelete: true
}));

var shardZeroEventsBeforeNewShard = [makeEvent(0, "insert"), makeEvent(20, "insert")];
var shardZeroEventsAfterNewShard = [makeEvent(20, "delete")];
var shardOneEvents = [makeEvent(20, "insert")];

// Check that each change stream returns the expected events.
checkEvents(changeStreamShardZero, shardZeroEventsBeforeNewShard);
checkEvents(changeStreamShardZero, shardZeroEventsAfterNewShard);
checkEvents(changeStreamShardOne, shardOneEvents);

// Insert into both the chunks.
assert.commandWorked(mongosColl.insert({_id: 1}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 21}, {writeConcern: {w: "majority"}}));

// Split again, and move a second chunk to the first shard. The new chunks are:
// [MinKey, 0), [0, 10), and [10, MaxKey].
jsTestLog("Moving [MinKey, 0] to shard 1");
assert.commandWorked(mongos.adminCommand({split: mongosColl.getFullName(), middle: {_id: 0}}));
assert.commandWorked(mongos.adminCommand({
    moveChunk: mongosColl.getFullName(),
    find: {_id: 5},
    to: st.shard1.shardName,
    _waitForDelete: true
}));

// Insert again, into all three chunks.
assert.commandWorked(mongosColl.insert({_id: -2}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 2}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 22}, {writeConcern: {w: "majority"}}));

var shardZeroEvents = [
    makeEvent(1, "insert"),
    makeEvent(0, "delete"),
    makeEvent(1, "delete"),
    makeEvent(-2, "insert"),
];
shardOneEvents = [
    makeEvent(21, "insert"),
    makeEvent(0, "insert"),
    makeEvent(1, "insert"),
    makeEvent(2, "insert"),
    makeEvent(22, "insert"),
];

// Check that each change stream returns the expected events.
checkEvents(changeStreamShardZero, shardZeroEvents);
checkEvents(changeStreamShardOne, shardOneEvents);

// Make sure we're at the end of the stream.
assert(!changeStreamShardZero.hasNext());
assert(!changeStreamShardOne.hasNext());

// Test that migrating the last chunk to shard 1 (meaning all chunks are now on the same shard)
// will not invalidate the change stream.

// Insert into all three chunks.
jsTestLog("Insert into all three chunks");
assert.commandWorked(mongosColl.insert({_id: -3}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 3}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 23}, {writeConcern: {w: "majority"}}));

jsTestLog("Move the [Minkey, 0) chunk to shard 1.");
assert.commandWorked(mongos.adminCommand({
    moveChunk: mongosColl.getFullName(),
    find: {_id: -5},
    to: st.shard1.shardName,
    _waitForDelete: true
}));

// Insert again, into all three chunks.
assert.commandWorked(mongosColl.insert({_id: -4}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 4}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 24}, {writeConcern: {w: "majority"}}));

// Inserts for a given migration are ordered by `RecordId` in the recipient shard. For normal
// collections that's the insertion order but for clustered colls that's the cluster key order.
const clustered = mongosColl.getIndexes()[0].clustered;

// Check that each change stream returns the expected events.
shardZeroEvents = [
    makeEvent(-3, "insert"),
    makeEvent(-3, "delete"),
    makeEvent(-2, "delete"),
];
shardOneEvents = clustered ? [
    makeEvent(3, "insert"),
    makeEvent(23, "insert"),
    makeEvent(-3, "insert"), // Clustered order.
    makeEvent(-2, "insert"),
    makeEvent(-4, "insert"),
    makeEvent(4, "insert"),
    makeEvent(24, "insert"),
] : [
    makeEvent(3, "insert"),
    makeEvent(23, "insert"),
    makeEvent(-2, "insert"),
    makeEvent(-3, "insert"), // Non-clustered order.
    makeEvent(-4, "insert"),
    makeEvent(4, "insert"),
    makeEvent(24, "insert"),
];

checkEvents(changeStreamShardZero, shardZeroEvents);
checkEvents(changeStreamShardOne, shardOneEvents);

// Now test that adding a new shard and migrating a chunk to it will continue to
// return the correct results.
const newShard = new ReplSetTest({name: "newShard", nodes: 1, nodeOptions: rsNodeOptions});
newShard.startSet({shardsvr: ''});
newShard.initiate();
assert.commandWorked(mongos.adminCommand({addShard: newShard.getURL(), name: "newShard"}));
const changeStreamNewShard = newShard.getPrimary().getCollection('test.chunk_mig').aggregate([
    {$changeStream: {showMigrationEvents: true}}
]);

// At this point, there haven't been any migrations to that shard; check that the changeStream
// works normally.
assert.commandWorked(mongosColl.insert({_id: -5}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 5}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 25}, {writeConcern: {w: "majority"}}));

shardOneEvents = [
    makeEvent(-5, "insert"),
    makeEvent(5, "insert"),
    makeEvent(25, "insert"),
];

assert(!changeStreamShardZero.hasNext(), "Do not expect any results");
checkEvents(changeStreamShardOne, shardOneEvents);
assert(!changeStreamNewShard.hasNext(), "Do not expect any results yet");

assert.commandWorked(mongosColl.insert({_id: 16}, {writeConcern: {w: "majority"}}));

// Now migrate a chunk to the new shard and verify the stream continues to return results
// from both before and after the migration.
jsTestLog("Migrating [10, MaxKey] chunk to new shard.");
assert.commandWorked(mongos.adminCommand(
    {moveChunk: mongosColl.getFullName(), find: {_id: 20}, to: "newShard", _waitForDelete: true}));
assert.commandWorked(mongosColl.insert({_id: -6}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 6}, {writeConcern: {w: "majority"}}));
assert.commandWorked(mongosColl.insert({_id: 26}, {writeConcern: {w: "majority"}}));

let shardOneEventsBeforeNewShard = [
    makeEvent(16, "insert"),
];
let shardOneEventsAfterNewShard = [
    makeEvent(16, "delete"),
    makeEvent(20, "delete"),
    makeEvent(21, "delete"),
    makeEvent(22, "delete"),
    makeEvent(23, "delete"),
    makeEvent(24, "delete"),
    makeEvent(25, "delete"),
    makeEvent(-6, "insert"),
    makeEvent(6, "insert"),
];
let newShardEvents = clustered ? [
    makeEvent(16, "insert"), // Clustered order.
    makeEvent(20, "insert"),
    makeEvent(21, "insert"),
    makeEvent(22, "insert"),
    makeEvent(23, "insert"),
    makeEvent(24, "insert"),
    makeEvent(25, "insert"),
    makeEvent(26, "insert"),
] : [
    makeEvent(20, "insert"),
    makeEvent(21, "insert"),
    makeEvent(22, "insert"),
    makeEvent(23, "insert"),
    makeEvent(24, "insert"),
    makeEvent(25, "insert"),
    makeEvent(16, "insert"), // Non-clustered order.
    makeEvent(26, "insert"),
];

// Check that each change stream returns the expected events.
assert(!changeStreamShardZero.hasNext(), "Do not expect any results");
checkEvents(changeStreamShardOne, shardOneEventsBeforeNewShard);
checkEvents(changeStreamShardOne, shardOneEventsAfterNewShard);
checkEvents(changeStreamNewShard, newShardEvents);

// Make sure all change streams are empty.
assert(!changeStreamShardZero.hasNext());
assert(!changeStreamShardOne.hasNext());
assert(!changeStreamNewShard.hasNext());

st.stop();
newShard.stopSet();