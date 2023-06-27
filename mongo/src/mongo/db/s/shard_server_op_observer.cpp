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

#include "mongo/db/s/shard_server_op_observer.h"

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <fmt/format.h>
#include <memory>
#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/timestamp.h"
#include "mongo/bson/util/bson_extract.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/database_name.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/s/balancer_stats_registry.h"
#include "mongo/db/s/collection_critical_section_document_gen.h"
#include "mongo/db/s/collection_metadata.h"
#include "mongo/db/s/collection_sharding_runtime.h"
#include "mongo/db/s/database_sharding_state.h"
#include "mongo/db/s/migration_source_manager.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/s/range_deletion_task_gen.h"
#include "mongo/db/s/shard_identity_rollback_notifier.h"
#include "mongo/db/s/sharding_initialization_mongod.h"
#include "mongo/db/s/sharding_migration_critical_section.h"
#include "mongo/db/s/sharding_recovery_service.h"
#include "mongo/db/s/type_shard_collection.h"
#include "mongo/db/s/type_shard_collection_gen.h"
#include "mongo/db/s/type_shard_database.h"
#include "mongo/db/s/type_shard_database_gen.h"
#include "mongo/db/s/type_shard_identity.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/tenant_id.h"
#include "mongo/db/update/update_oplog_entry_serialization.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/s/cannot_implicitly_create_collection_info.h"
#include "mongo/s/catalog/type_index_catalog.h"
#include "mongo/s/catalog/type_index_catalog_gen.h"
#include "mongo/s/catalog_cache_loader.h"
#include "mongo/s/index_version.h"
#include "mongo/s/sharding_index_catalog_cache.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/database_name_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/namespace_string_util.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {
namespace {

const auto documentIdDecoration = OplogDeleteEntryArgs::declareDecoration<BSONObj>();

bool isStandaloneOrPrimary(OperationContext* opCtx) {
    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    return replCoord->canAcceptWritesForDatabase(opCtx, DatabaseName::kAdmin);
}

/**
 * Used to notify the catalog cache loader of a new placement version and invalidate the in-memory
 * routing table cache once the oplog updates are committed and become visible.
 */
class CollectionPlacementVersionLogOpHandler final : public RecoveryUnit::Change {
public:
    CollectionPlacementVersionLogOpHandler(const NamespaceString& nss, bool droppingCollection)
        : _nss(nss), _droppingCollection(droppingCollection) {}

    void commit(OperationContext* opCtx, boost::optional<Timestamp>) override {
        invariant(opCtx->lockState()->isCollectionLockedForMode(_nss, MODE_IX));

        CatalogCacheLoader::get(opCtx).notifyOfCollectionPlacementVersionUpdate(_nss);

        // Force subsequent uses of the namespace to refresh the filtering metadata so they can
        // synchronize with any work happening on the primary (e.g., migration critical section).
        // TODO (SERVER-71444): Fix to be interruptible or document exception.
        UninterruptibleLockGuard noInterrupt(opCtx->lockState());  // NOLINT.
        auto scopedCss =
            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(opCtx, _nss);
        if (_droppingCollection)
            scopedCss->clearFilteringMetadataForDroppedCollection(opCtx);
        else
            scopedCss->clearFilteringMetadata(opCtx);
    }

