/**
 * Test that sort queries work properly on dates ouside the 32 bit epoch range,
 *  [1970-01-01 00:00:00 UTC - 2038-01-29 03:13:07 UTC], when a collection scan is used.
 *
 * @tags: [
 *     # Explain of a resolved view must be executed by mongos.
 *     directly_against_shardsvrs_incompatible,
 *     # This complicates aggregation extraction.
 *     do_not_wrap_aggregations_in_facets,
 *     # Refusing to run a test that issues an aggregation command with explain because it may
 *     # return incomplete results if interrupted by a stepdown/tenant migration.
 *     does_not_support_stepdowns,
 *     tenant_migration_incompatible,
 *     # We need a timeseries collection.
 *     requires_timeseries,
 * ]
 */
import {getAggPlanStages} from "jstests/libs/analyze_plan.js";
import {FixtureHelpers} from "jstests/libs/fixture_helpers.js";

if (FixtureHelpers.isMongos(db)) {
    const shards = db.getSiblingDB('config').shards.find().toArray();
    const shardName0 = shards.map(doc => doc._id)[0];
    const dbName = db.getName();
    db.getSiblingDB(dbName).dropDatabase();
    assert.commandWorked(db.adminCommand({enableSharding: dbName, primaryShard: shardName0}));
}

// Create unindexed collection
const coll = db.timeseries_internal_bounded_sort_extended_range;
const buckets = db['system.buckets.' + coll.getName()];
coll.drop();
assert.commandWorked(db.createCollection(coll.getName(), {timeseries: {timeField: 't'}}));

// Create collection indexed on time
const collIndexed = db.timeseries_internal_bounded_sort_extended_range_with_index;
const bucketsIndexed = db['system.buckets.' + collIndexed.getName()];
collIndexed.drop();
assert.commandWorked(db.createCollection(collIndexed.getName(), {timeseries: {timeField: 't'}}));
assert.commandWorked(collIndexed.createIndex({'t': 1}));

jsTestLog(collIndexed.getIndexes());
jsTestLog(bucketsIndexed.getIndexes());

function numShards() {
    return db.getSiblingDB('config').shards.count();
}
for (const collection of [buckets, bucketsIndexed]) {
    if (FixtureHelpers.isSharded(collection) && numShards() >= 2) {
        // Split and move data to create an interesting scenario: we have some data on each shard,
        // but all the extended-range data is on a non-primary shard. This means view resolution is
        // unaware of the extended-range data, because that happens on the primary shard.

        const shards = db.getSiblingDB('config').shards.find().toArray();
        const [shardName0, shardName1] = shards.map(doc => doc._id);

        const collName = collection.getFullName();
        // Our example data has documents between 2000-2003, and these dates are non-wrapping.
        // So this goes on the primary shard, and everything else goes on the non-primary.
        assert.commandWorked(sh.splitAt(collName, {'control.min.t': ISODate('2000-01-01')}));
        assert.commandWorked(sh.splitAt(collName, {'control.min.t': ISODate('2003-01-01')}));
        assert.commandWorked(
            sh.moveChunk(collName, {'control.min.t': ISODate('1969-01-01')}, shardName1));
        assert.commandWorked(
            sh.moveChunk(collName, {'control.min.t': ISODate('2000-01-01')}, shardName0));
        assert.commandWorked(
            sh.moveChunk(collName, {'control.min.t': ISODate('2003-01-01')}, shardName1));
    }
}

const intervalMillis = 60000;
function insertBucket(start) {
    jsTestLog("Inserting bucket starting with " + Date(start).toString());

    const batchSize = 1000;

    const batch =
        Array.from({length: batchSize}, (_, j) => ({t: new Date(start + j * intervalMillis)}));

    assert.commandWorked(coll.insert(batch));
    assert.commandWorked(collIndexed.insert(batch));
}

// Insert some data. We'll insert 5 buckets in each range, with values < 0,
// values between 0 and FFFFFFFF(unsigned), and values > FFFFFFFF. It turns out, however,
// that Javascript's Date doesn't handle dates beyond 2039 either, so we rely on lower dates
// to test for unexpected behavior.
function insertDocuments() {
    // We want to choose the underflow and overflow lower bits in such a way that we
    // encourage wrong results when the upper bytes are removed.
    const underflowMin = new Date("1969-01-01").getTime();  // Year before the 32 bit epoch
    const normalMin = new Date("2002-01-01").getTime();     // Middle of the 32 bit epoch

    insertBucket(underflowMin);

    var numBatches = 5;

    const batchOffset = Math.floor(intervalMillis / (numBatches + 1));
    for (let i = 0; i < numBatches; ++i) {
        const start = normalMin + i * batchOffset;
        insertBucket(start);
    }
    assert.gt(buckets.aggregate([{$count: 'n'}]).next().n, 1, 'Expected more than one bucket');
}

insertDocuments();

const unpackStage = getAggPlanStages(coll.explain().aggregate(), '$_internalUnpackBucket')[0];

