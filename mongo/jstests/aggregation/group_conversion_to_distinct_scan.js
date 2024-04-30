/**
 * $group stages with no accumulators or with only $first or only $last accumulators can sometimes
 * be converted into a DISTINCT_SCAN (see SERVER-9507 and SERVER-37304). This optimization
 * potentially applies to a $group when it begins the pipeline or when it is preceded only by one or
 * both of $match and $sort (in that order). In all cases, it must be possible to do a DISTINCT_SCAN
 * that sees each value of the distinct field exactly once among matching documents and also
 * provides any requested sort. The test queries below show most $match/$sort/$group combinations
 * where that is possible.
 *
 * @tags: [
 *   # The sharding and $facet passthrough suites modifiy aggregation pipelines in a way that
 *   # prevents the DISTINCT_SCAN optimization from being applied, which breaks the test.
 *   assumes_unsharded_collection,
 *   do_not_wrap_aggregations_in_facets,
 *   # Index filter commands do not support causal consistency.
 *   does_not_support_causal_consistency,
 *   requires_fcv_63,
 *   not_allowed_with_signed_security_token,
 * ]
 */

import {
    addIndex,
    assertPipelineResultsAndExplain,
    assertPlanDoesNotUseDistinctScan,
    assertPlanUsesCollScan,
    assertPlanUsesDistinctScan,
    assertPlanUsesIndexScan,
    coll,
    documents,
    prepareCollection,
    removeIndex,
} from "jstests/aggregation/libs/group_to_distinct_scan_utils.js";
import {getAggPlanStage, getQueryPlanner} from "jstests/libs/analyze_plan.js";

prepareCollection();

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN when the sort is available from an
// index.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1}}, {$group: {_id: "$a"}}],
    expectedOutput: [{_id: null}, {_id: 1}, {_id: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $group pipeline can use DISTINCT_SCAN even when the user does not specify a
// sort.
//
{
    const pipeline = [{$group: {_id: "$a"}}];
    const expectedOutput = [{_id: null}, {_id: 1}, {_id: 2}];
    const keyPattern = {a: 1, b: 1, c: 1};

    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        validateExplain: (explain) => assertPlanUsesDistinctScan(explain, keyPattern),
    });

    //
    // Verify that a $group pipeline with a $natural hint does not use DISTINCT_SCAN.
    //
    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        hint: {$natural: 1},
        validateExplain: assertPlanUsesCollScan,
    });

    //
    // Verify that a $group pipeline with a pertinent hint as string does use DISTINCT_SCAN.
    //
    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        hint: "a_1_b_1_c_1",
        validateExplain: (explain) => assertPlanUsesDistinctScan(explain, keyPattern),
    });

    //
    // Verify that a $group pipeline with a pertinent hint as an object does use DISTINCT_SCAN.
    //
    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        hint: {a: 1, b: 1, c: 1},
        validateExplain: (explain) => assertPlanUsesDistinctScan(explain, keyPattern),
    });

    //
    // Verify that a $group pipeline with a non-pertinent hint does not use DISTINCT_SCAN.
    //
    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        hint: {_id: 1},
        validateExplain: (explain) => assertPlanUsesIndexScan(explain, {_id: 1}),
    });

    //
    // Verify that a $group pipeline with an index filter still uses DISTINCT_SCAN.
    //
    assert.commandWorked(db.runCommand({
        planCacheSetFilter: coll.getName(),
        query: {},
        projection: {a: 1, _id: 0},
        indexes: ["a_1_b_1_c_1"]
    }));

    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        validateExplain: (explain) => assertPlanUsesDistinctScan(explain, keyPattern),
    });

    //
    // Verify that a $group pipeline with an index filter and $natural hint uses DISTINCT_SCAN.
    //
    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        hint: {$natural: 1},
        expectsIndexFilter: !TestData.isHintsToQuerySettingsSuite,
        validateExplain: (explain) => assertPlanUsesDistinctScan(explain, keyPattern),
    });

    //
    // Verify that a $group pipeline with an index filter and non-pertinent hint uses DISTINCT_SCAN.
    //
    assertPipelineResultsAndExplain({
        pipeline,
        expectedOutput,
        hint: {_id: 1},
        expectsIndexFilter: !TestData.isHintsToQuerySettingsSuite,
        validateExplain: (explain) => assertPlanUsesDistinctScan(explain, keyPattern),
    });

    assert.commandWorked(db.runCommand({planCacheClearFilters: coll.getName()}));
}