    void rollback(OperationContext* opCtx) override {}

private:
    const NamespaceString _nss;
    const bool _droppingCollection;
};

/**
 * Invalidates the in-memory routing table cache when a collection is dropped, so the next caller
 * with routing information will provoke a routing table refresh and see the drop.
 *
 * The query parameter must contain an _id field that identifies which collections entry is being
 * updated.
 *
 * This only runs on secondaries.
 * The global exclusive lock is expected to be held by the caller.
 */
void onConfigDeleteInvalidateCachedCollectionMetadataAndNotify(OperationContext* opCtx,
                                                               const BSONObj& query) {
    // Notification of routing table changes are only needed on secondaries
    if (isStandaloneOrPrimary(opCtx)) {
        return;
    }

    // Extract which collection entry is being deleted from the _id field.
    std::string deletedCollection;
    fassert(40479,
            bsonExtractStringField(query, ShardCollectionType::kNssFieldName, &deletedCollection));
    const NamespaceString deletedNss(deletedCollection);

    // Need the WUOW to retain the lock for CollectionPlacementVersionLogOpHandler::commit().
    // TODO SERVER-58223: evaluate whether this is safe or whether acquiring the lock can block.
    AllowLockAcquisitionOnTimestampedUnitOfWork allowLockAcquisition(opCtx->lockState());
    AutoGetCollection autoColl(opCtx, deletedNss, MODE_IX);

    opCtx->recoveryUnit()->registerChange(std::make_unique<CollectionPlacementVersionLogOpHandler>(
        deletedNss, /* droppingCollection */ true));
}

/**
 * Aborts any ongoing migration for the given namespace. Should only be called when observing
 * index operations.
 */
void abortOngoingMigrationIfNeeded(OperationContext* opCtx, const NamespaceString& nss) {
    const auto scopedCsr =
        CollectionShardingRuntime::assertCollectionLockedAndAcquireShared(opCtx, nss);
    if (auto msm = MigrationSourceManager::get(*scopedCsr)) {
        // Only interrupt the migration, but don't actually join
        (void)msm->abort();
    }
}

}  // namespace

ShardServerOpObserver::ShardServerOpObserver() = default;

ShardServerOpObserver::~ShardServerOpObserver() = default;

void ShardServerOpObserver::onInserts(OperationContext* opCtx,
                                      const CollectionPtr& coll,
                                      std::vector<InsertStatement>::const_iterator begin,
                                      std::vector<InsertStatement>::const_iterator end,
                                      std::vector<bool> fromMigrate,
                                      bool defaultFromMigrate,
                                      OpStateAccumulator* opAccumulator) {
    const auto& nss = coll->ns();

    for (auto it = begin; it != end; ++it) {
        const auto& insertedDoc = it->doc;

        if (nss == NamespaceString::kServerConfigurationNamespace) {
            if (auto idElem = insertedDoc["_id"]) {
                if (idElem.str() == ShardIdentityType::IdName) {
                    auto shardIdentityDoc =
                        uassertStatusOK(ShardIdentityType::fromShardIdentityDocument(insertedDoc));
                    uassertStatusOK(shardIdentityDoc.validate());
                    /**
                     * Perform shard identity initialization once we are certain that the document
                     * is committed.
                     */
                    opCtx->recoveryUnit()->onCommit([shardIdentity = std::move(shardIdentityDoc)](
                                                        OperationContext* opCtx,
                                                        boost::optional<Timestamp>) {
                        try {
                            ShardingInitializationMongoD::get(opCtx)->initializeFromShardIdentity(
                                opCtx, shardIdentity);
                        } catch (const AssertionException& ex) {
                            fassertFailedWithStatus(40071, ex.toStatus());
                        }
                    });
                }
            }
        }

        if (nss == NamespaceString::kRangeDeletionNamespace) {
            if (!isStandaloneOrPrimary(opCtx)) {
                return;
            }

            auto deletionTask =
                RangeDeletionTask::parse(IDLParserContext("ShardServerOpObserver"), insertedDoc);

            const auto numOrphanDocs = deletionTask.getNumOrphanDocs();
            BalancerStatsRegistry::get(opCtx)->onRangeDeletionTaskInsertion(
                deletionTask.getCollectionUuid(), numOrphanDocs);
        }

        if (nss == NamespaceString::kCollectionCriticalSectionsNamespace &&
            !sharding_recovery_util::inRecoveryMode(opCtx)) {
            const auto collCSDoc = CollectionCriticalSectionDocument::parse(
                IDLParserContext("ShardServerOpObserver"), insertedDoc);
            invariant(!collCSDoc.getBlockReads());
            opCtx->recoveryUnit()->onCommit(
                [insertedNss = collCSDoc.getNss(), reason = collCSDoc.getReason().getOwned()](
                    OperationContext* opCtx, boost::optional<Timestamp>) {
                    if (nsIsDbOnly(NamespaceStringUtil::serialize(insertedNss))) {
                        boost::optional<AutoGetDb> lockDbIfNotPrimary;
                        if (!isStandaloneOrPrimary(opCtx)) {
                            lockDbIfNotPrimary.emplace(opCtx, insertedNss.dbName(), MODE_IX);
                        }
                        // TODO (SERVER-71444): Fix to be interruptible or document exception.
                        UninterruptibleLockGuard noInterrupt(opCtx->lockState());  // NOLINT.
                        auto scopedDss = DatabaseShardingState::assertDbLockedAndAcquireExclusive(
                            opCtx, insertedNss.dbName());
                        scopedDss->enterCriticalSectionCatchUpPhase(opCtx, reason);
                    } else {
                        boost::optional<AutoGetCollection> lockCollectionIfNotPrimary;
                        if (!isStandaloneOrPrimary(opCtx)) {
                            lockCollectionIfNotPrimary.emplace(
                                opCtx,
                                insertedNss,
                                fixLockModeForSystemDotViewsChanges(insertedNss, MODE_IX),
                                AutoGetCollection::Options{}.viewMode(
                                    auto_get_collection::ViewMode::kViewsPermitted));
                        }

                        // TODO (SERVER-71444): Fix to be interruptible or document exception.
                        UninterruptibleLockGuard noInterrupt(opCtx->lockState());  // NOLINT.
                        auto scopedCsr =
                            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                                opCtx, insertedNss);
                        scopedCsr->enterCriticalSectionCatchUpPhase(reason);
                    }
                });
        }
    }
}

