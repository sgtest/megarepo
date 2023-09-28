/**
 * Test migrating a big chunk while deletions are happening within that chunk. Test is slightly
 * non-deterministic, since removes could happen before migrate starts. Protect against that by
 * making chunk very large.
 *
 * This test is labeled resource intensive because its total io_write is 88MB compared to a median
 * of 5MB across all sharding tests in wiredTiger.
 * @tags: [resource_intensive]
 */
const isCodeCoverageEnabled = buildInfo().buildEnvironment.ccflags.includes('-ftest-coverage');
const isSanitizerEnabled = buildInfo().buildEnvironment.ccflags.includes('-fsanitize');
const slowTestVariant = isCodeCoverageEnabled || isSanitizerEnabled;

var st = new ShardingTest({shards: 2, mongos: 1});

var dbname = "test";
var coll = "foo";
var ns = dbname + "." + coll;

assert.commandWorked(
    st.s0.adminCommand({enablesharding: dbname, primaryShard: st.shard1.shardName}));

var t = st.s0.getDB(dbname).getCollection(coll);

var bulk = t.initializeUnorderedBulkOp();
for (var i = 0; i < 200000; i++) {
    bulk.insert({a: i});
}
assert.commandWorked(bulk.execute());

// enable sharding of the collection. Only 1 chunk.
t.createIndex({a: 1});

assert.commandWorked(st.s0.adminCommand({shardcollection: ns, key: {a: 1}}));

// start a parallel shell that deletes things
var join = startParallelShell("db." + coll + ".remove({});", st.s0.port);

// migrate while deletions are happening
try {
    assert.commandWorked(st.s0.adminCommand(
        {moveChunk: ns, find: {a: 1}, to: st.getOther(st.getPrimaryShard(dbname)).name}));
} catch (e) {
    const expectedFailureMessage = "startCommit timed out waiting for the catch up completion.";
    if (!slowTestVariant || !e.message.match(expectedFailureMessage)) {
        throw e;
    }
}

join();

st.stop();