//
// Verify that a $sort-$group pipeline _does not_ use a DISTINCT_SCAN on a multikey field.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {mkA: 1}}, {$group: {_id: "$mkA"}}],
    expectedOutput: [{_id: null}, {_id: 1}, {_id: 2}, {_id: 3}, {_id: [2, 3, 4]}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN when the sort is available from an
// index and there are $first accumulators.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1, b: 1}}, {$group: {_id: "$a", accum: {$first: "$b"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN when the sort is available from an
// index and there are $last accumulators.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: -1, b: -1}}, {$group: {_id: "$a", accum: {$last: "$b"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN when a $first accumulator needs the
// entire document.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: -1, b: -1, c: -1}}, {$group: {_id: "$a", accum: {$first: "$$ROOT"}}}],
    expectedOutput: [
        {_id: null, accum: {_id: 6, a: null, b: 1, c: 1.5}},
        {_id: 1, accum: {_id: 3, a: 1, b: 3, c: 2}},
        {_id: 2, accum: {_id: 4, a: 2, b: 2, c: 2}}
    ],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN when a $last accumulator needs the
// entire document.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1, b: 1, c: 1}}, {$group: {_id: "$a", accum: {$last: "$$ROOT"}}}],
    expectedOutput: [
        {_id: null, accum: {_id: 6, a: null, b: 1, c: 1.5}},
        {_id: 1, accum: {_id: 3, a: 1, b: 3, c: 2}},
        {_id: 2, accum: {_id: 4, a: 2, b: 2, c: 2}}
    ],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN when sorting and grouping by fields
// with dotted paths.
//
assertPipelineResultsAndExplain({
    pipeline:
        [{$sort: {"foo.a": 1, "foo.b": 1}}, {$group: {_id: "$foo.a", accum: {$first: "$foo.b"}}}],
    expectedOutput:
        [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}, {_id: 3, accum: null}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {"foo.a": 1, "foo.b": 1}),
});

assertPipelineResultsAndExplain({
    pipeline:
        [{$sort: {"foo.a": 1, "foo.b": 1}}, {$group: {_id: "$foo.a", accum: {$last: "$foo.b"}}}],
    expectedOutput:
        [{_id: null, accum: 1}, {_id: 1, accum: 2}, {_id: 2, accum: 2}, {_id: 3, accum: null}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {"foo.a": 1, "foo.b": 1}),
});

