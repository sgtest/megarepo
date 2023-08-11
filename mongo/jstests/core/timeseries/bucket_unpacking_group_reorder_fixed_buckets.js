/**
 *  Test the behavior of $group on time-series collections. Specifically, we are targeting rewrites
 * that replace bucket unpacking with $group over fixed buckets. This optimization only applies if
 * the '_id' field is a combination of constant expressions, field paths referencing metaField,
 * and/or $dateTrunc expressions on the timeField.
 *
 * @tags: [
 *     # We need a timeseries collection.
 *     requires_timeseries,
 *     requires_fcv_71,
 *     # Explain of a resolved view must be executed by mongos.
 *     directly_against_shardsvrs_incompatible,
 *     # Refusing to run a test that issues an aggregation command with explain because it may
 *     # return incomplete results if interrupted by a stepdown.
 *     does_not_support_stepdowns,
 * ]
 */

import {getExplainedPipelineFromAggregation} from "jstests/aggregation/extras/utils.js";
import {checkSBEEnabled} from "jstests/libs/sbe_util.js";

(function() {
"use strict";

const coll = db.bucket_unpack_group_reorder_fixed_buckets;
const timeField = "time";
const controlMax = "control.max";
const controlMin = "control.min";
const accField = "b";
const metaField = "mt";

function checkResults(
    {pipeline, checkExplain = true, rewriteOccur = true, expectedDocs, validateFullExplain}) {
    // Only check the explain output if SBE is not enabled. SBE changes the explain output.
    if (!checkSBEEnabled(db, ["featureFlagTimeSeriesInSbe"])) {
        if (rewriteOccur && checkExplain) {
            checkExplainForRewrite(pipeline);
        } else if (checkExplain) {
            checkExplainForNoRewrite(pipeline, validateFullExplain);
        }
    }
    let results = coll.aggregate(pipeline).toArray();
    if (expectedDocs) {
        assert.sameMembers(results, expectedDocs);
    }
    // Run the pipeline with and without optimizations and assert the same results are returned.
    let noOptResults = coll.aggregate([{$_internalInhibitOptimization: {}}, pipeline[0]]).toArray();
    assert.sameMembers(
        results,
        noOptResults,
        "Results differ with and without the optimization. Results with the optimization are: " +
            results);
}

function checkExplainForRewrite(pipeline) {
    const explain =
        getExplainedPipelineFromAggregation(db, coll, pipeline, {inhibitOptimization: false});
    assert.eq(explain.length, 1, tojson(explain));
    const groupStage = explain[0]["$group"];
    assert(groupStage, "Expected group stage, but received: " + tojson(explain));
    // Validate the "date" field in $dateTrunc was rewritten.
    const dateField = groupStage["_id"]["t"]["$dateTrunc"]["date"];
    assert.eq(dateField,
              `$${controlMin}.${timeField}`,
              "Expected date field to be rewritten, but received: " + groupStage);
    // Validate the accumulators were rewritten.
    assert.eq(groupStage["accmin"]["$min"], `$${controlMin}.${accField}`);
    assert.eq(groupStage["accmax"]["$max"], `$${controlMax}.${accField}`);
    // The unit test validates the entire rewrite of the $count accumulator, we will just validate
    // part of the rewrite (that a $cond expression exists).
    if (groupStage["count"]) {
        assert(groupStage["count"]["$sum"]["$cond"]);
    }
}

function checkExplainForNoRewrite(pipeline, validateFullExplain) {
    const explain =
        getExplainedPipelineFromAggregation(db, coll, pipeline, {inhibitOptimization: false});
    assert.eq(explain.length, 2, tojson(explain));
    const unpackStage = explain[0]["$_internalUnpackBucket"];
    assert(unpackStage, tojson(explain));
    const groupStage = explain[1]["$group"];
    assert(groupStage, tojson(explain));
    if (validateFullExplain) {
        // If we passed in an explain object, we wanted to validate the entire explain output.
        assert.docEq(groupStage, validateFullExplain, tojson(explain));
    } else {
        // If not, we can just validate the "date" field and accumulators were not rewritten.
        const dateField = groupStage["_id"]["t"]["$dateTrunc"]["date"];
        assert.eq(dateField,
                  `$${timeField}`,
                  "Expected date field to not be rewritten, but received: " + groupStage);
        assert.eq(groupStage["accmin"]["$min"], `$${accField}`);
        assert.eq(groupStage["accmax"]["$max"], `$${accField}`);
    }
}

let b, times = [];
function setUpSmallCollection({roundingParam, startingTime}) {
    coll.drop();
    assert.commandWorked(db.createCollection(coll.getName(), {
        timeseries: {
            timeField: timeField,
            metaField: metaField,
            bucketMaxSpanSeconds: roundingParam,
            bucketRoundingSeconds: roundingParam
        }
    }));
    let docs = [];
    // Need to convert the 'bucketRoundingSeconds' and 'bucketMaxSpanSeconds' to milliseconds.
    const offset = roundingParam * 1000;
    // Add documents that will span over multiple buckets.
    times = [
        new Date(startingTime.getTime() - offset),
        new Date(startingTime.getTime() - offset / 2),
        new Date(startingTime.getTime() - offset / 3),
        startingTime,
        new Date(startingTime.getTime() + offset / 3),
        new Date(startingTime.getTime() + offset / 2),
        new Date(startingTime.getTime() + offset)
    ];
    b = [2, 1, 4, 3, 5, 6, 7];
    times.forEach((time, index) => {
        docs.push({
            _id: index,
            [timeField]: time,
            [metaField]: "MDB",
            [accField]: b[index],
            "otherTime": time
        });
    });
    assert.commandWorked(coll.insertMany(docs));
}

setUpSmallCollection({roundingParam: 3600, startingTime: ISODate("2022-09-30T15:00:00.000Z")});

///
// These tests will validate the group stage is rewritten when the '_id' field has a $dateTrunc
// expression.
///

// Validate the rewrite occurs with a simple case, where the bucket boundary and 'unit' are the
// same.
checkResults({
    pipeline: [{
        $group: {
            _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: "hour"}}},
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`}
        }
    }],
    expectedDocs: [
        {_id: {t: ISODate("2022-09-30T15:00:00Z")}, accmin: 3, accmax: 6},
        {_id: {t: ISODate("2022-09-30T16:00:00Z")}, accmin: 7, accmax: 7},
        {_id: {t: ISODate("2022-09-30T14:00:00Z")}, accmin: 1, accmax: 4}
    ],
});

// Validate the rewrite occurs with all the optional fields present.
checkResults({
    pipeline: [{
        $group: {
            _id: {
                t: {
                    $dateTrunc: {
                        date: `$${timeField}`,
                        unit: "day",
                        timezone: "+0500",
                        binSize: 2,
                        startOfWeek: "friday"
                    }
                }
            },
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`}
        }
    }],
    expectedDocs: [{_id: {t: ISODate("2022-09-29T19:00:00Z")}, accmin: 1, accmax: 7}],
});

