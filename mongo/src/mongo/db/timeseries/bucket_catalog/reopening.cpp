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

#include "mongo/db/timeseries/bucket_catalog/reopening.h"

#include <cstddef>

#include <absl/container/node_hash_map.h>

#include "mongo/db/timeseries/bucket_catalog/bucket_catalog.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_catalog_internal.h"
#include "mongo/db/timeseries/bucket_catalog/execution_stats.h"
#include "mongo/util/time_support.h"

namespace mongo::timeseries::bucket_catalog {

namespace {
boost::optional<OID> initializeRequest(BucketCatalog& catalog,
                                       Stripe& stripe,
                                       const BucketKey& key,
                                       const ReopeningContext::CandidateType& candidate) {
    boost::optional<OID> oid;
    if (holds_alternative<std::monostate>(candidate)) {
        // No need to initialize a request.
        return oid;
    } else if (auto* c = get_if<OID>(&candidate)) {
        oid = *c;
    }
    invariant(oid.has_value() || !stripe.outstandingReopeningRequests.contains(key));

    auto it = stripe.outstandingReopeningRequests.find(key);
    if (it == stripe.outstandingReopeningRequests.end()) {
        bool inserted = false;
        std::tie(it, inserted) = stripe.outstandingReopeningRequests.emplace(
            key, decltype(stripe.outstandingReopeningRequests)::mapped_type{});
        invariant(inserted);
    }
    auto& list = it->second;

    list.push_back(std::make_shared<ReopeningRequest>(
        ExecutionStatsController{internal::getOrInitializeExecutionStats(catalog, key.ns)}, oid));

    return oid;
}
}  // namespace

ReopeningContext::~ReopeningContext() {
    if (!_cleared) {
        clear();
    }
}

ReopeningContext::ReopeningContext(BucketCatalog& catalog,
                                   Stripe& s,
                                   WithLock,
                                   const BucketKey& k,
                                   uint64_t era,
                                   CandidateType&& c)
    : catalogEra{era},
      candidate{std::move(c)},
      _stripe(&s),
      _key(k),
      _oid{initializeRequest(catalog, s, k, candidate)},
      _cleared(holds_alternative<std::monostate>(candidate)) {}

ReopeningContext::ReopeningContext(ReopeningContext&& other)
    : catalogEra{other.catalogEra},
      candidate{std::move(other.candidate)},
      fetchedBucket{other.fetchedBucket},
      queriedBucket{other.queriedBucket},
      bucketToReopen{std::move(other.bucketToReopen)},
      _stripe(other._stripe),
      _key(std::move(other._key)),
      _oid(std::move(other._oid)),
      _cleared(other._cleared) {
    other._cleared = true;
}

ReopeningContext& ReopeningContext::operator=(ReopeningContext&& other) {
    if (this != &other) {
        catalogEra = other.catalogEra;
        candidate = std::move(other.candidate);
        fetchedBucket = other.fetchedBucket;
        queriedBucket = other.queriedBucket;
        bucketToReopen = std::move(other.bucketToReopen);
        _stripe = other._stripe;
        _key = std::move(other._key);
        _oid = std::move(other._oid);
        _cleared = other._cleared;
        other._cleared = true;
    }
    return *this;
}

void ReopeningContext::clear() {
    stdx::lock_guard stripeLock{_stripe->mutex};
    clear(stripeLock);
}

void ReopeningContext::clear(WithLock) {
    if (_cleared) {
        return;
    }

    auto keyIt = _stripe->outstandingReopeningRequests.find(_key);
    invariant(keyIt != _stripe->outstandingReopeningRequests.end());
    auto& list = keyIt->second;

    invariant(_oid.has_value() || list.size() == 1);
    auto requestIt = std::find_if(
        list.begin(), list.end(), [&](const std::shared_ptr<ReopeningRequest>& request) {
            return request->oid == _oid;
        });
    invariant(requestIt != list.end());

    // Notify any waiters and clean up state.
    (*requestIt)->promise.emplaceValue();
    list.erase(requestIt);
    if (list.empty()) {
        _stripe->outstandingReopeningRequests.erase(keyIt);
    }
    _cleared = true;
}

ArchivedBucket::ArchivedBucket(const BucketId& b, const std::string& t)
    : bucketId{b}, timeField{t} {}

long long marginalMemoryUsageForArchivedBucket(
    const ArchivedBucket& bucket, IncludeMemoryOverheadFromMap includeMemoryOverheadFromMap) {
    return sizeof(Date_t) +        // key in set of archived buckets for meta hash
        sizeof(ArchivedBucket) +   // main data for archived bucket
        bucket.timeField.size() +  // allocated space for timeField string, ignoring SSO
        (includeMemoryOverheadFromMap == IncludeMemoryOverheadFromMap::kInclude
             ? sizeof(std::size_t) +                                    // key in set (meta hash)
                 sizeof(decltype(Stripe::archivedBuckets)::value_type)  // set container
             : 0);
}

ReopeningRequest::ReopeningRequest(ExecutionStatsController&& s, boost::optional<OID> o)
    : stats{std::move(s)}, oid{o} {}

void waitForReopeningRequest(ReopeningRequest& request) {
    if (!request.promise.getFuture().isReady()) {
        request.stats.incNumWaits();
    }
    request.promise.getFuture().getNoThrow().ignore();
}

}  // namespace mongo::timeseries::bucket_catalog
