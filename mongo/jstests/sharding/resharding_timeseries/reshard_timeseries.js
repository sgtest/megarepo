// Basic tests for resharding for timeseries collection.
// @tags: [
//   featureFlagReshardingForTimeseries,
// ]
//
import {ReshardingTest} from "jstests/sharding/libs/resharding_test_fixture.js";
const ns = "reshardingDb.coll";

const reshardingTest = new ReshardingTest({numDonors: 2, numRecipients: 2, reshardInPlace: false});
reshardingTest.setup();

const donorShardNames = reshardingTest.donorShardNames;
const recipientShardNames = reshardingTest.recipientShardNames;

const timeseriesInfo = {
    timeField: 'ts',
    metaField: 'meta'
}

const timeseriesCollection = reshardingTest.createShardedCollection({
    ns: ns,
    shardKeyPattern: {'meta.x': 1},
    chunks: [
        {min: {'meta.x': MinKey}, max: {'meta.x': 0}, shard: donorShardNames[0]},
        {min: {'meta.x': 0}, max: {'meta.x': MaxKey}, shard: donorShardNames[1]},
    ],
    collOptions: {
        timeseries: timeseriesInfo,
    }
});

const bucketNss = "reshardingDb.system.buckets.coll";

let timeseriesCollDoc = reshardingTest._st.config.collections.findOne({_id: bucketNss})
assert.eq(timeseriesCollDoc.timeseriesFields.timeField, timeseriesInfo.timeField)
assert.eq(timeseriesCollDoc.timeseriesFields.metaField, timeseriesInfo.metaField)
assert.eq(timeseriesCollDoc.key, {"meta.x": 1})

// Insert some docs
assert.commandWorked(timeseriesCollection.insert([
    {data: 1, ts: new Date(), meta: {x: 1, y: -1}},
    {data: 3, ts: new Date(), meta: {x: 2, y: -2}},
    {data: 3, ts: new Date(), meta: {x: 4, y: -3}},
    {data: 1, ts: new Date(), meta: {x: 5, y: -4}}
]));

reshardingTest.withReshardingInBackground({
    newShardKeyPattern: {'meta.y': 1},
    newChunks: [
        {min: {'meta.y': MinKey}, max: {'meta.y': 0}, shard: recipientShardNames[0]},
        {min: {'meta.y': 0}, max: {'meta.y': MaxKey}, shard: recipientShardNames[1]},
    ],
});

const st = reshardingTest._st;

let timeseriesCollDocPostResharding = st.config.collections.findOne({_id: bucketNss})
// Resharding keeps timeseries fields.
assert.eq(timeseriesCollDocPostResharding.timeseriesFields.timeField, timeseriesInfo.timeField)
assert.eq(timeseriesCollDocPostResharding.timeseriesFields.metaField, timeseriesInfo.metaField)
// Resharding has updated shard key.
assert.eq(timeseriesCollDocPostResharding.key, {"meta.y": 1})

assert.eq(4, st.rs2.getPrimary().getCollection(bucketNss).countDocuments({}));
assert.eq(0, st.rs0.getPrimary().getCollection(bucketNss).countDocuments({}));
assert.eq(4, st.s0.getCollection(ns).countDocuments({}));

reshardingTest.teardown();