void ShardServerOpObserver::onUpdate(OperationContext* opCtx,
                                     const OplogUpdateEntryArgs& args,
                                     OpStateAccumulator* opAccumulator) {
    const auto& updateDoc = args.updateArgs->update;
    // Most of these handlers do not need to run when the update is a full document replacement.
    // An empty updateDoc implies a no-op update and is not a valid oplog entry.
    const bool needsSpecialHandling = !updateDoc.isEmpty() &&
        (update_oplog_entry::extractUpdateType(updateDoc) !=
         update_oplog_entry::UpdateType::kReplacement);
    if (needsSpecialHandling &&
        args.coll->ns() == NamespaceString::kShardConfigCollectionsNamespace) {
        // Notification of routing table changes are only needed on secondaries
        if (isStandaloneOrPrimary(opCtx)) {
            return;
        }

        // This logic runs on updates to the shard's persisted cache of the config server's
        // config.collections collection.
        //
        // If an update occurs to the 'lastRefreshedCollectionPlacementVersion' field it notifies
        // the catalog cache loader of a new placement version and clears the routing table so the
        // next caller with routing information will provoke a routing table refresh.
        //
        // When 'lastRefreshedCollectionPlacementVersion' is in 'update', it means that a chunk
        // metadata refresh has finished being applied to the collection's locally persisted
        // metadata store.
        //
        // If an update occurs to the 'enterCriticalSectionSignal' field, simply clear the routing
        // table immediately. This will provoke the next secondary caller to refresh through the
        // primary, blocking behind the critical section.

        // Extract which user collection was updated
        const auto updatedNss([&] {
            std::string coll;
            fassert(40477,
                    bsonExtractStringField(
                        args.updateArgs->criteria, ShardCollectionType::kNssFieldName, &coll));
            return NamespaceString(coll);
        }());

        auto enterCriticalSectionFieldNewVal = update_oplog_entry::extractNewValueForField(
            updateDoc, ShardCollectionType::kEnterCriticalSectionCounterFieldName);
        auto refreshingFieldNewVal = update_oplog_entry::extractNewValueForField(
            updateDoc, ShardCollectionType::kRefreshingFieldName);

        // Need the WUOW to retain the lock for CollectionPlacementVersionLogOpHandler::commit().
        // TODO SERVER-58223: evaluate whether this is safe or whether acquiring the lock can block.
        AllowLockAcquisitionOnTimestampedUnitOfWork allowLockAcquisition(opCtx->lockState());
        AutoGetCollection autoColl(opCtx, updatedNss, MODE_IX);
        if (refreshingFieldNewVal.isBoolean() && !refreshingFieldNewVal.boolean()) {
            opCtx->recoveryUnit()->registerChange(
                std::make_unique<CollectionPlacementVersionLogOpHandler>(
                    updatedNss, /* droppingCollection */ false));
        }

        if (enterCriticalSectionFieldNewVal.ok()) {
            // Force subsequent uses of the namespace to refresh the filtering metadata so they
            // can synchronize with any work happening on the primary (e.g., migration critical
            // section).
            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(opCtx, updatedNss)
                ->clearFilteringMetadata(opCtx);
        }
    }

    if (needsSpecialHandling &&
        args.coll->ns() == NamespaceString::kShardConfigDatabasesNamespace) {
        // Notification of routing table changes are only needed on secondaries
        if (isStandaloneOrPrimary(opCtx)) {
            return;
        }

        // This logic runs on updates to the shard's persisted cache of the config server's
        // config.databases collection.
        //
        // If an update occurs to the 'enterCriticalSectionSignal' field, clear the routing
        // table immediately. This will provoke the next secondary caller to refresh through the
        // primary, blocking behind the critical section.

        // Extract which database was updated
        // TODO SERVER-67789 Change to extract DatabaseName obj, and use when locking db below.
        std::string db;
        fassert(40478,
                bsonExtractStringField(
                    args.updateArgs->criteria, ShardDatabaseType::kNameFieldName, &db));

        auto enterCriticalSectionCounterFieldNewVal = update_oplog_entry::extractNewValueForField(
            updateDoc, ShardDatabaseType::kEnterCriticalSectionCounterFieldName);

        if (enterCriticalSectionCounterFieldNewVal.ok()) {
            // TODO SERVER-58223: evaluate whether this is safe or whether acquiring the lock can
            // block.
            AllowLockAcquisitionOnTimestampedUnitOfWork allowLockAcquisition(opCtx->lockState());

            DatabaseName dbName = DatabaseNameUtil::deserialize(boost::none, db);
            AutoGetDb autoDb(opCtx, dbName, MODE_X);
            auto scopedDss =
                DatabaseShardingState::assertDbLockedAndAcquireExclusive(opCtx, dbName);
            scopedDss->clearDbInfo(opCtx);
        }
    }

    if (args.coll->ns() == NamespaceString::kCollectionCriticalSectionsNamespace &&
        !sharding_recovery_util::inRecoveryMode(opCtx)) {
        const auto collCSDoc = CollectionCriticalSectionDocument::parse(
            IDLParserContext("ShardServerOpObserver"), args.updateArgs->updatedDoc);
        invariant(collCSDoc.getBlockReads());

        opCtx->recoveryUnit()->onCommit(
            [updatedNss = collCSDoc.getNss(), reason = collCSDoc.getReason().getOwned()](
                OperationContext* opCtx, boost::optional<Timestamp>) {
                if (nsIsDbOnly(NamespaceStringUtil::serialize(updatedNss))) {
                    boost::optional<AutoGetDb> lockDbIfNotPrimary;
                    if (!isStandaloneOrPrimary(opCtx)) {
                        lockDbIfNotPrimary.emplace(opCtx, updatedNss.dbName(), MODE_IX);
                    }

                    // TODO (SERVER-71444): Fix to be interruptible or document exception.
                    UninterruptibleLockGuard noInterrupt(opCtx->lockState());  // NOLINT.
                    auto scopedDss = DatabaseShardingState::assertDbLockedAndAcquireExclusive(
                        opCtx, updatedNss.dbName());
                    scopedDss->enterCriticalSectionCommitPhase(opCtx, reason);
                } else {
                    boost::optional<AutoGetCollection> lockCollectionIfNotPrimary;
                    if (!isStandaloneOrPrimary(opCtx)) {
                        lockCollectionIfNotPrimary.emplace(
                            opCtx,
                            updatedNss,
                            fixLockModeForSystemDotViewsChanges(updatedNss, MODE_IX),
                            AutoGetCollection::Options{}.viewMode(
                                auto_get_collection::ViewMode::kViewsPermitted));
                    }

                    // TODO (SERVER-71444): Fix to be interruptible or document exception.
                    UninterruptibleLockGuard noInterrupt(opCtx->lockState());  // NOLINT.
                    auto scopedCsr =
                        CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                            opCtx, updatedNss);
                    scopedCsr->enterCriticalSectionCommitPhase(reason);
                }
            });
    }

    const auto& nss = args.coll->ns();
    if (nss == NamespaceString::kServerConfigurationNamespace) {
        auto idElem = args.updateArgs->criteria["_id"];
        auto shardName = updateDoc["shardName"];
        if (idElem && idElem.str() == ShardIdentityType::IdName && shardName) {
            auto updatedShardIdentityDoc = args.updateArgs->updatedDoc;
            auto shardIdentityDoc = uassertStatusOK(
                ShardIdentityType::fromShardIdentityDocument(updatedShardIdentityDoc));
            uassertStatusOK(shardIdentityDoc.validate());
        }
    }
}

