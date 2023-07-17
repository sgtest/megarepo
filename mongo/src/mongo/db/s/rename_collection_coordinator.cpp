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


#include <absl/container/node_hash_map.h>
#include <algorithm>
#include <boost/none.hpp>
#include <boost/smart_ptr.hpp>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/bson/bson_field.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/simple_bsonobj_comparator.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/read_preference.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/collection_uuid_mismatch.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/logical_time.h"
#include "mongo/db/ops/write_ops_gen.h"
#include "mongo/db/ops/write_ops_parsers.h"
#include "mongo/db/persistent_task_store.h"
#include "mongo/db/query/distinct_command_gen.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/s/forwardable_operation_metadata.h"
#include "mongo/db/s/rename_collection_coordinator.h"
#include "mongo/db/s/sharded_index_catalog_commands_gen.h"
#include "mongo/db/s/sharding_ddl_coordinator.h"
#include "mongo/db/s/sharding_ddl_util.h"
#include "mongo/db/s/sharding_logging.h"
#include "mongo/db/s/sharding_recovery_service.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/session/logical_session_id_gen.h"
#include "mongo/db/shard_id.h"
#include "mongo/db/transaction/transaction_api.h"
#include "mongo/db/vector_clock.h"
#include "mongo/db/vector_clock_mutable.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/executor/async_rpc.h"
#include "mongo/executor/async_rpc_util.h"
#include "mongo/executor/task_executor.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_component.h"
#include "mongo/s/analyze_shard_key_documents_gen.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/catalog/type_collection.h"
#include "mongo/s/catalog/type_collection_gen.h"
#include "mongo/s/catalog/type_index_catalog_gen.h"
#include "mongo/s/catalog/type_namespace_placement_gen.h"
#include "mongo/s/catalog/type_tags.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/client/shard.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/grid.h"
#include "mongo/s/request_types/sharded_ddl_commands_gen.h"
#include "mongo/s/shard_version.h"
#include "mongo/s/write_ops/batched_command_request.h"
#include "mongo/s/write_ops/batched_command_response.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/out_of_line_executor.h"
#include "mongo/util/str.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {
namespace {

boost::optional<CollectionType> getShardedCollection(OperationContext* opCtx,
                                                     const NamespaceString& nss) {
    try {
        return Grid::get(opCtx)->catalogClient()->getCollection(opCtx, nss);
    } catch (ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
        // The collection is unsharded or doesn't exist
        return boost::none;
    }
}

boost::optional<UUID> getCollectionUUID(OperationContext* opCtx,
                                        NamespaceString const& nss,
                                        boost::optional<CollectionType> const& optCollectionType,
                                        bool throwOnNotFound = true) {
    if (optCollectionType) {
        return optCollectionType->getUuid();
    }
    Lock::DBLock dbLock(opCtx, nss.dbName(), MODE_IS);
    Lock::CollectionLock collLock(opCtx, nss, MODE_IS);
    const auto collPtr = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, nss);
    if (!collPtr && !throwOnNotFound) {
        return boost::none;
    }

    uassert(ErrorCodes::NamespaceNotFound,
            str::stream() << "Collection " << nss.toStringForErrorMsg() << " doesn't exist.",
            collPtr);

    return collPtr->uuid();
}

void renameIndexMetadataInShards(OperationContext* opCtx,
                                 const NamespaceString& nss,
                                 const RenameCollectionRequest& request,
                                 const OperationSessionInfo& osi,
                                 const std::shared_ptr<executor::TaskExecutor>& executor,
                                 RenameCollectionCoordinatorDocument* doc,
                                 const CancellationToken& token) {
    const auto [configTime, newIndexVersion] = [opCtx]() -> std::pair<LogicalTime, Timestamp> {
        VectorClock::VectorTime vt = VectorClock::get(opCtx)->getTime();
        return {vt.configTime(), vt.clusterTime().asTimestamp()};
    }();

    // Bump the index version only if there are indexes in the source collection.
    auto optShardedCollInfo = doc->getOptShardedCollInfo();
    if (optShardedCollInfo && optShardedCollInfo->getIndexVersion()) {
        // Bump sharding catalog's index version on the config server if the source collection is
        // sharded. It will be updated later on.
        optShardedCollInfo->setIndexVersion({optShardedCollInfo->getUuid(), newIndexVersion});
        doc->setOptShardedCollInfo(optShardedCollInfo);
    }

    // Update global index metadata in shards.
    auto& toNss = request.getTo();

    auto participants = Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx);
    ShardsvrRenameIndexMetadata renameIndexCatalogReq(
        nss, toNss, {doc->getSourceUUID().value(), newIndexVersion});
    renameIndexCatalogReq.setDbName(toNss.dbName());
    async_rpc::GenericArgs args;
    async_rpc::AsyncRPCCommandHelpers::appendMajorityWriteConcern(args);
    async_rpc::AsyncRPCCommandHelpers::appendOSI(args, osi);
    auto opts = std::make_shared<async_rpc::AsyncRPCOptions<ShardsvrRenameIndexMetadata>>(
        renameIndexCatalogReq, executor, token, args);
    sharding_ddl_util::sendAuthenticatedCommandToShards(opCtx, opts, participants);
}

