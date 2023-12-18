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

#include "mongo/db/catalog/drop_indexes.h"

#include <boost/algorithm/string/join.hpp>
#include <boost/cstdint.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
// IWYU pragma: no_include "ext/alloc_traits.h"
#include <algorithm>
#include <cstdint>
#include <memory>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/clustered_collection_options_gen.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/index_builds_coordinator.h"
#include "mongo/db/locker_api.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/repl/repl_settings.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl_set_member_in_standalone_mode.h"
#include "mongo/db/s/collection_sharding_state.h"
#include "mongo/db/s/database_sharding_state.h"
#include "mongo/db/s/scoped_collection_metadata.h"
#include "mongo/db/s/shard_key_index_util.h"
#include "mongo/db/server_feature_flags_gen.h"
#include "mongo/db/server_options.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/overloaded_visitor.h"  // IWYU pragma: keep
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kCommand

namespace mongo {
namespace {

MONGO_FAIL_POINT_DEFINE(hangAfterAbortingIndexes);

// Field name in dropIndexes command for indexes to drop.
constexpr auto kIndexFieldName = "index"_sd;

Status checkView(OperationContext* opCtx,
                 const NamespaceString& nss,
                 const CollectionPtr& collection) {
    if (!collection) {
        if (CollectionCatalog::get(opCtx)->lookupView(opCtx, nss)) {
            return Status(ErrorCodes::CommandNotSupportedOnView,
                          str::stream()
                              << "Cannot drop indexes on view " << nss.toStringForErrorMsg());
        }
        return Status(ErrorCodes::NamespaceNotFound,
                      str::stream() << "ns not found " << nss.toStringForErrorMsg());
    }
    return Status::OK();
}

Status checkReplState(OperationContext* opCtx,
                      NamespaceStringOrUUID dbAndUUID,
                      const CollectionPtr& collection) {
    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    auto canAcceptWrites = replCoord->canAcceptWritesFor(opCtx, dbAndUUID);
    bool writesAreReplicatedAndNotPrimary = opCtx->writesAreReplicated() && !canAcceptWrites;

    if (writesAreReplicatedAndNotPrimary) {
        return Status(ErrorCodes::NotWritablePrimary,
                      str::stream() << "Not primary while dropping indexes on database "
                                    << dbAndUUID.dbName().toStringForErrorMsg()
                                    << " with collection " << dbAndUUID.uuid());
    }

    // Disallow index drops on drop-pending namespaces (system.drop.*) if we are primary.
    auto isPrimary = replCoord->getSettings().isReplSet() && canAcceptWrites;
    const auto& nss = collection->ns();
    if (isPrimary && nss.isDropPendingNamespace()) {
        return Status(ErrorCodes::NamespaceNotFound,
                      str::stream() << "Cannot drop indexes on drop-pending namespace "
                                    << nss.toStringForErrorMsg() << " in database "
                                    << dbAndUUID.dbName().toStringForErrorMsg() << " with uuid "
                                    << dbAndUUID.uuid());
    }

    return Status::OK();
}

/**
 * Validates the key pattern passed through the command.
 */
StatusWith<const IndexDescriptor*> getDescriptorByKeyPattern(OperationContext* opCtx,
                                                             const IndexCatalog* indexCatalog,
                                                             const BSONObj& keyPattern) {
    std::vector<const IndexDescriptor*> indexes;
    indexCatalog->findIndexesByKeyPattern(opCtx,
                                          keyPattern,
                                          IndexCatalog::InclusionPolicy::kReady |
                                              IndexCatalog::InclusionPolicy::kUnfinished |
                                              IndexCatalog::InclusionPolicy::kFrozen,
                                          &indexes);
    if (indexes.empty()) {
        return Status(ErrorCodes::IndexNotFound,
                      str::stream() << "can't find index with key: " << keyPattern);
    } else if (indexes.size() > 1) {
        return Status(ErrorCodes::AmbiguousIndexKeyPattern,
                      str::stream() << indexes.size() << " indexes found for key: " << keyPattern
                                    << ", identify by name instead."
                                    << " Conflicting indexes: " << indexes[0]->infoObj() << ", "
                                    << indexes[1]->infoObj());
    }

    const IndexDescriptor* desc = indexes[0];
    if (desc->isIdIndex()) {
        return Status(ErrorCodes::InvalidOptions, "cannot drop _id index");
    }

    if (desc->indexName() == "*") {
        // Dropping an index named '*' results in an drop-index oplog entry with a name of '*',
        // which in 3.6 and later is interpreted by replication as meaning "drop all indexes on
        // this collection".
        return Status(ErrorCodes::InvalidOptions,
                      "cannot drop an index named '*' by key pattern.  You must drop the "
                      "entire collection, drop all indexes on the collection by using an index "
                      "name of '*', or downgrade to 3.4 to drop only this index.");
    }

    return desc;
}

/**
 * It is illegal to drop a collection's clusteredIndex.
 *
 * Returns true if 'index' is or contains the clusteredIndex.
 */
bool containsClusteredIndex(const CollectionPtr& collection, const IndexArgument& index) {
    invariant(collection && collection->isClustered());

    auto clusteredIndexSpec = collection->getClusteredInfo()->getIndexSpec();
    return visit(OverloadedVisitor{[&](const std::string& indexName) -> bool {
                                       // While the clusteredIndex's name is optional during user
                                       // creation, it should always be filled in by default on the
                                       // collection object.
                                       auto clusteredIndexName = clusteredIndexSpec.getName();
                                       invariant(clusteredIndexName.has_value());

                                       return clusteredIndexName.value() == indexName;
                                   },
                                   [&](const std::vector<std::string>& indexNames) -> bool {
                                       // While the clusteredIndex's name is optional during user
                                       // creation, it should always be filled in by default on the
                                       // collection object.
                                       auto clusteredIndexName = clusteredIndexSpec.getName();
                                       invariant(clusteredIndexName.has_value());

                                       return std::find(indexNames.begin(),
                                                        indexNames.end(),
                                                        clusteredIndexName.value()) !=
                                           indexNames.end();
                                   },
                                   [&](const BSONObj& indexKey) -> bool {
                                       return clusteredIndexSpec.getKey().woCompare(indexKey) == 0;
                                   }},
                 index);
}

/**
 * Returns a list of index names that the caller requested to abort/drop. Requires a collection lock
 * to be held to look up the index name from the key pattern.
 */
StatusWith<std::vector<std::string>> getIndexNames(OperationContext* opCtx,
                                                   const CollectionPtr& collection,
                                                   const IndexArgument& index) {
    invariant(
        shard_role_details::getLocker(opCtx)->isCollectionLockedForMode(collection->ns(), MODE_IX));

    return visit(
        OverloadedVisitor{
            [](const std::string& arg) -> StatusWith<std::vector<std::string>> { return {{arg}}; },
            [](const std::vector<std::string>& arg) -> StatusWith<std::vector<std::string>> {
                return arg;
            },
            [&](const BSONObj& arg) -> StatusWith<std::vector<std::string>> {
                auto swDescriptor =
                    getDescriptorByKeyPattern(opCtx, collection->getIndexCatalog(), arg);
                if (!swDescriptor.isOK()) {
                    return swDescriptor.getStatus();
                }
                return {{swDescriptor.getValue()->indexName()}};
            }},
        index);
}

/**
 * Attempts to abort a single index builder that is responsible for all the index names passed in.
 */
std::vector<UUID> abortIndexBuildByIndexNames(OperationContext* opCtx,
                                              UUID collectionUUID,
                                              std::vector<std::string> indexNames) {

    boost::optional<UUID> buildUUID =
        IndexBuildsCoordinator::get(opCtx)->abortIndexBuildByIndexNames(
            opCtx, collectionUUID, indexNames, std::string("dropIndexes command"));
    if (buildUUID) {
        return {*buildUUID};
    }
    return {};
}

/**
 * Drops single index given a descriptor.
 */
Status dropIndexByDescriptor(OperationContext* opCtx,
                             Collection* collection,
                             IndexCatalog* indexCatalog,
                             IndexCatalogEntry* entry) {
    if (entry->descriptor()->isIdIndex()) {
        return Status(ErrorCodes::InvalidOptions, "cannot drop _id index");
    }

    // Support dropping unfinished indexes, but only if the index is 'frozen'. These indexes only
    // exist in standalone mode.
    if (entry->isFrozen()) {
        invariant(!entry->isReady());
        invariant(getReplSetMemberInStandaloneMode(opCtx->getServiceContext()));
        // Return here. No need to fall through to op observer on standalone.
        return indexCatalog->dropUnfinishedIndex(opCtx, collection, entry);
    }

    // Do not allow dropping unfinished indexes that are not frozen.
    if (!entry->isReady()) {
        return Status(ErrorCodes::IndexNotFound,
                      str::stream() << "can't drop unfinished index with name: "
                                    << entry->descriptor()->indexName());
    }

    // Log the operation first, which reserves an optime in the oplog and sets the timestamp for
    // future writes. This guarantees the durable catalog's metadata change to share the same
    // timestamp when dropping the index below.
    opCtx->getServiceContext()->getOpObserver()->onDropIndex(opCtx,
                                                             collection->ns(),
                                                             collection->uuid(),
                                                             entry->descriptor()->indexName(),
                                                             entry->descriptor()->infoObj());

    auto s = indexCatalog->dropIndexEntry(opCtx, collection, entry);
    if (!s.isOK()) {
        return s;
    }

    return Status::OK();
}

/**
 * Aborts all the index builders on the collection if the first element in 'indexesToAbort' is "*",
 * otherwise this attempts to abort a single index builder building the given index names.
 */
std::vector<UUID> abortActiveIndexBuilders(OperationContext* opCtx,
                                           const NamespaceString& collectionNs,
                                           const UUID& collectionUUID,
                                           const std::vector<std::string>& indexNames) {
    if (indexNames.empty()) {
        return {};
    }

    if (indexNames.front() == "*") {
        return IndexBuildsCoordinator::get(opCtx)->abortCollectionIndexBuilds(
            opCtx, collectionNs, collectionUUID, "dropIndexes command");
    }

    return abortIndexBuildByIndexNames(opCtx, collectionUUID, indexNames);
}

void dropReadyIndexes(OperationContext* opCtx,
                      Collection* collection,
                      const std::vector<std::string>& indexNames,
                      DropIndexesReply* reply,
                      bool forceDropShardKeyIndex) {
    invariant(
        shard_role_details::getLocker(opCtx)->isCollectionLockedForMode(collection->ns(), MODE_X));

    if (indexNames.empty()) {
        return;
    }

    IndexCatalog* indexCatalog = collection->getIndexCatalog();
    auto collDescription =
        CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, collection->ns())
            ->getCollectionDescription(opCtx);