void ShardServerOpObserver::aboutToDelete(OperationContext* opCtx,
                                          const CollectionPtr& coll,
                                          BSONObj const& doc,
                                          OplogDeleteEntryArgs* args,
                                          OpStateAccumulator* opAccumulator) {

    if (coll->ns() == NamespaceString::kCollectionCriticalSectionsNamespace ||
        coll->ns() == NamespaceString::kRangeDeletionNamespace) {
        documentIdDecoration(args) = doc;
    } else {
        // Extract the _id field from the document. If it does not have an _id, use the
        // document itself as the _id.
        documentIdDecoration(args) = doc["_id"] ? doc["_id"].wrap() : doc;
    }
}

void ShardServerOpObserver::onModifyCollectionShardingIndexCatalog(OperationContext* opCtx,
                                                                   const NamespaceString& nss,
                                                                   const UUID& uuid,
                                                                   BSONObj indexDoc) {
    // If we are in recovery mode (STARTUP or ROLLBACK) let the sharding recovery service to take
    // care of the in-memory state.
    if (sharding_recovery_util::inRecoveryMode(opCtx)) {
        return;
    }
    LOGV2_DEBUG(6712303,
                1,
                "Updating sharding in-memory state onModifyCollectionShardingIndexCatalog",
                "indexDoc"_attr = indexDoc);
    auto indexCatalogOplog = ShardingIndexCatalogOplogEntry::parse(
        IDLParserContext("onModifyCollectionShardingIndexCatalogCtx"), indexDoc);
    switch (indexCatalogOplog.getOp()) {
        case ShardingIndexCatalogOpEnum::insert: {
            auto indexEntry = ShardingIndexCatalogInsertEntry::parse(
                IDLParserContext("OplogModifyCatalogEntryContext"), indexDoc);
            opCtx->recoveryUnit()->onCommit([nss, indexEntry](OperationContext* opCtx,
                                                              boost::optional<Timestamp>) {
                auto scsr = CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                    opCtx, nss);
                scsr->addIndex(
                    opCtx,
                    indexEntry.getI(),
                    {indexEntry.getI().getCollectionUUID(), indexEntry.getI().getLastmod()});
            });
            break;
        }
        case ShardingIndexCatalogOpEnum::remove: {
            auto removeEntry = ShardingIndexCatalogRemoveEntry::parse(
                IDLParserContext("OplogModifyCatalogEntryContext"), indexDoc);
            opCtx->recoveryUnit()->onCommit([nss, removeEntry](OperationContext* opCtx,
                                                               boost::optional<Timestamp>) {
                auto scsr = CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                    opCtx, nss);
                scsr->removeIndex(opCtx,
                                  removeEntry.getName().toString(),
                                  {removeEntry.getUuid(), removeEntry.getLastmod()});
            });
            break;
        }
        case ShardingIndexCatalogOpEnum::replace: {
            auto replaceEntry = ShardingIndexCatalogReplaceEntry::parse(
                IDLParserContext("OplogModifyCatalogEntryContext"), indexDoc);
            opCtx->recoveryUnit()->onCommit([nss, replaceEntry](OperationContext* opCtx,
                                                                boost::optional<Timestamp>) {
                auto scsr = CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                    opCtx, nss);
                scsr->replaceIndexes(opCtx,
                                     replaceEntry.getI(),
                                     {replaceEntry.getUuid(), replaceEntry.getLastmod()});
            });
            break;
        }
        case ShardingIndexCatalogOpEnum::clear:
            opCtx->recoveryUnit()->onCommit([nss](OperationContext* opCtx,
                                                  boost::optional<Timestamp>) {
                auto scsr = CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                    opCtx, nss);
                scsr->clearIndexes(opCtx);
            });

            break;
        case ShardingIndexCatalogOpEnum::drop: {
            opCtx->recoveryUnit()->onCommit([nss](OperationContext* opCtx,
                                                  boost::optional<Timestamp>) {
                auto scsr = CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                    opCtx, nss);
                scsr->clearIndexes(opCtx);
            });

            break;
        }
        case ShardingIndexCatalogOpEnum::rename: {
            auto renameEntry = ShardingIndexCatalogRenameEntry::parse(
                IDLParserContext("OplogModifyCatalogEntryContext"), indexDoc);
            opCtx->recoveryUnit()->onCommit([renameEntry](OperationContext* opCtx,
                                                          boost::optional<Timestamp>) {
                std::vector<IndexCatalogType> fromIndexes;
                boost::optional<UUID> uuid;
                {
                    auto fromCSR =
                        CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                            opCtx, renameEntry.getFromNss());
                    auto indexCache = fromCSR->getIndexesInCritSec(opCtx);
                    indexCache->forEachGlobalIndex([&](const auto& index) {
                        fromIndexes.push_back(index);
                        return true;
                    });
                    uuid.emplace(indexCache->getCollectionIndexes().uuid());

                    fromCSR->clearIndexes(opCtx);
                }
                auto toCSR = CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                    opCtx, renameEntry.getToNss());
                uassert(7079505,
                        format(FMT_STRING("The critical section for collection {} must be taken in "
                                          "order to execute this command"),
                               renameEntry.getToNss().toStringForErrorMsg()),
                        toCSR->getCriticalSectionSignal(opCtx,
                                                        ShardingMigrationCriticalSection::kWrite));
                toCSR->replaceIndexes(opCtx, fromIndexes, {*uuid, renameEntry.getLastmod()});
            });
            break;
        }
        default:
            MONGO_UNREACHABLE;
    }
}

