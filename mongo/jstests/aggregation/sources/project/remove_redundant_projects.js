// Tests that the aggregation pipeline correctly coalesces a $project stage at the front of the
// pipeline that can be covered by a normal query.
// @tags: [
//   do_not_wrap_aggregations_in_facets,
//   requires_pipeline_optimization,
// ]
import {orderedArrayEq} from "jstests/aggregation/extras/utils.js";
import {
    getWinningPlan,
    isAggregationPlan,
    isQueryPlan,
    planHasStage
} from "jstests/libs/analyze_plan.js";
import {checkSBEEnabled} from "jstests/libs/sbe_util.js";

let coll = db.remove_redundant_projects;
coll.drop();

assert.commandWorked(coll.insert({_id: {a: 1, b: 1}, a: 1, c: {d: 1}, e: ['elem1']}));

let indexSpec = {a: 1, 'c.d': 1, 'e.0': 1};

const groupPushdownEnabled = checkSBEEnabled(db);

/**
 * Helper to test that for a given pipeline, the same results are returned whether or not an
 * index is present.  Also tests whether a projection is absorbed by the pipeline
 * ('expectProjectToCoalesce') and the corresponding project stage ('removedProjectStage') does
 * not exist in the explain output.
 */
function assertResultsMatch({
    pipeline = [],
    expectProjectToCoalesce = false,
    removedProjectStage = null,
    index = indexSpec,
    pipelineOptimizedAway = false
} = {}) {
    // Add a match stage to ensure index scans are considered for planning (workaround for
    // SERVER-20066).
    pipeline = [{$match: {a: {$gte: 0}}}].concat(pipeline);

    // Once with an index.
    assert.commandWorked(coll.createIndex(index));
    let explain = coll.explain().aggregate(pipeline);
    let resultsWithIndex = coll.aggregate(pipeline).toArray();

    // Projection does not get pushed down when sharding filter is used.
    if (!explain.hasOwnProperty("shards")) {
        let result;

        if (pipelineOptimizedAway) {
            assert(isQueryPlan(explain), explain);
            result = getWinningPlan(explain.queryPlanner);
        } else {
            assert(isAggregationPlan(explain), explain);
            result = getWinningPlan(explain.stages[0].$cursor.queryPlanner);
        }

        // Check that $project uses the query system.
        assert.eq(expectProjectToCoalesce,
                  planHasStage(db, result, "PROJECTION_DEFAULT") ||
                      planHasStage(db, result, "PROJECTION_COVERED") ||
                      planHasStage(db, result, "PROJECTION_SIMPLE"),
                  explain);

        if (!pipelineOptimizedAway) {
            // Check that $project was removed from pipeline and pushed to the query system.
            explain.stages.forEach(function(stage) {
                if (stage.hasOwnProperty("$project"))
                    assert.neq(removedProjectStage, stage["$project"], explain);
            });
        }
    }

    // Again without an index.
    assert.commandWorked(coll.dropIndex(index));
    let resultsWithoutIndex = coll.aggregate(pipeline).toArray();

    assert(orderedArrayEq(resultsWithIndex, resultsWithoutIndex));
}

// Test that covered projections correctly use the query system for projection and the $project
// stage is removed from the pipeline.
assertResultsMatch({
    pipeline: [{$project: {_id: 0, a: 1}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});
assertResultsMatch({
    pipeline: [{$project: {_id: 0, a: 1}}, {$group: {_id: null, a: {$sum: "$a"}}}],
    expectProjectToCoalesce: true,
    removedProjectStage: {_id: 0, a: 1},
    pipelineOptimizedAway: groupPushdownEnabled
});
assertResultsMatch({
    pipeline: [{$sort: {a: -1}}, {$project: {_id: 0, a: 1}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});
assertResultsMatch({
    pipeline: [
        {$sort: {a: 1, 'c.d': 1}},
        {$project: {_id: 0, a: 1}},
        {$group: {_id: "$a", a: {$sum: "$a"}}}
    ],
    expectProjectToCoalesce: true,
    removedProjectStage: {_id: 0, a: 1},
    pipelineOptimizedAway: groupPushdownEnabled
});
assertResultsMatch({
    pipeline: [{$project: {_id: 0, c: {d: 1}}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});

// Test that projections with renamed fields are removed from the pipeline.
assertResultsMatch({
    pipeline: [{$project: {_id: 0, f: "$a"}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});
assertResultsMatch({
    pipeline: [{$project: {_id: 0, a: 1, f: "$a"}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});

// Test that uncovered projections are removed from the pipeline.
assertResultsMatch({
    pipeline: [{$sort: {a: 1}}, {$project: {_id: 1, b: 1}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});
// TODO SERVER-72549: Remove use of featureFlagSbeFull by SBE Pushdown feature.
if (checkSBEEnabled(db, ["featureFlagSbeFull"])) {
    assertResultsMatch({
        pipeline: [{$sort: {a: 1}}, {$group: {_id: "$_id", a: {$sum: "$a"}}}, {$project: {arr: 1}}],
        expectProjectToCoalesce: true,
        pipelineOptimizedAway: true
    });
} else {
    assertResultsMatch({
        pipeline: [{$sort: {a: 1}}, {$group: {_id: "$_id", a: {$sum: "$a"}}}, {$project: {arr: 1}}],
        expectProjectToCoalesce:
            !groupPushdownEnabled,  // lowering $group into SBE prevents coalesing of projects
    });
}

// Test that projections with computed fields are removed from the pipeline.
assertResultsMatch({
    pipeline: [{$project: {computedField: {$sum: "$a"}}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});
assertResultsMatch({
    pipeline: [{$project: {a: ["$a", "$b"]}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});
assertResultsMatch({
    pipeline:
        [{$project: {e: {$filter: {input: "$e", as: "item", cond: {"$eq": ["$$item", "elem0"]}}}}}],
    expectProjectToCoalesce: true,
    pipelineOptimizedAway: true
});

// TODO SERVER-72549: Remove use of featureFlagSbeFull by SBE Pushdown feature.
if (checkSBEEnabled(db, ["featureFlagSbeFull"])) {
    assertResultsMatch({
        pipeline: [
            {$project: {_id: 0, a: 1}},
            {$group: {_id: "$a", c: {$sum: "$c"}, a: {$sum: "$a"}}},
            {$project: {_id: 0}}
        ],
        expectProjectToCoalesce: true,
        pipelineOptimizedAway: true
    });
} else {
    // Test that only the first projection is removed from the pipeline.
    assertResultsMatch({
        pipeline: [
            {$project: {_id: 0, a: 1}},
            {$group: {_id: "$a", c: {$sum: "$c"}, a: {$sum: "$a"}}},
            {$project: {_id: 0}}
        ],
        expectProjectToCoalesce: true,
        removedProjectStage: {_id: 0, a: 1},
    });
}

// Test that projections on _id with nested fields are removed from pipeline.
indexSpec = {
    '_id.a': 1,
    a: 1
};
assertResultsMatch({
    pipeline: [{$match: {"_id.a": 1}}, {$project: {'_id.a': 1}}],
    expectProjectToCoalesce: true,
    index: indexSpec,
    pipelineOptimizedAway: true,
    removedProjectStage: {'_id.a': 1},
});