    if (indexNames.front() == "*") {
        if (collDescription.isSharded() && !forceDropShardKeyIndex) {
            indexCatalog->dropIndexes(
                opCtx,
                collection,
                [&](const IndexDescriptor* desc) {
                    if (desc->isIdIndex()) {
                        return false;
                    }
                    // For any index that is compatible with the shard key, if
                    // gFeatureFlagShardKeyIndexOptionalHashedSharding is enabled and
                    // the shard key is hashed, allow users to drop the hashed index. Note
                    // skipDroppingHashedShardKeyIndex is used in some tests to prevent dropIndexes
                    // from dropping the hashed shard key index so we can continue to test chunk
                    // migration with hashed sharding. Otherwise, dropIndexes with '*' would drop
                    // the index and prevent chunk migration from running.
                    const auto& shardKey = collDescription.getShardKeyPattern();
                    const bool skipDropIndex = skipDroppingHashedShardKeyIndex ||
                        !(gFeatureFlagShardKeyIndexOptionalHashedSharding.isEnabled(
                              serverGlobalParams.featureCompatibility.acquireFCVSnapshot()) &&
                          shardKey.isHashedPattern());
                    if (isCompatibleWithShardKey(opCtx,
                                                 CollectionPtr(collection),
                                                 desc->getEntry(),
                                                 shardKey.toBSON(),
                                                 false /* requiresSingleKey */) &&
                        skipDropIndex) {
                        return false;
                    }

                    return true;
                },
                [opCtx, collection](const IndexDescriptor* desc) {
                    opCtx->getServiceContext()->getOpObserver()->onDropIndex(opCtx,
                                                                             collection->ns(),
                                                                             collection->uuid(),
                                                                             desc->indexName(),
                                                                             desc->infoObj());
                });

            reply->setMsg("non-_id indexes and non-shard key indexes dropped for collection"_sd);
        } else {
            indexCatalog->dropAllIndexes(
                opCtx, collection, false, [opCtx, collection](const IndexDescriptor* desc) {
                    opCtx->getServiceContext()->getOpObserver()->onDropIndex(opCtx,
                                                                             collection->ns(),
                                                                             collection->uuid(),
                                                                             desc->indexName(),
                                                                             desc->infoObj());
                });

            reply->setMsg("non-_id indexes dropped for collection"_sd);
        }
        return;
    }