void ShardServerOpObserver::onDelete(OperationContext* opCtx,
                                     const CollectionPtr& coll,
                                     StmtId stmtId,
                                     const OplogDeleteEntryArgs& args,
                                     OpStateAccumulator* opAccumulator) {
    const auto& nss = coll->ns();
    auto& documentId = documentIdDecoration(args);
    invariant(!documentId.isEmpty());

    if (nss == NamespaceString::kShardConfigCollectionsNamespace) {
        onConfigDeleteInvalidateCachedCollectionMetadataAndNotify(opCtx, documentId);
    }

    if (nss == NamespaceString::kShardConfigDatabasesNamespace) {
        if (isStandaloneOrPrimary(opCtx)) {
            return;
        }

        // Extract which database entry is being deleted from the _id field.
        // TODO SERVER-67789 Change to extract DatabaseName obj, and use when locking db below.
        std::string deletedDatabase;
        fassert(50772,
                bsonExtractStringField(
                    documentId, ShardDatabaseType::kNameFieldName, &deletedDatabase));

        // TODO SERVER-58223: evaluate whether this is safe or whether acquiring the lock can block.
        AllowLockAcquisitionOnTimestampedUnitOfWork allowLockAcquisition(opCtx->lockState());

        DatabaseName dbName = DatabaseNameUtil::deserialize(boost::none, deletedDatabase);
        AutoGetDb autoDb(opCtx, dbName, MODE_X);
        auto scopedDss = DatabaseShardingState::assertDbLockedAndAcquireExclusive(opCtx, dbName);
        scopedDss->clearDbInfo(opCtx);
    }

    if (nss == NamespaceString::kServerConfigurationNamespace) {
        if (auto idElem = documentId.firstElement()) {
            auto idStr = idElem.str();
            if (idStr == ShardIdentityType::IdName) {
                if (!repl::ReplicationCoordinator::get(opCtx)->getMemberState().rollback()) {
                    uasserted(40070,
                              "cannot delete shardIdentity document while in --shardsvr mode");
                } else {
                    LOGV2_WARNING(23779,
                                  "Shard identity document rolled back.  Will shut down after "
                                  "finishing rollback.");
                    ShardIdentityRollbackNotifier::get(opCtx)->recordThatRollbackHappened();
                }
            }
        }
    }

    if (nss == NamespaceString::kCollectionCriticalSectionsNamespace &&
        !sharding_recovery_util::inRecoveryMode(opCtx)) {
        const auto& deletedDoc = documentId;
        const auto collCSDoc = CollectionCriticalSectionDocument::parse(
            IDLParserContext("ShardServerOpObserver"), deletedDoc);

        opCtx->recoveryUnit()->onCommit(
            [deletedNss = collCSDoc.getNss(), reason = collCSDoc.getReason().getOwned()](
                OperationContext* opCtx, boost::optional<Timestamp>) {
                if (nsIsDbOnly(NamespaceStringUtil::serialize(deletedNss))) {
                    boost::optional<AutoGetDb> lockDbIfNotPrimary;
                    if (!isStandaloneOrPrimary(opCtx)) {
                        lockDbIfNotPrimary.emplace(opCtx, deletedNss.dbName(), MODE_IX);
                    }

                    // TODO (SERVER-71444): Fix to be interruptible or document exception.
                    UninterruptibleLockGuard noInterrupt(opCtx->lockState());  // NOLINT.
                    auto scopedDss = DatabaseShardingState::assertDbLockedAndAcquireExclusive(
                        opCtx, deletedNss.dbName());

                    // Secondary nodes must clear the database metadata before releasing the
                    // in-memory critical section.
                    if (!isStandaloneOrPrimary(opCtx)) {
                        scopedDss->clearDbInfo(opCtx);
                    }

                    scopedDss->exitCriticalSection(opCtx, reason);
                } else {
                    boost::optional<AutoGetCollection> lockCollectionIfNotPrimary;
                    if (!isStandaloneOrPrimary(opCtx)) {
                        lockCollectionIfNotPrimary.emplace(
                            opCtx,
                            deletedNss,
                            fixLockModeForSystemDotViewsChanges(deletedNss, MODE_IX),
                            AutoGetCollection::Options{}.viewMode(
                                auto_get_collection::ViewMode::kViewsPermitted));
                    }

                    // TODO (SERVER-71444): Fix to be interruptible or document exception.
                    UninterruptibleLockGuard noInterrupt(opCtx->lockState());  // NOLINT.
                    auto scopedCsr =
                        CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                            opCtx, deletedNss);

                    // Secondary nodes must clear the collection filtering metadata before releasing
                    // the in-memory critical section.
                    if (!isStandaloneOrPrimary(opCtx)) {
                        scopedCsr->clearFilteringMetadata(opCtx);
                    }

                    scopedCsr->exitCriticalSection(reason);
                }
            });
    }

    if (nss == NamespaceString::kRangeDeletionNamespace) {
        const auto& deletedDoc = documentId;

        const auto numOrphanDocs = [&] {
            auto numOrphanDocsElem = update_oplog_entry::extractNewValueForField(
                deletedDoc, RangeDeletionTask::kNumOrphanDocsFieldName);
            return numOrphanDocsElem.exactNumberLong();
        }();

        auto collUuid = [&] {
            BSONElement collUuidElem;
            uassertStatusOK(bsonExtractField(
                documentId, RangeDeletionTask::kCollectionUuidFieldName, &collUuidElem));
            return uassertStatusOK(UUID::parse(std::move(collUuidElem)));
        }();

        opCtx->recoveryUnit()->onCommit([collUuid = std::move(collUuid), numOrphanDocs](
                                            OperationContext* opCtx, boost::optional<Timestamp>) {
            BalancerStatsRegistry::get(opCtx)->onRangeDeletionTaskDeletion(collUuid, numOrphanDocs);
        });
    }
}