std::vector<ShardId> getLatestCollectionPlacementInfoFor(OperationContext* opCtx,
                                                         const NamespaceString& nss,
                                                         const UUID& uuid) {
    // Use the content of config.chunks to obtain the placement of the collection being renamed.
    // The request is equivalent to 'configDb.chunks.distinct("shard", {uuid:collectionUuid})'.
    auto query = BSON(NamespacePlacementType::kNssFieldName << NamespaceStringUtil::serialize(nss));

    auto configShard = Grid::get(opCtx)->shardRegistry()->getConfigShard();


    DistinctCommandRequest distinctRequest(ChunkType::ConfigNS);
    distinctRequest.setKey(ChunkType::shard.name());
    distinctRequest.setQuery(BSON(ChunkType::collectionUUID.name() << uuid));
    auto rc = BSON(repl::ReadConcernArgs::kReadConcernFieldName << repl::ReadConcernArgs::kLocal);

    auto reply = uassertStatusOK(configShard->runCommandWithFixedRetryAttempts(
        opCtx,
        ReadPreferenceSetting(ReadPreference::PrimaryOnly, TagSet{}),
        DatabaseName::kConfig.toString(),
        distinctRequest.toBSON({rc}),
        Shard::RetryPolicy::kIdempotent));

    uassertStatusOK(Shard::CommandResponse::getEffectiveStatus(reply));
    std::vector<ShardId> shardIds;
    for (const auto& valueElement : reply.response.getField("values").Array()) {
        shardIds.emplace_back(valueElement.String());
    }

    return shardIds;
}

SemiFuture<BatchedCommandResponse> noOpStatement() {
    BatchedCommandResponse noOpResponse;
    noOpResponse.setStatus(Status::OK());
    noOpResponse.setN(0);
    return SemiFuture<BatchedCommandResponse>(std::move(noOpResponse));
}

SemiFuture<BatchedCommandResponse> deleteShardedCollectionStatement(
    const txn_api::TransactionClient& txnClient,
    const NamespaceString& nss,
    const boost::optional<UUID>& uuid,
    int stmtId) {

    if (uuid) {
        const auto deleteCollectionQuery =
            BSON(CollectionType::kNssFieldName << NamespaceStringUtil::serialize(nss)
                                               << CollectionType::kUuidFieldName << *uuid);

        write_ops::DeleteCommandRequest deleteOp(CollectionType::ConfigNS);
        deleteOp.setDeletes({[&]() {
            write_ops::DeleteOpEntry entry;
            entry.setMulti(false);
            entry.setQ(deleteCollectionQuery);
            return entry;
        }()});

        return txnClient.runCRUDOp(deleteOp, {stmtId});
    } else {
        return noOpStatement();
    }
}

SemiFuture<BatchedCommandResponse> renameShardedCollectionStatement(
    const txn_api::TransactionClient& txnClient,
    const CollectionType& oldCollection,
    const NamespaceString& newNss,
    const Timestamp& timeInsertion,
    int stmtId) {
    auto newCollectionType = oldCollection;
    newCollectionType.setNss(newNss);
    newCollectionType.setTimestamp(timeInsertion);
    newCollectionType.setEpoch(OID::gen());

    // Implemented as an upsert to be idempotent
    auto query = BSON(CollectionType::kNssFieldName << NamespaceStringUtil::serialize(newNss));
    write_ops::UpdateCommandRequest updateOp(CollectionType::ConfigNS);
    updateOp.setUpdates({[&] {
        write_ops::UpdateOpEntry entry;
        entry.setQ(query);
        entry.setU(
            write_ops::UpdateModification::parseFromClassicUpdate(newCollectionType.toBSON()));
        entry.setUpsert(true);
        entry.setMulti(false);
        return entry;
    }()});

    return txnClient.runCRUDOp(updateOp, {stmtId} /*stmtIds*/);
}

