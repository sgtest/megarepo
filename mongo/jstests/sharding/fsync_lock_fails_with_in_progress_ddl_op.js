/**
 * This test script
 * - Verifies that fsync with lock: true fails when a DDL operation is in progress.
 *
 * @tags: [
 *   requires_fsync,
 *   featureFlagClusterFsyncLock
 * ]
 */
import {configureFailPoint} from "jstests/libs/fail_point_util.js";
(function() {
"use strict";
const dbName = "test";
const collName = "collTest";
const ns = dbName + "." + collName;
const st = new ShardingTest({
    shards: 2,
    mongos: 1,
    mongosOptions: {setParameter: {featureFlagClusterFsyncLock: true}},
    config: 1
});
assert.commandWorked(st.s.adminCommand({enableSharding: dbName}));
st.ensurePrimaryShard(dbName, st.shard1.shardName);
const coll = st.s.getDB(dbName).getCollection(collName);
coll.insert({x: 1});
assert.eq(coll.count(), 1);

// Start refineCollectionShardKey DDL operation
let newShardKey = {_id: 1, x: 1};
st.s.getCollection(ns).createIndex(newShardKey);
let ddlOpThread = new Thread((mongosConnString, nss, newShardKey) => {
    let mongos = new Mongo(mongosConnString);
    mongos.adminCommand({refineCollectionShardKey: nss, key: newShardKey});
}, st.s0.host, ns, newShardKey);
let ddlCoordinatorFailPoint =
    configureFailPoint(st.rs1.getPrimary(), 'hangBeforeRemovingCoordinatorDocument');

ddlOpThread.start();
ddlCoordinatorFailPoint.wait();

// Run fsync command, should fail when DDL op is in progress
let fsyncLockCommand = assert.commandFailed(st.s.adminCommand({fsync: 1, lock: true}));
const errmsg = "Cannot take lock while DDL operation is in progress";
assert.eq(fsyncLockCommand.errmsg.includes(errmsg), true);

ddlCoordinatorFailPoint.off();
ddlOpThread.join();

// ensure writes are allowed since fsync failed
coll.insert({x: 2});
assert.eq(coll.count(), 2);

st.stop();
}());
