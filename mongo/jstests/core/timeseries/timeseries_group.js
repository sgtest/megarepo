/**
 * Tests $group usage of block processing for time series.
 * @tags: [
 *   requires_timeseries,
 *   does_not_support_stepdowns,
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

import "jstests/libs/sbe_assert_error_override.js";

import {assertErrorCode} from "jstests/aggregation/extras/utils.js";
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

    function runTests(allowDiskUse, forceIncreasedSpilling) {
        assert.commandWorked(db.adminCommand({
            setParameter: 1,
            internalQuerySlotBasedExecutionHashAggForceIncreasedSpilling: forceIncreasedSpilling
        }));
        const dateUpperBound = new Date(datePrefix + 500);
        const dateLowerBound = new Date(datePrefix);

        const testcases = [
            {
                name: "GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null}},
                    {$project: {_id: 1}}
                ],
                expectedResults: [{_id: null}],
                usesBlockProcessing: false
            },
            {
                name: "Min_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 11}],
                usesBlockProcessing: false
            },
            {
                name: "Min_GroupByNullAllPass",
                pipeline: [
                    {$match: {[timeFieldName]: {$gt: dateLowerBound}}},
                    {$group: {_id: null, a: {$min: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 11}],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}}}
                ],
                expectedResults: [{_id: null, a: 11}],
                usesBlockProcessing: false
            },
            {
                name: "Max_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$max: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 99}],
                usesBlockProcessing: false
            },
            {
                name: "Max_GroupByNullAllPass",
                pipeline: [
                    {$match: {[timeFieldName]: {$gt: dateLowerBound}}},
                    {$group: {_id: null, a: {$max: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 99}],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$max: '$y'}}}
                ],
                expectedResults: [{_id: null, a: 99}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMin_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 0, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMin_GroupByNullAllPass",
                pipeline: [
                    {$match: {[timeFieldName]: {$gt: dateLowerBound}}},
                    {$group: {_id: null, a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 0, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: null, a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MinAndMaxWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: null, a: {$min: '$y'}, b: {$max: '$y'}}}
                ],
                expectedResults: [{_id: null, a: 11, b: 99}],
                usesBlockProcessing: false
            },
            {
                name: "Min_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 11}, {a: 42}],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}}}
                ],
                expectedResults: [{_id: 123, a: 42}, {_id: 456, a: 11}],
                usesBlockProcessing: false
            },
            {
                name: "Max_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$max: '$y'}}},
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: 73}, {a: 99}],
                usesBlockProcessing: false
            },
            {
                name: "MaxWithId_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$max: '$y'}}}
                ],
                expectedResults: [{_id: 123, a: 73}, {_id: 456, a: 99}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMin_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 0, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{a: 31}, {a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: 123, a: 31}, {_id: 456, a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MinAndMaxWithId_GroupByX",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}}
                ],
                expectedResults: [{_id: 123, a: 42, b: 73}, {_id: 456, a: 11, b: 99}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByDateTrunc",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {$dateTrunc: {date: "$time", unit: "hour"}},
                            a: {$min: '$y'},
                            b: {$max: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: ISODate("1970-01-20T10:00:00Z"), a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByDateAdd",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {$dateAdd: {startDate: "$time", unit: "millisecond", amount: 100}},
                            a: {$min: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: '$a'}}
                ],
                expectedResults: [
                    {_id: ISODate("1970-01-20T10:55:12.640Z"), a: 73},
                    {_id: ISODate("1970-01-20T10:55:12.740Z"), a: 42},
                    {_id: ISODate("1970-01-20T10:55:12.840Z"), a: 11},
                    {_id: ISODate("1970-01-20T10:55:12.940Z"), a: 99}
                ],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByDateAddAndDateDiff",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {
                                $dateAdd: {
                                    startDate: ISODate("2024-01-01T00:00:00"),
                                    unit: "millisecond",
                                    amount: {
                                        $dateDiff: {
                                            startDate: new Date(datePrefix),
                                            endDate: "$time",
                                            unit: "millisecond"
                                        }
                                    }
                                }
                            },
                            a: {$min: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: '$a'}}
                ],
                expectedResults: [
                    {_id: ISODate("2024-01-01T00:00:00.100Z"), a: 73},
                    {_id: ISODate("2024-01-01T00:00:00.200Z"), a: 42},
                    {_id: ISODate("2024-01-01T00:00:00.300Z"), a: 11},
                    {_id: ISODate("2024-01-01T00:00:00.400Z"), a: 99}
                ],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByDateSubtract",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {
                                $dateSubtract:
                                    {startDate: "$time", unit: "millisecond", amount: 100}
                            },
                            a: {$min: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: '$a'}}
                ],
                expectedResults: [
                    {_id: ISODate("1970-01-20T10:55:12.440Z"), a: 73},
                    {_id: ISODate("1970-01-20T10:55:12.540Z"), a: 42},
                    {_id: ISODate("1970-01-20T10:55:12.640Z"), a: 11},
                    {_id: ISODate("1970-01-20T10:55:12.740Z"), a: 99},
                ],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByDateSubtractAndDateDiff",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {
                                $dateSubtract: {
                                    startDate: ISODate("2024-01-01T00:00:00"),
                                    unit: "millisecond",
                                    amount: {
                                        $dateDiff: {
                                            startDate: new Date(datePrefix),
                                            endDate: "$time",
                                            unit: "millisecond"
                                        }
                                    }
                                }
                            },
                            a: {$min: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: '$a'}}
                ],
                expectedResults: [
                    {_id: ISODate("2023-12-31T23:59:59.600Z"), a: 99},
                    {_id: ISODate("2023-12-31T23:59:59.700Z"), a: 11},
                    {_id: ISODate("2023-12-31T23:59:59.800Z"), a: 42},
                    {_id: ISODate("2023-12-31T23:59:59.900Z"), a: 73},
                ],
                usesBlockProcessing: false
            },
            {
                name: "MaxAndMinOfDateDiffWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lte: dateUpperBound}}},
                    {
                        $group: {
                            _id: null,
                            a: {
                                $min: {
                                    $dateDiff: {
                                        startDate: new Date(datePrefix),
                                        endDate: "$time",
                                        unit: "millisecond"
                                    }
                                }
                            },
                            b: {
                                $max: {
                                    $dateDiff: {
                                        startDate: new Date(datePrefix),
                                        endDate: "$time",
                                        unit: "millisecond"
                                    }
                                }
                            },
                            c: {
                                $min: {
                                    $dateDiff: {
                                        startDate: "$time",
                                        endDate: new Date(datePrefix),
                                        unit: "millisecond"
                                    }
                                }
                            },
                            d: {
                                $max: {
                                    $dateDiff: {
                                        startDate: "$time",
                                        endDate: new Date(datePrefix),
                                        unit: "millisecond"
                                    }
                                }
                            },
                        }
                    },
                    {$project: {_id: 0, a: 1, b: 1, c: 1, d: 1}}
                ],
                expectedResults: [
                    {a: 100, b: 500, c: -500, d: -100},
                ],
                usesBlockProcessing: false
            },
            {
                name: "MaxAndMinOfDateAddDateSubtractDateTruncWithId_GroupByNull",
                pipeline: [
                    {$match: {[timeFieldName]: {$lte: dateUpperBound}}},
                    {
                        $group: {
                            _id: null,
                            a: {
                                $min: {
                                    $dateAdd: {startDate: "$time", unit: "millisecond", amount: 100}
                                }
                            },
                            b: {
                                $max: {
                                    $dateSubtract:
                                        {startDate: "$time", unit: "millisecond", amount: 100}
                                }
                            },
                            c: {$max: {$dateTrunc: {date: "$time", unit: "second"}}},
                        }
                    },
                    {$project: {_id: 0, a: 1, b: 1, c: 1}}
                ],
                expectedResults: [
                    {
                        a: ISODate("1970-01-20T10:55:12.640Z"),
                        b: ISODate("1970-01-20T10:55:12.840Z"),
                        c: ISODate("1970-01-20T10:55:12.000Z")
                    },
                ],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByDateTruncAndDateAdd",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {
                                $dateTrunc: {
                                    date: {
                                        $dateAdd:
                                            {startDate: "$time", unit: "millisecond", amount: 100}
                                    },
                                    unit: "hour"
                                }
                            },
                            a: {$min: '$y'},
                            b: {$max: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: ISODate("1970-01-20T10:00:00Z"), a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByDateTruncAndDateSubtract",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {
                                $dateTrunc: {
                                    date: {
                                        $dateSubtract:
                                            {startDate: "$time", unit: "millisecond", amount: 100}
                                    },
                                    unit: "hour"
                                }
                            },
                            a: {$min: '$y'},
                            b: {$max: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: ISODate("1970-01-20T10:00:00Z"), a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByDateDiffAndDateAdd",
                pipeline: [
                    {$match: {[timeFieldName]: {$lt: dateUpperBound}}},
                    {
                        $group: {
                            _id: {
                                $dateDiff: {
                                    startDate: new Date(datePrefix),
                                    endDate: {
                                        $dateAdd:
                                            {startDate: "$time", unit: "millisecond", amount: 100}
                                    },
                                    unit: "millisecond"
                                }
                            },
                            a: {$min: '$y'},
                        }
                    },
                    {$project: {_id: 1, a: 1}}
                ],
                expectedResults:
                    [{_id: 200, a: 73}, {_id: 300, a: 42}, {_id: 400, a: 11}, {_id: 500, a: 99}],

                usesBlockProcessing: false
            },
            {
                name: "MinOfDateDiffWithId_GroupByNull_InvalidDate",
                pipeline: [
                    {$match: {[timeFieldName]: {$lte: dateUpperBound}}},
                    {
                        $group: {
                            _id: null,
                            a: {
                                $min: {
                                    $dateDiff: {
                                        startDate: new Date(datePrefix),
                                        endDate: "$y",
                                        unit: "millisecond"
                                    }
                                }
                            },
                        }
                    },
                    {$project: {_id: 0, a: 1}}
                ],
                expectedErrorCode: 7157922,
                usesBlockProcessing: false
            },
            {
                name: "MinOfDateAddWithId_GroupByNull_MissingAmount",
                pipeline: [
                    {$match: {[timeFieldName]: {$lte: dateUpperBound}}},
                    {
                        $group: {
                            _id: null,
                            a: {
                                $min: {
                                    $dateAdd: {
                                        startDate: new Date(datePrefix),
                                        unit: "millisecond",
                                        amount: "$k"
                                    }
                                }
                            },
                        }
                    },
                    {$project: {_id: 0, a: 1}}
                ],
                expectedResults: [{a: null}],
                usesBlockProcessing: false
            },
            {
                name: "MaxPlusMinWithId_GroupByDateDiff",
                pipeline: [
                    {$match: {[timeFieldName]: {$lte: dateUpperBound}}},
                    {
                        $group: {
                            _id: {
                                $dateDiff: {
                                    startDate: new Date(datePrefix),
                                    endDate: "$time",
                                    unit: "millisecond"
                                }
                            },
                            a: {$min: '$y'},
                            b: {$max: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: {$add: ['$b', '$a']}}}
                ],
                expectedResults: [
                    {_id: 100, a: 73 + 73},
                    {_id: 200, a: 42 + 42},
                    {_id: 300, a: 11 + 11},
                    {_id: 400, a: 99 + 99},
                    {_id: 500, a: null}
                ],
                usesBlockProcessing: false
            },
            {
                name: "MaxPlusMinWithId_GroupByFilteredComputedDateDiff",
                pipeline: [
                    {$match: {[timeFieldName]: {$lte: new Date(datePrefix + 300)}}},
                    {
                        $addFields: {
                            msDiff: {
                                $dateDiff: {
                                    startDate: new Date(datePrefix),
                                    endDate: "$time",
                                    unit: "millisecond"
                                }
                            }
                        }
                    },
                    {$match: {msDiff: {$gte: 300}}},
                    {$group: {_id: "$msDiff", a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$add: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: 300, a: 11 + 11}],
                usesBlockProcessing: false
            },
            {
                name: "Min_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$min: '$y'}}}, {$project: {_id: 0, a: 1}}],
                expectedResults: [{a: 11}, {a: 42}, {a: null}],
                usesBlockProcessing: false
            },
            {
                name: "MinWithId_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$min: '$y'}}}],
                expectedResults: [{_id: 123, a: 42}, {_id: 456, a: 11}, {_id: null, a: null}],
                usesBlockProcessing: false
            },
            {
                name: "Max_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$max: '$y'}}}, {$project: {_id: 0, a: 1}}],
                expectedResults: [{a: 73}, {a: 99}, {a: null}],
                usesBlockProcessing: false
            },
            {
                name: "MaxWithId_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$max: '$y'}}}],
                expectedResults: [{_id: 123, a: 73}, {_id: 456, a: 99}, {_id: null, a: null}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMin_GroupByX_NoFilter",
                pipeline: [
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 0, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{a: 31}, {a: 88}, {a: null}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByX_NoFilter",
                pipeline: [
                    {$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: 123, a: 31}, {_id: 456, a: 88}, {_id: null, a: null}],
                usesBlockProcessing: false
            },
            {
                name: "MinAndMaxWithId_GroupByX_NoFilter",
                pipeline: [{$group: {_id: '$x', a: {$min: '$y'}, b: {$max: '$y'}}}],
                expectedResults: [
                    {_id: 123, a: 42, b: 73},
                    {_id: 456, a: 11, b: 99},
                    {_id: null, a: null, b: null}
                ],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByDateTrunc_NoFilter",
                pipeline: [
                    {
                        $group: {
                            _id: {"$dateTrunc": {date: "$time", unit: "minute", binSize: 1}},
                            a: {$min: '$y'},
                            b: {$max: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: ISODate("1970-01-20T10:55:00Z"), a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByDateTruncAndDateDiff_NoFilter",
                pipeline: [
                    {
                        $group: {
                            _id: {
                                date: {
                                    $dateTrunc: {date: "$time", unit: "millisecond", binSize: 200}
                                },
                                delta: {
                                    $dateDiff: {
                                        startDate: new Date(datePrefix),
                                        endDate: "$time",
                                        unit: "millisecond"
                                    }
                                }
                            },
                            a: {$min: '$y'},
                            b: {$max: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [
                    {_id: {date: ISODate("1970-01-20T10:55:12.400Z"), delta: 100}, a: 0},
                    {_id: {date: ISODate("1970-01-20T10:55:12.600Z"), delta: 200}, a: 0},
                    {_id: {date: ISODate("1970-01-20T10:55:12.600Z"), delta: 300}, a: 0},
                    {_id: {date: ISODate("1970-01-20T10:55:12.800Z"), delta: 400}, a: 0},
                    {_id: {date: ISODate("1970-01-20T10:55:12.800Z"), delta: 500}, a: null}
                ],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByDateTruncAndMeta_NoFilter",
                pipeline: [
                    {
                        $group: {
                            _id: {
                                date: {$dateTrunc: {date: "$time", unit: "minute", binSize: 1}},
                                symbol: "$measurement"
                            },
                            a: {$min: '$y'},
                            b: {$max: '$y'}
                        }
                    },
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults:
                    [{_id: {date: ISODate("1970-01-20T10:55:00Z"), symbol: "foo"}, a: 88}],
                usesBlockProcessing: false
            },
            {
                name: "MaxMinusMinWithId_GroupByMeta_NoFilter",
                pipeline: [
                    {$group: {_id: "$measurement", a: {$min: '$y'}, b: {$max: '$y'}}},
                    {$project: {_id: 1, a: {$subtract: ['$b', '$a']}}}
                ],
                expectedResults: [{_id: "foo", a: 88}],
                usesBlockProcessing: false
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
            {
                name: "GroupWithProjectedOutFieldInAccumulator",
                pipeline: [
                    {$project: {_id: 0}},
                    {$match: {[metaFieldName]: "foo"}},
                    {$group: {_id: null, minY: {$min: "$y"}}},
                ],
                expectedResults: [{_id: null, minY: 11}],
                usesBlockProcessing: false,
            },
            {
                name: "GroupWithProjectedOutFieldInGb",
                pipeline: [
                    {$project: {_id: 0}},
                    {$match: {[metaFieldName]: "foo"}},
                    {$group: {_id: "$y", a: {$min: "$x"}}},
                ],
                expectedResults: [
                    {_id: 11, a: 456},
                    {_id: 42, a: 123},
                    {_id: 73, a: 123},
                    {_id: 99, a: 456},
                    {_id: null, a: null}
                ],
                usesBlockProcessing: false,
            },
            {
                name: "GroupWithMixOfProjectedOutField",
                pipeline: [
                    {$project: {_id: 0, x: 1 /* y not included */}},
                    {$match: {[metaFieldName]: "foo"}},
                    {$group: {_id: "$y", a: {$min: "$x"}}},
                ],
                expectedResults: [],
                usesBlockProcessing: false,
            }
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
            const expectedErrorCode = testcase.expectedErrorCode;
            const usesBlockProcessing = testcase.usesBlockProcessing;

            if (expectedResults) {
                // Issue the aggregate() query and collect the results (together with their
                // JSON representations).
                const results = coll.aggregate(pipeline, options).toArray();

                // Sort the results.
                results.sort(compareResultEntries);

                const errMsgFn = () => "Test case '" + name + "':\nExpected " +
                    tojson(expectedResults) + "\n  !=\nActual " + tojson(results);

                // Check that the expected result and actual results have the same number of
                // elements.
                assert.eq(expectedResults.length, results.length, errMsgFn);

                // Check that each entry in the expected results array matches the corresponding
                // element in the actual results array.
                for (let i = 0; i < expectedResults.length; ++i) {
                    assert.docEq(expectedResults[i], results[i], errMsgFn);
                }
            } else if (expectedErrorCode) {
                assertErrorCode(coll, pipeline, expectedErrorCode);
            }

            // Check that explain indicates block processing is being used. This is a best effort
            // check.
            const explain = coll.explain().aggregate(pipeline, options);
            const engineUsed = getEngine(explain);
            const singleNodeQueryPlanner = getQueryPlanner(getSingleNodeExplain(explain));

            function testcaseAndExplainFn(description) {
                return () => description + " for test case '" + name + "' failed with explain " +
                    tojson(singleNodeQueryPlanner);
            }

            const hasSbePlan = engineUsed === "sbe";
            const sbePlan =
                hasSbePlan ? singleNodeQueryPlanner.winningPlan.slotBasedPlan.stages : null;

            if (usesBlockProcessing) {
                // Verify that we have an SBE plan, and verify that "block_group" appears in the
                // plan.
                assert.eq(engineUsed, "sbe");

                assert(sbePlan.includes("block_group"),
                       testcaseAndExplainFn("Expected explain to use block processing"));
            } else {
                if (hasSbePlan) {
                    // If 'usesBlockProcessing' is false and we have an SBE plan, verify that
                    // "block_group" does not appear anywhere in the SBE plan.
                    assert(!sbePlan.includes("block_group"),
                           testcaseAndExplainFn("Expected explain not to use block processing"));
                }
            }
        }
    }

    // Run the tests with allowDiskUse=false.
    runTests(false /* allowDiskUse */, false);

    // Run the tests with allowDiskUse=true.
    runTests(true /* allowDiskUse */, false);

    // Run the tests with allowDiskUse=true and force spilling.
    runTests(true /* allowDiskUse */, true);
});