void ShardServerOpObserver::onCreateCollection(OperationContext* opCtx,
                                               const CollectionPtr& coll,
                                               const NamespaceString& collectionName,
                                               const CollectionOptions& options,
                                               const BSONObj& idIndex,
                                               const OplogSlot& createOpTime,
                                               bool fromMigrate) {
    // Only the shard primay nodes control the collection creation and secondaries just follow
    // Secondaries CSR will be the defaulted one (UNKNOWN in most of the cases)
    if (!opCtx->writesAreReplicated()) {
        return;
    }

    // Collections which are always UNSHARDED have a fixed CSS, which never changes, so we don't
    // need to do anything
    if (collectionName.isNamespaceAlwaysUnsharded()) {
        return;
    }

    // Temp collections are always UNSHARDED
    if (options.temp) {
        CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(opCtx, collectionName)
            ->setFilteringMetadata(opCtx, CollectionMetadata());
        return;
    }

    const auto& oss = OperationShardingState::get(opCtx);
    uassert(CannotImplicitlyCreateCollectionInfo(collectionName),
            "Implicit collection creation on a sharded cluster must go through the "
            "CreateCollectionCoordinator",
            oss._allowCollectionCreation);

    // If the check above passes, this means the collection doesn't exist and is being created and
    // that the caller will be responsible to eventially set the proper placement version.
    auto scopedCsr =
        CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(opCtx, collectionName);
    if (oss._forceCSRAsUnknownAfterCollectionCreation) {
        scopedCsr->clearFilteringMetadata(opCtx);
    } else if (!scopedCsr->getCurrentMetadataIfKnown()) {
        scopedCsr->setFilteringMetadata(opCtx, CollectionMetadata());
    }
}