//
// Verify that a $group pipeline can use DISTINCT_SCAN to group on a dotted path field, even
// when the user does not specify a sort.
//
assertPipelineResultsAndExplain({
    pipeline: [{$group: {_id: "$foo.a"}}],
    expectedOutput: [{_id: null}, {_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: assertPlanUsesDistinctScan,
});

//
// Verify that we _do not_ attempt to use a DISTINCT_SCAN on a multikey field.
//
assertPipelineResultsAndExplain({
    pipeline: [{$group: {_id: "$mkA"}}],
    expectedOutput: [{_id: null}, {_id: 1}, {_id: 2}, {_id: 3}, {_id: [2, 3, 4]}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that we may not use a DISTINCT_SCAN on a dotted field when the last component
// is not multikey, but an intermediate component is.
//
assertPipelineResultsAndExplain({
    pipeline: [{$group: {_id: "$mkFoo.a"}}],
    expectedOutput: [
        {_id: null},
        {_id: 1},
        {_id: 2},
        {_id: [3, 4]},
    ],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that we _do not_ attempt to use a DISTINCT_SCAN on a multikey dotted-path field when
// a sort is present.
//
assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {"mkFoo.a": 1, "mkFoo.b": 1}},
        {$group: {_id: "$mkFoo.a", accum: {$first: "$mkFoo.b"}}}
    ],
    expectedOutput: [
        {_id: null, accum: null},
        {_id: 1, accum: 1},
        {_id: 2, accum: 2},
        {_id: [3, 4], accum: [4, 3]}
    ],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {"mkFoo.a": 1, "mkFoo.b": 1}},
        {$group: {_id: "$mkFoo.a", accum: {$last: "$mkFoo.b"}}}
    ],
    expectedOutput: [
        {_id: null, accum: 1},
        {_id: 1, accum: 2},
        {_id: 2, accum: 2},
        {_id: [3, 4], accum: [4, 3]}
    ],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that we use a DISTINCT_SCAN to satisfy a sort on a multikey field if the bounds
// are [minKey, maxKey].
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {aa: 1, mkB: 1}}, {$group: {_id: "$aa", accum: {$first: "$mkB"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: [1, 3]}, {_id: 2, accum: []}],
    validateExplain: (explain) => {
        assertPlanUsesDistinctScan(explain, {aa: 1, mkB: 1, c: 1});
        assert.neq(null, getAggPlanStage(explain, "FETCH"));
    }
});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {aa: -1, mkB: -1}}, {$group: {_id: "$aa", accum: {$first: "$mkB"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: [1, 3]}, {_id: 2, accum: []}],
    validateExplain: (explain) => {
        assertPlanUsesDistinctScan(explain, {aa: 1, mkB: 1, c: 1});
        assert.neq(null, getAggPlanStage(explain, "FETCH"));
    }
});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {aa: -1, mkB: -1}}, {$group: {_id: "$aa", accum: {$last: "$mkB"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 2}, {_id: 2, accum: []}],
    validateExplain: assertPlanDoesNotUseDistinctScan
});

//
// Verify that we _do not_ attempt a DISTINCT_SCAN because "mkB" is multikey, and we don't use
// DISTINCT_SCAN for a compound group key.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {aa: 1, mkB: 1}}, {$group: {_id: {aa: "$aa", mkB: "$mkB"}}}],
    expectedOutput:
        [{_id: {aa: 1, mkB: [1, 3]}}, {_id: {}}, {_id: {aa: 1, mkB: 2}}, {_id: {aa: 2, mkB: []}}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that with dotted paths we use a DISTINCT_SCAN to satisfy a sort on a multikey field if the
// bounds are [minKey, maxKey].
//
assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {"foo.a": 1, "mkFoo.b": 1}},
        {$group: {_id: "$foo.a", accum: {$first: "$mkFoo.b"}}}
    ],
    expectedOutput:
        [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}, {_id: 3, accum: [4, 3]}],
    validateExplain: (explain) => {
        assertPlanUsesDistinctScan(explain, {"foo.a": 1, "mkFoo.b": 1});
        assert.neq(null, getAggPlanStage(explain, "FETCH"));
    },
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {"foo.a": 1, "mkFoo.b": -1}},
        {$group: {_id: "$foo.a", accum: {$last: "$mkFoo.b"}}}
    ],
    expectedOutput:
        [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}, {_id: 3, accum: [4, 3]}],
    validateExplain: assertPlanUsesCollScan,
});

