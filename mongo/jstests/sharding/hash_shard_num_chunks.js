// Hash sharding with initial chunk count set.
import {findChunksUtil} from "jstests/sharding/libs/find_chunks_util.js";

var s = new ShardingTest({shards: 3});

var dbname = "test";
var coll = "foo";
var db = s.getDB(dbname);

assert.commandWorked(db.adminCommand({enablesharding: dbname, primaryShard: s.shard1.shardName}));

assert.commandWorked(db.adminCommand(
    {shardcollection: dbname + "." + coll, key: {a: "hashed"}, numInitialChunks: 500}));

s.printShardingStatus();

var numChunks = findChunksUtil.countChunksForNs(s.config, "test.foo");
assert.eq(numChunks, 500, "should be exactly 500 chunks");

s.config.shards.find().forEach(
    // Check that each shard has one third the numInitialChunks
    function(shard) {
        var numChunksOnShard = s.config.chunks.find({"shard": shard._id}).count();
        assert.gte(numChunksOnShard, Math.floor(500 / 3));
    });

// Check that the collection gets dropped correctly (which doesn't happen if pre-splitting fails
// to create the collection on all shards).
assert.commandWorked(db.runCommand({"drop": coll}), "couldn't drop empty, pre-split collection");

s.stop();