SemiFuture<BatchedCommandResponse> insertToPlacementHistoryStatement(
    const txn_api::TransactionClient& txnClient,
    const NamespaceString& nss,
    const boost::optional<UUID>& uuid,
    const Timestamp& clusterTime,
    const std::vector<ShardId>& shards,
    int stmtId,
    const BatchedCommandResponse& previousOperationResult) {

    // Skip the insertion of the placement entry if the previous statement didn't change any
    // document - we can deduce that the whole transaction was already committed in a previous
    // attempt.
    if (previousOperationResult.getN() == 0) {
        return noOpStatement();
    }

    NamespacePlacementType placementInfo(NamespaceString(nss), clusterTime, shards);
    if (uuid)
        placementInfo.setUuid(*uuid);
    write_ops::InsertCommandRequest insertPlacementEntry(
        NamespaceString::kConfigsvrPlacementHistoryNamespace, {placementInfo.toBSON()});

    return txnClient.runCRUDOp(insertPlacementEntry, {stmtId} /*stmtIds*/);
}


SemiFuture<BatchedCommandResponse> updateZonesStatement(const txn_api::TransactionClient& txnClient,
                                                        const NamespaceString& oldNss,
                                                        const NamespaceString& newNss) {

    const auto query = BSON(TagsType::ns(NamespaceStringUtil::serialize(oldNss)));
    const auto update = BSON("$set" << BSON(TagsType::ns(NamespaceStringUtil::serialize(newNss))));

    BatchedCommandRequest request([&] {
        write_ops::UpdateCommandRequest updateOp(TagsType::ConfigNS);
        updateOp.setUpdates({[&] {
            write_ops::UpdateOpEntry entry;
            entry.setQ(query);
            entry.setU(write_ops::UpdateModification::parseFromClassicUpdate(update));
            entry.setUpsert(false);
            entry.setMulti(true);
            return entry;
        }()});
        return updateOp;
    }());
    return txnClient.runCRUDOp(request, {-1} /*stmtIds*/);
}

SemiFuture<BatchedCommandResponse> deleteZonesStatement(const txn_api::TransactionClient& txnClient,
                                                        const NamespaceString& nss) {

    const auto query = BSON(TagsType::ns(NamespaceStringUtil::serialize(nss)));
    const auto hint = BSON(TagsType::ns() << 1 << TagsType::min() << 1);

    BatchedCommandRequest request([&] {
        write_ops::DeleteCommandRequest deleteOp(TagsType::ConfigNS);
        deleteOp.setDeletes({[&] {
            write_ops::DeleteOpEntry entry;
            entry.setQ(query);
            entry.setMulti(true);
            entry.setHint(hint);
            return entry;
        }()});
        return deleteOp;
    }());

    return txnClient.runCRUDOp(request, {-1});
}

SemiFuture<BatchedCommandResponse> deleteShardingIndexCatalogMetadataStatement(
    const txn_api::TransactionClient& txnClient, const boost::optional<UUID>& uuid) {
    if (uuid) {
        // delete index catalog metadata
        BatchedCommandRequest request([&] {
            write_ops::DeleteCommandRequest deleteOp(
                NamespaceString::kConfigsvrIndexCatalogNamespace);
            deleteOp.setDeletes({[&] {
                write_ops::DeleteOpEntry entry;
                entry.setQ(BSON(IndexCatalogType::kCollectionUUIDFieldName << *uuid));
                entry.setMulti(true);
                return entry;
            }()});
            return deleteOp;
        }());

        return txnClient.runCRUDOp(request, {-1});
    } else {
        return noOpStatement();
    }
}


