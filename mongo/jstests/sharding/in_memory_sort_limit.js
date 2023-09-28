// Test SERVER-14306.  Do a query directly against a mongod with an in-memory sort and a limit that
// doesn't cause the in-memory sort limit to be reached, then make sure the same limit also doesn't
// cause the in-memory sort limit to be reached when running through a mongos.
var st = new ShardingTest({
    shards: 2,
    other: {
        shardOptions:
            {setParameter: {internalQueryMaxBlockingSortMemoryUsageBytes: 32 * 1024 * 1024}}
    }
});
assert.commandWorked(
    st.s.adminCommand({enableSharding: 'test', primaryShard: st.shard0.shardName}));

// Make sure that at least 1 chunk is on another shard so that mongos doesn't treat this as a
// single-shard query (which doesn't exercise the bug)
assert.commandWorked(
    st.s.adminCommand({shardCollection: 'test.skip', key: {_id: 'hashed'}, numInitialChunks: 64}));

var mongosCol = st.s.getDB('test').getCollection('skip');
var shardCol = st.shard0.getDB('test').getCollection('skip');

// Create enough data to exceed the 32MB in-memory sort limit (per shard)
var filler = new Array(10240).toString();
var bulkOp = mongosCol.initializeOrderedBulkOp();
for (var i = 0; i < 12800; i++) {
    bulkOp.insert({x: i, str: filler});
}
assert.commandWorked(bulkOp.execute());

var passLimit = 2000;
var failLimit = 4000;

// Test on MongoD
jsTestLog("Test no error with limit of " + passLimit + " on mongod");
assert.eq(passLimit, shardCol.find().sort({x: 1}).allowDiskUse(false).limit(passLimit).itcount());

jsTestLog("Test error with limit of " + failLimit + " on mongod");
assert.throwsWithCode(
    () => shardCol.find().sort({x: 1}).allowDiskUse(false).limit(failLimit).itcount(),
    ErrorCodes.QueryExceededMemoryLimitNoDiskUseAllowed);

// Test on MongoS
jsTestLog("Test no error with limit of " + passLimit + " on mongos");
assert.eq(passLimit, mongosCol.find().sort({x: 1}).allowDiskUse(false).limit(passLimit).itcount());

jsTestLog("Test error with limit of " + failLimit + " on mongos");
assert.throwsWithCode(
    () => mongosCol.find().sort({x: 1}).allowDiskUse(false).limit(failLimit).itcount(),
    ErrorCodes.QueryExceededMemoryLimitNoDiskUseAllowed);

st.stop();