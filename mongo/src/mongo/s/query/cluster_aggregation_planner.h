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

#include <absl/container/node_hash_map.h>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <functional>
#include <memory>
#include <utility>

#include "mongo/base/status.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/auth/privilege.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/query/explain_options.h"
#include "mongo/db/shard_id.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/database_version.h"
#include "mongo/s/query/cluster_aggregate.h"
#include "mongo/s/query/cluster_client_cursor_guard.h"
#include "mongo/s/query/cluster_client_cursor_impl.h"
#include "mongo/s/query/cluster_client_cursor_params.h"
#include "mongo/stdx/unordered_set.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace cluster_aggregation_planner {

/**
 * Builds a ClusterClientCursor which will execute 'pipeline'. If 'pipeline' consists entirely of
 * $skip and $limit stages, the pipeline is eliminated entirely and replaced with a RouterExecStage
 * tree that does same thing but will avoid using a RouterStagePipeline. Avoiding a
 * RouterStagePipeline will remove an expensive conversion from BSONObj -> Document for each result.
 */
ClusterClientCursorGuard buildClusterCursor(OperationContext* opCtx,
                                            std::unique_ptr<Pipeline, PipelineDeleter> pipeline,
                                            ClusterClientCursorParams&&);

/**
 *  Returns the "collation" and "uuid" for the collection given by "nss" with the following
 *  semantics:
 *  - The "collation" parameter will be set to the default collation for the collection or the
 *    simple collation if there is no default. If the collection does not exist or if the aggregate
 *    is on the collectionless namespace, this will be set to an empty object.
 *  - The "uuid" is retrieved from the chunk manager for sharded collections or the listCollections
 *    output for unsharded collections. The UUID will remain unset if the aggregate is on the
 *    collectionless namespace.
 */
std::pair<BSONObj, boost::optional<UUID>> getCollationAndUUID(
    OperationContext* opCtx,
    const boost::optional<ChunkManager>& cm,
    const NamespaceString& nss,
    const BSONObj& collation);

/**
 * This structure contains information for targeting an aggregation pipeline in a sharded cluster.
 */
struct AggregationTargeter {
    /**
     * Populates and returns targeting info for an aggregation pipeline on the given namespace
     * 'executionNss'.
     */
    static AggregationTargeter make(
        OperationContext* opCtx,
        const NamespaceString& executionNss,
        std::function<std::unique_ptr<Pipeline, PipelineDeleter>()> buildPipelineFn,
        boost::optional<CollectionRoutingInfo> cri,
        stdx::unordered_set<NamespaceString> involvedNamespaces,
        bool hasChangeStream,
        bool startsWithDocuments,
        bool allowedToPassthrough,
        bool perShardCursor);

    enum TargetingPolicy {
        kPassthrough,
        kMongosRequired,
        kAnyShard,
        kSpecificShardOnly,
    } policy;

    std::unique_ptr<Pipeline, PipelineDeleter> pipeline;
    boost::optional<CollectionRoutingInfo> cri;
};

/**
 * Runs a pipeline on the primary shard. See 'runPipelineOnSpecificShardOnly' for more details.
 */
Status runPipelineOnPrimaryShard(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                 const ClusterAggregate::Namespaces& namespaces,
                                 const ChunkManager& cm,
                                 boost::optional<ExplainOptions::Verbosity> explain,
                                 Document serializedCommand,
                                 const PrivilegeVector& privileges,
                                 bool eligibleForSampling,
                                 BSONObjBuilder* out);

/**
 * Runs a pipeline on mongoS, having first validated that it is eligible to do so. This can be a
 * pipeline which is split for merging, or an intact pipeline which must run entirely on mongoS.
 */
Status runPipelineOnMongoS(const ClusterAggregate::Namespaces& namespaces,
                           long long batchSize,
                           std::unique_ptr<Pipeline, PipelineDeleter> pipeline,
                           BSONObjBuilder* result,
                           const PrivilegeVector& privileges);

/**
 * Dispatches the pipeline in 'targeter' to the shards that are involved, and merges the results if
 * necessary on either mongos or a randomly designated shard. If 'eligibleForSampling' is true,
 * attaches a unique sample id to the request for one of the targeted shards if the collection has
 * query sampling enabled and the rate-limited sampler successfully generates a sample id for it.
 */
Status dispatchPipelineAndMerge(OperationContext* opCtx,
                                AggregationTargeter targeter,
                                Document serializedCommand,
                                long long batchSize,
                                const ClusterAggregate::Namespaces& namespaces,
                                const PrivilegeVector& privileges,
                                BSONObjBuilder* result,
                                bool hasChangeStream,
                                bool startsWithDocuments,
                                bool eligibleForSampling);

/**
 * Runs a pipeline on a specific shard. Used for running a pipeline on the primary shard (i.e. by
 * 'runPipelineOnPrimaryShard') and on a specifc shard  (i.e. by per shard $changeStream cursors).
 * If 'forPerShardCursor' is true shard versions will not be added to the request sent to mongod.
 * If 'eligibleForSampling' is true, attaches a unique sample id to the request for that shard if
 * the collection has query sampling enabled and the rate-limited sampler successfully generates a
 * sample id for it.
 */
Status runPipelineOnSpecificShardOnly(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                      const ClusterAggregate::Namespaces& namespaces,
                                      boost::optional<DatabaseVersion> dbVersion,
                                      boost::optional<ExplainOptions::Verbosity> explain,
                                      Document serializedCommand,
                                      const PrivilegeVector& privileges,
                                      ShardId shardId,
                                      bool forPerShardCursor,
                                      bool eligibleForSampling,
                                      BSONObjBuilder* out);

}  // namespace cluster_aggregation_planner
}  // namespace mongo
