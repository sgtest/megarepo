/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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

#include <algorithm>
#include <cstddef>
#include <functional>
#include <iostream>
#include <memory>
#include <string>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/json.h"
#include "mongo/db/exec/sbe/abt/sbe_abt_test_util.h"
#include "mongo/db/service_context.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo::optimizer {
namespace {

using TestContextFn = std::function<ServiceContext::UniqueOperationContext()>;

static std::vector<BSONObj> fromjson(const std::vector<std::string>& jsonVector) {
    std::vector<BSONObj> bsonObjs;
    bsonObjs.reserve(jsonVector.size());
    for (const std::string& jsonStr : jsonVector) {
        bsonObjs.push_back(mongo::fromjson(jsonStr));
    }
    return bsonObjs;
}

static bool compareSBEABTAgainstExpected(const TestContextFn& fn,
                                         const std::string& pipelineStr,
                                         const std::vector<BSONObj>& inputObjs,
                                         const std::vector<BSONObj>& expected) {
    const auto& actual = runSBEAST(fn().get(), pipelineStr, inputObjs);
    return compareResults(expected, actual, true /*preserveFieldOrder*/);
}

static bool comparePipelineAgainstExpected(const TestContextFn& fn,
                                           const std::string& pipelineStr,
                                           const std::vector<BSONObj>& inputObjs,
                                           const std::vector<BSONObj>& expected) {
    const auto& actual = runPipeline(fn().get(), pipelineStr, inputObjs);
    return compareResults(expected, actual, true /*preserveFieldOrder*/);
}

static bool compareSBEABTAgainstPipeline(const TestContextFn& fn,
                                         const std::string& pipelineStr,
                                         const std::vector<BSONObj>& inputObjs,
                                         const bool preserveFieldOrder = true) {
    const auto& pipelineResults = runPipeline(fn().get(), pipelineStr, inputObjs);
    const auto& sbeResults = runSBEAST(fn().get(), pipelineStr, inputObjs);

    std::cout << "Pipeline: " << pipelineStr << ", input size: " << inputObjs.size() << "\n";
    const bool result = compareResults(pipelineResults, sbeResults, preserveFieldOrder);
    if (result) {
        std::cout << "Success. Result count: " << pipelineResults.size() << "\n";
        constexpr size_t maxResults = 1;
        for (size_t i = 0; i < std::min(pipelineResults.size(), maxResults); i++) {
            std::cout << "Result " << (i + 1) << "/" << pipelineResults.size()
                      << ": expected (pipeline): " << pipelineResults.at(i)
                      << " vs actual (SBE): " << sbeResults.at(i) << "\n";
        }
    }

    return result;
}

TEST_F(NodeSBE, DiffTestBasic) {
    const auto contextFn = [this]() {
        return makeOperationContext();
    };
    const auto compare = [&contextFn](const std::string& pipelineStr,
                                      const std::vector<std::string>& jsonVector) {
        return compareSBEABTAgainstPipeline(
            contextFn, pipelineStr, fromjson(jsonVector), true /*preserveFieldOrder*/);
    };

    ASSERT_TRUE(compareSBEABTAgainstExpected(
        contextFn, "[]", fromjson({"{a:1, b:2, c:3}"}), fromjson({"{ a: 1, b: 2, c: 3 }"})));
    ASSERT_TRUE(compareSBEABTAgainstExpected(contextFn,
                                             "[{$addFields: {c: {$literal: 3}}}]",
                                             fromjson({"{a:1, b:2}"}),
                                             fromjson({"{ a: 1, b: 2, c: 3 }"})));

    ASSERT_TRUE(comparePipelineAgainstExpected(
        contextFn, "[]", fromjson({"{a:1, b:2, c:3}"}), fromjson({"{ a: 1, b: 2, c: 3 }"})));
    ASSERT_TRUE(comparePipelineAgainstExpected(contextFn,
                                               "[{$addFields: {c: {$literal: 3}}}]",
                                               fromjson({"{a:1, b:2}"}),
                                               fromjson({"{ a: 1, b: 2, c: 3 }"})));

    ASSERT_TRUE(comparePipelineAgainstExpected(
        contextFn,
        "[{$match: {a: NaN}}]",
        {BSON("a" << Decimal128::kNegativeNaN), BSON("a" << Decimal128::kPositiveNaN)},
        {BSON("a" << Decimal128::kNegativeNaN), BSON("a" << Decimal128::kPositiveNaN)}));

    ASSERT_TRUE(compare("[]", {"{a:1, b:2, c:3}"}));
    ASSERT_TRUE(compare("[{$addFields: {c: {$literal: 3}}}]", {"{a:1, b:2}"}));
}

TEST_F(NodeSBE, DiffTest) {
    const auto contextFn = [this]() {
        return makeOperationContext();
    };
    const auto compare = [&contextFn](const std::string& pipelineStr,
                                      const std::vector<std::string>& jsonVector) {
        return compareSBEABTAgainstPipeline(
            contextFn, pipelineStr, fromjson(jsonVector), true /*preserveFieldOrder*/);
    };

    // Consider checking if compare() works first.
    const auto compareUnordered = [&contextFn](const std::string& pipelineStr,
                                               const std::vector<std::string>& jsonVector) {
        return compareSBEABTAgainstPipeline(
            contextFn, pipelineStr, fromjson(jsonVector), false /*preserveFieldOrder*/);
    };

    ASSERT_TRUE(compare("[]", {}));

    ASSERT_TRUE(compare("[{$project: {a: 1, b: 1}}]", {"{a: 10, b: 20, c: 30}"}));
    ASSERT_TRUE(compare("[{$match: {a: 2}}]", {"{a: [1, 2, 3, 4]}"}));
    ASSERT_TRUE(compare("[{$match: {a: 5}}]", {"{a: [1, 2, 3, 4]}"}));
    ASSERT_TRUE(compare("[{$match: {a: {$gte: 3}}}]", {"{a: [1, 2, 3, 4]}"}));
    ASSERT_TRUE(compare("[{$match: {a: {$gte: 30}}}]", {"{a: [1, 2, 3, 4]}"}));
    ASSERT_TRUE(
        compare("[{$match: {a: {$elemMatch: {$gte: 2, $lte: 3}}}}]", {"{a: [1, 2, 3, 4]}"}));
    ASSERT_TRUE(
        compare("[{$match: {a: {$elemMatch: {$gte: 20, $lte: 30}}}}]", {"{a: [1, 2, 3, 4]}"}));

    ASSERT_TRUE(compare("[{$project: {'a.b': '$c'}}]", {"{a: {d: 1}, c: 2}"}));
    ASSERT_TRUE(compare("[{$project: {'a.b': '$c'}}]", {"{a: [{d: 1}, {d: 2}, {b: 10}], c: 2}"}));

    ASSERT_TRUE(compareUnordered("[{$project: {'a.b': '$c', c: 1}}]", {"{a: {d: 1}, c: 2}"}));
    ASSERT_TRUE(compareUnordered("[{$project: {'a.b': '$c', 'a.d': 1, c: 1}}]",
                                 {"{a: [{d: 1}, {d: 2}, {b: 10}], c: 2}"}));

    ASSERT_TRUE(
        compare("[{$project: {a: {$filter: {input: '$b', as: 'num', cond: {$and: [{$gte: ['$$num', "
                "2]}, {$lte: ['$$num', 3]}]}}}}}]",
                {"{b: [1, 2, 3, 4]}"}));
    ASSERT_TRUE(
        compare("[{$project: {a: {$filter: {input: '$b', as: 'num', cond: {$and: [{$gte: ['$$num', "
                "3]}, {$lte: ['$$num', 2]}]}}}}}]",
                {"{b: [1, 2, 3, 4]}"}));

    ASSERT_TRUE(compare("[{$unwind: {path: '$a'}}]", {"{a: [1, 2, 3, 4]}"}));
    ASSERT_TRUE(compare("[{$unwind: {path: '$a.b'}}]", {"{a: {b: [1, 2, 3, 4]}}"}));

    ASSERT_TRUE(compare("[{$match:{'a.b.c':'aaa'}}]", {"{a: {b: {c: 'aaa'}}}"}));
    ASSERT_TRUE(
        compare("[{$match:{'a.b.c':'aaa'}}]", {"{a: {b: {c: 'aaa'}}}", "{a: {b: {c: 'aaa'}}}"}));

    ASSERT_TRUE(compare("[{$match: {a: {$lt: 5, $gt: 5}}}]", {"{_id: 1, a: [4, 6]}"}));
    ASSERT_TRUE(compare("[{$match: {a: {$gt: null}}}]", {"{_id: 1, a: 1}"}));

    ASSERT_TRUE(compare("[{$match: {a: {$elemMatch: {$lt: 6, $gt: 4}}}}]", {"{a: [5]}"}));
    ASSERT_TRUE(compare("[{$match: {'a.b': {$elemMatch: {$lt: 6, $gt: 4}}}}]",
                        {"{a: {b: [5]}}", "{a: [{b: 5}]}"}));

    ASSERT_TRUE(compare("[{$match: {a: {$elemMatch: {$elemMatch: {$lt: 6, $gt: 4}}}}}]",
                        {"{a: [[4, 5, 6], [5]]}", "{a: [4, 5, 6]}"}));

    // "{a: [2]}" will not match on classic.
    ASSERT_TRUE(compare("[{$match: {'a.b': {$eq: null}}}]",
                        {"{a: 2}",
                         "{}",
                         "{a: []}",
                         "{a: [{}]}",
                         "{a: {b: null}}",
                         "{a: {c: 1}}",
                         "{a: {b: 2}}",
                         "{a: [{b: null}, {b: 1}]}"}));

    ASSERT_TRUE(compare("[{$match: {'a': {$eq: null}}}]", {"{a: 2}"}));

    ASSERT_TRUE(compare("[{$match: {'a': {$ne: 2}}}]",
                        {"{a: 1}", "{a: 2}", "{a: [1, 2]}", "{a: [1]}", "{a: [2]}"}));


    ASSERT_TRUE(compare("[{$project: {concat: {$concat: ['$a', ' - ', '$b', ' - ', '$c']}}}]",
                        {"{a: 'a1', b: 'b1', c: 'c1'}"}));
    ASSERT_TRUE(compare(
        "[{$project: {res1: {$divide: ['$a', '$b']}, res2: {$divide: ['$c', '$a']}, res3: {$mod: "
        "['$d', '$b']}, res4: {$abs: '$e'}, res5: {$floor: '$f'}, res6: {$ceil: {$ln: '$d'}}}}]",
        {"{a: 5, b: 10, c: 20, d: 25, e: -5, f: 2.4}"}));
}

}  // namespace
}  // namespace mongo::optimizer