    for (const auto& indexName : indexNames) {
        if (collDescription.isSharded()) {
            uassert(
                ErrorCodes::CannotDropShardKeyIndex,
                "Cannot drop the only compatible index for this collection's shard key",
                !isLastNonHiddenRangedShardKeyIndex(
                    opCtx, CollectionPtr(collection), indexName, collDescription.getKeyPattern()));
        }

        auto writableEntry = indexCatalog->getWritableEntryByName(
            opCtx,
            indexName,
            IndexCatalog::InclusionPolicy::kReady | IndexCatalog::InclusionPolicy::kUnfinished |
                IndexCatalog::InclusionPolicy::kFrozen);
        if (!writableEntry) {
            uasserted(ErrorCodes::IndexNotFound,
                      str::stream() << "index not found with name [" << indexName << "]");
        }
        uassertStatusOK(dropIndexByDescriptor(opCtx, collection, indexCatalog, writableEntry));
    }
}

void assertNoMovePrimaryInProgress(OperationContext* opCtx, const NamespaceString& nss) {
    try {
        const auto scopedDss =
            DatabaseShardingState::assertDbLockedAndAcquireShared(opCtx, nss.dbName());
        auto scopedCss = CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, nss);

        auto collDesc = scopedCss->getCollectionDescription(opCtx);
        collDesc.throwIfReshardingInProgress(nss);

        // Only collections that are not registered in the sharding catalog are affected by
        // movePrimary
        if (!collDesc.hasRoutingTable()) {
            if (scopedDss->isMovePrimaryInProgress()) {
                LOGV2(4976500, "assertNoMovePrimaryInProgress", logAttrs(nss));

                uasserted(ErrorCodes::MovePrimaryInProgress,
                          "movePrimary is in progress for namespace " + nss.toStringForErrorMsg());
            }
        }
    } catch (const DBException& ex) {
        if (ex.toStatus() != ErrorCodes::MovePrimaryInProgress) {
            LOGV2(4976501, "Error when getting collection description", "what"_attr = ex.what());
            return;
        }
        throw;
    }
}

}  // namespace

