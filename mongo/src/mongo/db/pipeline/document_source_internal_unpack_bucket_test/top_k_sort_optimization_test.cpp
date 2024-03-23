/**
 *    Copyright (C) 2024-present MongoDB, Inc.
 *
 *    This program is free software: you can redistribute it and/or modify
 *    it under the terms of the Server Side Public License, version 1,
 *    as published by MongoDB, Inc.
 *
 *    This program is distributed in the hope that it will be useful,
 *    but WITHOUT ANY WARRANTY; without even the implied warranty of
 *    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *    Server Side Public License for more details.
 *
 *    You should have received a copy of the Server Side Public License
 *    along with this program. If not, see
 *    <http://www.mongodb.com/licensing/server-side-public-license>.
 *
 *    As a special exception, the copyright holders give permission to link the
 *    code of portions of this program with the OpenSSL library under certain
 *    conditions as described in each individual source file and distribute
 *    linked combinations including the program with the OpenSSL library. You
 *    must comply with the Server Side Public License in all respects for
 *    all of the code used other than as permitted herein. If you modify file(s)
 *    with this exception, you may extend this exception to your version of the
 *    file(s), but you are not obligated to do so. If you do not wish to do so,
 *    delete this exception statement from your version. If you delete this
 *    exception statement from all source files in the program, then also delete
 *    it in the license file.
 */

#include <memory>
#include <vector>

#include "mongo/bson/json.h"
#include "mongo/db/pipeline/aggregation_context_fixture.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/query/explain_options.h"
#include "mongo/db/query/util/make_data_structure.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"

namespace mongo {
namespace {

using TopKSortOptimization = AggregationContextFixture;
const auto kExplain = SerializationOptions{
    .verbosity = boost::make_optional(ExplainOptions::Verbosity::kQueryPlanner)};

const auto commonInitialUnpackSpecObj = fromjson(R"(
{
    $_internalUnpackBucket: {
        exclude: [],
        timeField: 'time',
        metaField: 'tag',
        bucketMaxSpanSeconds: 3600
    }
}
)");

const auto firstLastGroupSpecObj = fromjson(R"(
{
    $group: {
        _id: {
            hour: {$dateTrunc: {date: '$time', unit: 'hour'}},
            symbol: '$tag.symbol'
        },
        open: {$first: '$price'},
        close: {$last: '$price'}
    }
}
)");

// The $group should absorb the $sort stage by replacing $first/$last by $top/$bottom respectively.
constexpr size_t kExpectedGroupIndexForMatchOnly = 2;
const auto expectedOptimizedGroupForMatchOnly = fromjson(R"(
{
    $group: {
        _id: {hour: {$dateTrunc: {date: '$time', unit: {$const: 'hour'}}}, symbol: '$tag.symbol'},
        open: {$top: {output: '$price', sortBy: {time: 1}}},
        close: {$bottom: {output: '$price', sortBy: {time: 1}}}
    }
}
)");

// The $match stage should be pushed down before the $_internalUnpackBucket.
constexpr size_t kExpectedMatchIndexForMatchOnly = 0;
const auto expectedOptimizedMatchForMatchOnly = fromjson(R"(
{
    $match: {
        $or: [
            {"control.max.price": {$_internalExprGte: 100}},
            {$expr: {$ne: [{$type: ["$control.min.price"]}, {$type: ["$control.max.price"]}]}}
        ]
    }
}
)");

// The $_internalUnpackBucket should have the event filter while $match being pushed down.
constexpr size_t kExpectedUnpackBucketIndexForMatchOnly = 1;
const auto expectedOptimizedUnpackBucketForMatchOnly = fromjson(R"(
{
    $_internalUnpackBucket: {
        include: ["price", "time", "tag"],
        timeField: 'time',
        metaField: 'tag',
        bucketMaxSpanSeconds: 3600,
        eventFilter: {price: {$gte: 100}}
    }
}
)");

TEST_F(TopKSortOptimization, MatchOnlyAfterTopKSortPushedDownWithTopKSortOptimizationApplied) {
    // The $match stage is after the $sort stage.
    auto pipeline = Pipeline::parse(makeVector(commonInitialUnpackSpecObj,
                                               fromjson("{$sort: {time: 1}}"),
                                               fromjson("{$match: {price: {$gte: 100}}}"),
                                               firstLastGroupSpecObj),
                                    getExpCtx());

    ASSERT_EQ(pipeline->getSources().size(), 4U);

    pipeline->optimizePipeline();

    // The $match stage should be pushed down before the $_internalUnpackBucket and the
    // $_internalUnpackBucket should have the event filter and the $sort stage should be absorbed
    // into the $group stage.
    auto actualOptimizedPipeline = pipeline->serializeToBson();
    ASSERT_EQ(actualOptimizedPipeline.size(), 3U)
        << "Expected three stages but got: " << to_string(actualOptimizedPipeline);

    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedGroupIndexForMatchOnly],
                      expectedOptimizedGroupForMatchOnly);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedMatchIndexForMatchOnly],
                      expectedOptimizedMatchForMatchOnly);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedUnpackBucketIndexForMatchOnly],
                      expectedOptimizedUnpackBucketForMatchOnly);
}