//
// Verify that we _do not_ attempt a DISTINCT_SCAN to satisfy a sort on a multikey field if
// the bounds are not [minKey, maxKey].
//
assertPipelineResultsAndExplain({
    pipeline: [
        {$match: {mkB: {$ne: 9999}}},
        {$sort: {aa: 1, mkB: 1}},
        {$group: {_id: "$aa", accum: {$first: "$mkB"}}}
    ],
    expectedOutput: [{_id: 1, accum: [1, 3]}, {_id: null, accum: null}, {_id: 2, accum: []}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$match: {mkB: {$gt: -5}}},
        {$sort: {aa: 1, mkB: 1}},
        {$group: {_id: "$aa", accum: {$first: "$mkB"}}}
    ],
    expectedOutput: [{_id: 1, accum: [1, 3]}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

// Repeat above tests for $last.

assertPipelineResultsAndExplain({
    pipeline: [
        {$match: {mkB: {$ne: 9999}}},
        {$sort: {aa: 1, mkB: 1}},
        {$group: {_id: "$aa", accum: {$last: "$mkB"}}}
    ],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 2}, {_id: 2, accum: []}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$match: {mkB: {$gt: -5}}},
        {$sort: {aa: 1, mkB: 1}},
        {$group: {_id: "$aa", accum: {$last: "$mkB"}}}
    ],
    expectedOutput: [{_id: 1, accum: 2}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that with dotted paths we _do not_ attempt a DISTINCT_SCAN to satisfy a sort on a
// multikey field if the bounds are not [minKey, maxKey].
//
assertPipelineResultsAndExplain({
    pipeline: [
        {$match: {"mkFoo.b": {$ne: 20000}}},
        {$sort: {"foo.a": 1, "mkFoo.b": 1}},
        {$group: {_id: "$foo.a", accum: {$first: "$mkFoo.b"}}}
    ],
    expectedOutput:
        [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}, {_id: 3, accum: [4, 3]}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$match: {"mkFoo.b": {$ne: 20000}}},
        {$sort: {"foo.a": 1, "mkFoo.b": -1}},
        {$group: {_id: "$foo.a", accum: {$last: "$mkFoo.b"}}}
    ],
    expectedOutput:
        [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}, {_id: 3, accum: [4, 3]}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that we can use a DISTINCT_SCAN on a multikey index to sort and group on a dotted-path
// field, so long as the field we are sorting over is not multikey and comes before any multikey
// fields in the index key pattern.
//
// We drop the {"foo.a": 1, "foo.b": 1} to force this test to use the multikey
// {"foo.a": 1, "mkFoo.b"} index. The rest of the test doesn't use either of those indexes.
//
removeIndex({"foo.a": 1, "foo.b": 1});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {"foo.a": 1}}, {$group: {_id: "$foo.a"}}],
    expectedOutput: [{_id: null}, {_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {"foo.a": 1, "mkFoo.b": 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN even when there is a $first
// accumulator that accesses a multikey field.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {aa: 1, bb: 1}}, {$group: {_id: "$aa", accum: {$first: "$mkB"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: [1, 3]}, {_id: 2, accum: []}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {aa: 1, bb: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN when there is a $last
// accumulator that accesses a multikey field, provided that field is not part of the index.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {aa: 1, bb: 1}}, {$group: {_id: "$aa", accum: {$last: "$mkB"}}}],
    expectedOutput: [{_id: 1, accum: 2}, {_id: 2, accum: []}, {_id: null, accum: null}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {aa: 1, bb: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN even when there is a $first
// accumulator that includes an expression.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1, b: 1}}, {$group: {_id: "$a", accum: {$first: {$add: ["$b", "$c"]}}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 2}, {_id: 2, accum: 4}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline can use DISTINCT_SCAN even when there is a $last
// accumulator that includes an expression.
//
assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {a: 1, b: 1}},
        {$group: {_id: "$a", accum: {$last: {$add: ["$b", "$c"]}}}},
        /* There are 2 documents with {a: null}, one of which is missing that key entirely.
         * Because those compare equal, we don't know whether the $last one will be the one
         * matching {c: 1} or {c: 1.5}. To make the test deterministic, we add 0.6 to
         * whatever result we get and round it to the nearest integer. */
        {$project: {_id: "$_id", accum: {$round: [{$add: ["$accum", 0.6]}, 0]}}}
    ],
    expectedOutput: [{_id: null, accum: 3.0}, {_id: 1, accum: 6}, {_id: 2, accum: 5}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $match-$sort-$group pipeline can use a DISTINCT_SCAN to sort and group by a
// field that is not the first field in a compound index, so long as the previous fields are
// scanned with equality bounds (i.e., are point queries).
//
assertPipelineResultsAndExplain({
    pipeline: [{$match: {a: 1}}, {$sort: {b: 1}}, {$group: {_id: "$b"}}],
    expectedOutput: [{_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Same as the previous case but with the sort order matching the index key pattern, so the
// query planner does not need to infer the availability of a sort on {b: 1} based on the
// equality bounds for the 'a field.
//
assertPipelineResultsAndExplain({
    pipeline: [{$match: {a: 1}}, {$sort: {a: 1, b: 1}}, {$group: {_id: "$b"}}],
    expectedOutput: [{_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Same as the previous case but with no user-specified sort.
//
assertPipelineResultsAndExplain({
    pipeline: [{$match: {a: 1}}, {$group: {_id: "$b"}}],
    expectedOutput: [{_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $match-$sort-$group pipeline _does not_ use a DISTINCT_SCAN to sort and group
// on the second field of an index when there is no equality match on the first field.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1, b: 1}}, {$group: {_id: "$b"}}],
    expectedOutput: [{_id: null}, {_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that a $match-$sort-$limit-$group pipeline _does not_ coalesce the $sort-$limit and
// then consider the result eligible for the DISTINCT_SCAN optimization.
//
// In this example, the {$limit: 3} filters out the document {a: 1, b: 3, c: 2}, which means we
// don't see a {_id: 3} group. If we instead applied the {$limit: 3} after the $group stage, we
// would incorrectly list three groups. DISTINCT_SCAN won't work here, because we have to
// examine each document in order to determine which groups get filtered out by the $limit.
//
assertPipelineResultsAndExplain({
    pipeline: [{$match: {a: 1}}, {$sort: {a: 1, b: 1}}, {$limit: 3}, {$group: {_id: "$b"}}],
    expectedOutput: [{_id: 1}, {_id: 2}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that an additional $project stage does not lead to incorrect results (although it will
// preclude the use of the DISTINCT_SCAN optimization).
//
assertPipelineResultsAndExplain({
    pipeline:
        [{$match: {a: 1}}, {$project: {a: 1, b: 1}}, {$sort: {a: 1, b: 1}}, {$group: {_id: "$b"}}],
    expectedOutput: [{_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that a $sort-$group can use a DISTINCT_SCAN even when the requested sort is the
// reverse of the index's sort.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: -1, b: -1}}, {$group: {_id: "$a", accum: {$first: "$b"}}}],
    expectedOutput: [{_id: null, accum: 1}, {_id: 1, accum: 3}, {_id: 2, accum: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: -1, b: -1}}, {$group: {_id: "$a", accum: {$last: "$b"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline _does not_ use DISTINCT_SCAN when there are non-$first/$last
// accumulators.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1}}, {$group: {_id: "$a", accum: {$sum: "$b"}}}],
    expectedOutput: [{_id: null, accum: 2}, {_id: 1, accum: 8}, {_id: 2, accum: 2}],
    validateExplain: (explain) => assertPlanUsesIndexScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline _does not_ use DISTINCT_SCAN when there are both $first and
// $last accumulators.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: -1, b: -1}}, {$group: {_id: "$a", f: {$first: "$b"}, l: {$last: "$b"}}}],
    expectedOutput: [{_id: null, f: 1, l: null}, {_id: 1, f: 3, l: 1}, {_id: 2, f: 2, l: 2}],
    validateExplain: (explain) => assertPlanUsesIndexScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline with multiple $first accumulators uses DISTINCT_SCAN.
//
assertPipelineResultsAndExplain({
    pipeline:
        [{$sort: {a: -1, b: -1}}, {$group: {_id: "$a", f1: {$first: "$b"}, f2: {$first: "$b"}}}],
    expectedOutput: [{_id: null, f1: 1, f2: 1}, {_id: 1, f1: 3, f2: 3}, {_id: 2, f1: 2, f2: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {a: -1, b: -1, c: -1}},
        {$group: {_id: "$a", fb: {$first: "$b"}, fc: {$first: "$c"}}}
    ],
    expectedOutput: [{_id: null, fb: 1, fc: 1.5}, {_id: 1, fb: 3, fc: 2}, {_id: 2, fb: 2, fc: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline with multiple $last accumulators uses DISTINCT_SCAN.
//
assertPipelineResultsAndExplain({
    pipeline:
        [{$sort: {a: -1, b: -1}}, {$group: {_id: "$a", l1: {$last: "$b"}, l2: {$last: "$b"}}}],
    expectedOutput:
        [{_id: null, l1: null, l2: null}, {_id: 1, l1: 1, l2: 1}, {_id: 2, l1: 2, l2: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {a: -1, b: -1, c: -1}},
        {$group: {_id: "$a", lb: {$last: "$b"}, lc: {$last: "$c"}}}
    ],
    expectedOutput:
        [{_id: null, lb: null, lc: null}, {_id: 1, lb: 1, lc: 1}, {_id: 2, lb: 2, lc: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline _does not_ use DISTINCT_SCAN when documents are not
// sorted by the field used for grouping.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {b: 1}}, {$group: {_id: "$a", accum: {$first: "$b"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {b: -1}}, {$group: {_id: "$a", accum: {$last: "$b"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: 1, accum: 1}, {_id: 2, accum: 2}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that a $match-$sort-$group pipeline _does not_ use a DISTINCT_SCAN when the match does
// not provide equality (point query) bounds for each field before the grouped-by field in the
// index.
//
assertPipelineResultsAndExplain({
    pipeline: [{$match: {a: {$gt: 0}}}, {$sort: {b: 1}}, {$group: {_id: "$b"}}],
    expectedOutput: [{_id: 1}, {_id: 2}, {_id: 3}],
    validateExplain: assertPlanDoesNotUseDistinctScan,
});

//
// Verify that a $sort-$group pipeline with a $first accumulator can use DISTINCT_SCAN, even when
// the group _id field is a singleton object instead of a fieldPath.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1, b: 1}}, {$group: {_id: {v: "$a"}, accum: {$first: "$b"}}}],
    expectedOutput:
        [{_id: {v: null}, accum: null}, {_id: {v: 1}, accum: 1}, {_id: {v: 2}, accum: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

//
// Verify that a $sort-$group pipeline with a $last accumulator can use DISTINCT_SCAN, even when
// the group _id field is a singleton object instead of a fieldPath.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {a: 1, b: 1}}, {$group: {_id: {v: "$a"}, accum: {$last: "$b"}}}],
    expectedOutput: [{_id: {v: null}, accum: 1}, {_id: {v: 1}, accum: 3}, {_id: {v: 2}, accum: 2}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {a: 1, b: 1, c: 1}),
});

////////////////////////////////////////////////////////////////////////////////////////////////
// We execute all the collation-related tests three times with three different configurations
// (no index, index without collation, index with collation).
//
// Collation tests 1: no index on string field.
////////////////////////////////////////////////////////////////////////////////////////////////

const collationOption = {
    collation: {locale: "en_US", strength: 2}
};

//
// Verify that a $group on an unindexed field uses a collection scan.
//
assertPipelineResultsAndExplain({
    pipeline: [{$group: {_id: "$str"}}],
    expectedOutput: [{_id: null}, {_id: "FoO"}, {_id: "bAr"}, {_id: "bar"}, {_id: "foo"}],
    validateExplain: assertPlanUsesCollScan,
});

//
// Verify that a collated $group on an unindexed field uses a collection scan.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str"}}],
    expectedOutput: [{_id: null}, {_id: "bAr"}, {_id: "foo"}],
    validateExplain: assertPlanUsesCollScan,
    options: collationOption,
});

//
// Verify that a $sort-$group pipeline uses a collection scan.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$first: "$d"}}}],
    expectedOutput: [
        {_id: null, accum: null},
        {_id: "FoO", accum: 2},
        {_id: "bAr", accum: 3},
        {_id: "bar", accum: 4},
        {_id: "foo", accum: 1}
    ],
    validateExplain: assertPlanUsesCollScan,
});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$last: "$d"}}}],
    expectedOutput: [
        {_id: null, accum: null},
        {_id: "FoO", accum: 2},
        {_id: "bAr", accum: 3},
        {_id: "bar", accum: 4},
        {_id: "foo", accum: 1}
    ],
    validateExplain: assertPlanUsesCollScan,
});

//
// Verify that a collated $sort-$group pipeline with a $first accumulator uses a collection
// scan.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$first: "$d"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: "bAr", accum: 3}, {_id: "foo", accum: 1}],
    validateExplain: assertPlanUsesCollScan,
    options: collationOption,
});

//
// Verify that a collated $sort-$group pipeline with a $last accumulator uses a collection
// scan.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$last: "$d"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: "bAr", accum: 4}, {_id: "foo", accum: 2}],
    validateExplain: assertPlanUsesCollScan,
    options: collationOption,
});

////////////////////////////////////////////////////////////////////////////////////////////////
// Collation tests 2: index on string field with no collation.
////////////////////////////////////////////////////////////////////////////////////////////////

addIndex({str: 1, d: 1});

//
// Verify that a $group uses a DISTINCT_SCAN.
//
assertPipelineResultsAndExplain({
    pipeline: [{$group: {_id: "$str"}}],
    expectedOutput: [{_id: null}, {_id: "FoO"}, {_id: "bAr"}, {_id: "bar"}, {_id: "foo"}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {str: 1, d: 1}),
});

//
// Verify that a $sort-$group pipeline with a collation _does not_ scan the index, which is not
// aware of the collation.
//
// Note that, when using a case-insensitive collation, "bAr" and "bar" will get grouped
// together, and the decision as to which one will represent the group is arbitary. The
// tie-breaking {d: 1} component of the sort forces a specific decision for this aggregation,
// making this test more reliable.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str"}}],
    expectedOutput: [{_id: null}, {_id: "bAr"}, {_id: "foo"}],
    validateExplain: assertPlanUsesCollScan,
    options: collationOption,
});

//
// Verify that a $sort-$group pipeline uses a DISTINCT_SCAN.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$first: "$d"}}}],
    expectedOutput: [
        {_id: null, accum: null},
        {_id: "FoO", accum: 2},
        {_id: "bAr", accum: 3},
        {_id: "bar", accum: 4},
        {_id: "foo", accum: 1}
    ],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {str: 1, d: 1}),
});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$last: "$d"}}}],
    expectedOutput: [
        {_id: null, accum: null},
        {_id: "FoO", accum: 2},
        {_id: "bAr", accum: 3},
        {_id: "bar", accum: 4},
        {_id: "foo", accum: 1}
    ],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {str: 1, d: 1}),
});

//
// Verify that a $sort-$group that use a collation and includes a $first accumulators  _does
// not_ scan the index, which is not aware of the collation.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$first: "$d"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: "bAr", accum: 3}, {_id: "foo", accum: 1}],
    validateExplain: assertPlanUsesCollScan,
    options: collationOption
});

//
// Verify that a $sort-$group that use a collation and includes a $last accumulators  _does
// not_ scan the index, which is not aware of the collation.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: -1}}, {$group: {_id: "$str", accum: {$first: "$d"}}}],
    expectedOutput: [{_id: "bar", accum: 4}, {_id: "FoO", accum: 2}, {_id: null, accum: null}],
    validateExplain: assertPlanUsesCollScan,
    options: collationOption
});

////////////////////////////////////////////////////////////////////////////////////////////////
// Collation tests 3: index on string field with case-insensitive collation.
////////////////////////////////////////////////////////////////////////////////////////////////

removeIndex({str: 1, d: 1});
addIndex({str: 1, d: 1}, collationOption);

//
// Verify that a $group with no collation _does not_ scan the index, which does have a
// collation.
//
assertPipelineResultsAndExplain({
    pipeline: [{$group: {_id: "$str"}}],
    expectedOutput: [{_id: null}, {_id: "FoO"}, {_id: "bAr"}, {_id: "bar"}, {_id: "foo"}],
    validateExplain: assertPlanUsesCollScan,
});

//
// Verify that a $sort-$group with a collation uses a DISTINCT_SCAN on the index, which uses a
// matching collation.
//
// Note that, when using a case-insensitive collation, "bAr" and "bar" will get grouped
// together, and the decision as to which one will represent the group is arbitary. The
// tie-breaking {d: 1} component of the sort forces a specific decision for this aggregation,
// making this test more reliable.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str"}}],
    expectedOutput: [{_id: null}, {_id: "bAr"}, {_id: "foo"}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {str: 1, d: 1}),
    options: collationOption
});

//
// Verify that a $sort-$group pipeline with no collation _does not_ scan the index, which does
// have a collation.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$first: "$d"}}}],
    expectedOutput: [
        {_id: null, accum: null},
        {_id: "FoO", accum: 2},
        {_id: "bAr", accum: 3},
        {_id: "bar", accum: 4},
        {_id: "foo", accum: 1}
    ],
    validateExplain: assertPlanUsesCollScan
});

assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$last: "$d"}}}],
    expectedOutput: [
        {_id: null, accum: null},
        {_id: "FoO", accum: 2},
        {_id: "bAr", accum: 3},
        {_id: "bar", accum: 4},
        {_id: "foo", accum: 1}
    ],
    validateExplain: assertPlanUsesCollScan
});