function assertSorted(result, ascending) {
    let prev = ascending ? {t: -Infinity} : {t: Infinity};
    for (const doc of result) {
        if (ascending) {
            assert.lt(+prev.t,
                      +doc.t,
                      'Found two docs not in ascending time order: ' + tojson({prev, doc}));
        } else {
            assert.gt(+prev.t,
                      +doc.t,
                      'Found two docs not in descending time order: ' + tojson({prev, doc}));
        }

        prev = doc;
    }
}

function checkAgainstReferenceBoundedSortUnexpected(
    collection, reference, pipeline, hint, sortOrder) {
    const options = hint ? {hint: hint} : {};

    const plan = collection.explain().aggregate(pipeline, options);
    if (FixtureHelpers.isSharded(buckets) && numShards() >= 2) {
        // With a sharded collection, some shards might not have any extended-range data,
        // so they might still use $_internalBoundedSort. But we know at least one
        // shard has extended-range data, so we know at least one shard has to
        // use a blocking sort.
        const bounded = getAggPlanStages(plan, "$_internalBoundedSort");
        const blocking = getAggPlanStages(plan, "$sort");
        assert.gt(blocking.length, 0, {bounded, blocking, plan});
        assert.lt(bounded.length,
                  FixtureHelpers.numberOfShardsForCollection(buckets),
                  {bounded, blocking, plan});
    } else {
        const stages = getAggPlanStages(plan, "$_internalBoundedSort");
        assert.eq([], stages, plan);
    }

    const opt = collection.aggregate(pipeline, options).toArray();
    assertSorted(opt, sortOrder);

    assert.eq(reference.length, opt.length);
    for (var i = 0; i < opt.length; ++i) {
        assert.docEq(reference[i], opt[i]);
    }
}

function checkAgainstReferenceBoundedSortExpected(
    collection, reference, pipeline, hint, sortOrder) {
    const options = hint ? {hint: hint} : {};

    const plan = collection.explain().aggregate(pipeline, options);
    const stages = getAggPlanStages(plan, "$_internalBoundedSort");
    assert.neq([], stages, plan);

    const opt = collection.aggregate(pipeline, options).toArray();
    assertSorted(opt, sortOrder);

    assert.eq(reference.length, opt.length);
    for (var i = 0; i < opt.length; ++i) {
        assert.docEq(reference[i], opt[i]);
    }
}

function runTest(ascending) {
    const reference = buckets
                          .aggregate([
                              unpackStage,
                              {$_internalInhibitOptimization: {}},
                              {$sort: {t: ascending ? 1 : -1}},
                          ])
                          .toArray();
    assertSorted(reference, ascending);

    // Check plan using collection scan
    checkAgainstReferenceBoundedSortUnexpected(coll,
                                               reference,
                                               [
                                                   {$sort: {t: ascending ? 1 : -1}},
                                               ],
                                               {},
                                               ascending);

    // Check plan using hinted collection scan
    checkAgainstReferenceBoundedSortUnexpected(coll,
                                               reference,
                                               [
                                                   {$sort: {t: ascending ? 1 : -1}},
                                               ],
                                               {"$natural": ascending ? 1 : -1},
                                               ascending);

    const referenceIndexed = bucketsIndexed
                                 .aggregate([
                                     unpackStage,
                                     {$_internalInhibitOptimization: {}},
                                     {$sort: {t: ascending ? 1 : -1}},
                                 ])
                                 .toArray();
    assertSorted(referenceIndexed, ascending);

    // Check plan using index scan. If we've inserted a date before 1-1-1970, we round the min
    // up towards 1970, rather then down, which has the effect of increasing the control.min.t.
    // This means the minimum time in the bucket is likely to be lower than indicated and thus,
    // actual dates may be out of order relative to what's indicated by the bucket bounds.
    checkAgainstReferenceBoundedSortUnexpected(collIndexed,
                                               referenceIndexed,
                                               [
                                                   {$sort: {t: ascending ? 1 : -1}},
                                               ],
                                               {},
                                               ascending);

    // Check plan using hinted index scan
    checkAgainstReferenceBoundedSortUnexpected(collIndexed,
                                               referenceIndexed,
                                               [
                                                   {$sort: {t: ascending ? 1 : -1}},
                                               ],
                                               {"t": 1},
                                               ascending);

    // Check plan using hinted collection scan
    checkAgainstReferenceBoundedSortUnexpected(collIndexed,
                                               referenceIndexed,
                                               [
                                                   {$sort: {t: ascending ? 1 : -1}},
                                               ],
                                               {"$natural": ascending ? 1 : -1},
                                               ascending);

    // The workaround in all cases is to create a reverse index on the time field, though
    // it's necessary to force use of the reversed index.
    const reverseIdxName = "reverseIdx";
    collIndexed.createIndex({t: -1}, {name: reverseIdxName});

    checkAgainstReferenceBoundedSortExpected(collIndexed,
                                             referenceIndexed,
                                             [
                                                 {$sort: {t: ascending ? 1 : -1}},
                                             ],
                                             {"t": -1},
                                             ascending);

    collIndexed.dropIndex(reverseIdxName);
}

runTest(false);  // descending
runTest(true);   // ascending