TEST_F(TopKSortOptimization, MatchOnlyBeforeTopKSortPushedDownWithTopKSortOptimizationApplied) {
    // The $match stage is before the $sort stage.
    auto pipeline = Pipeline::parse(makeVector(commonInitialUnpackSpecObj,
                                               fromjson("{$match: {price: {$gte: 100}}}"),
                                               fromjson("{$sort: {time: 1}}"),
                                               firstLastGroupSpecObj),
                                    getExpCtx());

    ASSERT_EQ(pipeline->getSources().size(), 4U);

    pipeline->optimizePipeline();

    // The $match stage should be pushed down before the $_internalUnpackBucket and the
    // $_internalUnpackBucket should have the event filter and the $sort stage should be absorbed
    // into the $group stage.
    auto actualOptimizedPipeline = pipeline->serializeToBson();
    ASSERT_EQ(actualOptimizedPipeline.size(), 3U)
        << "Expected three stages but got: " << to_string(actualOptimizedPipeline);

    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedGroupIndexForMatchOnly],
                      expectedOptimizedGroupForMatchOnly);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedMatchIndexForMatchOnly],
                      expectedOptimizedMatchForMatchOnly);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedUnpackBucketIndexForMatchOnly],
                      expectedOptimizedUnpackBucketForMatchOnly);
}

const auto firstSumGroupSpecObj = fromjson(R"(
{
    $group: {
        _id: {hour: {$dateTrunc: {date: '$time', unit: 'hour'}}, symbol: '$s'},
        open: {$first: '$price'},
        totalVol: {$sum: '$vol'}
    }
}
)");

// The $group should absorb the $sort stage by replacing $first by $top.
constexpr size_t kExpectedGroupIndexForMatchAndProject = 2;
const auto expectedOptimizedGroupForMatchAndProject = fromjson(R"(
{
    $group: {
        _id: {hour: {$dateTrunc: {date: '$time', unit: {$const: 'hour'}}}, symbol: '$s'},
        open: {$top: {output: '$price', sortBy: {s: 1}}},
        totalVol: {$sum: '$vol'}
    }
}
)");

// The $_internalUnpackBucket should have the event filter while absorbing the $match and also have
// the computed meta field for 's' for renaming 'tag.symbol' to 's'.
constexpr size_t kExpectedUnpackBucketIndexForMatchAndProject = 1;
const auto expectedOptimizedUnpackBucketForMatchAndProject = fromjson(R"(
{
    $_internalUnpackBucket: {
        include: ['_id', 's'],
        timeField: 'time',
        metaField: 'tag',
        bucketMaxSpanSeconds: 3600,
        computedMetaProjFields: ['s'],
        eventFilter: {s: {$in: ['abc', 'bcd']}}
    }
}
    )");

// The $project stage should be pushed down before the $_internalUnpackBucket as an $addFields
// stage.
constexpr size_t kExpectedAddFieldsIndexForMatchAndProject = 0;
const auto expectedOptimizedAddFieldsForMatchAndProject = fromjson(R"(
{
    $addFields: {s: '$meta.symbol'}
}
)");

TEST_F(TopKSortOptimization,
       Project_Match_Before_TopKSort_Optimized_WithTopKSortOptimizationApplied) {
    auto pipeline =
        Pipeline::parse(makeVector(commonInitialUnpackSpecObj,
                                   // Renames 'tag.symbol' to 's'.
                                   fromjson("{$project: {s: '$tag.symbol'}}"),
                                   // The $match can be absorbed into the $_internalUnpackBucket.
                                   fromjson("{$match: {s: {$in: ['abc', 'bcd']}}}"),
                                   // And yet the $sort can be absorbed into the $group stage.
                                   fromjson("{$sort: {s: 1}}"),
                                   firstSumGroupSpecObj),
                        getExpCtx());

    ASSERT_EQ(pipeline->getSources().size(), 5U);

    pipeline->optimizePipeline();

    // The $match stage should be absorbed into the $_internalUnpackBucket and the
    // $_internalUnpackBucket should have the event filter and the $sort stage should be absorbed
    // into the $group stage. The $project stage should be pushed down before the
    // $_internalUnpackBucket as an $addFields stage.
    auto actualOptimizedPipeline = pipeline->serializeToBson();
    ASSERT_EQ(actualOptimizedPipeline.size(), 3U)
        << "Expected three stages but got: " << to_string(actualOptimizedPipeline);

    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedGroupIndexForMatchAndProject],
                      expectedOptimizedGroupForMatchAndProject);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedUnpackBucketIndexForMatchAndProject],
                      expectedOptimizedUnpackBucketForMatchAndProject);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedAddFieldsIndexForMatchAndProject],
                      expectedOptimizedAddFieldsForMatchAndProject);
}