DropIndexesReply dropIndexes(OperationContext* opCtx,
                             const NamespaceString& nss,
                             const boost::optional<UUID>& expectedUUID,
                             const IndexArgument& index) {
    // We only need to hold an intent lock to send abort signals to the active index builder(s) we
    // intend to abort.
    boost::optional<AutoGetCollection> collection;
    collection.emplace(
        opCtx, nss, MODE_IX, AutoGetCollection::Options{}.expectedUUID(expectedUUID));

    uassertStatusOK(checkView(opCtx, nss, collection->getCollection()));

    const UUID collectionUUID = (*collection)->uuid();
    const NamespaceStringOrUUID dbAndUUID = {nss.dbName(), collectionUUID};
    uassertStatusOK(checkReplState(opCtx, dbAndUUID, collection->getCollection()));
    if (!serverGlobalParams.quiet.load()) {
        LOGV2(51806,
              "CMD: dropIndexes",
              logAttrs(nss),
              "uuid"_attr = collectionUUID,
              "indexes"_attr = visit(OverloadedVisitor{[](const std::string& arg) { return arg; },
                                                       [](const std::vector<std::string>& arg) {
                                                           return boost::algorithm::join(arg, ",");
                                                       },
                                                       [](const BSONObj& arg) {
                                                           return arg.toString();
                                                       }},
                                     index));
    }

    if ((*collection)->isClustered() &&
        containsClusteredIndex(collection->getCollection(), index)) {
        uasserted(5979800, "It is illegal to drop the clusteredIndex");
    }

    DropIndexesReply reply;
    reply.setNIndexesWas((*collection)->getIndexCatalog()->numIndexesTotal());

    const bool isWildcard = holds_alternative<std::string>(index) && get<std::string>(index) == "*";

    IndexBuildsCoordinator* indexBuildsCoord = IndexBuildsCoordinator::get(opCtx);

    // When releasing the collection lock to send the abort signal to the index builders, it's
    // possible for new index builds to start. Keep aborting in-progress index builds if they
    // satisfy the caller's input.
    std::vector<UUID> abortedIndexBuilders;
    std::vector<std::string> indexNames;
    while (true) {
        indexNames = uassertStatusOK(getIndexNames(opCtx, collection->getCollection(), index));

        // Copy the namespace and UUID before dropping locks.
        auto collUUID = (*collection)->uuid();
        auto collNs = (*collection)->ns();

        // Release locks before aborting index builds. The helper will acquire locks on our
        // behalf.
        collection = boost::none;

        // Send the abort signal to any index builders that match the users request. Waits until
        // all aborted builders complete.
        auto justAborted = abortActiveIndexBuilders(opCtx, collNs, collUUID, indexNames);
        abortedIndexBuilders.insert(
            abortedIndexBuilders.end(), justAborted.begin(), justAborted.end());

        if (MONGO_unlikely(hangAfterAbortingIndexes.shouldFail())) {
            LOGV2(4731900, "Hanging on hangAfterAbortingIndexes fail point");
            hangAfterAbortingIndexes.pauseWhileSet();
        }

        // Abandon the snapshot as the index catalog will compare the in-memory state to the
        // disk state, which may have changed when we released the lock temporarily.
        opCtx->recoveryUnit()->abandonSnapshot();

        // Take an exclusive lock on the collection now to be able to perform index catalog
        // writes when removing ready indexes from disk.
        collection.emplace(opCtx, dbAndUUID, MODE_X);

        if (!*collection) {
            uasserted(ErrorCodes::NamespaceNotFound,
                      str::stream()
                          << "Collection '" << nss.toStringForErrorMsg() << "' with UUID "
                          << dbAndUUID.uuid() << " in database "
                          << dbAndUUID.dbName().toStringForErrorMsg() << " does not exist.");
        }

        // The collection could have been renamed when we dropped locks.
        collNs = (*collection)->ns();

        uassertStatusOK(checkReplState(opCtx, dbAndUUID, collection->getCollection()));

        // Check to see if a new index build was started that the caller requested to be
        // aborted.
        bool abortAgain = false;
        if (isWildcard) {
            abortAgain = indexBuildsCoord->inProgForCollection(collectionUUID);
        } else {
            abortAgain = indexBuildsCoord->hasIndexBuilder(opCtx, collectionUUID, indexNames);
        }

        if (!abortAgain) {
            assertNoMovePrimaryInProgress(opCtx, collNs);
            break;
        }
    }

    // Drop any ready indexes that were created while we yielded our locks while aborting using
    // similar index specs.
    if (!isWildcard && !abortedIndexBuilders.empty()) {
        // The index catalog requires that no active index builders are running when dropping ready
        // indexes.
        IndexBuildsCoordinator::get(opCtx)->assertNoIndexBuildInProgForCollection(collectionUUID);
        writeConflictRetry(opCtx, "dropIndexes", dbAndUUID, [&] {
            WriteUnitOfWork wuow(opCtx);

            // This is necessary to check shard version.
            OldClientContext ctx(opCtx, (*collection)->ns());

            // Iterate through all the aborted indexes and drop any indexes that are ready in
            // the index catalog. This would indicate that while we yielded our locks during the
            // abort phase, a new identical index was created.
            auto indexCatalog = collection->getWritableCollection(opCtx)->getIndexCatalog();
            for (const auto& indexName : indexNames) {
                auto collDesc =
                    CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, nss)
                        ->getCollectionDescription(opCtx);
                if (collDesc.isSharded()) {
                    uassert(ErrorCodes::CannotDropShardKeyIndex,
                            "Cannot drop the only compatible index for this collection's shard key",
                            !isLastNonHiddenRangedShardKeyIndex(opCtx,
                                                                collection->getCollection(),
                                                                indexName,
                                                                collDesc.getKeyPattern()));
                }

                auto writableEntry = indexCatalog->getWritableEntryByName(
                    opCtx,
                    indexName,
                    IndexCatalog::InclusionPolicy::kReady |
                        IndexCatalog::InclusionPolicy::kUnfinished |
                        IndexCatalog::InclusionPolicy::kFrozen);
                if (!writableEntry) {
                    // A similar index wasn't created while we yielded the locks during abort.
                    continue;
                }

                uassertStatusOK(dropIndexByDescriptor(
                    opCtx, collection->getWritableCollection(opCtx), indexCatalog, writableEntry));
            }

            wuow.commit();
        });

        return reply;
    }

    if (!abortedIndexBuilders.empty()) {
        // All the index builders were sent the abort signal, remove all the remaining indexes
        // in the index catalog.
        invariant(isWildcard);
        invariant(indexNames.size() == 1);
        invariant(indexNames.front() == "*");
        invariant((*collection)->getIndexCatalog()->numIndexesInProgress() == 0);
    }

    writeConflictRetry(opCtx, "dropIndexes", dbAndUUID, [opCtx, &collection, &indexNames, &reply] {
        WriteUnitOfWork wunit(opCtx);

        // This is necessary to check shard version.
        OldClientContext ctx(opCtx, (*collection)->ns());
        dropReadyIndexes(
            opCtx, collection->getWritableCollection(opCtx), indexNames, &reply, false);
        wunit.commit();
    });

    return reply;
}

