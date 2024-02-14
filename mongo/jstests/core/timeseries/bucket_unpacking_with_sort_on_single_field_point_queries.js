/**
 * Test that the bucket unpacking with sorting rewrite is performed and doesn't cause incorrect
 * results to be created. This test is focused on timeseries with sort on single field point
 * queries.
 *
 * @tags: [
 *     # Explain of a resolved view must be executed by mongos.
 *     directly_against_shardsvrs_incompatible,
 *     # This complicates aggregation extraction.
 *     do_not_wrap_aggregations_in_facets,
 *     # Refusing to run a test that issues an aggregation command with explain because it may
 *     # return incomplete results if interrupted by a stepdown.
 *     does_not_support_stepdowns,
 *     # We need a timeseries collection.
 *     requires_timeseries,
 * ]
 */
import {
    backwardIxscan,
    forwardIxscan,
    runRewritesTest,
    setupColl
} from "jstests/core/timeseries/libs/timeseries_sort_util.js"
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

// TODO(SERVER-86317): re-enable this test
if (FeatureFlagUtil.isPresentAndEnabled(
        db, "TimeseriesAlwaysUseCompressedBuckets", /*user=*/ undefined, /*ignoreFCV=*/ true)) {
    jsTestLog("This test is disabled for featureFlagTimeseriesAlwaysUseCompressedBuckets.");
    quit();
}

const metaCollName = "bucket_unpacking_with_sort_with_meta_on_single_field_point_queries";
const metaColl = db[metaCollName];

setupColl(metaColl, metaCollName, true);

// Test point predicate on a single meta field.
for (const sort of [-1, +1]) {
    for (const m of [-1, +1]) {
        for (const t of [-1, +1]) {
            const index = {m, t};
            const expectedAccessPath = t === sort ? forwardIxscan : backwardIxscan;
            runRewritesTest({t: sort}, index, index, expectedAccessPath, metaColl, t === sort, [
                {$match: {m: 7}}
            ]);
            runRewritesTest({t: sort}, index, null, expectedAccessPath, metaColl, t === sort, [
                {$match: {m: 7}}
            ]);
        }
    }
}
