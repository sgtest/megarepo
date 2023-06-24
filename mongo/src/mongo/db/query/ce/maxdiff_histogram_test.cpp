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

#include <cstddef>
#include <iostream>
#include <memory>
#include <string>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/query/ce/histogram_predicate_estimation.h"
#include "mongo/db/query/ce/test_utils.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/stats/array_histogram.h"
#include "mongo/db/query/stats/max_diff.h"
#include "mongo/db/query/stats/maxdiff_test_utils.h"
#include "mongo/db/query/stats/rand_utils.h"
#include "mongo/db/query/stats/scalar_histogram.h"
#include "mongo/db/query/stats/value_utils.h"
#include "mongo/db/service_context_test_fixture.h"
#include "mongo/db/storage/key_string.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo::optimizer::ce {
namespace {
namespace value = sbe::value;

using stats::ArrayHistogram;
using stats::Bucket;
using stats::DataDistribution;
using stats::genFixedValueArray;
using stats::getDataDistribution;
using stats::makeHistogram;
using stats::makeInt64Value;
using stats::ScalarHistogram;

const double kTolerance = 0.001;

class HistogramTest : public ServiceContextTest {};

static double estimateCard(const ScalarHistogram& hist, const int v, const EstimationType type) {
    const auto [tag, val] = makeInt64Value(v);
    return estimate(hist, tag, val, type).card;
};

TEST_F(HistogramTest, BasicCreate) {
    std::vector<BucketData> data{{0, 1.0, 11.0, 1.0},
                                 {10, 2.0, 12.0, 2.0},
                                 {20, 3.0, 13.0, 3.0},
                                 {30, 4.0, 14.0, 4.0},
                                 {40, 5.0, 15.0, 5.0},
                                 {50, 6.0, 16.0, 6.0}};
    const ScalarHistogram hist = createHistogram(data);

    ASSERT_EQ(102.0, getTotals(hist).card);

    ASSERT_APPROX_EQUAL(1.0, estimateCard(hist, 0, EstimationType::kEqual), kTolerance);
    ASSERT_APPROX_EQUAL(6.0, estimateCard(hist, 5, EstimationType::kEqual), kTolerance);
    ASSERT_APPROX_EQUAL(0.0, estimateCard(hist, 55, EstimationType::kEqual), kTolerance);

    ASSERT_APPROX_EQUAL(28.1667, estimateCard(hist, 15, EstimationType::kLess), kTolerance);
    ASSERT_APPROX_EQUAL(32.5, estimateCard(hist, 15, EstimationType::kLessOrEqual), kTolerance);
    ASSERT_APPROX_EQUAL(39, estimateCard(hist, 20, EstimationType::kLess), kTolerance);
    ASSERT_APPROX_EQUAL(42.0, estimateCard(hist, 20, EstimationType::kLessOrEqual), kTolerance);

    ASSERT_APPROX_EQUAL(69.5, estimateCard(hist, 15, EstimationType::kGreater), kTolerance);
}

TEST_F(HistogramTest, CreateFixed) {
    std::vector<BucketData> data;
    for (int i = 0; i < 100; i++) {
        data.push_back(BucketData{i * 10, 1.0, 0.0, 0.0});
    }
    const ScalarHistogram hist = createHistogram(data);

    ASSERT_APPROX_EQUAL(50.0, estimateCard(hist, 50 * 10, EstimationType::kLess), kTolerance);
}

TEST_F(HistogramTest, MaxDiffTestInt) {
    constexpr size_t nElems = 100;
    constexpr size_t nBuckets = 10;

    auto data = genFixedValueArray(nElems, 1.0, 0.0);
    auto opCtx = makeOperationContext();
    const size_t actualCard = getActualCard(opCtx.get(), data, "[{$match: {a: {$lt: 10}}}]");

    const ScalarHistogram& hist = makeHistogram(data, nBuckets);
    std::cout << hist.toString();

    ASSERT_LTE(hist.getBuckets().size(), nBuckets);
    const double estimatedCard = estimateCard(hist, 11, EstimationType::kLess);
    ASSERT_EQ(36, actualCard);
    ASSERT_APPROX_EQUAL(43.7333, estimatedCard, kTolerance);
}

TEST_F(HistogramTest, MaxDiffTestString) {
    constexpr size_t nElems = 100;
    constexpr size_t nBuckets = 10;

    auto randData = genFixedValueArray(nElems, 0.0, 1.0);
    std::cout << "Generated " << nElems << " random values:\n"
              << printValueArray(randData) << "\n"
              << std::flush;

    auto opCtx = makeOperationContext();
    const size_t actualCard =
        getActualCard(opCtx.get(), randData, "[{$match: {a: {$lt: '91YgOvBB'}}}]");

    sortValueVector(randData);
    const DataDistribution& dataDistrib = getDataDistribution(randData);

    const ScalarHistogram& hist = genMaxDiffHistogram(dataDistrib, nBuckets);
    std::cout << hist.toString();
    ASSERT_LTE(hist.getBuckets().size(), nBuckets);

    const auto [tag, val] = value::makeNewString("91YgOvBB"_sd);
    value::ValueGuard vg(tag, val);
    const double estimatedCard = estimate(hist, tag, val, EstimationType::kLess).card;

    ASSERT_EQ(15, actualCard);
    ASSERT_APPROX_EQUAL(15.9443, estimatedCard, kTolerance);
}

TEST_F(HistogramTest, MaxDiffTestMixedTypes) {
    constexpr size_t nElems = 100;
    constexpr size_t nBuckets = 10;

    auto randData = genFixedValueArray(nElems, 0.5, 0.5);
    std::cout << "Generated " << nElems << " random values:\n"
              << printValueArray(randData) << "\n"
              << std::flush;

    auto opCtx = makeOperationContext();
    const size_t actualCard = getActualCard(opCtx.get(), randData, "[{$match: {a: {$lt: 10}}}]");

    sortValueVector(randData);
    const DataDistribution& dataDistrib = getDataDistribution(randData);

    const ScalarHistogram& hist = genMaxDiffHistogram(dataDistrib, nBuckets);
    std::cout << hist.toString();
    ASSERT_LTE(hist.getBuckets().size(), nBuckets);
    const double estimatedCard = estimateCard(hist, 10, EstimationType::kLess);

    ASSERT_EQ(18, actualCard);
    ASSERT_APPROX_EQUAL(18.0, estimatedCard, kTolerance);
}

TEST_F(HistogramTest, MaxDiffIntArrays) {
    constexpr size_t nElems = 100;
    constexpr size_t nBuckets = 10;

    auto rawData = genFixedValueArray(nElems, 1.0, 0.0);
    auto arrayData = nestArrays(rawData, 0 /* No empty arrays */);

    auto estimator = createArrayEstimator(arrayData, nBuckets);

    auto opCtx = makeOperationContext();
    {
        const size_t actualCard =
            getActualCard(opCtx.get(), arrayData, "[{$match: {a: {$eq: 2}}}]");

        const auto [tag, val] = makeInt64Value(2);
        value::ValueGuard vg(tag, val);
        const CEType estimatedCard = estimateCardEq(*estimator, tag, val, true /*includeScalar*/);

        ASSERT_CE_APPROX_EQUAL(4.0, estimatedCard, kTolerance);
        ASSERT_EQ(4, actualCard);
    }

    {
        const size_t actualCard =
            getActualCard(opCtx.get(), arrayData, "[{$match: {a: {$lt: 3}}}]");

        const auto [tag, val] = makeInt64Value(3);
        value::ValueGuard vg(tag, val);
        const CEType estimatedCard = estimateCardRange(*estimator,
                                                       false /*lowInclusive*/,
                                                       value::TypeTags::MinKey,
                                                       0,
                                                       false /*highInclusive*/,
                                                       tag,
                                                       val,
                                                       true /* includeScalar */);
        ASSERT_EQ(6, actualCard);
        ASSERT_CE_APPROX_EQUAL(6.0, estimatedCard, kTolerance);
    }

    {
        const size_t actualCard = getActualCard(
            opCtx.get(), arrayData, "[{$match: {a: {$elemMatch: {$gt: 2, $lt: 5}}}}]");

        const auto [lowTag, lowVal] = makeInt64Value(2);
        value::ValueGuard vgLow(lowTag, lowVal);
        const auto [highTag, highVal] = makeInt64Value(5);
        value::ValueGuard vgHigh(highTag, highVal);

        const CEType estimatedCard = estimateCardRange(*estimator,
                                                       false /*lowInclusive*/,
                                                       lowTag,
                                                       lowVal,
                                                       false /*highInclusive*/,
                                                       highTag,
                                                       highVal,
                                                       false /* includeScalar */);

        ASSERT_EQ(2, actualCard);
        ASSERT_CE_APPROX_EQUAL(3.15479, estimatedCard, kTolerance);
    }
}

TEST_F(HistogramTest, MaxDiffEmptyArrays) {
    constexpr size_t nElems = 21;
    constexpr size_t nBuckets = 5;
    constexpr size_t emptyArrayCount = 3;

    auto rawData = genFixedValueArray(nElems, 1.0, 0.0);
    auto arrayData = nestArrays(rawData, emptyArrayCount);
    std::cout << "Generated " << nElems << " arrayData:\n"
              << printValueArray(arrayData) << "\n"
              << std::flush;

    const auto arrayHist = createArrayEstimator(arrayData, nBuckets);
    ASSERT_EQ(arrayHist->getEmptyArrayCount(), emptyArrayCount);
}

}  // namespace
}  // namespace mongo::optimizer::ce
