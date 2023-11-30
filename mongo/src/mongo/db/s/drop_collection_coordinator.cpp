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

#include "mongo/db/s/drop_collection_coordinator.h"

#include <absl/container/node_hash_map.h>
#include <algorithm>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/smart_ptr.hpp>
#include <fmt/format.h>
#include <tuple>
#include <utility>
#include <vector>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/db/cancelable_operation_context.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/drop_collection.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/drop_gen.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/s/collection_sharding_runtime.h"
#include "mongo/db/s/forwardable_operation_metadata.h"
#include "mongo/db/s/participant_block_gen.h"
#include "mongo/db/s/range_deletion_util.h"
#include "mongo/db/s/sharding_ddl_coordinator.h"
#include "mongo/db/s/sharding_ddl_util.h"
#include "mongo/db/s/sharding_index_catalog_ddl_util.h"
#include "mongo/db/s/sharding_logging.h"
#include "mongo/db/service_context.h"
#include "mongo/db/session/logical_session_id_gen.h"
#include "mongo/db/shard_id.h"
#include "mongo/db/vector_clock_mutable.h"
#include "mongo/executor/async_rpc.h"
#include "mongo/executor/async_rpc_util.h"
#include "mongo/executor/task_executor_pool.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/s/analyze_shard_key_documents_gen.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/catalog/type_collection.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/catalog_cache_loader.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/grid.h"
#include "mongo/s/sharding_state.h"
#include "mongo/util/decorable.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/out_of_line_executor.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {

void DropCollectionCoordinator::dropCollectionLocally(OperationContext* opCtx,
                                                      const NamespaceString& nss,
                                                      bool fromMigrate,
                                                      bool dropSystemCollections) {

    boost::optional<UUID> collectionUUID;
    {
        Lock::DBLock dbLock(opCtx, nss.dbName(), MODE_IX);
        Lock::CollectionLock collLock(opCtx, nss, MODE_IX);

        // Get collectionUUID
        collectionUUID = [&]() -> boost::optional<UUID> {
            auto localCatalog = CollectionCatalog::get(opCtx);
            const auto coll = localCatalog->lookupCollectionByNamespace(opCtx, nss);
            if (coll) {
                return coll->uuid();
            }
            return boost::none;
        }();

        // Clear CollectionShardingRuntime entry.
        CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(opCtx, nss)
            ->clearFilteringMetadataForDroppedCollection(opCtx);
    }

    dropCollectionShardingIndexCatalog(opCtx, nss);

    // Remove all range deletion task documents present on disk for the collection to drop. This is
    // a best-effort tentative considering that migrations are not blocked, hence some new document
    // may be inserted before actually dropping the collection.
    if (collectionUUID) {
        // The multi-document remove command cannot be run in  transactions, so run it using
        // an alternative client.
        auto newClient =
            opCtx->getService()->makeClient("removeRangeDeletions-" + collectionUUID->toString());
        AlternativeClientRegion acr{newClient};
        auto executor =
            Grid::get(opCtx->getServiceContext())->getExecutorPool()->getFixedExecutor();

        CancelableOperationContext alternativeOpCtx(
            cc().makeOperationContext(), opCtx->getCancellationToken(), executor);

        try {
            rangedeletionutil::removePersistentRangeDeletionTasksByUUID(alternativeOpCtx.get(),
                                                                        *collectionUUID);
        } catch (const DBException& e) {
            LOGV2_ERROR(6501601,
                        "Failed to remove persistent range deletion tasks on drop collection",
                        logAttrs(nss),
                        "collectionUUID"_attr = (*collectionUUID).toString(),
                        "error"_attr = e);
            throw;
        }
    }

    try {
        DropReply unused;
        uassertStatusOK(dropCollection(
            opCtx,
            nss,
            &unused,
            (dropSystemCollections
                 ? DropCollectionSystemCollectionMode::kAllowSystemCollectionDrops
                 : DropCollectionSystemCollectionMode::kDisallowSystemCollectionDrops),
            fromMigrate));
    } catch (const ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
        // Note that even if the namespace was not found we have to execute the code below!
        LOGV2_DEBUG(5280920,
                    1,
                    "Namespace not found while trying to delete local collection",
                    logAttrs(nss));
    }

    // Force the refresh of the catalog cache to purge outdated information. Note also that this
    // code is indirectly used to notify to secondary nodes to clear their filtering information.
    const auto catalog = Grid::get(opCtx)->catalogCache();
    uassertStatusOK(catalog->getCollectionRoutingInfoWithRefresh(opCtx, nss));
    CatalogCacheLoader::get(opCtx).waitForCollectionFlush(opCtx, nss);

    // Ensures the remove of range deletions and the refresh of the catalog cache will be waited for
    // majority at the end of the command
    repl::ReplClientInfo::forClient(opCtx->getClient()).setLastOpToSystemLastOpTime(opCtx);
}