TEST_F(TopKSortOptimization,
       Match_Project_Before_TopKSort_Optimized_WithTopKSortOptimizationApplied) {
    auto pipeline =
        Pipeline::parse(makeVector(commonInitialUnpackSpecObj,
                                   // The $match can be absorbed into the $_internalUnpackBucket.
                                   fromjson("{$match: {'tag.symbol': {$in: ['abc', 'bcd']}}}"),
                                   // Renames 'tag.symbol' to 's'.
                                   fromjson("{$project: {s: '$tag.symbol'}}"),
                                   // And yet the $sort can be absorbed into the $group stage.
                                   fromjson("{$sort: {s: 1}}"),
                                   firstSumGroupSpecObj),
                        getExpCtx());

    ASSERT_EQ(pipeline->getSources().size(), 5U);

    pipeline->optimizePipeline();

    // The $match stage should be pushed down before the $_internalUnpackBucket and the $sort stage
    // should be absorbed into the $group stage. The $project stage should be pushed down before the
    // $_internalUnpackBucket as an $addFields stage.
    auto actualOptimizedPipeline = pipeline->serializeToBson();
    ASSERT_EQ(actualOptimizedPipeline.size(), 4U)
        << "Expected four stages but got: " << to_string(actualOptimizedPipeline);

    // Hides the global 'kExpectedGroupIndex' intentionally since the optimized query is slightly
    // different.
    const size_t kExpectedGroupIndex = 3;
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedGroupIndex],
                      expectedOptimizedGroupForMatchAndProject);

    // Hides the global 'kExpectedGroupIndex' & 'expectedOptimizedUnpackBucket' intentionally since
    // the optimized query is slightly different.
    const size_t kExpectedUnpackBucketIndex = 2;
    const auto expectedOptimizedUnpackBucket = fromjson(R"(
{
    $_internalUnpackBucket: {
        include: ['_id', 's'],
        timeField: 'time',
        metaField: 'tag',
        bucketMaxSpanSeconds: 3600,
        computedMetaProjFields: ['s']
    }
}
        )");
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedUnpackBucketIndex],
                      expectedOptimizedUnpackBucket);

    // Hides the global 'kExpectedAddFieldsIndex' intentionally since the optimized query is
    // slightly different.
    const size_t kExpectedAddFieldsIndex = 1;
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedAddFieldsIndex],
                      expectedOptimizedAddFieldsForMatchAndProject);

    // The $match stage should be pushed down before the $_internalUnpackBucket.
    const size_t kExpectedMatchIndex = 0;
    const auto expectedOptimizedMatch = fromjson(R"(
{$match: {"meta.symbol": {$in: ["abc", "bcd"]}}}
        )");
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedMatchIndex], expectedOptimizedMatch);
}

TEST_F(TopKSortOptimization,
       Project_TopKSort_Then_Match_Optimized_WithTopKSortOptimizationApplied) {
    auto pipeline =
        Pipeline::parse(makeVector(commonInitialUnpackSpecObj,
                                   // Renames 'tag.symbol' to 's'.
                                   fromjson("{$project: {s: '$tag.symbol'}}"),
                                   // And yet the $sort can be absorbed into the $group stage.
                                   fromjson("{$sort: {'s': 1}}"),
                                   // The $match can be absorbed into the $_internalUnpackBucket.
                                   fromjson("{$match: {s: {$in: ['abc', 'bcd']}}}"),
                                   firstSumGroupSpecObj),
                        getExpCtx());

    ASSERT_EQ(pipeline->getSources().size(), 5U);

    pipeline->optimizePipeline();

    // The $match stage should be absorbed into the $_internalUnpackBucket and the
    // $_internalUnpackBucket should have the event filter and the $sort stage should be absorbed
    // into the $group stage. The $project stage should be pushed down before the
    // $_internalUnpackBucket as an $addFields stage.
    auto actualOptimizedPipeline = pipeline->serializeToBson();
    ASSERT_EQ(actualOptimizedPipeline.size(), 3U)
        << "Expected three stages but got: " << to_string(actualOptimizedPipeline);

    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedGroupIndexForMatchAndProject],
                      expectedOptimizedGroupForMatchAndProject);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedUnpackBucketIndexForMatchAndProject],
                      expectedOptimizedUnpackBucketForMatchAndProject);
    ASSERT_BSONOBJ_EQ(actualOptimizedPipeline[kExpectedAddFieldsIndexForMatchAndProject],
                      expectedOptimizedAddFieldsForMatchAndProject);
}
}  // namespace
}  // namespace mongo