void renameCollectionMetadataInTransaction(OperationContext* opCtx,
                                           const boost::optional<CollectionType>& optFromCollType,
                                           const NamespaceString& fromNss,
                                           const NamespaceString& toNss,
                                           const boost::optional<UUID>& droppedTargetUUID,
                                           const WriteConcernOptions& writeConcern,
                                           const std::shared_ptr<executor::TaskExecutor>& executor,
                                           const OperationSessionInfo& osi) {

    std::string logMsg = str::stream()
        << toStringForLogging(fromNss) << " to " << toStringForLogging(toNss);
    if (optFromCollType) {
        // Case sharded FROM collection
        auto fromUUID = optFromCollType->getUuid();

        // Every statement in the transaction runs under the same clusterTime. To ensure in the
        // placementHistory the drop of the target will appear earlier then the insert of the target
        // we forcely add a tick to have 2 valid timestamp that we can use to differentiate the 2
        // operations.
        auto now = VectorClock::get(opCtx)->getTime();
        auto nowClusterTime = now.clusterTime();
        auto timeDrop = nowClusterTime.asTimestamp();

        nowClusterTime.addTicks(1);
        auto timeInsert = nowClusterTime.asTimestamp();

        // Retrieve the latest placement information about "FROM".
        auto fromNssShards = getLatestCollectionPlacementInfoFor(opCtx, fromNss, fromUUID);

        auto transactionChain = [&](const txn_api::TransactionClient& txnClient,
                                    ExecutorPtr txnExec) {
            // Remove config.collection entry. Query by 'ns' AND 'uuid' so that the remove can be
            // resolved with an IXSCAN (thanks to the index on '_id') and is idempotent (thanks to
            // the 'uuid') delete TO collection if exists.
            return deleteShardedCollectionStatement(txnClient, toNss, droppedTargetUUID, 1)
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& deleteCollResponse) {
                    uassertStatusOK(deleteCollResponse.toStatus());

                    return insertToPlacementHistoryStatement(
                        txnClient, toNss, droppedTargetUUID, timeDrop, {}, 2, deleteCollResponse);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& response) {
                    uassertStatusOK(response.toStatus());

                    return deleteShardingIndexCatalogMetadataStatement(txnClient,
                                                                       droppedTargetUUID);
                })
                // Delete "FROM" collection
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& response) {
                    uassertStatusOK(response.toStatus());
                    return deleteShardedCollectionStatement(txnClient, fromNss, fromUUID, 3);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& deleteCollResponse) {
                    uassertStatusOK(deleteCollResponse.toStatus());

                    return insertToPlacementHistoryStatement(
                        txnClient, fromNss, fromUUID, timeDrop, {}, 4, deleteCollResponse);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& deleteCollResponse) {
                    uassertStatusOK(deleteCollResponse.toStatus());
                    // Use the modified entries to insert collection and placement entries for "TO".
                    return renameShardedCollectionStatement(
                        txnClient, *optFromCollType, toNss, timeInsert, 5);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& upsertCollResponse) {
                    uassertStatusOK(upsertCollResponse.toStatus());

                    return insertToPlacementHistoryStatement(txnClient,
                                                             toNss,
                                                             fromUUID,
                                                             timeInsert,
                                                             fromNssShards,
                                                             6,
                                                             upsertCollResponse);
                })
                // update tags and check it was successful
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& insertCollResponse) {
                    uassertStatusOK(insertCollResponse.toStatus());

                    return updateZonesStatement(txnClient, fromNss, toNss);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& response) {
                    uassertStatusOK(response.toStatus());
                })
                .semi();
        };
        const bool useClusterTransaction = true;
        sharding_ddl_util::runTransactionOnShardingCatalog(
            opCtx, std::move(transactionChain), writeConcern, osi, useClusterTransaction, executor);

        ShardingLogging::get(opCtx)->logChange(
            opCtx,
            "renameCollection.metadata",
            str::stream() << logMsg << ": dropped target collection and renamed source collection",
            BSON("newCollMetadata" << optFromCollType->toBSON()),
            ShardingCatalogClient::kMajorityWriteConcern,
            Grid::get(opCtx)->shardRegistry()->getConfigShard(),
            Grid::get(opCtx)->catalogClient());
    } else {
        // Case unsharded FROM collection : just delete the target collection if sharded
        auto now = VectorClock::get(opCtx)->getTime();
        auto newTimestamp = now.clusterTime().asTimestamp();

        auto transactionChain = [&](const txn_api::TransactionClient& txnClient,
                                    ExecutorPtr txnExec) {
            return deleteShardedCollectionStatement(txnClient, toNss, droppedTargetUUID, 1)
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& deleteCollResponse) {
                    uassertStatusOK(deleteCollResponse.toStatus());
                    return insertToPlacementHistoryStatement(txnClient,
                                                             toNss,
                                                             droppedTargetUUID,
                                                             newTimestamp,
                                                             {},
                                                             2,
                                                             deleteCollResponse);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& response) {
                    uassertStatusOK(response.toStatus());

                    return deleteShardingIndexCatalogMetadataStatement(txnClient,
                                                                       droppedTargetUUID);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& response) {
                    uassertStatusOK(response.toStatus());

                    return deleteZonesStatement(txnClient, toNss);
                })
                .thenRunOn(txnExec)
                .then([&](const BatchedCommandResponse& response) {
                    uassertStatusOK(response.toStatus());
                })
                .semi();
        };

        const bool useClusterTransaction = true;
        sharding_ddl_util::runTransactionOnShardingCatalog(
            opCtx, std::move(transactionChain), writeConcern, osi, useClusterTransaction, executor);

        ShardingLogging::get(opCtx)->logChange(opCtx,
                                               "renameCollection.metadata",
                                               str::stream()
                                                   << logMsg << " : dropped target collection.",
                                               BSONObj(),
                                               ShardingCatalogClient::kMajorityWriteConcern,
                                               Grid::get(opCtx)->shardRegistry()->getConfigShard(),
                                               Grid::get(opCtx)->catalogClient());
    }
}
}  // namespace

