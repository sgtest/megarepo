/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
#include <memory>
#include <vector>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/s/shard_filtering_metadata_refresh.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/session/logical_session_id_gen.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/grid.h"
#include "mongo/s/resharding/common_types_gen.h"
#include "mongo/s/shard_version.h"
#include "mongo/s/stale_exception.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/functional.h"
#include "mongo/util/future.h"
#include "mongo/util/uuid.h"

namespace mongo {

class NamespaceString;
class OperationContext;
class Pipeline;

namespace resharding::data_copy {

/**
 * Creates the specified collection with the given options if the collection does not already exist.
 * If the collection already exists, we do not compare the options because the resharding process
 * will always use the same options for the same namespace.
 */
void ensureCollectionExists(OperationContext* opCtx,
                            const NamespaceString& nss,
                            const CollectionOptions& options);

/**
 * Drops the specified collection or returns without error if the collection has already been
 * dropped. A particular incarnation of the collection can be dropped by specifying its UUID.
 *
 * This functions assumes the collection being dropped doesn't have any two-phase index builds
 * active on it.
 */
void ensureCollectionDropped(OperationContext* opCtx,
                             const NamespaceString& nss,
                             const boost::optional<UUID>& uuid = boost::none);

/**
 * Removes documents from the oplog applier progress and transaction applier progress collections
 * that are associated with an in-progress resharding operation. Also drops all oplog buffer
 * collections and conflict stash collections that are associated with the in-progress resharding
 * operation.
 */
void ensureOplogCollectionsDropped(OperationContext* opCtx,
                                   const UUID& reshardingUUID,
                                   const UUID& sourceUUID,
                                   const std::vector<DonorShardFetchTimestamp>& donorShards);

/**
 * Renames the temporary resharding collection to the source namespace string, or is a no-op if the
 * collection has already been renamed to it.
 *
 * This function throws an exception if the collection doesn't exist as the temporary resharding
 * namespace string or the source namespace string.
 */
void ensureTemporaryReshardingCollectionRenamed(OperationContext* opCtx,
                                                const CommonReshardingMetadata& metadata);

/**
 * Removes all entries matching the given reshardingUUID from the recipient resume data table.
 */
void deleteRecipientResumeData(OperationContext* opCtx, const UUID& reshardingUUID);

/**
 * Returns the largest _id value in the collection.
 */
Value findHighestInsertedId(OperationContext* opCtx, const CollectionPtr& collection);

/**
 * Returns the full document of the largest _id value in the collection.
 */
boost::optional<Document> findDocWithHighestInsertedId(OperationContext* opCtx,
                                                       const CollectionPtr& collection);

/**
 * Returns a batch of documents suitable for being inserted with insertBatch().
 *
 * The batch of documents is returned once its size exceeds batchSizeLimitBytes or the pipeline has
 * been exhausted.
 */
std::vector<InsertStatement> fillBatchForInsert(Pipeline& pipeline, int batchSizeLimitBytes);

/**
 * Atomically inserts a batch of documents in a single multi-document transaction, along with also
 * storing the resume token in the same transaction. Returns the number of bytes inserted.
 */
int insertBatchTransactionally(OperationContext* opCtx,
                               const NamespaceString& nss,
                               const boost::optional<ShardingIndexesCatalogCache>& sii,
                               TxnNumber& txnNumber,
                               std::vector<InsertStatement>& batch,
                               const UUID& reshardingUUID,
                               ShardId donorShard,
                               HostAndPort donorHost,
                               const BSONObj& resumeToken);

/**
 * Atomically inserts a batch of documents in a single storage transaction. Returns the number of
 * bytes inserted.
 *
 * Throws NamespaceNotFound if the collection doesn't already exist.
 */
int insertBatch(OperationContext* opCtx,
                const NamespaceString& nss,
                std::vector<InsertStatement>& batch);

/**
 * Checks out the logical session in the opCtx and runs the supplied lambda function in a
 * transaction, using the transaction number supplied in the opCtx.
 */
void runWithTransactionFromOpCtx(OperationContext* opCtx,
                                 const NamespaceString& nss,
                                 const boost::optional<ShardingIndexesCatalogCache>& sii,
                                 unique_function<void(OperationContext*)> func);
/**
 * Checks out the logical session and acts in one of the following ways depending on the state of
 * this shard's config.transactions table:
 *
 *   (a) When this shard already knows about a higher transaction than txnNumber,
 *       withSessionCheckedOut() skips calling the supplied lambda function and returns boost::none.
 *
 *   (b) When this shard already knows about the retryable write statement (txnNumber, *stmtId),
 *       withSessionCheckedOut() skips calling the supplied lambda function and returns boost::none.
 *
 *   (c) When this shard has an earlier prepared transaction still active, withSessionCheckedOut()
 *       skips calling the supplied lambda function and returns a future that becomes ready once the
 *       active prepared transaction on this shard commits or aborts. After waiting for the returned
 *       future to become ready, the caller should then invoke withSessionCheckedOut() with the same
 *       arguments a second time.
 *
 *   (d) Otherwise, withSessionCheckedOut() calls the lambda function and returns boost::none.
 */
boost::optional<SharedSemiFuture<void>> withSessionCheckedOut(OperationContext* opCtx,
                                                              LogicalSessionId lsid,
                                                              TxnNumber txnNumber,
                                                              boost::optional<StmtId> stmtId,
                                                              unique_function<void()> callable);

/**
 * Updates this shard's config.transactions table based on a retryable write or multi-statement
 * transaction that already executed on some donor shard.
 *
 * This function assumes it is being called while the corresponding logical session is checked out
 * by the supplied OperationContext.
 */
void updateSessionRecord(OperationContext* opCtx,
                         BSONObj o2Field,
                         std::vector<StmtId> stmtIds,
                         boost::optional<repl::OpTime> preImageOpTime,
                         boost::optional<repl::OpTime> postImageOpTime);

/**
 * Calls and returns the value from the supplied lambda function.
 *
 * If a StaleConfig error is thrown during its execution, then this function will attempt to refresh
 * the collection and invoke the supplied lambda function a second time.
 */
template <typename Callable>
auto withOneStaleConfigRetry(OperationContext* opCtx, Callable&& callable) {
    try {
        return callable();
    } catch (const ExceptionForCat<ErrorCategory::StaleShardVersionError>& ex) {
        if (auto sce = ex.extraInfo<StaleConfigInfo>()) {
            // Cause a catalog cache refresh in case the index information is stale. Invalidate even
            // if the shard metadata was unknown so that we require only one stale config retry.
            Grid::get(opCtx)
                ->catalogCache()
                ->invalidateShardOrEntireCollectionEntryForShardedCollection(
                    sce->getNss(), sce->getVersionWanted(), sce->getShardId());

            // Recover the sharding metadata if there was no wanted version in the staleConfigInfo
            bool shardRefreshSucceeded;
            if (!sce->getVersionWanted()) {
                shardRefreshSucceeded =
                    onCollectionPlacementVersionMismatchNoExcept(
                        opCtx, sce->getNss(), sce->getVersionReceived().placementVersion())
                        .isOK();
            }

            // If a wanted version was returned, the metadata is already known, so we care about the
            // advancement of the catalog cache rather than the shard refresh. If the wanted version
            // is not set, then we only want to retry if we succeeded in recovering the collection
            // metadata.
            if (sce->getVersionWanted() || shardRefreshSucceeded) {
                return callable();
            }
        }
        throw;
    }
}

}  // namespace resharding::data_copy
}  // namespace mongo
