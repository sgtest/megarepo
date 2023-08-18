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

#pragma once

#include <algorithm>
#include <cstdint>
#include <memory>

#include "mongo/db/commands/server_status_metric.h"
#include "mongo/db/query/query_stats/aggregated_metric.h"
#include "mongo/db/query/query_stats/key_generator.h"
#include "mongo/db/query/query_stats/transform_algorithm_gen.h"
#include "mongo/util/time_support.h"

namespace mongo::query_stats {

extern CounterMetric queryStatsStoreSizeEstimateBytesMetric;

const auto kKeySize = sizeof(std::size_t);

/**
 * The value stored in the query stats store. It contains a KeyGenerator representing this "kind" of
 * query, and some metrics about that shape. This class is responsible for knowing its size and
 * updating our server status metrics about the size of the query stats store accordingly. At the
 * time of this writing, the LRUCache utility does not easily expose its size in a way we could use
 * as server status metrics.
 */
class QueryStatsEntry {
public:
    QueryStatsEntry(std::unique_ptr<KeyGenerator> keyGenerator)
        : firstSeenTimestamp(Date_t::now()), keyGenerator(std::move(keyGenerator)) {
        // Increment by size of query stats store key (hash returns size_t) and value
        // (QueryStatsEntry)
        queryStatsStoreSizeEstimateBytesMetric.increment(kKeySize + size());
    }

    ~QueryStatsEntry() {
        // Decrement by size of query stats store key (hash returns size_t) and value
        // (QueryStatsEntry)
        queryStatsStoreSizeEstimateBytesMetric.decrement(kKeySize + size());
    }

    BSONObj toBSON() const;

    int64_t size() {
        return sizeof(*this) + (keyGenerator ? keyGenerator->size() : 0);
    }

    /**
     * Generate the queryStats key for this entry's request. If algorithm is not
     * TransformAlgorithm::kNone, any identifying information (field names, namespace) will be
     * anonymized.
     */
    BSONObj computeQueryStatsKey(OperationContext* opCtx,
                                 TransformAlgorithmEnum algorithm,
                                 std::string hmacKey) const;

    BSONObj getRepresentativeQueryShapeForDebug() const {
        return keyGenerator->getRepresentativeQueryShapeForDebug();
    }

    /**
     * Timestamp for when this query shape was added to the store. Set on construction.
     */
    const Date_t firstSeenTimestamp;

    /**
     * Timestamp for when the latest time this query shape was seen.
     */
    Date_t latestSeenTimestamp;

    /**
     * Last execution time in microseconds.
     */
    uint64_t lastExecutionMicros = 0;

    /**
     * Number of query executions.
     */
    uint64_t execCount = 0;

    /**
     * Aggregates the total time for execution including getMore requests.
     */
    AggregatedMetric totalExecMicros;

    /**
     * Aggregates the time for execution for first batch only.
     */
    AggregatedMetric firstResponseExecMicros;

    AggregatedMetric docsReturned;

    /**
     * The KeyGenerator that can generate the query stats key for this request.
     */
    const std::shared_ptr<const KeyGenerator> keyGenerator;
};

}  // namespace mongo::query_stats