repl::OpTime ShardServerOpObserver::onDropCollection(OperationContext* opCtx,
                                                     const NamespaceString& collectionName,
                                                     const UUID& uuid,
                                                     std::uint64_t numRecords,
                                                     const CollectionDropType dropType,
                                                     bool markFromMigrate) {
    if (collectionName == NamespaceString::kServerConfigurationNamespace) {
        // Dropping system collections is not allowed for end users
        invariant(!opCtx->writesAreReplicated());
        invariant(repl::ReplicationCoordinator::get(opCtx)->getMemberState().rollback());

        // Can't confirm whether there was a ShardIdentity document or not yet, so assume there was
        // one and shut down the process to clear the in-memory sharding state
        LOGV2_WARNING(23780,
                      "admin.system.version collection rolled back. Will shut down after finishing "
                      "rollback");

        ShardIdentityRollbackNotifier::get(opCtx)->recordThatRollbackHappened();
    }

    return {};
}

void ShardServerOpObserver::onCreateIndex(OperationContext* opCtx,
                                          const NamespaceString& nss,
                                          const UUID& uuid,
                                          BSONObj indexDoc,
                                          bool fromMigrate) {
    abortOngoingMigrationIfNeeded(opCtx, nss);
}

void ShardServerOpObserver::onStartIndexBuild(OperationContext* opCtx,
                                              const NamespaceString& nss,
                                              const UUID& collUUID,
                                              const UUID& indexBuildUUID,
                                              const std::vector<BSONObj>& indexes,
                                              bool fromMigrate) {
    abortOngoingMigrationIfNeeded(opCtx, nss);
}

