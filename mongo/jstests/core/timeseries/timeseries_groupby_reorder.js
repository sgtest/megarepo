/**
 * Test the behavior of $group on time-series collections. Specifically, we are targeting rewrites
 * that replace bucket unpacking with $group over the buckets collection. Currently, only $min/$max
 * are supported for the rewrites.
 *
 * @tags: [
 *   directly_against_shardsvrs_incompatible,
 *   does_not_support_stepdowns,
 *   does_not_support_transactions,
 *   requires_fcv_61,
 * ]
 */
import {FixtureHelpers} from "jstests/libs/fixture_helpers.js";

const coll = db.timeseries_groupby_reorder;
coll.drop();

// We will only check correctness of the results here as checking the plan in JSTests is brittle and
// is better done in document_source_internal_unpack_bucket_test/group_reorder_test.cpp. For the
// cases when the re-write isn't applicable, the used datasets should yield wrong result if the
// re-write is applied.
function runGroupRewriteTest(docs, pipeline, expectedResults) {
    coll.drop();
    db.createCollection(coll.getName(), {timeseries: {metaField: "meta", timeField: "time"}});
    coll.insertMany(docs);
    assert.docEq(expectedResults, coll.aggregate(pipeline).toArray(), () => {
        return `Pipeline: ${tojson(pipeline)}. Explain: ${
            tojson(coll.explain().aggregate(pipeline))}`;
    });
}

// Test with measurement group key -- a rewrite in this situation would be wrong.
(function testNonMetaGroupKey() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, key: 2, val: 1},  // global min
        {time: t, meta: 1, key: 1, val: 3},  // min for key = 1
        {time: t, meta: 1, key: 1, val: 5},  // max for key = 1
        {time: t, meta: 1, key: 2, val: 7},  // global max
    ];
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$key", min: {$min: "$val"}}}, {$match: {_id: 1}}],
                        [{"_id": 1, "min": 3}]);
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$key", max: {$max: "$val"}}}, {$match: {_id: 1}}],
                        [{"_id": 1, "max": 5}]);
})();

// While a group with const group key can be re-written in terms of a group on the buckets, we don't
// currently do it. However, when/if we start doing it, it should work.
(function testConstGroupKey_NoFilter() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 0},
        {time: t, meta: 1, val: 1},
        {time: t, meta: 1, val: 5},
    ];
    runGroupRewriteTest(
        docs, [{$group: {_id: null, min: {$min: "$val"}}}], [{"_id": null, "min": 0}]);
    runGroupRewriteTest(
        docs, [{$group: {_id: null, max: {$max: "$val"}}}], [{"_id": null, "max": 5}]);
})();

// With a filter on meta the group re-write would still apply if the group key is const.
(function testConstGroupKey_WithFilterOnMeta() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 0},
        {time: t, meta: 2, val: 1},
        {time: t, meta: 2, val: 3},
        {time: t, meta: 1, val: 5},
    ];
    runGroupRewriteTest(docs,
                        [{$match: {meta: 2}}, {$group: {_id: null, min: {$min: "$val"}}}],
                        [{"_id": null, "min": 1}]);
    runGroupRewriteTest(docs,
                        [{$match: {meta: 2}}, {$group: {_id: null, max: {$max: "$val"}}}],
                        [{"_id": null, "max": 3}]);
})();

// In presense of a non-meta filter the group re-write doesn't apply even if the group key is const.
(function testConstGroupKey_WithFilterOnMeasurement() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 0, include: false},
        {time: t, meta: 1, val: 1, include: true},
        {time: t, meta: 1, val: 5, include: false},
    ];
    runGroupRewriteTest(docs,
                        [{$match: {include: true}}, {$group: {_id: null, min: {$min: "$val"}}}],
                        [{"_id": null, "min": 1}]);
    runGroupRewriteTest(docs,
                        [{$match: {include: true}}, {$group: {_id: null, max: {$max: "$val"}}}],
                        [{"_id": null, "max": 1}]);
})();

// Test with meta group key. The group re-write applies.
(function testMetaGroupKey_NoFilter() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 5},
        {time: t, meta: 2, val: 4},
        {time: t, meta: 2, val: 3},
        {time: t, meta: 1, val: 1},
    ];
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$meta", min: {$min: "$val"}}}, {$match: {_id: 2}}],
                        [{"_id": 2, "min": 3}]);
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$meta", max: {$max: "$val"}}}, {$match: {_id: 2}}],
                        [{"_id": 2, "max": 4}]);
})();