ExecutorFuture<void> DropCollectionCoordinator::_runImpl(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    const CancellationToken& token) noexcept {
    return ExecutorFuture<void>(**executor)
        .then([this, executor = executor, anchor = shared_from_this()] {
            if (_doc.getPhase() < Phase::kFreezeCollection)
                _checkPreconditionsAndSaveArgumentsOnDoc();
        })
        .then(_buildPhaseHandler(Phase::kFreezeCollection,
                                 [this, executor = executor, anchor = shared_from_this()] {
                                     _freezeMigrations(executor);
                                 }))

        .then(_buildPhaseHandler(Phase::kEnterCriticalSection,
                                 [this, token, executor = executor, anchor = shared_from_this()] {
                                     _enterCriticalSection(executor, token);
                                 }))
        .then(_buildPhaseHandler(Phase::kDropCollection,
                                 [this, executor = executor, anchor = shared_from_this()] {
                                     _commitDropCollection(executor);
                                 }))
        .then(_buildPhaseHandler(Phase::kReleaseCriticalSection,
                                 [this, token, executor = executor, anchor = shared_from_this()] {
                                     _exitCriticalSection(executor, token);
                                 }));
}

void DropCollectionCoordinator::_checkPreconditionsAndSaveArgumentsOnDoc() {
    auto opCtxHolder = cc().makeOperationContext();
    auto* opCtx = opCtxHolder.get();
    getForwardableOpMetadata().setOn(opCtx);

    // If the request had an expected UUID for the collection being dropped, we should verify that
    // it matches the one from the local catalog
    {
        AutoGetCollection coll{opCtx,
                               nss(),
                               MODE_IS,
                               AutoGetCollection::Options{}
                                   .viewMode(auto_get_collection::ViewMode::kViewsPermitted)
                                   .expectedUUID(_doc.getCollectionUUID())};

        // The drop operation is aborted if the namespace does not exist or does not comply with
        // naming restrictions. Non-system namespaces require additional logic that cannot be done
        // at this level, such as the time series collection must be resolved to remove the
        // corresponding bucket collection, or tag documents associated to non-existing collections
        // must be cleaned up.
        using namespace fmt::literals;
        if (nss().isSystem()) {
            uassert(ErrorCodes::NamespaceNotFound,
                    "namespace {} does not exist"_format(nss().toStringForErrorMsg()),
                    *coll);

            uassertStatusOK(isDroppableCollection(opCtx, nss()));
        }
    }

    try {
        auto coll = Grid::get(opCtx)->catalogClient()->getCollection(opCtx, nss());
        _doc.setCollInfo(std::move(coll));
    } catch (const ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
        // The collection is not sharded or doesn't exist.
        _doc.setCollInfo(boost::none);
    }
}

void DropCollectionCoordinator::_freezeMigrations(
    std::shared_ptr<executor::ScopedTaskExecutor> executor) {
    auto opCtxHolder = cc().makeOperationContext();
    auto* opCtx = opCtxHolder.get();
    getForwardableOpMetadata().setOn(opCtx);

    BSONObjBuilder logChangeDetail;
    if (_doc.getCollInfo()) {
        logChangeDetail.append("collectionUUID", _doc.getCollInfo()->getUuid().toBSON());
    }

    ShardingLogging::get(opCtx)->logChange(
        opCtx, "dropCollection.start", nss(), logChangeDetail.obj());

    if (_doc.getCollInfo()) {
        const auto collUUID = _doc.getCollInfo()->getUuid();
        sharding_ddl_util::stopMigrations(opCtx, nss(), collUUID, getNewSession(opCtx));
    }
}

void DropCollectionCoordinator::_enterCriticalSection(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    LOGV2_DEBUG(7038100, 2, "Acquiring critical section", logAttrs(nss()));

    auto opCtxHolder = cc().makeOperationContext();
    auto* opCtx = opCtxHolder.get();
    getForwardableOpMetadata().setOn(opCtx);

    ShardsvrParticipantBlock blockCRUDOperationsRequest(nss());
    blockCRUDOperationsRequest.setBlockType(mongo::CriticalSectionBlockTypeEnum::kReadsAndWrites);
    blockCRUDOperationsRequest.setReason(_critSecReason);

    async_rpc::GenericArgs args;
    async_rpc::AsyncRPCCommandHelpers::appendMajorityWriteConcern(args);
    async_rpc::AsyncRPCCommandHelpers::appendOSI(args, getNewSession(opCtx));
    auto opts = std::make_shared<async_rpc::AsyncRPCOptions<ShardsvrParticipantBlock>>(
        **executor, token, blockCRUDOperationsRequest, args);
    sharding_ddl_util::sendAuthenticatedCommandToShards(
        opCtx, opts, Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx));

    LOGV2_DEBUG(7038101, 2, "Acquired critical section", logAttrs(nss()));
}