void ShardServerOpObserver::onStartIndexBuildSinglePhase(OperationContext* opCtx,
                                                         const NamespaceString& nss) {
    abortOngoingMigrationIfNeeded(opCtx, nss);
}

void ShardServerOpObserver::onAbortIndexBuildSinglePhase(OperationContext* opCtx,
                                                         const NamespaceString& nss) {
    abortOngoingMigrationIfNeeded(opCtx, nss);
}

void ShardServerOpObserver::onDropIndex(OperationContext* opCtx,
                                        const NamespaceString& nss,
                                        const UUID& uuid,
                                        const std::string& indexName,
                                        const BSONObj& indexInfo) {
    abortOngoingMigrationIfNeeded(opCtx, nss);
};

void ShardServerOpObserver::onCollMod(OperationContext* opCtx,
                                      const NamespaceString& nss,
                                      const UUID& uuid,
                                      const BSONObj& collModCmd,
                                      const CollectionOptions& oldCollOptions,
                                      boost::optional<IndexCollModInfo> indexInfo) {
    abortOngoingMigrationIfNeeded(opCtx, nss);
};

void ShardServerOpObserver::onReplicationRollback(OperationContext* opCtx,
                                                  const RollbackObserverInfo& rbInfo) {
    ShardingRecoveryService::get(opCtx)->recoverStates(opCtx, rbInfo.rollbackNamespaces);
}


}  // namespace mongo