RenameCollectionCoordinator::RenameCollectionCoordinator(ShardingDDLCoordinatorService* service,
                                                         const BSONObj& initialState)
    : RecoverableShardingDDLCoordinator(service, "RenameCollectionCoordinator", initialState),
      _request(_doc.getRenameCollectionRequest()) {}

void RenameCollectionCoordinator::checkIfOptionsConflict(const BSONObj& doc) const {
    const auto otherDoc = RenameCollectionCoordinatorDocument::parse(
        IDLParserContext("RenameCollectionCoordinatorDocument"), doc);

    const auto& selfReq = _request.toBSON();
    const auto& otherReq = otherDoc.getRenameCollectionRequest().toBSON();

    uassert(ErrorCodes::ConflictingOperationInProgress,
            str::stream() << "Another rename collection for namespace "
                          << originalNss().toStringForErrorMsg()
                          << " is being executed with different parameters: " << selfReq,
            SimpleBSONObjComparator::kInstance.evaluate(selfReq == otherReq));
}

std::set<NamespaceString> RenameCollectionCoordinator::_getAdditionalLocksToAcquire(
    OperationContext* opCtx) {
    return {_request.getTo()};
}

void RenameCollectionCoordinator::appendCommandInfo(BSONObjBuilder* cmdInfoBuilder) const {
    cmdInfoBuilder->appendElements(_request.toBSON());
}

