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

#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
#include <unordered_set>
#include <vector>

#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/db/auth/validated_tenancy_scope.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/s/balancer/balancer_policy.h"
#include "mongo/db/s/balancer/cluster_statistics.h"
#include "mongo/db/shard_id.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/stdx/unordered_set.h"

namespace mongo {

/**
 * Class used by the balancer for selecting chunks, which need to be moved around in order for
 * the sharded cluster to be balanced.
 */
class BalancerChunkSelectionPolicy {
    BalancerChunkSelectionPolicy(const BalancerChunkSelectionPolicy&) = delete;
    BalancerChunkSelectionPolicy& operator=(const BalancerChunkSelectionPolicy&) = delete;

public:
    BalancerChunkSelectionPolicy(ClusterStatistics* clusterStats);
    ~BalancerChunkSelectionPolicy();

    /**
     * Potentially blocking method, which gives out a set of chunks, which need to be split because
     * they violate the policy for some reason. The reason is decided by the policy and may include
     * chunk is too big or chunk straddles a zone range.
     */
    StatusWith<SplitInfoVector> selectChunksToSplit(OperationContext* opCtx);

    /**
     * Given a valid namespace returns all the splits the balancer would need to perform with the
     * current state
     */
    StatusWith<SplitInfoVector> selectChunksToSplit(OperationContext* opCtx,
                                                    const NamespaceString& ns);

    /**
     * Potentially blocking method, which gives out a set of chunks to be moved.
     */
    StatusWith<MigrateInfoVector> selectChunksToMove(
        OperationContext* opCtx,
        const std::vector<ClusterStatistics::ShardStatistics>& shardStats,
        stdx::unordered_set<ShardId>* availableShards,
        stdx::unordered_set<NamespaceString>* imbalancedCollectionsCachePtr);

    /**
     * Given a valid namespace returns all the Migrations the balancer would need to perform with
     * the current state.
     */
    StatusWith<MigrateInfosWithReason> selectChunksToMove(OperationContext* opCtx,
                                                          const NamespaceString& ns);

    /**
     * Requests a single chunk to be relocated to a different shard, if possible. If some error
     * occurs while trying to determine the best location for the chunk, a failed status is
     * returned. If the chunk is already at the best shard that it can be, returns `boost::none`.
     * Otherwise returns migration information for where the chunk should be moved.
     */
    StatusWith<boost::optional<MigrateInfo>> selectSpecificChunkToMove(OperationContext* opCtx,
                                                                       const NamespaceString& nss,
                                                                       const ChunkType& chunk);

private:
    /**
     * Synchronous method, which iterates the collection's chunks and uses the zones information to
     * figure out whether some of them validate the zone range boundaries and need to be split.
     */
    StatusWith<SplitInfoVector> _getSplitCandidatesForCollection(
        OperationContext* opCtx,
        const NamespaceString& nss,
        const ShardStatisticsVector& shardStats);

    /**
     * Synchronous method, which iterates the collection's size per shard  to figure out where to
     * place them.
     */
    StatusWith<MigrateInfosWithReason> _getMigrateCandidatesForCollection(
        OperationContext* opCtx,
        const NamespaceString& nss,
        const ShardStatisticsVector& shardStats,
        const CollectionDataSizeInfoForBalancing& collDataSizeInfo,
        stdx::unordered_set<ShardId>* availableShards);

    // Source for obtaining cluster statistics. Not owned and must not be destroyed before the
    // policy object is destroyed.
    ClusterStatistics* const _clusterStats;
};

}  // namespace mongo
