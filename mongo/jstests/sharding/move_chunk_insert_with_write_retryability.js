import {testMoveChunkWithSession} from "jstests/sharding/move_chunk_with_session_helper.js";

// Prevent unnecessary elections in the first shard replica set. Shard 'rs1' shard will need its
// secondary to get elected, so we don't give it a zero priority.
var st = new ShardingTest({
    mongos: 2,
    shards: {
        rs0: {nodes: [{rsConfig: {}}, {rsConfig: {priority: 0}}]},
        rs1: {nodes: [{rsConfig: {}}, {rsConfig: {}}]}
    }
});
assert.commandWorked(
    st.s.adminCommand({enableSharding: 'test', primaryShard: st.shard0.shardName}));

var coll = 'insert';
var cmd = {
    insert: coll,
    documents: [{x: 10}, {x: 30}],
    ordered: false,
    lsid: {id: UUID()},
    txnNumber: NumberLong(34),
};
var setup = function() {};
var checkRetryResult = function(result, retryResult) {
    assert.eq(result.ok, retryResult.ok);
    assert.eq(result.n, retryResult.n);
    assert.eq(result.writeErrors, retryResult.writeErrors);
    assert.eq(result.writeConcernErrors, retryResult.writeConcernErrors);
};
var checkDocuments = function(coll) {
    assert.eq(1, coll.find({x: 10}).itcount());
    assert.eq(1, coll.find({x: 30}).itcount());
};

testMoveChunkWithSession(st, coll, cmd, setup, checkRetryResult, checkDocuments);

st.stop();
