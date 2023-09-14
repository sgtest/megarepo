/**
 * Tests for basic functionality of the move collection feature.
 *
 * @tags: [
 *  requires_fcv_72,
 *  featureFlagReshardingImprovements,
 *  featureFlagMoveCollection
 * ]
 */

import {DiscoverTopology} from "jstests/libs/discover_topology.js";

(function() {
'use strict';

var st = new ShardingTest({mongos: 1, shards: 2});

const dbName = 'db';
const collName = 'foo';
const ns = dbName + '.' + collName;
let mongos = st.s0;
let shard = st.shard0;
let cmdObj = {moveCollection: ns, toShard: st.shard0.shardName};
const topology = DiscoverTopology.findConnectedNodes(mongos);
const configsvr = new Mongo(topology.configsvr.nodes[0]);

// Fail if sharding is disabled.
assert.commandFailedWithCode(mongos.adminCommand(cmdObj), ErrorCodes.NamespaceNotFound);

// Implicit collection creation.
const coll = st.s.getDB(dbName)["collName"];
assert.commandWorked(coll.insert({oldKey: 1}));

// Fail if collection is not tracked.
// TODO(SERVER-80156): update test case to succeed on unsharded collections
assert.commandFailedWithCode(mongos.adminCommand(cmdObj), ErrorCodes.NamespaceNotFound);

assert.commandWorked(mongos.adminCommand({enableSharding: dbName}));
assert.commandWorked(mongos.getCollection(ns).createIndex({oldKey: 1}));
assert.commandWorked(mongos.adminCommand({shardCollection: ns, key: {oldKey: 1}}));

// Fail if missing required field toShard.
assert.commandFailedWithCode(mongos.adminCommand({moveCollection: ns}),
                             ErrorCodes.IDLFailedToParse);

// Succeed if command called on mongos.
assert.commandWorked(mongos.adminCommand(cmdObj));

// Fail if command called on shard.
assert.commandFailedWithCode(shard.adminCommand(cmdObj), ErrorCodes.CommandNotFound);

const metrics = configsvr.getDB('admin').serverStatus({}).shardingStatistics.moveCollection;

assert.eq(metrics.countStarted, 1);
assert.eq(metrics.countSucceeded, 1);
assert.eq(metrics.countFailed, 0);
assert.eq(metrics.countCanceled, 0);

st.stop();
})();