ExecutorFuture<void> RenameCollectionCoordinator::_runImpl(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    const CancellationToken& token) noexcept {
    return ExecutorFuture<void>(**executor)
        .then(_buildPhaseHandler(
            Phase::kCheckPreconditions,
            [this, executor = executor, anchor = shared_from_this()] {
                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                const auto& fromNss = nss();
                const auto& toNss = _request.getTo();

                const auto criticalSectionReason =
                    sharding_ddl_util::getCriticalSectionReasonForRename(fromNss, toNss);

                try {
                    uassert(ErrorCodes::InvalidOptions,
                            "Cannot provide an expected collection UUID when renaming between "
                            "databases",
                            fromNss.db_forSharding() == toNss.db_forSharding() ||
                                (!_doc.getExpectedSourceUUID() && !_doc.getExpectedTargetUUID()));

                    {
                        AutoGetCollection coll{
                            opCtx,
                            fromNss,
                            MODE_IS,
                            AutoGetCollection::Options{}
                                .viewMode(auto_get_collection::ViewMode::kViewsPermitted)
                                .expectedUUID(_doc.getExpectedSourceUUID())};

                        uassert(ErrorCodes::CommandNotSupportedOnView,
                                str::stream()
                                    << "Can't rename source collection `"
                                    << fromNss.toStringForErrorMsg() << "` because it is a view.",
                                !CollectionCatalog::get(opCtx)->lookupView(opCtx, fromNss));

                        checkCollectionUUIDMismatch(
                            opCtx, fromNss, *coll, _doc.getExpectedSourceUUID());

                        uassert(ErrorCodes::NamespaceNotFound,
                                str::stream() << "Collection " << fromNss.toStringForErrorMsg()
                                              << " doesn't exist.",
                                coll.getCollection());

                        uassert(ErrorCodes::IllegalOperation,
                                "Cannot rename an encrypted collection",
                                !coll || !coll->getCollectionOptions().encryptedFieldConfig ||
                                    _doc.getAllowEncryptedCollectionRename().value_or(false));
                    }

                    // Make sure the source collection exists
                    const auto optSourceCollType = getShardedCollection(opCtx, fromNss);
                    const bool sourceIsSharded = (bool)optSourceCollType;

                    _doc.setSourceUUID(getCollectionUUID(opCtx, fromNss, optSourceCollType));
                    if (sourceIsSharded) {
                        uassert(ErrorCodes::CommandFailed,
                                str::stream() << "Source and destination collections must be on "
                                                 "the same database because "
                                              << fromNss.toStringForErrorMsg() << " is sharded.",
                                fromNss.db_forSharding() == toNss.db_forSharding());
                        _doc.setOptShardedCollInfo(optSourceCollType);
                    } else if (fromNss.db_forSharding() != toNss.db_forSharding()) {
                        sharding_ddl_util::checkDbPrimariesOnTheSameShard(opCtx, fromNss, toNss);
                    }

                    const auto optTargetCollType = getShardedCollection(opCtx, toNss);
                    const bool targetIsSharded = (bool)optTargetCollType;
                    _doc.setTargetIsSharded(targetIsSharded);
                    _doc.setTargetUUID(getCollectionUUID(
                        opCtx, toNss, optTargetCollType, /*throwNotFound*/ false));

                    if (!targetIsSharded) {
                        // (SERVER-67325) Acquire critical section on the target collection in order
                        // to disallow concurrent `createCollection`. In case the collection does
                        // not exist, it will be later released by the rename participant. In case
                        // the collection exists and is unsharded, the critical section can be
                        // released right away as the participant will re-acquire it when needed.
                        auto criticalSection = ShardingRecoveryService::get(opCtx);
                        criticalSection->acquireRecoverableCriticalSectionBlockWrites(
                            opCtx,
                            toNss,
                            criticalSectionReason,
                            ShardingCatalogClient::kLocalWriteConcern);
                        criticalSection->promoteRecoverableCriticalSectionToBlockAlsoReads(
                            opCtx,
                            toNss,
                            criticalSectionReason,
                            ShardingCatalogClient::kLocalWriteConcern);

                        // Make sure the target namespace is not a view
                        uassert(ErrorCodes::NamespaceExists,
                                str::stream() << "a view already exists with that name: "
                                              << toNss.toStringForErrorMsg(),
                                !CollectionCatalog::get(opCtx)->lookupView(opCtx, toNss));

                        if (CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx,
                                                                                       toNss)) {
                            // Release the critical section because the unsharded target collection
                            // already exists, hence no risk of concurrent `createCollection`
                            criticalSection->releaseRecoverableCriticalSection(
                                opCtx,
                                toNss,
                                criticalSectionReason,
                                WriteConcerns::kLocalWriteConcern);
                        }
                    }

                    sharding_ddl_util::checkRenamePreconditions(
                        opCtx, sourceIsSharded, toNss, _doc.getDropTarget());

                    sharding_ddl_util::checkCatalogConsistencyAcrossShardsForRename(
                        opCtx, fromNss, toNss, _doc.getDropTarget(), executor);

                    {
                        AutoGetCollection coll{
                            opCtx,
                            toNss,
                            MODE_IS,
                            AutoGetCollection::Options{}
                                .viewMode(auto_get_collection::ViewMode::kViewsPermitted)
                                .expectedUUID(_doc.getExpectedTargetUUID())};
                        uassert(ErrorCodes::IllegalOperation,
                                "Cannot rename to an existing encrypted collection",
                                !coll || !coll->getCollectionOptions().encryptedFieldConfig ||
                                    _doc.getAllowEncryptedCollectionRename().value_or(false));
                    }

                } catch (const DBException&) {
                    auto criticalSection = ShardingRecoveryService::get(opCtx);
                    criticalSection->releaseRecoverableCriticalSection(
                        opCtx,
                        toNss,
                        criticalSectionReason,
                        WriteConcerns::kLocalWriteConcern,
                        false /* throwIfReasonDiffers */);
                    _completeOnError = true;
                    throw;
                }
            }))
        .then(_buildPhaseHandler(
            Phase::kFreezeMigrations,
            [this, executor = executor, anchor = shared_from_this()] {
                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                const auto& fromNss = nss();
                const auto& toNss = _request.getTo();

                ShardingLogging::get(opCtx)->logChange(
                    opCtx,
                    "renameCollection.start",
                    NamespaceStringUtil::serialize(fromNss),
                    BSON("source" << NamespaceStringUtil::serialize(fromNss) << "destination"
                                  << NamespaceStringUtil::serialize(toNss)),
                    ShardingCatalogClient::kMajorityWriteConcern);

                // Block migrations on involved sharded collections
                if (_doc.getOptShardedCollInfo()) {
                    const auto& osi = getNewSession(opCtx);
                    sharding_ddl_util::stopMigrations(opCtx, fromNss, _doc.getSourceUUID(), osi);
                }

                if (_doc.getTargetIsSharded()) {
                    const auto& osi = getNewSession(opCtx);
                    sharding_ddl_util::stopMigrations(opCtx, toNss, _doc.getTargetUUID(), osi);
                }
            }))
        .then(_buildPhaseHandler(
            Phase::kBlockCrudAndRename,
            [this, token, executor = executor, anchor = shared_from_this()] {
                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                if (!_firstExecution) {
                    _performNoopRetryableWriteOnAllShardsAndConfigsvr(
                        opCtx, getNewSession(opCtx), **executor);
                }

                const auto& fromNss = nss();

                // On participant shards:
                // - Block CRUD on source and target collection in case at least one of such
                //   collections is currently sharded
                // - Locally drop the target collection
                // - Locally rename source to target
                ShardsvrRenameCollectionParticipant renameCollParticipantRequest(
                    fromNss, _doc.getSourceUUID().value());
                renameCollParticipantRequest.setDbName(fromNss.dbName());
                renameCollParticipantRequest.setTargetUUID(_doc.getTargetUUID());
                renameCollParticipantRequest.setRenameCollectionRequest(_request);

                // We need to send the command to all the shards because both movePrimary and
                // moveChunk leave garbage behind for sharded collections. At the same time, the
                // primary shard needs to be last participant to perfom its local rename operation:
                // this will ensure that the op entries generated by the collections being
                // renamed/dropped will be generated at points in time where all shards have a
                // consistent view of the metadata and no concurrent writes are being performed.
                const auto primaryShardId = ShardingState::get(opCtx)->shardId();
                auto participants = Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx);
                participants.erase(
                    std::remove(participants.begin(), participants.end(), primaryShardId),
                    participants.end());

                async_rpc::GenericArgs args;
                async_rpc::AsyncRPCCommandHelpers::appendMajorityWriteConcern(args);
                async_rpc::AsyncRPCCommandHelpers::appendOSI(args, getNewSession(opCtx));
                auto opts = std::make_shared<
                    async_rpc::AsyncRPCOptions<ShardsvrRenameCollectionParticipant>>(
                    renameCollParticipantRequest, **executor, token, args);
                sharding_ddl_util::sendAuthenticatedCommandToShards(opCtx, opts, participants);
                sharding_ddl_util::sendAuthenticatedCommandToShards(opCtx, opts, {primaryShardId});
            }))
        .then(_buildPhaseHandler(
            Phase::kRenameMetadata,
            [this, token, executor = executor, anchor = shared_from_this()] {
                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                // Remove the query sampling configuration documents for the source and destination
                // collections, if they exist.
                sharding_ddl_util::removeQueryAnalyzerMetadataFromConfig(
                    opCtx,
                    BSON(analyze_shard_key::QueryAnalyzerDocument::kNsFieldName
                         << BSON("$in" << BSON_ARRAY(
                                     NamespaceStringUtil::serialize(nss())
                                     << NamespaceStringUtil::serialize(_request.getTo())))));

                // For an unsharded collection the CSRS server can not verify the targetUUID.
                // Use the session ID + txnNumber to ensure no stale requests get through.
                if (!_firstExecution) {
                    _performNoopRetryableWriteOnAllShardsAndConfigsvr(
                        opCtx, getNewSession(opCtx), **executor);
                }

                if ((_doc.getTargetIsSharded() || _doc.getOptShardedCollInfo())) {
                    const auto& osi = getNewSession(opCtx);
                    renameIndexMetadataInShards(
                        opCtx, nss(), _request, osi, **executor, &_doc, token);
                }

                const auto& osi = getNewSession(opCtx);
                renameCollectionMetadataInTransaction(opCtx,
                                                      _doc.getOptShardedCollInfo(),
                                                      nss(),
                                                      _request.getTo(),
                                                      _doc.getTargetUUID(),
                                                      ShardingCatalogClient::kMajorityWriteConcern,
                                                      **executor,
                                                      osi);

                // Checkpoint the configTime to ensure that, in the case of a stepdown, the new
                // primary will start-up from a configTime that is inclusive of the renamed
                // metadata.
                VectorClockMutable::get(opCtx)->waitForDurableConfigTime().get(opCtx);
            }))
        .then(_buildPhaseHandler(
            Phase::kUnblockCRUD,
            [this, token, executor = executor, anchor = shared_from_this()] {
                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                if (!_firstExecution) {
                    _performNoopRetryableWriteOnAllShardsAndConfigsvr(
                        opCtx, getNewSession(opCtx), **executor);
                }

                const auto& fromNss = nss();
                // On participant shards:
                // - Unblock CRUD on participants for both source and destination collections
                ShardsvrRenameCollectionUnblockParticipant unblockParticipantRequest(
                    fromNss, _doc.getSourceUUID().value());
                unblockParticipantRequest.setDbName(fromNss.dbName());
                unblockParticipantRequest.setRenameCollectionRequest(_request);
                auto participants = Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx);

                async_rpc::GenericArgs args;
                async_rpc::AsyncRPCCommandHelpers::appendMajorityWriteConcern(args);
                async_rpc::AsyncRPCCommandHelpers::appendOSI(args, getNewSession(opCtx));
                auto opts = std::make_shared<
                    async_rpc::AsyncRPCOptions<ShardsvrRenameCollectionUnblockParticipant>>(
                    unblockParticipantRequest, **executor, token, args);
                sharding_ddl_util::sendAuthenticatedCommandToShards(opCtx, opts, participants);

                // Delete chunks belonging to the previous incarnation of the target collection.
                // This is performed after releasing the critical section in order to reduce stalls
                // and performed outside of a transaction to prevent timeout.
                auto targetUUID = _doc.getTargetUUID();
                if (targetUUID) {
                    auto query = BSON("uuid" << *targetUUID);
                    uassertStatusOK(Grid::get(opCtx)->catalogClient()->removeConfigDocuments(
                        opCtx,
                        ChunkType::ConfigNS,
                        query,
                        ShardingCatalogClient::kMajorityWriteConcern));
                }
            }))
        .then(_buildPhaseHandler(Phase::kSetResponse, [this, anchor = shared_from_this()] {
            auto opCtxHolder = cc().makeOperationContext();
            auto* opCtx = opCtxHolder.get();
            getForwardableOpMetadata().setOn(opCtx);

            // Retrieve the new collection version
            const auto catalog = Grid::get(opCtx)->catalogCache();
            const auto cri = uassertStatusOK(
                catalog->getCollectionRoutingInfoWithRefresh(opCtx, _request.getTo()));
            _response = RenameCollectionResponse(cri.cm.isSharded() ? cri.getCollectionVersion()
                                                                    : ShardVersion::UNSHARDED());

            ShardingLogging::get(opCtx)->logChange(
                opCtx,
                "renameCollection.end",
                NamespaceStringUtil::serialize(nss()),
                BSON("source" << NamespaceStringUtil::serialize(nss()) << "destination"
                              << NamespaceStringUtil::serialize(_request.getTo())),
                ShardingCatalogClient::kMajorityWriteConcern);
            LOGV2(5460504, "Collection renamed", logAttrs(nss()));
        }));
}

}  // namespace mongo
