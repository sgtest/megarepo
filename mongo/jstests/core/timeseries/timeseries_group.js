/**
 * Tests $group usage of block processing for time series.
 * @tags: [
 *   requires_timeseries,
 *   does_not_support_stepdowns,
 *   directly_against_shardsvrs_incompatible,
 *   # During fcv upgrade/downgrade the engine might not be what we expect.
 *   cannot_run_during_upgrade_downgrade,
 *   # "Explain of a resolved view must be executed by mongos"
 *   directly_against_shardsvrs_incompatible,
 *   # Some suites use mixed-binary cluster setup where some nodes might have the flag enabled while
 *   # others -- not. For this test we need control over whether the flag is set on the node that
 *   # ends up executing the query.
 *   assumes_standalone_mongod
 * ]
 */

import {TimeseriesTest} from "jstests/core/timeseries/libs/timeseries.js";
import {getEngine, getQueryPlanner, getSingleNodeExplain} from "jstests/libs/analyze_plan.js";
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js"
import {checkSbeFullyEnabled} from "jstests/libs/sbe_util.js";

TimeseriesTest.run((insert) => {
    const datePrefix = 1680912440;

    let coll = db.timeseries_group;

    const timeFieldName = 'time';
    const metaFieldName = 'measurement';

    coll.drop();
    assert.commandWorked(db.createCollection(coll.getName(), {
        timeseries: {timeField: timeFieldName, metaField: metaFieldName},
    }));

    insert(coll, {
        _id: 0,
        [timeFieldName]: new Date(datePrefix + 100),
        [metaFieldName]: "foo",
        x: 123,
        y: 73,
        z: 7,
    });
    insert(coll, {
        _id: 1,
        [timeFieldName]: new Date(datePrefix + 200),
        [metaFieldName]: "foo",
        x: 123,
        y: 42,
        z: 9,
    });
    insert(coll, {
        _id: 2,
        [timeFieldName]: new Date(datePrefix + 300),
        [metaFieldName]: "foo",
        x: 456,
        y: 11,
        z: 4,
    });
    insert(coll, {
        _id: 3,
        [timeFieldName]: new Date(datePrefix + 400),
        [metaFieldName]: "foo",
        x: 456,
        y: 99,
        z: 2,
    });
    insert(coll, {
        _id: 4,
        [timeFieldName]: new Date(datePrefix + 500),
        [metaFieldName]: "foo",

        // All fields missing.
    });

    // Block-based $group requires sbe to be fully enabled and featureFlagTimeSeriesInSbe to be set.
    const sbeFullEnabled = checkSbeFullyEnabled(db) &&
        FeatureFlagUtil.isPresentAndEnabled(db.getMongo(), 'TimeSeriesInSbe');

    function runTests(allowDiskUse) {
        const dateUpperBound = new Date(datePrefix + 500);

        const testcases = [
            {
                name: "Min_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 11}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MinWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}}},
                    {$project: {_id: 1, a: 1}}
                ],
                expectedResults: [{_id: null, a: 11}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "Max_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$max: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 99}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MinWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$max: '$y'}}},
                    {$project: {_id: 1, a: 1}}
                ],
                expectedResults: [{_id: null, a: 99}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxMinusMin_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 0, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{a: 88}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxMinusMinWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: null, a: 88}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "Min_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 11}, {a: 42}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MinWithId_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}}},
                    {$project: {_id: 1, a: 1}}
                ],
                expectedResults: [{_id: 123, a: 42}, {_id: 456, a: 11}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "Max_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$max: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 73}, {a: 99}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxWithId_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$max: '$y'}}},
                    {$project: {_id: 1, a: 1}}
                ],
                expectedResults: [{_id: 123, a: 73}, {_id: 456, a: 99}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxMinusMin_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 0, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{a: 31}, {a: 88}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxMinusMinWithId_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: 123, a: 31}, {_id: 456, a: 88}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "Min_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$min: '$y'}}}, {$project: {_id: 0, a: 1}}],
                expectedResults: [{a: 11}, {a: 42}, {a: null}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MinWithId_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$min: '$y'}}}, {$project: {_id: 1, a: 1}}],
                expectedResults: [{_id: 123, a: 42}, {_id: 456, a: 11}, {_id: null, a: null}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "Max_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$max: '$y'}}}, {$project: {_id: 0, a: 1}}],
                expectedResults: [{a: 73}, {a: 99}, {a: null}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxWithId_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$max: '$y'}}}, {$project: {_id: 1, a: 1}}],
                expectedResults: [{_id: 123, a: 73}, {_id: 456, a: 99}, {_id: null, a: null}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxMinusMin_GroupByX_NoFilter",
                pipeline: [
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 0, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{a: 31}, {a: 88}, {a: null}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "MaxMinusMinWithId_GroupByX_NoFilter",
                pipeline: [
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: 123, a: 31}, {_id: 456, a: 88}, {_id: null, a: null}],
                usesBlockProcessing: sbeFullEnabled && !allowDiskUse
            },
            {
                name: "Avg_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$avg: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 55}, {a: 57.5}],
                usesBlockProcessing: false
            },
            {
                name: "Min_GroupByXAndY",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: {x: '$x', y: '$y'}, a: {$min: '$z'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 2}, {a: 4}, {a: 7}, {a: 9}],
                usesBlockProcessing: false
            },
            {
                name: "Min_GroupByMetaSortKey",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: {$meta: 'sortKey'}, a: {$min: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 11}],
                usesBlockProcessing: false
            },
            {
                name: "MinOfMetaSortKey_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: {$meta: 'sortKey'}}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: null}, {a: null}],
                usesBlockProcessing: false
            },
        ];

        function compareResultEntries(lhs, rhs) {
            const lhsJson = tojson(lhs);
            const rhsJson = tojson(rhs);
            return lhsJson < rhsJson ? -1 : (lhsJson > rhsJson ? 1 : 0);
        }

        const options = {allowDiskUse: allowDiskUse};
        const allowDiskUseStr = allowDiskUse ? "true" : "false";

        for (let testcase of testcases) {
            const name = testcase.name + " (allowDiskUse=" + allowDiskUseStr + ")";
            const pipeline = testcase.pipeline;
            const expectedResults = testcase.expectedResults;
            const usesBlockProcessing = testcase.usesBlockProcessing;

            // Issue the aggregate() query and collect the results (together with their
            // JSON representations).
            const results = coll.aggregate(pipeline, options).toArray();

            // Sort the results.
            results.sort(compareResultEntries);

            const errMsgFn = () => "Test case '" + name + "':\nExpected " +
                tojson(expectedResults) + "\n  !=\nActual " + tojson(results);

            // Check that the expected result and actual results have the same number of elements.
            assert.eq(expectedResults.length, results.length, errMsgFn);

            // Check that each entry in the expected results array matches the corresponding element
            // in the actual results array.
            for (let i = 0; i < expectedResults.length; ++i) {
                assert.docEq(expectedResults[i], results[i], errMsgFn);
            }

            // Check that explain indicates block processing is being used. This is a best effort
            // check.
            const explain = coll.explain().aggregate(pipeline, options);
            const engineUsed = getEngine(explain);
            const singleNodeQueryPlanner = getQueryPlanner(getSingleNodeExplain(explain));
            printjson(singleNodeQueryPlanner);
            function testcaseAndExplainFn(description) {
                return () => description + " for test case '" + name + "' failed with explain " +
                    tojson(singleNodeQueryPlanner);
            }

            const hasSbePlan = engineUsed === "sbe";
            const sbePlan =
                hasSbePlan ? singleNodeQueryPlanner.winningPlan.slotBasedPlan.stages : null;

            if (usesBlockProcessing) {
                // Verify that we have an SBE plan, and verify that "block_hashagg" appears in the
                // plan.
                assert.eq(engineUsed, "sbe");

                assert(sbePlan.includes("block_hashagg"),
                       testcaseAndExplainFn("Expected explain to use block processing"));
            } else {
                if (hasSbePlan) {
                    // If 'usesBlockProcessing' is false and we have an SBE plan, verify that
                    // "block_hashagg" does not appear anywhere in the SBE plan.
                    assert(!sbePlan.includes("block_hashagg"),
                           testcaseAndExplainFn("Expected explain not to use block processing"));
                }
            }
        }
    }

    // Run the tests with allowDiskUse=false.
    runTests(false /* allowDiskUse */);

    // Run the tests with allowDiskUse=true.
    runTests(true /* allowDiskUse */);
});