// Validate the rewrite occurs with multiple expressions in the '_id' field.
checkResults({
    pipeline: [{
        $group: {
            _id: {
                constant: "hello",
                m: `$${metaField}`,
                t: {$dateTrunc: {date: `$${timeField}`, unit: "day"}}
            },
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`}
        }
    }],
    expectedDocs: [{
        _id: {t: ISODate("2022-09-30T00:00:00Z"), m: "MDB", constant: "hello"},
        accmin: 1,
        accmax: 7
    }],
});

// Validate the rewrite occurs with a timezone with the same hourly boundaries, and
// bucketMaxSpanSeconds == 3600.
checkResults({
    pipeline: [{
        $group: {
            _id: {
                m: `$${metaField}`,
                t: {$dateTrunc: {date: `$${timeField}`, unit: "day", timezone: "+0800"}}
            },
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`}
        }
    }],
    expectedDocs: [
        {_id: {"m": "MDB", t: ISODate("2022-09-29T16:00:00Z")}, accmin: 1, accmax: 6},
        {_id: {"m": "MDB", t: ISODate("2022-09-30T16:00:00Z")}, accmin: 7, accmax: 7}
    ],
});

// The 'unit' field in $dateTrunc is larger than 'week', but 'bucketMaxSpanSeconds' is less than 1
// day. The rewrite applies.
checkResults({
    pipeline: [{
        $group: {
            _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: "year"}}},
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`}
        }
    }],
    expectedDocs: [{_id: {t: ISODate("2022-01-01T00:00:00Z")}, accmin: 1, accmax: 7}],
});

// Validate the rewrite occurs with the $count accumulator.
checkResults({
    pipeline: [{
        $group: {
            _id: {c: "string", t: {$dateTrunc: {date: `$${timeField}`, unit: "month"}}},
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`},
            count: {$count: {}},
        }
    }],
    expectedDocs: [
        {_id: {"c": "string", t: ISODate("2022-09-01T00:00:00Z")}, accmin: 1, accmax: 7, count: 7}
    ],
});

