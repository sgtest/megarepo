/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/json.h"
#include "mongo/db/pipeline/aggregation_context_fixture.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/query/util/make_data_structure.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo {
namespace {

using InternalUnpackBucketGroupReorder = AggregationContextFixture;

std::vector<BSONObj> makeAndOptimizePipeline(
    boost::intrusive_ptr<mongo::ExpressionContextForTest> expCtx,
    BSONObj groupSpec,
    int bucketMaxSpanSeconds,
    bool fixedBuckets) {
    auto unpackSpecObj = BSON("$_internalUnpackBucket"
                              << BSON("include" << BSON_ARRAY("a"
                                                              << "b"
                                                              << "c")
                                                << "timeField"
                                                << "t"
                                                << "metaField"
                                                << "meta1"
                                                << "bucketMaxSpanSeconds" << bucketMaxSpanSeconds
                                                << "fixedBuckets" << fixedBuckets));

    auto pipeline = Pipeline::parse(makeVector(unpackSpecObj, groupSpec), expCtx);
    pipeline->optimizePipeline();
    return pipeline->serializeToBson();
}

// The following tests confirm the expected behavior for the $count aggregation stage rewrite.
TEST_F(InternalUnpackBucketGroupReorder, OptimizeForCountAggStage) {
    auto unpackSpecObj = fromjson(
        "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], metaField: 'meta1', timeField: 't', "
        "bucketMaxSpanSeconds: 3600}}");
    auto countSpecObj = fromjson("{$count: 'foo'}");

    auto pipeline = Pipeline::parse(makeVector(unpackSpecObj, countSpecObj), getExpCtx());
    pipeline->optimizePipeline();
    auto serialized = pipeline->serializeToBson();

    // $count gets rewritten to $group + $project without the $unpack stage.
    ASSERT_EQ(2, serialized.size());
    auto groupOptimized = fromjson(
        "{ $group : { _id : {$const: null }, foo : { $sum : { $cond: [{$gte : [ "
        "'$control.version', {$const : 2} ]}, '$control.count', {$size : [ {$objectToArray : "
        "['$data.t']} ] } ] } } } }");
    ASSERT_BSONOBJ_EQ(groupOptimized, serialized[0]);
}

TEST_F(InternalUnpackBucketGroupReorder, OptimizeForCountInGroup) {
    auto groupSpecObj = fromjson("{$group: {_id: '$meta1.a.b', acccount: {$count: {} }}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto groupOptimized = fromjson(
        "{ $group : { _id : '$meta.a.b', acccount : { $sum : { $cond: [{$gte : [ "
        "'$control.version', {$const : 2} ]}, '$control.count', {$size : [ {$objectToArray : "
        "['$data.t']} ] } ] } } } }");
    ASSERT_BSONOBJ_EQ(groupOptimized, serialized[0]);
}

TEST_F(InternalUnpackBucketGroupReorder, OptimizeForCountNegative) {
    auto groupSpecObj = fromjson("{$group: {_id: '$a', s: {$sum: '$b'}}}");
    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(2, serialized.size());

    // We do not get the reorder since we are grouping on a field.
    auto optimized = fromjson(
        "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: 'meta1', "
        "bucketMaxSpanSeconds: 3600}}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

// The following tests confirms the $group rewrite applies when the _id field is a field path
// referencing the metaField, a constant expression, and/or for fixed buckets $dateTrunc expression
// on the timeField
TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMetadata) {
    auto groupSpecObj =
        fromjson("{$group: {_id: '$meta1.a.b', accmin: {$min: '$b'}, accmax: {$max: '$c'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto optimized = fromjson(
        "{$group: {_id: '$meta.a.b', accmin: {$min: '$control.min.b'}, accmax: {$max: "
        "'$control.max.c'}}}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

// Test SERVER-73822 fix: complex $min and $max (i.e. not just straight field refs) work correctly.
TEST_F(InternalUnpackBucketGroupReorder, MinMaxComplexGroupOnMetadata) {
    auto groupSpecObj = fromjson(
        "{$group: {_id: '$meta1.a.b', accmin: {$min: {$add: ['$b', {$const: 0}]}}, accmax: {$max: "
        "{$add: [{$const: 0}, '$c']}}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(2, serialized.size());
    // Order of fields may be different between original 'unpackSpecObj' and 'serialized[0]'.
    //   ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
    ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMetafield) {
    auto groupSpecObj = fromjson("{$group: {_id: '$meta1.a.b', accmin: {$min: '$meta1.f1'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto optimized = fromjson("{$group: {_id: '$meta.a.b', accmin: {$min: '$meta.f1'}}}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMetafieldIdObj) {
    auto groupSpecObj =
        fromjson("{$group: {_id: { d: '$meta1.a.b' }, accmin: {$min: '$meta1.f1'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto optimized = fromjson("{$group: {_id: {d: '$meta.a.b'}, accmin: {$min: '$meta.f1'}}}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxDateTruncTimeField) {
    auto groupSpecObj = fromjson(
        "{$group: {_id: {time: {$dateTrunc: {date: '$t', unit: 'day'}}}, accmin: {$min: '$a'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, true /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto optimized = fromjson(
        "{$group: {_id: {time: {$dateTrunc: {date: '$control.min.t', unit: {$const: 'day'}}}}, "
        "accmin: {$min: '$control.min.a'} }}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxConstantGroupKey) {
    // Test with a null group key.
    {
        auto groupSpecObj = fromjson("{$group: {_id: null, accmin: {$min: '$meta1.f1'}}}");

        auto serialized = makeAndOptimizePipeline(
            getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
        ASSERT_EQ(1, serialized.size());

        auto optimized = fromjson("{$group: {_id: { $const: null }, accmin: {$min: '$meta.f1'}}}");
        ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
    }
    // Test with an int group key.
    {
        auto groupSpecObj = fromjson("{$group: {_id: 0, accmin: {$min: '$meta1.f1'}}}");

        auto serialized = makeAndOptimizePipeline(
            getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
        ASSERT_EQ(1, serialized.size());

        auto optimized = fromjson("{$group: {_id:  {$const: 0}, accmin: {$min: '$meta.f1'}}}");
        ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
    }
    // Test with an expression that is optimized to a constant.
    {
        auto groupSpecObj =
            fromjson("{$group: {_id: {$add: [2, 3]}, accmin: {$min: '$meta1.f1'}}}");

        auto serialized = makeAndOptimizePipeline(
            getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
        ASSERT_EQ(1, serialized.size());

        auto optimized = fromjson("{$group: {_id:  {$const: 5}, accmin: {$min: '$meta.f1'}}}");
        ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
    }
    // Test with an int group key and no metaField.
    {
        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', "
            "bucketMaxSpanSeconds: 3600}}");
        auto groupSpecObj = fromjson("{$group: {_id: 0, accmin: {$min: '$meta1.f1'}}}");

        auto pipeline = Pipeline::parse(makeVector(unpackSpecObj, groupSpecObj), getExpCtx());
        pipeline->optimizePipeline();
        auto serialized = pipeline->serializeToBson();

        ASSERT_EQ(1, serialized.size());

        auto optimized =
            fromjson("{$group: {_id:  {$const: 0}, accmin: {$min: '$control.min.meta1.f1'}}}");
        ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
    }
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMultipleMetaFields) {
    auto groupSpecObj = fromjson(
        "{$group: {_id: {m1: '$meta1.m1', m2: '$meta1.m2', m3: '$meta1' }, accmin: {$min: "
        "'$meta1.f1'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto optimized = fromjson(
        "{$group: {_id: {m1: '$meta.m1', m2: '$meta.m2', m3: '$meta' }, accmin: {$min: "
        "'$meta.f1'}}}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMultipleMetaFieldsAndConst) {
    auto groupSpecObj = fromjson(
        "{$group: {_id: {m1: 'hello', m2: '$meta1.m1', m3: '$meta1' }, accmin: {$min: "
        "'$meta1.f1'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto optimized = fromjson(
        "{$group: {_id: {m1: {$const: 'hello'}, m2: '$meta.m1', m3: '$meta' }, accmin: {$min: "
        "'$meta.f1'}}}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

// The following tests demonstrate that $group rewrites for the _id field will recurse into
// arbitrary expressions.
TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMetaFieldsExpression) {
    {
        auto groupSpecObj =
            fromjson("{$group: {_id: {m1: {$toUpper: '$meta1.m1'}}, accmin: {$min: '$val'}}}");
        auto serialized = makeAndOptimizePipeline(getExpCtx(), groupSpecObj, 3600, false);
        ASSERT_EQ(1, serialized.size());

        auto optimized = fromjson(
            "{$group: {_id: {m1: {$toUpper: [ '$meta.m1' ] }}, accmin: {$min: "
            "'$control.min.val'}}}");
        ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
    }
    {
        auto groupSpecObj = fromjson(
            "{$group: {_id: {m1: {$concat: [{$trim: {input: {$toUpper: '$meta1.m1'}}}, '-', "
            "{$trim: {input: {$toUpper: '$meta1.m2'}}}]}}, accmin: {$min: '$val'}}}");
        auto serialized = makeAndOptimizePipeline(getExpCtx(), groupSpecObj, 3600, false);
        ASSERT_EQ(1, serialized.size());

        auto optimized = fromjson(
            "{$group: {_id: {m1: {$concat: [{$trim: {input: {$toUpper: [ '$meta.m1' ]}}}, "
            "{$const: '-'}, {$trim: {input: {$toUpper: [ '$meta.m2' ]}}}]}}, accmin: {$min: "
            "'$control.min.val'}}}");
        ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
    }
}

TEST_F(InternalUnpackBucketGroupReorder, MaxGroupRewriteTimeField) {
    // Validate $max can be rewritten if on the timeField to use control.max.time, since
    // control.max.time is not rounded, like control.min.time.
    auto groupSpecObj = fromjson("{$group: {_id:'$meta1.m1', accmax: {$max: '$t'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(1, serialized.size());

    auto optimized = fromjson("{$group: {_id: '$meta.m1', accmax: {$max: '$control.max.t'}}}");
    ASSERT_BSONOBJ_EQ(optimized, serialized[0]);
}

// The following tests confirms the $group rewrite does not apply when some requirements are not
// met.
TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMetadataNegative) {
    // This rewrite does not apply because the $group stage uses the $sum accumulator.
    auto groupSpecObj =
        fromjson("{$group: {_id: '$meta1', accmin: {$min: '$b'}, s: {$sum: '$c'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(2, serialized.size());

    auto unpackSpecObj = fromjson(
        "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: 'meta1', "
        "bucketMaxSpanSeconds: 3600}}");
    ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
    ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMetadataNegative1) {
    // This rewrite does not apply because the $min accumulator is on a nested field referencing the
    // timeField.
    auto groupSpecObj = fromjson("{$group: {_id: '$meta1', accmin: {$min: '$t.a'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(2, serialized.size());

    auto unpackSpecObj = fromjson(
        "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: 'meta1', "
        "bucketMaxSpanSeconds: 3600}}");
    ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
    ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMetadataExpressionNegative) {
    // This rewrite does not apply because we are grouping on an expression that references a field.
    {
        auto groupSpecObj =
            fromjson("{$group: {_id: {m1: {$toUpper: [ '$val.a' ]}}, accmin: {$min: '$val.b'}}}");
        auto serialized = makeAndOptimizePipeline(getExpCtx(), groupSpecObj, 3600, false);
        ASSERT_EQ(2, serialized.size());

        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: "
            "'meta1', bucketMaxSpanSeconds: 3600}}");
        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
    }
    // This rewrite does not apply because _id.m2 references a field. Moreover, the
    // original group spec remains unchanged even though we were able to rewrite _id.m1.
    {
        auto groupSpecObj = fromjson(
            "{$group: {_id: {"
            // m1 is allowed since all field paths reference the metaField.
            "  m1: {$concat: [{$trim: {input: {$toUpper: [ '$meta1.m1' ]}}}, {$trim: {input: "
            "    {$toUpper: [ '$meta1.m2' ]}}}]},"
            // m2 is not allowed and so inhibits the optimization.
            "  m2: {$trim: {input: {$toUpper: [ '$val.a']}}}"
            "}, accmin: {$min: '$val'}}}");
        auto serialized = makeAndOptimizePipeline(getExpCtx(), groupSpecObj, 3600, false);
        ASSERT_EQ(2, serialized.size());

        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: "
            "'meta1', bucketMaxSpanSeconds: 3600}}");
        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
    }
    // When there is no metaField, any field path prevents rewriting the $group stage.
    {
        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', "
            "bucketMaxSpanSeconds: 3600}}");
        auto groupSpecObj =
            fromjson("{$group: {_id: {g0: {$toUpper: [ '$x' ] }}, accmin: {$min: '$meta1.f1'}}}");

        auto pipeline = Pipeline::parse(makeVector(unpackSpecObj, groupSpecObj), getExpCtx());
        pipeline->optimizePipeline();
        auto serialized = pipeline->serializeToBson();

        ASSERT_EQ(2, serialized.size());

        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
    }
    // When there is no metaField, any field path prevents rewriting the $group stage, even if the
    // field path starts with $$CURRENT.
    {
        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', "
            "bucketMaxSpanSeconds: 3600}}");
        auto groupSpecObj = fromjson(
            "{$group: {_id: {g0: {$toUpper: [ '$$CURRENT.x' ] }}, accmin: {$min: '$meta1.f1'}}}");

        auto pipeline = Pipeline::parse(makeVector(unpackSpecObj, groupSpecObj), getExpCtx());
        pipeline->optimizePipeline();
        auto serialized = pipeline->serializeToBson();

        ASSERT_EQ(2, serialized.size());

        // The $$CURRENT.x field path will be simplified to $x before it reaches the group
        // optimization.
        auto wantGroupSpecObj =
            fromjson("{$group: {_id: {g0: {$toUpper: [ '$x' ] }}, accmin: {$min: '$meta1.f1'}}}");
        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(wantGroupSpecObj, serialized[1]);
    }
    // When there is no metaField, any field path prevents rewriting the $group stage, even if the
    // field path starts with $$ROOT.
    {
        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', "
            "bucketMaxSpanSeconds: 3600}}");
        auto groupSpecObj = fromjson(
            "{$group: {_id: {g0: {$toUpper: [ '$$ROOT.x' ] }}, accmin: {$min: '$meta1.f1'}}}");

        auto pipeline = Pipeline::parse(makeVector(unpackSpecObj, groupSpecObj), getExpCtx());
        pipeline->optimizePipeline();
        auto serialized = pipeline->serializeToBson();

        ASSERT_EQ(2, serialized.size());

        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
    }
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxDateTruncTimeFieldNegative) {
    // The rewrite does not apply because the buckets are not fixed.
    {
        auto groupSpecObj = fromjson(
            "{$group: {_id: {time: {$dateTrunc: {date: '$t', unit: 'day'}}}, accmin: {$min: "
            "'$a'}}}");

        auto serialized = makeAndOptimizePipeline(
            getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
        ASSERT_EQ(2, serialized.size());

        auto serializedGroup = fromjson(
            "{$group: {_id: {time: {$dateTrunc: {date: '$t', unit: {$const: 'day'}}}}, accmin: "
            "{$min: "
            "'$a'}}}");
        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: "
            "'meta1', bucketMaxSpanSeconds: 3600}}");
        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(serializedGroup, serialized[1]);
    }
    // The rewrite does not apply because bucketMaxSpanSeconds is too large.
    {
        auto groupSpecObj = fromjson(
            "{$group: {_id: {time: {$dateTrunc: {date: '$t', unit: 'day'}}}, accmin: {$min: "
            "'$a'}}}");

        auto serialized = makeAndOptimizePipeline(
            getExpCtx(), groupSpecObj, 604800 /* bucketMaxSpanSeconds */, true /* fixedBuckets */);
        ASSERT_EQ(2, serialized.size());

        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: "
            "'meta1', bucketMaxSpanSeconds: 604800, fixedBuckets: true}}");
        auto serializedGroupObj = fromjson(
            "{$group: {_id: {time: {$dateTrunc: {date: '$t', unit: {$const: 'day'}}}}, accmin: "
            "{$min: '$a'}}}");
        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(serializedGroupObj, serialized[1]);
    }
    // The rewrite does not apply because $dateTrunc is not on the timeField.
    {
        auto groupSpecObj = fromjson(
            "{$group: {_id: {time: {$dateTrunc: {date: '$c', unit: 'day'}}}, accmin: {$min: "
            "'$a'}}}");

        auto serialized = makeAndOptimizePipeline(
            getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, true /* fixedBuckets */);
        ASSERT_EQ(2, serialized.size());

        auto unpackSpecObj = fromjson(
            "{$_internalUnpackBucket: { include: ['a', 'b', 'c'], timeField: 't', metaField: "
            "'meta1', bucketMaxSpanSeconds: 3600, fixedBuckets: true}}");
        auto serializedGroupObj = fromjson(
            "{$group: {_id: {time: {$dateTrunc: {date: '$c', unit: {$const: 'day'}}}}, accmin: "
            "{$min: '$a'}}}");
        ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
        ASSERT_BSONOBJ_EQ(serializedGroupObj, serialized[1]);
    }
}

TEST_F(InternalUnpackBucketGroupReorder, MinMaxGroupOnMultipleMetaFieldsNegative) {
    // The rewrite does not apply, because some fields in the group key are not referencing the
    // metaField.
    auto groupSpecObj =
        fromjson("{$group: {_id: {m1: '$meta1.m1', m2: '$val' }, accmin: {$min: '$meta1.f1'}}}");

    auto serialized = makeAndOptimizePipeline(
        getExpCtx(), groupSpecObj, 3600 /* bucketMaxSpanSeconds */, false /* fixedBuckets */);
    ASSERT_EQ(2, serialized.size());

    auto unpackSpecObj = fromjson(
        "{$_internalUnpackBucket: { include: ['a', 'b', 'c'],  timeField: 't', metaField: 'meta1', "
        "bucketMaxSpanSeconds: 3600}}");
    ASSERT_BSONOBJ_EQ(unpackSpecObj, serialized[0]);
    ASSERT_BSONOBJ_EQ(groupSpecObj, serialized[1]);
}

}  // namespace
}  // namespace mongo
