/**
 *    Copyright (C) 2018-present MongoDB, Inc.
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

#include <map>
#include <set>
#include <vector>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/status_with.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobj_comparator_interface.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/simple_bsonobj_comparator.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/query/canonical_query.h"
#include "mongo/db/shard_id.h"
#include "mongo/db/timeseries/timeseries_gen.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/ns_targeter.h"
#include "mongo/s/shard_key_pattern.h"
#include "mongo/s/stale_exception.h"
#include "mongo/s/write_ops/batched_command_request.h"

namespace mongo {

struct TargeterStats {
    // Map of chunk shard minKey -> approximate delta. This is used for deciding whether a chunk
    // might need splitting or not.
    BSONObjIndexedMap<int> chunkSizeDelta{
        SimpleBSONObjComparator::kInstance.makeBSONObjIndexedMap<int>()};
};

using StaleShardPlacementVersionMap = std::map<ShardId, ChunkVersion>;

/**
 * NSTargeter based on a CollectionRoutingInfo implementation. Wraps all exception codepaths and
 * returns NamespaceNotFound status on applicable failures.
 *
 * Must be initialized before use, and initialization may fail.
 */
class CollectionRoutingInfoTargeter : public NSTargeter {
public:
    enum class LastErrorType { kCouldNotTarget, kStaleShardVersion, kStaleDbVersion };
    /**
     * Initializes the targeter with the latest routing information for the namespace, which means
     * it may have to block and load information from the config server.
     *
     * If 'nss' is a sharded time-series collection, replaces this value with namespace string of a
     * time-series buckets collection.
     *
     * If 'expectedEpoch' is specified, the targeter will throws 'StaleEpoch' exception if the epoch
     * for 'nss' ever becomes different from 'expectedEpoch'. Otherwise, the targeter will continue
     * targeting even if the collection gets dropped and recreated.
     */
    CollectionRoutingInfoTargeter(OperationContext* opCtx,
                                  const NamespaceString& nss,
                                  boost::optional<OID> expectedEpoch = boost::none);

    /* Initializes the targeter with a custom CollectionRoutingInfo cri, in order to support
     * using a custom (synthetic) routing table */
    CollectionRoutingInfoTargeter(const CollectionRoutingInfo& cri);

    const NamespaceString& getNS() const override;

    ShardEndpoint targetInsert(OperationContext* opCtx,
                               const BSONObj& doc,
                               std::set<ChunkRange>* chunkRange = nullptr) const override;

    std::vector<ShardEndpoint> targetUpdate(
        OperationContext* opCtx,
        const BatchItemRef& itemRef,
        std::set<ChunkRange>* chunkRange = nullptr) const override;

    std::vector<ShardEndpoint> targetDelete(
        OperationContext* opCtx,
        const BatchItemRef& itemRef,
        std::set<ChunkRange>* chunkRange = nullptr) const override;

    std::vector<ShardEndpoint> targetAllShards(
        OperationContext* opCtx, std::set<ChunkRange>* chunkRanges = nullptr) const override;

    void noteCouldNotTarget() override;

    void noteStaleShardResponse(OperationContext* opCtx,
                                const ShardEndpoint& endpoint,
                                const StaleConfigInfo& staleInfo) override;

    void noteStaleDbResponse(OperationContext* opCtx,
                             const ShardEndpoint& endpoint,
                             const StaleDbRoutingVersion& staleInfo) override;

    /**
     * Replaces the targeting information with the latest information from the cache.  If this
     * information is stale WRT the noted stale responses or a remote refresh is needed due
     * to a targeting failure, will contact the config servers to reload the metadata.
     *
     * Return true if the metadata was different after this reload.
     *
     * Also see NSTargeter::refreshIfNeeded().
     */
    bool refreshIfNeeded(OperationContext* opCtx) override;

    int getNShardsOwningChunks() const override;

    bool isShardedTimeSeriesBucketsNamespace() const override;

    bool timeseriesNamespaceNeedsRewrite(const NamespaceString& nss) const;

    const CollectionRoutingInfo& getRoutingInfo() const;

    static BSONObj extractBucketsShardKeyFromTimeseriesDoc(
        const BSONObj& doc,
        const ShardKeyPattern& pattern,
        const TimeseriesOptions& timeseriesOptions);

    /**
     * This returns "does the query have an _id field" and "is the _id field querying for a direct
     * value like _id : 3 and not _id : { $gt : 3 }"
     *
     * If the query does not use the collection default collation, the _id field cannot contain
     * strings, objects, or arrays.
     *
     * Ex: { _id : 1 } => true
     *     { foo : <anything>, _id : 1 } => true
     *     { _id : { $lt : 30 } } => false
     *     { foo : <anything> } => false
     */
    static bool isExactIdQuery(OperationContext* opCtx,
                               const CanonicalQuery& query,
                               const ChunkManager& cm);
    static bool isExactIdQuery(OperationContext* opCtx,
                               const NamespaceString& nss,
                               const BSONObj& query,
                               const BSONObj& collation,
                               const ChunkManager& cm);

private:
    CollectionRoutingInfo _init(OperationContext* opCtx, bool refresh);

    /**
     * Returns a vector of ShardEndpoints for a potentially multi-shard query.
     *
     * Returns !OK with message if query could not be targeted.
     *
     * If 'collation' is empty, we use the collection default collation for targeting.
     */
    StatusWith<std::vector<ShardEndpoint>> _targetQuery(
        boost::intrusive_ptr<ExpressionContext> expCtx,
        const BSONObj& query,
        const BSONObj& collation,
        std::set<ChunkRange>* chunkRanges) const;

    /**
     * Returns a ShardEndpoint for an exact shard key query.
     *
     * Also has the side effect of updating the chunks stats with an estimate of the amount of
     * data targeted at this shard key.
     *
     * If 'collation' is empty, we use the collection default collation for targeting.
     */
    StatusWith<ShardEndpoint> _targetShardKey(const BSONObj& shardKey,
                                              const BSONObj& collation,
                                              std::set<ChunkRange>* chunkRanges) const;

    // Full namespace of the collection for this targeter
    NamespaceString _nss;

    // Used to identify the original namespace that the user has requested. Note: this will only be
    // true if the buckets namespace is sharded.
    bool _isRequestOnTimeseriesViewNamespace = false;

    // Stores last error occurred
    boost::optional<LastErrorType> _lastError;

    // Set to the epoch of the namespace we are targeting. If we ever refresh the catalog cache and
    // find a new epoch, we immediately throw a StaleEpoch exception.
    boost::optional<OID> _targetEpoch;

    // The latest loaded routing cache entry
    CollectionRoutingInfo _cri;
};

}  // namespace mongo