void DropCollectionCoordinator::_commitDropCollection(
    std::shared_ptr<executor::ScopedTaskExecutor> executor) {
    auto opCtxHolder = cc().makeOperationContext();
    auto* opCtx = opCtxHolder.get();
    getForwardableOpMetadata().setOn(opCtx);

    const auto collIsSharded = bool(_doc.getCollInfo());

    LOGV2_DEBUG(5390504, 2, "Dropping collection", logAttrs(nss()), "sharded"_attr = collIsSharded);

    // Remove the query sampling configuration document for this collection, if it exists.
    sharding_ddl_util::removeQueryAnalyzerMetadataFromConfig(
        opCtx,
        BSON(analyze_shard_key::QueryAnalyzerDocument::kNsFieldName
             << NamespaceStringUtil::serialize(nss(), SerializationContext::stateDefault())));

    if (collIsSharded) {
        invariant(_doc.getCollInfo());
        const auto coll = _doc.getCollInfo().value();

        // This always runs in the shard role so should use a cluster transaction to guarantee
        // targeting the config server.
        bool useClusterTransaction = true;
        sharding_ddl_util::removeCollAndChunksMetadataFromConfig(
            opCtx,
            Grid::get(opCtx)->shardRegistry()->getConfigShard(),
            Grid::get(opCtx)->catalogClient(),
            coll,
            ShardingCatalogClient::kMajorityWriteConcern,
            getNewSession(opCtx),
            useClusterTransaction,
            **executor);
    }

    // Remove tags even if the collection is not sharded or didn't exist
    sharding_ddl_util::removeTagsMetadataFromConfig(opCtx, nss(), getNewSession(opCtx));

    // Checkpoint the configTime to ensure that, in the case of a stepdown, the new primary will
    // start-up from a configTime that is inclusive of the metadata removable that was committed
    // during the critical section.
    VectorClockMutable::get(opCtx)->waitForDurableConfigTime().get(opCtx);

    const auto primaryShardId = ShardingState::get(opCtx)->shardId();

    // We need to send the drop to all the shards because both movePrimary and
    // moveChunk leave garbage behind for sharded collections.
    auto participants = Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx);
    // Remove primary shard from participants
    participants.erase(std::remove(participants.begin(), participants.end(), primaryShardId),
                       participants.end());

    sharding_ddl_util::sendDropCollectionParticipantCommandToShards(
        opCtx,
        nss(),
        participants,
        **executor,
        getNewSession(opCtx),
        true /* fromMigrate */,
        false /* dropSystemCollections */);

    // The sharded collection must be dropped on the primary shard after it has been
    // dropped on all of the other shards to ensure it can only be re-created as
    // unsharded with a higher optime than all of the drops.
    sharding_ddl_util::sendDropCollectionParticipantCommandToShards(
        opCtx,
        nss(),
        {primaryShardId},
        **executor,
        getNewSession(opCtx),
        false /* fromMigrate */,
        false /* dropSystemCollections */);

    ShardingLogging::get(opCtx)->logChange(opCtx, "dropCollection", nss());
    LOGV2(5390503, "Collection dropped", logAttrs(nss()));
}

void DropCollectionCoordinator::_exitCriticalSection(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    LOGV2_DEBUG(7038102, 2, "Releasing critical section", logAttrs(nss()));

    auto opCtxHolder = cc().makeOperationContext();
    auto* opCtx = opCtxHolder.get();
    getForwardableOpMetadata().setOn(opCtx);

    ShardsvrParticipantBlock unblockCRUDOperationsRequest(nss());
    unblockCRUDOperationsRequest.setBlockType(CriticalSectionBlockTypeEnum::kUnblock);
    unblockCRUDOperationsRequest.setReason(_critSecReason);

    async_rpc::GenericArgs args;
    async_rpc::AsyncRPCCommandHelpers::appendMajorityWriteConcern(args);
    async_rpc::AsyncRPCCommandHelpers::appendOSI(args, getNewSession(opCtx));
    auto opts = std::make_shared<async_rpc::AsyncRPCOptions<ShardsvrParticipantBlock>>(
        **executor, token, unblockCRUDOperationsRequest, args);
    sharding_ddl_util::sendAuthenticatedCommandToShards(
        opCtx, opts, Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx));

    LOGV2_DEBUG(7038103, 2, "Released critical section", logAttrs(nss()));
}

}  // namespace mongo
