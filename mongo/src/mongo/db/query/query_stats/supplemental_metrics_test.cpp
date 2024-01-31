/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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


#include "mongo/db/query/query_stats/supplemental_metrics_stats.h"
#include <memory>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/json.h"
#include "mongo/db/query/optimizer/utils/unit_test_utils.h"
#include "mongo/db/query/query_stats/optimizer_metrics_stats_entry.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/time_support.h"

namespace mongo::query_stats {
TEST(SupplementalMetricsStats, ClassicMetrics) {
    query_stats::SupplementalStatsMap metrics;
    auto entry = std::make_unique<query_stats::OptimizerMetricsClassicStatsEntry>(1);
    metrics.update(std::move(entry));
    BSONObj res1 = metrics.toBSON();
    ASSERT_BSONOBJ_EQ_AUTO(
        R"({
            "Classic": {
                "updateCount": 1,
                "optimizationTimeMicros": {
                    "sum": 1,
                    "max": 1,
                    "min": 1,
                    "sumOfSquares": 1
                }
            }
        })",
        res1);
    auto entry2 = std::make_unique<query_stats::OptimizerMetricsClassicStatsEntry>(10);
    metrics.update(std::move(entry2));
    BSONObj res2 = metrics.toBSON();
    ASSERT_BSONOBJ_EQ_AUTO(
        R"({
            "Classic": {
                "updateCount": 2,
                "optimizationTimeMicros": {
                    "sum": 11,
                    "max": 10,
                    "min": 1,
                    "sumOfSquares": 101
                }
            }
        })",
        res2);
}

TEST(SupplementalMetricsStats, SbeMetrics) {
    query_stats::SupplementalStatsMap metrics;
    auto entry = std::make_unique<query_stats::OptimizerMetricsClassicStatsEntry>(
        1, query_stats::SupplementalMetricType::SBE);
    metrics.update(std::move(entry));
    BSONObj res1 = metrics.toBSON();
    ASSERT_BSONOBJ_EQ_AUTO(
        R"({
            "SBE": {
                "updateCount": 1,
                "optimizationTimeMicros": {
                    "sum": 1,
                    "max": 1,
                    "min": 1,
                    "sumOfSquares": 1
                }
            }
        })",
        res1);
}

TEST(SupplementalMetricsStats, BonsaiM2Metrics) {
    query_stats::SupplementalStatsMap metrics;
    auto entry = std::make_unique<query_stats::OptimizerMetricsBonsaiStatsEntry>(
        1, 10.5, 100.5, query_stats::SupplementalMetricType::BonsaiM2);
    metrics.update(std::move(entry));
    BSONObj res = metrics.toBSON();
    ASSERT_BSONOBJ_EQ_AUTO(
        R"({
            "BonsaiM2": {
                "updateCount": 1,
                "optimizationTimeMicros": {
                    "sum": 1,
                    "max": 1,
                    "min": 1,
                    "sumOfSquares": 1
                },
                "estimatedCost": {
                    "sum": 10.5,
                    "max": 10.5,
                    "min": 10.5,
                    "sumOfSquares": 110.25
                },
                "estimatedCardinality": {
                    "sum": 100.5,
                    "max": 100.5,
                    "min": 100.5,
                    "sumOfSquares": 10100.25
                }
            }
        })",
        res);
}

TEST(SupplementalMetricsStats, BonsaiM4Metrics) {
    query_stats::SupplementalStatsMap metrics;
    auto entry = std::make_unique<query_stats::OptimizerMetricsBonsaiStatsEntry>(
        1, 10, 101, query_stats::SupplementalMetricType::BonsaiM4);
    metrics.update(std::move(entry));
    BSONObj res = metrics.toBSON();
    ASSERT_BSONOBJ_EQ_AUTO(
        R"({
            "BonsaiM4": {
                "updateCount": 1,
                "optimizationTimeMicros": {
                    "sum": 1,
                    "max": 1,
                    "min": 1,
                    "sumOfSquares": 1
                },
                "estimatedCost": {
                    "sum": 10,
                    "max": 10,
                    "min": 10,
                    "sumOfSquares": 100
                },
                "estimatedCardinality": {
                    "sum": 101,
                    "max": 101,
                    "min": 101,
                    "sumOfSquares": 10201
                }
            }
        })",
        res);
}
}  // namespace mongo::query_stats