///
// These tests will validate the optimization did not occur.
///

// There is a timezone with different hourly boundaries that causes the boundaries to not align.
// Asia/Kathmandu has a UTC offset of +05:45.
checkResults({
    pipeline: [{
        $group: {
            _id: {
                t: {
                    $dateTrunc: {
                        date: `$${timeField}`,
                        unit: "hour",
                        binSize: 24,
                        timezone: "Asia/Kathmandu"
                    }
                }
            },
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`}
        }
    }],
    expectedDocs: [{_id: {t: ISODate("2022-09-29T18:15:00Z")}, accmin: b[1], accmax: b[6]}],
    rewriteOccur: false
});

// The $dateTrunc expression doesn't align with bucket boundaries.
checkResults({
    pipeline: [{
        $group: {
            _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: "second"}}},
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`},
        }
    }],
    expectedDocs: [
        {_id: {t: times[0]}, accmin: b[0], accmax: b[0]},
        {_id: {t: times[1]}, accmin: b[1], accmax: b[1]},
        {_id: {t: times[2]}, accmin: b[2], accmax: b[2]},
        {_id: {t: times[3]}, accmin: b[3], accmax: b[3]},
        {_id: {t: times[4]}, accmin: b[4], accmax: b[4]},
        {_id: {t: times[5]}, accmin: b[5], accmax: b[5]},
        {_id: {t: times[6]}, accmin: b[6], accmax: b[6]}
    ],
    rewriteOccur: false
});