Status dropIndexesForApplyOps(OperationContext* opCtx,
                              const NamespaceString& nss,
                              const BSONObj& cmdObj) try {
    BSONObjBuilder bob(cmdObj);
    bob.append("$db", nss.dbName().serializeWithoutTenantPrefix_UNSAFE());
    auto cmdObjWithDb = bob.obj();
    auto parsed = DropIndexes::parse(IDLParserContext{"dropIndexes",
                                                      false /* apiStrict */,
                                                      auth::ValidatedTenancyScope::get(opCtx),
                                                      nss.tenantId(),
                                                      SerializationContext::stateStorageRequest()},
                                     cmdObjWithDb);

    return writeConflictRetry(opCtx, "dropIndexes", nss, [opCtx, &nss, &cmdObj, &parsed] {
        AutoGetCollection collection(opCtx, nss, MODE_X);

        // If db/collection does not exist, short circuit and return.
        Status status = checkView(opCtx, nss, collection.getCollection());
        if (!status.isOK()) {
            return status;
        }

        if (!serverGlobalParams.quiet.load()) {
            LOGV2(20344,
                  "CMD: dropIndexes",
                  logAttrs(nss),
                  "indexes"_attr = cmdObj[kIndexFieldName].toString(false));
        }

        auto swIndexNames = getIndexNames(opCtx, collection.getCollection(), parsed.getIndex());
        if (!swIndexNames.isOK()) {
            return swIndexNames.getStatus();
        }

        WriteUnitOfWork wunit(opCtx);

        // This is necessary to check shard version.
        OldClientContext ctx(opCtx, nss);

        DropIndexesReply ignoredReply;
        dropReadyIndexes(opCtx,
                         collection.getWritableCollection(opCtx),
                         swIndexNames.getValue(),
                         &ignoredReply,
                         true);

        wunit.commit();
        return Status::OK();
    });
} catch (const DBException& exc) {
    return exc.toStatus();
}

}  // namespace mongo