// Test with meta group key preceeded by a filter on the meta key. The re-write still applies.
(function testMetaGroupKey_WithFilterOnMeta() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 5},
        {time: t, meta: 2, val: 4},
        {time: t, meta: 2, val: 3},
        {time: t, meta: 1, val: 1},
    ];
    runGroupRewriteTest(docs,
                        [{$match: {meta: 2}}, {$group: {_id: "$meta", min: {$min: "$val"}}}],
                        [{"_id": 2, "min": 3}]);
    runGroupRewriteTest(docs,
                        [{$match: {meta: 2}}, {$group: {_id: "$meta", max: {$max: "$val"}}}],
                        [{"_id": 2, "max": 4}]);
})();

// Test with meta group key preceeded by a filter on a measurement key. The re-write doesn't apply.
(function testMetaGroupKey_WithFilterOnMeasurement() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 3, include: false},
        {time: t, meta: 1, val: 4, include: true},
        {time: t, meta: 1, val: 5, include: false},
    ];
    runGroupRewriteTest(docs,
                        [{$match: {include: true}}, {$group: {_id: "$meta", min: {$min: "$val"}}}],
                        [{"_id": 1, "min": 4}]);
    runGroupRewriteTest(docs,
                        [{$match: {include: true}}, {$group: {_id: "$meta", max: {$max: "$val"}}}],
                        [{"_id": 1, "max": 4}]);
})();

// Test SERVER-73822 fix: complex $min and $max (i.e. not just straight field refs) work correctly.
(function testMetaGroupKey_WithNonPathExpressionUnderMinMax() {
    const t = new Date();
    // min(a+b) != min(a) + min(b) and max(a+b) != max(a) + max(b)
    const docs = [
        {time: t, meta: 1, a: 1, b: 20},  // max(a + b)
        {time: t, meta: 1, a: 2, b: 10},
        {time: t, meta: 1, a: 3, b: 1},  // min(a + b)
    ];
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$meta", min: {$min: {$add: ["$a", "$b"]}}}}],
                        [{"_id": 1, "min": 4}]);
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$meta", max: {$max: {$add: ["$a", "$b"]}}}}],
                        [{"_id": 1, "max": 21}]);
})();

// Test with meta group key and a non-min/max accumulator that doesn't use any fields. The buckets
// still have to be unpacked because we don't know the number of events in uncompressed buckets.
(function testMetaGroupKey_WithAccumulatorNotUsingAnyFields() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 3},
        {time: t, meta: 3, val: 4},
        {time: t, meta: 1, val: 5},
    ];
    runGroupRewriteTest(
        docs, [{$group: {_id: "$meta", x: {$sum: 1}}}, {$match: {_id: 1}}], [{"_id": 1, "x": 2}]);
})();

// Test with meta group key and a non-min/max accumulator that uses only the meta field. This query
// is _not_ eligible for the re-write w/o bucket unpacking because while it doesn't depend on any
// fields of the individual events it still depends on the number of events in a bucket.
(function testMetaGroupKey_WithNonMinMaxAccumulatorOnMeta() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, val: 3},
        {time: t, meta: 3, val: 4},
        {time: t, meta: 1, val: 5},
    ];
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$meta", x: {$sum: "$meta"}}}, {$match: {_id: 1}}],
                        [{"_id": 1, "x": 2}]);
})();

// In presence of a filter $min and $max on the meta field cannot be re-written because a filter
// might end up selecting nothing in buckets with a particular meta.
(function testMetaGroupKey_WithAccumulatorOnMeta_WithFilterOnMeasurement() {
    const t = new Date();
    const docs = [
        {time: t, meta: 1, include: false},
    ];
    runGroupRewriteTest(
        docs, [{$match: {include: true}}, {$group: {_id: "$meta", x: {$min: "$meta"}}}], []);
})();

// Test min/max on the time field (cannot rewrite $min because the control.time.min is rounded
// down).
(function testMetaGroupKey_WithMinMaxOnTime() {
    const docs = [
        {time: ISODate("2023-07-20T23:16:47.683Z"), meta: 1},
    ];
    runGroupRewriteTest(docs,
                        [{$group: {_id: "$meta", min: {$min: "$time"}}}],
                        [{_id: 1, min: ISODate("2023-07-20T23:16:47.683Z")}]);
})();