// The $dateTrunc expression is not on the timeField.
checkResults({
    pipeline: [{
        $group: {
            _id: {t: {$dateTrunc: {date: "$otherTime", unit: "day"}}},
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`},
        }
    }],
    expectedDocs: [{_id: {t: ISODate("2022-09-30T00:00:00Z")}, accmax: 7, accmin: 1}],
    validateFullExplain: {
        _id: {t: {$dateTrunc: {date: "$otherTime", unit: {"$const": "day"}}}},
        accmax: {$max: `$${accField}`},
        accmin: {$min: `$${accField}`},
    },
    rewriteOccur: false
});

// There are other expressions in the '_id' field that are not on the meta nor time fields.
checkResults({
    pipeline: [{
        $group: {
            _id: {m: `$${metaField}`, t: {$dateTrunc: {date: "$otherTime", unit: "day"}}},
            accmax: {$max: `$${accField}`},
        }
    }],
    expectedDocs: [{_id: {"m": "MDB", t: ISODate("2022-09-30T00:00:00Z")}, accmax: 7}],
    validateFullExplain: {
        _id: {m: `$${metaField}`, t: {$dateTrunc: {date: "$otherTime", unit: {"$const": "day"}}}},
        accmax: {$max: `$${accField}`},
    },
    rewriteOccur: false
});

// The fields in the $dateTrunc expression are not constant.
checkResults({
    pipeline: [{
        $group: {
            _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: "hour", binSize: "$a"}}},
            accmax: {$max: `$${accField}`},
            accmin: {$min: `$${accField}`},
        }
    }],
    expectedDocs: [{_id: {t: null}, accmax: 7, accmin: 1}],
    rewriteOccur: false
});

// The parameters have changed, and thus the buckets are not fixed. This test must be run last,
// since the collection will never be considered fixed, unless it is dropped.
assert.commandWorked(db.runCommand({
    "collMod": coll.getName(),
    "timeseries": {bucketMaxSpanSeconds: 100000, bucketRoundingSeconds: 100000}
}));
checkResults({
    pipeline: [{
        $group: {
            _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: "day"}}},
            accmin: {$min: `$${accField}`},
            accmax: {$max: `$${accField}`}
        }
    }],
    expectedDocs: [{_id: {t: ISODate("2022-09-30T00:00:00Z")}, accmin: 1, accmax: 7}],
    rewriteOccur: false
});

// Validate the rewrite does not apply for fixed buckets with a 'bucketMaxSpanSeconds' set to
// greater than one day. This is because the bucket rounding logic and $dateTrunc rounding is
// different and becomes too unreliable.
(function testLargeBucketSpan() {
    const secondsInTwoDays = 3600 * 48;
    setUpSmallCollection(
        {roundingParam: secondsInTwoDays, startingTime: ISODate("2012-06-30T23:00:00.000Z")});
    const timeUnits = ["minute", "second", "day", "week", "year"];
    timeUnits.forEach(timeUnit => {
        checkResults({
            pipeline: [{
                $group: {
                    _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: timeUnit}}},
                    accmin: {$min: `$${accField}`},
                    accmax: {$max: `$${accField}`}
                }
            }],
            rewriteOccur: false
        });
    });
})();

// Validate the results with and without the optimization are the same with a random
// bucketMaxSpanSeconds. bucketMaxSpanSeconds can be any integer between 1-31536000 inclusive.
(function testRandomBucketSpan() {
    const seedVal = new Date().getTime();
    jsTestLog("In testRandomBucketSpan using seed value: " + seedVal);
    Random.setRandomSeed(seedVal);

    const randomSpan = Math.floor(Random.rand() * (31536000 - 1) + 1);
    setUpSmallCollection(
        {roundingParam: randomSpan, startingTime: ISODate("2015-06-26T23:00:00.000Z")});
    const timeUnits = ["millisecond", "minute", "second", "day", "week", "month", "year"];
    timeUnits.forEach(timeUnit => {
        checkResults({
            pipeline: [{
                $group: {
                    _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: timeUnit}}},
                    accmin: {$min: `$${accField}`},
                    accmax: {$max: `$${accField}`}
                }
            }],
            checkExplain: false,
        });
    });
})();

// Validate the rewrite works for a smaller fixed bucketing parameter and accounts for leap seconds.
// A leap second occurred on 2012-06-30:23:59:60. $dateTrunc and time-series rounding logic rounds
// this time to the next minute.
(function testLeapSeconds() {
    setUpSmallCollection({roundingParam: 60, startingTime: ISODate("2012-06-30T23:00:00.000Z")});
    // Insert documents close and at the leap second. These numbers are larger and smaller than the
    // originally inserted documents, so they should change the values of "$min" and "$max".
    const leapSecondDocs = [
        {[timeField]: ISODate("2012-06-30T23:59:60.000Z"), [metaField]: "MDB", b: 16},
        {[timeField]: ISODate("2012-06-30T23:59:40.000Z"), [metaField]: "MDB", b: 11},
        {[timeField]: ISODate("2012-06-30T23:59:45.000Z"), [metaField]: "MDB", B: 12},
        {[timeField]: ISODate("2012-06-30T23:59:50.000Z"), [metaField]: "MDB", b: -1},
        {[timeField]: ISODate("2012-06-30T23:59:59.000Z"), [metaField]: "MDB", b: 0},
        {[timeField]: ISODate("2012-07-01T00:00:05.000Z"), [metaField]: "MDB", b: 15}
    ];
    assert.commandWorked(coll.insertMany(leapSecondDocs));
    checkResults({
        pipeline: [{
            $group: {
                _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: "minute"}}},
                accmin: {$min: `$${accField}`},
                accmax: {$max: `$${accField}`}
            }
        }],
        expectedDocs: [
            {_id: {t: ISODate("2012-06-30T23:59:00Z")}, accmin: -1, accmax: 11},
            {_id: {t: ISODate("2012-06-30T22:59:00Z")}, accmin: 1, accmax: 4},
            {_id: {t: ISODate("2012-07-01T00:00:00Z")}, accmin: 15, accmax: 16},
            {_id: {t: ISODate("2012-06-30T23:01:00Z")}, accmin: 7, accmax: 7},
            {_id: {t: ISODate("2012-06-30T23:00:00Z")}, accmin: 3, accmax: 6},
        ],
    });
})();

// Validate the rewrite works for with daylight savings. Due to daylight savings March 13, 2022
// was 23 hours long, since the hour between 2-3:00am was skipped. We will be testing the New York
// timezone, so 2:00 for New York in UTC is 7:00.
(function testDaylightSavings() {
    setUpSmallCollection({roundingParam: 3600, startingTime: ISODate("2022-03-13T07:00:00.000Z")});
    // Insert documents for every hour of the day in the New York timezone, even though the day was
    // only 23 hours long.   Two hours after "startTime", will be the skipped hour, but we expect
    // that document to still be valid and exist. To double check that document will have the
    // minimum value.
    const startTime = ISODate("2022-03-13T05:30:00.000Z");
    let inc = 0;
    let dayLightDocs = [];
    for (let i = 0; i < 23; i++) {
        const accValue = i == 2 ? -1 : i + 8;  // set the "skipped" hour to the minimum value.
        const newTime = new Date(startTime.getTime() + (1000 * i * 60));  // i hours in the future.
        dayLightDocs.push({
            [timeField]: newTime,
            [metaField]: 1,
            [accField]: accValue  // avoid duplicates 'b' values in the original set.
        });
    }
    assert.commandWorked(coll.insertMany(dayLightDocs));
    checkResults({
        pipeline: [{
            $group: {
                _id: {
                    t: {
                        $dateTrunc: {
                            date: `$${timeField}`,
                            unit: "hour",
                            binSize: 24,
                            timezone: "America/New_York"
                        }
                    }
                },
                accmin: {$min: `$${accField}`},
                accmax: {$max: `$${accField}`}
            }
        }],
        expectedDocs: [{_id: {t: ISODate("2022-03-13T05:00:00Z")}, accmin: -1, accmax: 30}]
    });
})();

// Validate a few simple queries with a randomized larger dataset return the same results with and
// without the optimization.
(function testRandomizedInput() {
    const seedVal = new Date().getTime();
    jsTestLog("In testRandomizedInput using seed value: " + seedVal);
    Random.setRandomSeed(seedVal);
    coll.drop();
    assert.commandWorked(db.createCollection(coll.getName(), {
        timeseries: {
            timeField: timeField,
            metaField: metaField,
            bucketMaxSpanSeconds: 86400,
            bucketRoundingSeconds: 86400
        }
    }));

    let docs = [];
    const startTime = ISODate("2012-01-01T00:01:00.000Z");
    const maxTime = ISODate("2015-12-31T23:59:59.000Z");
    // Insert 1000 documents at random times spanning 3 years (between 2012 and 2015). These dates
    // were chosen arbitrarily.
    for (let i = 0; i < 1000; i++) {
        const randomTime = new Date(Math.floor(Random.rand() * (maxTime - startTime) + startTime));
        docs.push({[timeField]: randomTime, [metaField]: "location"});
    }
    assert.commandWorked(coll.insertMany(docs));

    const timeUnits = ["day", "week", "month", "quarter", "year"];
    timeUnits.forEach(timeUnit => {
        checkResults({
            pipeline: [{
                $group: {
                    _id: {t: {$dateTrunc: {date: `$${timeField}`, unit: timeUnit}}},
                    accmin: {$min: `$${accField}`},
                    accmax: {$max: `$${accField}`}
                }
            }]
        });
    });
})();
}());
