/**
 * Tests deleteOne with id without shard key uses PM-3190 for retryable
 * writes and doesn't for transactions.
 *
 * @tags: [featureFlagUpdateOneWithIdWithoutShardKey, requires_fcv_73]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {funWithArgs} from "jstests/libs/parallel_shell_helpers.js";
import {CreateShardedCollectionUtil} from "jstests/sharding/libs/create_sharded_collection_util.js";

const st = new ShardingTest({shards: 2, mongos: 1});
const mongos = st.s0;
let db = mongos.getDB(jsTestName());

const coll = db.coll;
coll.drop();

CreateShardedCollectionUtil.shardCollectionWithChunks(coll, {x: 1}, [
    {min: {x: MinKey}, max: {x: 0}, shard: st.shard0.shardName},
    {min: {x: 0}, max: {x: MaxKey}, shard: st.shard1.shardName},
]);

// Write three documents.
assert.commandWorked(coll.insert({x: -1, _id: -1}));
assert.commandWorked(coll.insert({x: -1, _id: 0}));
assert.commandWorked(coll.insert({x: 1, _id: 1}));

const fp = configureFailPoint(st.s, 'hangAfterCompletingWriteWithoutShardKeyWithId');

// Test that transactions do not use broadcast protocol per PM-3190.
let session = st.s.startSession({retryWrites: false});
session.startTransaction();

let sessionColl = session.getDatabase(db.getName()).getCollection(coll.getName());

let deleteCmd = {
    deletes: [
        {q: {_id: -1}, limit: 1},
    ],
    txnNumber: NumberLong(0),
};
assert.commandWorked(sessionColl.runCommand("delete", deleteCmd));
session.commitTransaction();
session.endSession();

// Test that retryable transactions do not use broadcast protocol per PM-3190.
session = st.s.startSession({retryWrites: true});
session.startTransaction();

sessionColl = session.getDatabase(db.getName()).getCollection(coll.getName());
deleteCmd = {
    deletes: [
        {q: {_id: 0}, limit: 1},
    ],
    txnNumber: NumberLong(0),
};
assert.commandWorked(sessionColl.runCommand("delete", deleteCmd));
session.commitTransaction();
session.endSession();

// Test that retryable writes use broadcast protocol per PM-3190
session = st.s.startSession({retryWrites: true});
const lsid = session.getSessionId();

const joinDelete = startParallelShell(
    funWithArgs(function(dbName, collName, lsid) {
        assert.commandWorked(db.getSiblingDB(dbName).getCollection(collName).runCommand("delete", {
            deletes: [
                {q: {_id: 1}, limit: 1},
            ],
            lsid: lsid,
            txnNumber: NumberLong(5)
        }));
    }, db.getName(), coll.getName(), lsid), mongos.port);

// We should hit the configured failpoint if PM-3190 code is used.
fp.wait();
fp.off();
joinDelete();

session.endSession();

st.stop();
