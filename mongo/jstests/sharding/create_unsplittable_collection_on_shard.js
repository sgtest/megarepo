/*
 * Test the test command createUnsplittableCollection. This command is a temporary wrapper on
 * shardCollection that allows you to create unsplittable collection aka tracked unsharded
 * collection. Since we use the same coordinator, we both check the createUnsplittableCollection
 * works and that shardCollection won't generate unsplittable collection.
 * @tags: [
 *   featureFlagTrackUnshardedCollectionsOnShardingCatalog,
 *   featureFlagUnsplittableCollectionsOnNonPrimaryShard,
 *   multiversion_incompatible,
 *   assumes_balancer_off,
 * ]
 */

const kDbName = "test";

const st = new ShardingTest({shards: 2});
const mongos = st.s;
const shard0 = st.shard0.shardName;
const shard1 = st.shard1.shardName;

jsTest.log(
    'Check that test command createUnsplittableCollection can create a collection in a different shard than the dbPrimary');
{
    const kDataColl = 'unsplittable_collection_on_different_shard';
    const kDataCollNss = kDbName + '.' + kDataColl;

    assert.commandWorked(st.s.getDB(kDbName).runCommand(
        {createUnsplittableCollection: kDataColl, dataShard: shard1}));

    let res = assert.commandWorked(
        st.rs1.getPrimary().getDB(kDbName).runCommand({listIndexes: kDataColl}));
    let indexes = res.cursor.firstBatch;
    assert(indexes.length === 1);

    let col = st.s.getCollection('config.collections').findOne({_id: kDataCollNss});

    assert.eq(st.s.getCollection('config.chunks').countDocuments({uuid: col.uuid}), 1);

    let chunk = st.s.getCollection('config.chunks').findOne({uuid: col.uuid});

    assert.eq(chunk.shard, shard1);
}
st.stop();