//
// Verify that a $sort-$group pipeline that uses a collation and includes a $first accumulator
// uses a DISTINCT_SCAN, which uses a matching collation.
//
assertPipelineResultsAndExplain({
    pipeline: [{$sort: {str: 1, d: 1}}, {$group: {_id: "$str", accum: {$first: "$d"}}}],
    expectedOutput: [{_id: null, accum: null}, {_id: "bAr", accum: 3}, {_id: "foo", accum: 1}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {str: 1, d: 1}),
    options: collationOption
});

//
// Verify that a $sort-$group pipeline that uses a collation and includes a $last accumulator
// uses a DISTINCT_SCAN, which uses a matching collation. Note that because strings FoO and foo are
// treated the same by this collation, it is equally valid to have either as the _id. For that
// reason, we project _id to lowercase in the output. This converts _id: null to _id: "".
//
assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {str: 1, d: 1}},
        {$group: {_id: "$str", accum: {$last: "$d"}}},
        {$addFields: {_id: {$toLower: "$_id"}}}
    ],
    expectedOutput: [{_id: "foo", accum: 2}, {_id: "", accum: null}, {_id: "bar", accum: 4}],
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {str: 1, d: 1}),
    options: collationOption
});

//
// Verify that a $sort-$_internalStreamingGroup pipeline can use DISTINCT_SCAN
//
const expectedResult = [];
for (let i = 0; i < documents.length; i++) {
    let resultDocument = {_id: i, value: null};
    if (documents[i].hasOwnProperty("a")) {
        resultDocument["value"] = documents[i].a;
    }
    expectedResult.push(resultDocument);
}
assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {_id: 1}},
        {
            $_internalStreamingGroup:
                {_id: "$_id", value: {$first: "$a"}, $monotonicIdFields: ["_id"]}
        }
    ],
    expectedOutput: expectedResult,
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {_id: 1})
});

assertPipelineResultsAndExplain({
    pipeline: [
        {$sort: {_id: 1}},
        {$_internalStreamingGroup: {_id: "$_id", value: {$last: "$a"}, $monotonicIdFields: ["_id"]}}
    ],
    expectedOutput: expectedResult,
    validateExplain: (explain) => assertPlanUsesDistinctScan(explain, {_id: 1})
});
