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

#include "mongo/db/catalog/rename_collection.h"

#include <algorithm>
#include <boost/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <memory>
#include <string>
#include <vector>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/simple_bsonobj_comparator.h"
#include "mongo/bson/unordered_fields_bsonobj_comparator.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/collection_uuid_mismatch.h"
#include "mongo/db/catalog/collection_write_path.h"
#include "mongo/db/catalog/database.h"
#include "mongo/db/catalog/database_holder.h"
#include "mongo/db/catalog/document_validation.h"
#include "mongo/db/catalog/drop_collection.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/catalog/list_indexes.h"
#include "mongo/db/catalog/local_oplog_info.h"
#include "mongo/db/catalog/unique_collection_name.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/curop.h"
#include "mongo/db/database_name.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/index_builds_coordinator.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/op_observer/batched_write_policy.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/ops/insert.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/s/database_sharding_state.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/service_context.h"
#include "mongo/db/stats/top.h"
#include "mongo/db/storage/record_data.h"
#include "mongo/db/storage/record_store.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kCommand

namespace mongo {
namespace {

MONGO_FAIL_POINT_DEFINE(writeConflictInRenameCollCopyToTmp);

boost::optional<NamespaceString> getNamespaceFromUUID(OperationContext* opCtx, const UUID& uuid) {
    return CollectionCatalog::get(opCtx)->lookupNSSByUUID(opCtx, uuid);
}

// From a replicated to an unreplicated collection or vice versa.
bool isReplicatedChanged(OperationContext* opCtx,
                         const NamespaceString& source,
                         const NamespaceString& target) {
    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    auto sourceIsUnreplicated = replCoord->isOplogDisabledFor(opCtx, source);
    auto targetIsUnreplicated = replCoord->isOplogDisabledFor(opCtx, target);
    return (sourceIsUnreplicated != targetIsUnreplicated);
}

Status checkSourceAndTargetNamespaces(OperationContext* opCtx,
                                      const NamespaceString& source,
                                      const NamespaceString& target,
                                      RenameCollectionOptions options,
                                      bool targetExistsAllowed) {

    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    if (opCtx->writesAreReplicated() && !replCoord->canAcceptWritesFor(opCtx, source))
        return Status(ErrorCodes::NotWritablePrimary,
                      str::stream() << "Not primary while renaming collection "
                                    << source.toStringForErrorMsg() << " to "
                                    << target.toStringForErrorMsg());

    if (isReplicatedChanged(opCtx, source, target))
        return {ErrorCodes::IllegalOperation,
                "Cannot rename collections between a replicated and an unreplicated database"};

    auto db = DatabaseHolder::get(opCtx)->getDb(opCtx, source.dbName());
    if (!db || db->isDropPending(opCtx))
        return Status(ErrorCodes::NamespaceNotFound,
                      str::stream() << "Database " << source.dbName().toStringForErrorMsg()
                                    << " does not exist or is drop pending");

    auto catalog = CollectionCatalog::get(opCtx);
    const auto sourceColl = catalog->lookupCollectionByNamespace(opCtx, source);
    if (!sourceColl) {
        if (CollectionCatalog::get(opCtx)->lookupView(opCtx, source))
            return Status(ErrorCodes::CommandNotSupportedOnView,
                          str::stream() << "cannot rename view: " << source.toStringForErrorMsg());
        return Status(ErrorCodes::NamespaceNotFound,
                      str::stream() << "Source collection " << source.toStringForErrorMsg()
                                    << " does not exist");
    }

    if (sourceColl->getCollectionOptions().encryptedFieldConfig &&
        !AuthorizationSession::get(opCtx->getClient())
             ->isAuthorizedForActionsOnResource(
                 ResourcePattern::forClusterResource(target.tenantId()),
                 ActionType::setUserWriteBlockMode)) {
        return Status(ErrorCodes::IllegalOperation, "Cannot rename an encrypted collection");
    }

    IndexBuildsCoordinator::get(opCtx)->assertNoIndexBuildInProgForCollection(sourceColl->uuid());

    const auto targetColl = catalog->lookupCollectionByNamespace(opCtx, target);

    if (!targetColl) {
        if (CollectionCatalog::get(opCtx)->lookupView(opCtx, target))
            return Status(ErrorCodes::NamespaceExists,
                          str::stream() << "a view already exists with that name: "
                                        << target.toStringForErrorMsg());
    } else {
        if (targetColl->getCollectionOptions().encryptedFieldConfig &&
            !AuthorizationSession::get(opCtx->getClient())
                 ->isAuthorizedForActionsOnResource(
                     ResourcePattern::forClusterResource(target.tenantId()),
                     ActionType::setUserWriteBlockMode)) {
            return Status(ErrorCodes::IllegalOperation,
                          "Cannot rename to an existing encrypted collection");
        }

        if (!targetExistsAllowed && !options.dropTarget)
            return Status(ErrorCodes::NamespaceExists, "target namespace exists");
    }

    return Status::OK();
}

Status renameTargetCollectionToTmp(OperationContext* opCtx,
                                   const NamespaceString& sourceNs,
                                   const UUID& sourceUUID,
                                   Database* const targetDB,
                                   const NamespaceString& targetNs,
                                   const UUID& targetUUID) {
    repl::UnreplicatedWritesBlock uwb(opCtx);

    // The generated unique collection name is only guaranteed to exist if the database is
    // exclusively locked.
    invariant(opCtx->lockState()->isDbLockedForMode(targetDB->name(), LockMode::MODE_X));
    auto tmpNameResult = makeUniqueCollectionName(opCtx, targetDB->name(), "tmp%%%%%.rename");
    if (!tmpNameResult.isOK()) {
        return tmpNameResult.getStatus().withContext(
            str::stream() << "Cannot generate a temporary collection name for the target "
                          << targetNs.toStringForErrorMsg() << " (" << targetUUID
                          << ") so that the source" << sourceNs.toStringForErrorMsg() << " ("
                          << sourceUUID << ") could be renamed to "
                          << targetNs.toStringForErrorMsg());
    }
    const auto& tmpName = tmpNameResult.getValue();
    const bool stayTemp = true;
    return writeConflictRetry(opCtx, "renameCollection", targetNs, [&] {
        WriteUnitOfWork wunit(opCtx);
        auto status = targetDB->renameCollection(opCtx, targetNs, tmpName, stayTemp);
        if (!status.isOK())
            return status;

        wunit.commit();

        LOGV2(20397,
              "Successfully renamed the target {targetNs} ({targetUUID}) to {tmpName} so that the "
              "source {sourceNs} ({sourceUUID}) could be renamed to {targetNs2}",
              "Successfully renamed the target so that the source could be renamed",
              "existingTargetNamespace"_attr = targetNs,
              "existingTargetUUID"_attr = targetUUID,
              "renamedExistingTarget"_attr = tmpName,
              "sourceNamespace"_attr = sourceNs,
              "sourceUUID"_attr = sourceUUID,
              "newTargetNamespace"_attr = targetNs);

        return Status::OK();
    });
}

Status renameCollectionDirectly(OperationContext* opCtx,
                                Database* db,
                                const UUID& uuid,
                                NamespaceString source,
                                NamespaceString target,
                                RenameCollectionOptions options) {
    return writeConflictRetry(opCtx, "renameCollection", target, [&] {
        WriteUnitOfWork wunit(opCtx);

        {
            // No logOp necessary because the entire renameCollection command is one logOp.
            repl::UnreplicatedWritesBlock uwb(opCtx);
            auto status = db->renameCollection(opCtx, source, target, options.stayTemp);
            if (!status.isOK())
                return status;
        }

        // We have to override the provided 'dropTarget' setting for idempotency reasons to
        // avoid unintentionally removing a collection on a secondary with the same name as
        // the target.
        auto opObserver = opCtx->getServiceContext()->getOpObserver();
        opObserver->onRenameCollection(
            opCtx, source, target, uuid, {}, 0U, options.stayTemp, options.markFromMigrate);

        wunit.commit();
        return Status::OK();
    });
}

Status renameCollectionAndDropTarget(OperationContext* opCtx,
                                     Database* db,
                                     const UUID& uuid,
                                     NamespaceString source,
                                     NamespaceString target,
                                     const CollectionPtr& targetColl,
                                     RenameCollectionOptions options,
                                     repl::OpTime renameOpTimeFromApplyOps) {
    return writeConflictRetry(opCtx, "renameCollection", target, [&] {
        WriteUnitOfWork wunit(opCtx);

        // Target collection exists - drop it.
        invariant(options.dropTarget);

        auto replCoord = repl::ReplicationCoordinator::get(opCtx);
        auto isOplogDisabledForNamespace = replCoord->isOplogDisabledFor(opCtx, target);
        if (!isOplogDisabledForNamespace) {
            invariant(opCtx->writesAreReplicated());
            invariant(renameOpTimeFromApplyOps.isNull());
        }

        IndexBuildsCoordinator::get(opCtx)->assertNoIndexBuildInProgForCollection(
            targetColl->uuid());

        auto numRecords = targetColl->numRecords(opCtx);
        auto opObserver = opCtx->getServiceContext()->getOpObserver();

        auto renameOpTime = opObserver->preRenameCollection(opCtx,
                                                            source,
                                                            target,
                                                            uuid,
                                                            targetColl->uuid(),
                                                            numRecords,
                                                            options.stayTemp,
                                                            options.markFromMigrate);

        if (!renameOpTimeFromApplyOps.isNull()) {
            // 'renameOpTime' must be null because a valid 'renameOpTimeFromApplyOps' implies
            // replicated writes are not enabled.
            if (!renameOpTime.isNull()) {
                LOGV2_FATAL(
                    40616,
                    "renameCollection: {from} to {to} (with dropTarget=true) - unexpected "
                    "renameCollection oplog entry written to the oplog with optime {renameOpTime}",
                    "renameCollection (with dropTarget=true): unexpected renameCollection oplog "
                    "entry written to the oplog",
                    "from"_attr = source,
                    "to"_attr = target,
                    "renameOpTime"_attr = renameOpTime);
            }
            renameOpTime = renameOpTimeFromApplyOps;
        }

        // No logOp necessary because the entire renameCollection command is one logOp.
        repl::UnreplicatedWritesBlock uwb(opCtx);

        auto status = db->dropCollection(opCtx, targetColl->ns(), renameOpTime);
        if (!status.isOK())
            return status;

        status = db->renameCollection(opCtx, source, target, options.stayTemp);
        if (!status.isOK())
            return status;

        opObserver->postRenameCollection(
            opCtx, source, target, uuid, targetColl->uuid(), options.stayTemp);
        wunit.commit();
        return Status::OK();
    });
}

Status renameCollectionWithinDB(OperationContext* opCtx,
                                const NamespaceString& source,
                                const NamespaceString& target,
                                RenameCollectionOptions options) {
    invariant(source.isEqualDb(target));
    DisableDocumentValidation validationDisabler(opCtx);

    AutoGetDb autoDb(opCtx, source.dbName(), MODE_IX);

    boost::optional<Lock::CollectionLock> sourceLock;
    boost::optional<Lock::CollectionLock> targetLock;
    // To prevent deadlock, always lock system.views collection in the end because concurrent
    // view-related operations always lock system.views in the end.
    if (!source.isSystemDotViews() &&
        (target.isSystemDotViews() ||
         ResourceId(RESOURCE_COLLECTION, source) < ResourceId(RESOURCE_COLLECTION, target))) {
        // To prevent deadlock, always lock source and target in ascending resourceId order.
        sourceLock.emplace(opCtx, source, MODE_X);
        targetLock.emplace(opCtx, target, MODE_X);
    } else {
        targetLock.emplace(opCtx, target, MODE_X);
        sourceLock.emplace(opCtx, source, MODE_X);
    }

    auto db = DatabaseHolder::get(opCtx)->getDb(opCtx, source.dbName());
    auto catalog = CollectionCatalog::get(opCtx);
    const auto sourceColl = catalog->lookupCollectionByNamespace(opCtx, source);
    const auto targetColl = catalog->lookupCollectionByNamespace(opCtx, target);

    checkCollectionUUIDMismatch(opCtx, source, sourceColl, options.expectedSourceUUID);
    checkCollectionUUIDMismatch(opCtx, target, targetColl, options.expectedTargetUUID);

    auto status = checkSourceAndTargetNamespaces(
        opCtx, source, target, options, /* targetExistsAllowed */ false);
    if (!status.isOK())
        return status;

    AutoStatsTracker statsTracker(
        opCtx,
        source,
        Top::LockType::NotLocked,
        AutoStatsTracker::LogMode::kUpdateCurOp,
        CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(source.dbName()));

    if (!targetColl) {
        return renameCollectionDirectly(opCtx, db, sourceColl->uuid(), source, target, options);
    } else {
        return renameCollectionAndDropTarget(
            opCtx, db, sourceColl->uuid(), source, target, CollectionPtr(targetColl), options, {});
    }
}

Status renameCollectionWithinDBForApplyOps(OperationContext* opCtx,
                                           const NamespaceString& source,
                                           const NamespaceString& target,
                                           const boost::optional<UUID>& uuidToDrop,
                                           repl::OpTime renameOpTimeFromApplyOps,
                                           const RenameCollectionOptions& options) {
    invariant(source.isEqualDb(target));
    DisableDocumentValidation validationDisabler(opCtx);

    AutoGetDb autoDb(opCtx, source.dbName(), MODE_X);

    auto status = checkSourceAndTargetNamespaces(
        opCtx, source, target, options, /* targetExistsAllowed */ true);
    if (!status.isOK())
        return status;

    auto db = DatabaseHolder::get(opCtx)->getDb(opCtx, source.dbName());
    const auto sourceColl =
        CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, source);

    AutoStatsTracker statsTracker(
        opCtx,
        source,
        Top::LockType::NotLocked,
        AutoStatsTracker::LogMode::kUpdateCurOp,
        CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(source.dbName()));

    return writeConflictRetry(opCtx, "renameCollection", target, [&] {
        auto targetColl = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, target);
        WriteUnitOfWork wuow(opCtx);
        if (targetColl) {
            if (sourceColl->uuid() == targetColl->uuid()) {
                if (!uuidToDrop || uuidToDrop == targetColl->uuid()) {
                    wuow.commit();
                    return Status::OK();
                }

                // During initial sync, it is possible that the collection already
                // got renamed to the target, so there is not much left to do other
                // than drop the dropTarget. See SERVER-40861 for more details.
                auto collToDropBasedOnUUID = getNamespaceFromUUID(opCtx, *uuidToDrop);
                if (!collToDropBasedOnUUID) {
                    wuow.commit();
                    return Status::OK();
                }
                repl::UnreplicatedWritesBlock uwb(opCtx);
                Status status =
                    db->dropCollection(opCtx, *collToDropBasedOnUUID, renameOpTimeFromApplyOps);
                if (!status.isOK())
                    return status;
                wuow.commit();
                return Status::OK();
            }

            if (!uuidToDrop || uuidToDrop != targetColl->uuid()) {
                // We need to rename the targetColl to a temporary name.
                auto status = renameTargetCollectionToTmp(
                    opCtx, source, sourceColl->uuid(), db, target, targetColl->uuid());
                if (!status.isOK())
                    return status;
                targetColl = nullptr;
            }
        }

        // When reapplying oplog entries (such as in the case of initial sync) we need
        // to identify the collection to drop by UUID, as otherwise we might end up
        // dropping the wrong collection.
        if (!targetColl && uuidToDrop) {
            invariant(options.dropTarget);
            auto collToDropBasedOnUUID = getNamespaceFromUUID(opCtx, uuidToDrop.value());
            if (collToDropBasedOnUUID && !collToDropBasedOnUUID->isDropPendingNamespace()) {
                invariant(collToDropBasedOnUUID->isEqualDb(target));
                targetColl = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(
                    opCtx, *collToDropBasedOnUUID);
            }
        }

        Status ret = Status::OK();
        if (!targetColl) {
            ret = renameCollectionDirectly(opCtx, db, sourceColl->uuid(), source, target, options);
        } else {
            if (sourceColl == targetColl) {
                wuow.commit();
                return Status::OK();
            }

            ret = renameCollectionAndDropTarget(opCtx,
                                                db,
                                                sourceColl->uuid(),
                                                source,
                                                target,
                                                CollectionPtr(targetColl),
                                                options,
                                                renameOpTimeFromApplyOps);
        }

        if (ret.isOK()) {
            wuow.commit();
        }

        return ret;
    });
}

Status renameCollectionAcrossDatabases(OperationContext* opCtx,
                                       const NamespaceString& source,
                                       const NamespaceString& target,
                                       const RenameCollectionOptions& options) {
    invariant(
        !source.isEqualDb(target),
        str::stream()
            << "cannot rename within same database (use renameCollectionWithinDB instead): source: "
            << source.toStringForErrorMsg() << "; target: " << target.toStringForErrorMsg());

    // Refer to txnCmdAllowlist in commands.cpp.
    invariant(!opCtx->inMultiDocumentTransaction(),
              str::stream() << "renameCollectionAcrossDatabases not supported in multi-document "
                               "transaction: source: "
                            << source.toStringForErrorMsg()
                            << "; target: " << target.toStringForErrorMsg());

    uassert(ErrorCodes::InvalidOptions,
            "Cannot provide an expected collection UUID when renaming across databases",
            !options.expectedSourceUUID && !options.expectedTargetUUID);

    boost::optional<Lock::DBLock> sourceDbLock;
    boost::optional<Lock::CollectionLock> sourceCollLock;
    if (!opCtx->lockState()->isCollectionLockedForMode(source, MODE_S)) {
        // Lock the DB using MODE_IX to ensure we have the global lock in that mode, as to prevent
        // upgrade from MODE_IS to MODE_IX, which caused deadlock on systems not supporting Database
        // locking and should be avoided in general.
        sourceDbLock.emplace(opCtx, source.dbName(), MODE_IX);
        sourceCollLock.emplace(opCtx, source, MODE_S);
    }

    boost::optional<Lock::DBLock> targetDBLock;
    if (!opCtx->lockState()->isDbLockedForMode(target.dbName(), MODE_X)) {
        targetDBLock.emplace(opCtx, target.dbName(), MODE_X);
    }

    DatabaseShardingState::assertMatchingDbVersion(opCtx, source.dbName());

    DisableDocumentValidation validationDisabler(opCtx);

    auto sourceDB = DatabaseHolder::get(opCtx)->getDb(opCtx, source.dbName());
    if (!sourceDB)
        return Status(ErrorCodes::NamespaceNotFound, "source namespace does not exist");

    boost::optional<AutoStatsTracker> statsTracker(
        boost::in_place_init,
        opCtx,
        source,
        Top::LockType::NotLocked,
        AutoStatsTracker::LogMode::kUpdateCurOp,
        CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(source.dbName()));

    auto catalog = CollectionCatalog::get(opCtx);
    const auto sourceColl = catalog->lookupCollectionByNamespace(opCtx, source);
    if (!sourceColl) {
        if (CollectionCatalog::get(opCtx)->lookupView(opCtx, source))
            return Status(ErrorCodes::CommandNotSupportedOnView,
                          str::stream() << "cannot rename view: " << source.toStringForErrorMsg());
        return Status(ErrorCodes::NamespaceNotFound, "source namespace does not exist");
    }

    if (isReplicatedChanged(opCtx, source, target))
        return {ErrorCodes::IllegalOperation,
                "Cannot rename collections across a replicated and an unreplicated database"};

    IndexBuildsCoordinator::get(opCtx)->assertNoIndexBuildInProgForCollection(sourceColl->uuid());

    auto targetDB = DatabaseHolder::get(opCtx)->getDb(opCtx, target.dbName());

    // Check if the target namespace exists and if dropTarget is true.
    // Return a non-OK status if target exists and dropTarget is not true or if the collection
    // is sharded.
    const auto targetColl =
        targetDB ? catalog->lookupCollectionByNamespace(opCtx, target) : nullptr;
    if (targetColl) {
        if (sourceColl->uuid() == targetColl->uuid()) {
            invariant(source == target);
            return Status::OK();
        }

        if (!options.dropTarget) {
            return Status(ErrorCodes::NamespaceExists, "target namespace exists");
        }

    } else if (CollectionCatalog::get(opCtx)->lookupView(opCtx, target)) {
        return Status(ErrorCodes::NamespaceExists,
                      str::stream() << "a view already exists with that name: "
                                    << target.toStringForErrorMsg());
    }

    // Create a temporary collection in the target database. It will be removed if we fail to
    // copy the collection, or on restart, so there is no need to replicate these writes.
    if (!targetDB) {
        targetDB = DatabaseHolder::get(opCtx)->openDb(opCtx, target.dbName());
    }

    // The generated unique collection name is only guaranteed to exist if the database is
    // exclusively locked.
    invariant(opCtx->lockState()->isDbLockedForMode(targetDB->name(), LockMode::MODE_X));

    // Note that this temporary collection name is used by MongoMirror and thus must not be changed
    // without consultation.
    auto tmpNameResult =
        makeUniqueCollectionName(opCtx, target.dbName(), "tmp%%%%%.renameCollection");
    if (!tmpNameResult.isOK()) {
        return tmpNameResult.getStatus().withContext(
            str::stream() << "Cannot generate temporary collection name to rename "
                          << source.toStringForErrorMsg() << " to "
                          << target.toStringForErrorMsg());
    }
    const auto& tmpName = tmpNameResult.getValue();

    LOGV2(705520,
          "Attempting to create temporary collection",
          "temporaryCollection"_attr = tmpName,
          "sourceCollection"_attr = source);

    // Renaming across databases will result in a new UUID.
    NamespaceStringOrUUID tmpCollUUID{tmpName.dbName(), UUID::gen()};

    {
        auto collectionOptions = sourceColl->getCollectionOptions();
        collectionOptions.uuid = tmpCollUUID.uuid();

        writeConflictRetry(opCtx, "renameCollection", tmpName, [&] {
            WriteUnitOfWork wunit(opCtx);
            targetDB->createCollection(opCtx, tmpName, collectionOptions);
            wunit.commit();
        });
    }

    // Dismissed on success
    ScopeGuard tmpCollectionDropper([&] {
        Status status = Status::OK();
        try {
            status = dropCollectionForApplyOps(
                opCtx,
                tmpName,
                {},
                DropCollectionSystemCollectionMode::kAllowSystemCollectionDrops);
        } catch (...) {
            status = exceptionToStatus();
        }
        if (!status.isOK()) {
            // Ignoring failure case when dropping the temporary collection during cleanup because
            // the rename operation has already failed for another reason.
            LOGV2(705521,
                  "Unable to drop temporary collection {tmpName} while renaming from {source} to "
                  "{target}: {error}",
                  "Unable to drop temporary collection while renaming",
                  "tempCollection"_attr = tmpName,
                  "source"_attr = source,
                  "target"_attr = target,
                  "error"_attr = status);
        }
    });

    // Copy the index descriptions from the source collection.
    std::vector<BSONObj> indexesToCopy;
    for (auto sourceIndIt = sourceColl->getIndexCatalog()->getIndexIterator(
             opCtx,
             IndexCatalog::InclusionPolicy::kReady | IndexCatalog::InclusionPolicy::kUnfinished |
                 IndexCatalog::InclusionPolicy::kFrozen);
         sourceIndIt->more();) {
        auto descriptor = sourceIndIt->next()->descriptor();
        if (descriptor->isIdIndex()) {
            continue;
        }

        indexesToCopy.push_back(descriptor->infoObj());
    }

    // Create indexes using the index specs on the empty temporary collection that was just created.
    // Since each index build is possibly replicated to downstream nodes, each createIndex oplog
    // entry must have a distinct timestamp to support correct rollback operation. This is achieved
    // by writing the createIndexes oplog entry *before* creating the index. Using
    // IndexCatalog::createIndexOnEmptyCollection() for the index creation allows us to add and
    // commit the index within a single WriteUnitOfWork and avoids the possibility of seeing the
    // index in an unfinished state. For more information on assigning timestamps to multiple index
    // builds, please see SERVER-35780 and SERVER-35070.
    if (!indexesToCopy.empty()) {
        Status status = writeConflictRetry(opCtx, "renameCollection", tmpName, [&] {
            WriteUnitOfWork wunit(opCtx);
            auto fromMigrate = false;
            try {
                CollectionWriter tmpCollWriter(opCtx, tmpCollUUID.uuid());
                IndexBuildsCoordinator::get(opCtx)->createIndexesOnEmptyCollection(
                    opCtx, tmpCollWriter, indexesToCopy, fromMigrate);
            } catch (DBException& ex) {
                return ex.toStatus();
            }
            wunit.commit();
            return Status::OK();
        });
        if (!status.isOK()) {
            return status;
        }
    }

    {
        statsTracker.reset();

        // Copy over all the data from source collection to temporary collection. For this we can
        // drop the exclusive database lock on the target and grab an intent lock on the temporary
        // collection.
        targetDBLock.reset();

        AutoGetCollection autoTmpColl(opCtx, tmpCollUUID, MODE_IX);
        if (!autoTmpColl) {
            return Status(ErrorCodes::NamespaceNotFound,
                          str::stream() << "Temporary collection '" << tmpName.toStringForErrorMsg()
                                        << "' was removed while renaming collection across DBs");
        }

        auto replCoord = repl::ReplicationCoordinator::get(opCtx);
        auto isOplogDisabledForTmpColl = replCoord->isOplogDisabledFor(opCtx, tmpName);
        bool canBeBatched =
            !(autoTmpColl->isCapped() && autoTmpColl->getIndexCatalog()->haveAnyIndexes());

        auto cursor = sourceColl->getCursor(opCtx);
        auto record = cursor->next();
        auto batchedWriteMaxSizeBytes =
            gMaxSizeOfBatchedInsertsForRenameAcrossDatabasesBytes.load();
        auto batchedWriteMaxNumberOfInserts =
            gMaxNumberOfInsertsBatchInsertsForRenameAcrossDatabases.load();
        while (record) {
            opCtx->checkForInterrupt();
            // Cursor is left one past the end of the batch inside writeConflictRetry.
            auto beginBatchId = record->id;
            Status status = writeConflictRetry(opCtx, "renameCollection", tmpName, [&] {
                // Always reposition cursor in case it gets a WCE midway through.
                record = cursor->seekExact(beginBatchId);

                std::vector<InsertStatement> stmts;
                // Inserts to indexed capped collections cannot be batched.
                // Otherwise, CollectionImpl::_insertDocuments() will fail with
                // OperationCannotBeBatched. See SERVER-21512.
                buildBatchedWritesWithPolicy(
                    batchedWriteMaxSizeBytes,
                    batchedWriteMaxNumberOfInserts,
                    [&cursor]() { return cursor->next(); },
                    record,
                    stmts,
                    canBeBatched);

                bool isGroupedOplogEntries = stmts.size() > 1U;
                WriteUnitOfWork wunit(opCtx, isGroupedOplogEntries);

                if (!isOplogDisabledForTmpColl && !isGroupedOplogEntries) {
                    auto oplogInfo = LocalOplogInfo::get(opCtx);
                    auto slots = oplogInfo->getNextOpTimes(opCtx, 1U);
                    stmts[0].oplogSlot = slots[0];
                }

                OpDebug* const opDebug = nullptr;
                auto status = collection_internal::insertDocuments(opCtx,
                                                                   *autoTmpColl,
                                                                   stmts.begin(),
                                                                   stmts.end(),
                                                                   opDebug,
                                                                   false /* fromMigrate */);
                if (!status.isOK()) {
                    return status;
                }

                // Used to make sure that a WCE can be handled by this logic without data loss.
                if (MONGO_unlikely(writeConflictInRenameCollCopyToTmp.shouldFail())) {
                    throwWriteConflictException(
                        str::stream() << "Hit failpoint '"
                                      << writeConflictInRenameCollCopyToTmp.getName() << "'.");
                }

                wunit.commit();

                // Time to yield; make a safe copy of the current record before releasing our
                // cursor.
                if (record)
                    record->data.makeOwned();

                cursor->save();
                // When this exits via success or WCE, we need to restore the cursor.
                ON_BLOCK_EXIT([opCtx, ns = tmpName, &cursor]() {
                    writeConflictRetry(
                        opCtx, "retryRestoreCursor", ns, [&cursor] { cursor->restore(); });
                });
                return Status::OK();
            });
            if (!status.isOK())
                return status;
        }
    }
    sourceCollLock.reset();
    sourceDbLock.reset();

    // Getting here means we successfully built the target copy. We now do the final
    // in-place rename and remove the source collection.
    invariant(tmpName.isEqualDb(target));
    RenameCollectionOptions tempOptions(options);
    Status status = renameCollectionWithinDB(opCtx, tmpName, target, tempOptions);
    if (!status.isOK())
        return status;

    tmpCollectionDropper.dismiss();
    return dropCollectionForApplyOps(
        opCtx, source, {}, DropCollectionSystemCollectionMode::kAllowSystemCollectionDrops);
}

}  // namespace

void doLocalRenameIfOptionsAndIndexesHaveNotChanged(OperationContext* opCtx,
                                                    const NamespaceString& sourceNs,
                                                    const NamespaceString& targetNs,
                                                    const RenameCollectionOptions& options,
                                                    std::list<BSONObj> originalIndexes,
                                                    BSONObj originalCollectionOptions) {
    AutoGetDb dbLock(opCtx, targetNs.dbName(), MODE_X);
    auto collection = dbLock.getDb()
        ? CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, targetNs)
        : nullptr;
    BSONObj collectionOptions = {};
    if (collection) {
        // We do not include the UUID field in the options comparison. It is ok if the target
        // collection was dropped and recreated, as long as the new target collection has the same
        // options and indexes as the original one did. This is mainly to support concurrent $out
        // to the same collection.
        collectionOptions = collection->getCollectionOptions().toBSON().removeField("uuid");
    }

    uassert(ErrorCodes::CommandFailed,
            str::stream() << "collection options of target collection "
                          << targetNs.toStringForErrorMsg()
                          << " changed during processing. Original options: "
                          << originalCollectionOptions << ", new options: " << collectionOptions,
            SimpleBSONObjComparator::kInstance.evaluate(
                originalCollectionOptions.removeField("uuid") == collectionOptions));

    auto currentIndexes =
        listIndexesEmptyListIfMissing(opCtx, targetNs, ListIndexesInclude::Nothing);

    UnorderedFieldsBSONObjComparator comparator;
    uassert(
        ErrorCodes::CommandFailed,
        str::stream() << "indexes of target collection " << targetNs.toStringForErrorMsg()
                      << " changed during processing.",
        originalIndexes.size() == currentIndexes.size() &&
            std::equal(originalIndexes.begin(),
                       originalIndexes.end(),
                       currentIndexes.begin(),
                       [&](auto& lhs, auto& rhs) { return comparator.compare(lhs, rhs) == 0; }));

    validateAndRunRenameCollection(opCtx, sourceNs, targetNs, options);
}

void validateNamespacesForRenameCollection(OperationContext* opCtx,
                                           const NamespaceString& source,
                                           const NamespaceString& target,
                                           const RenameCollectionOptions& options) {
    uassert(ErrorCodes::InvalidNamespace,
            str::stream() << "Invalid source namespace: " << source.toStringForErrorMsg(),
            source.isValid());
    uassert(ErrorCodes::InvalidNamespace,
            str::stream() << "Invalid target namespace: " << target.toStringForErrorMsg(),
            target.isValid());

    if (repl::ReplicationCoordinator::get(opCtx)->getSettings().isReplSet()) {
        uassert(ErrorCodes::IllegalOperation,
                "can't rename live oplog while replicating",
                !source.isOplog());
        uassert(ErrorCodes::IllegalOperation,
                "can't rename to live oplog while replicating",
                !target.isOplog());
    }

    uassert(ErrorCodes::IllegalOperation,
            "If either the source or target of a rename is an oplog name, both must be",
            source.isOplog() == target.isOplog());

    Status sourceStatus = userAllowedWriteNS(opCtx, source);
    uassert(ErrorCodes::IllegalOperation,
            "error with source namespace: " + sourceStatus.reason(),
            sourceStatus.isOK());
    Status targetStatus = userAllowedWriteNS(opCtx, target);
    uassert(ErrorCodes::IllegalOperation,
            "error with target namespace: " + targetStatus.reason(),
            targetStatus.isOK());

    if (source.isServerConfigurationCollection()) {
        uasserted(ErrorCodes::IllegalOperation,
                  "renaming the server configuration "
                  "collection (admin.system.version) is not "
                  "allowed");
    }

    uassert(ErrorCodes::NamespaceNotFound,
            str::stream() << "renameCollection cannot accept a source collection that is in a "
                             "drop-pending state: "
                          << source.toStringForErrorMsg(),
            !source.isDropPendingNamespace());

    uassert(ErrorCodes::IllegalOperation,
            "renaming system.views collection or renaming to system.views is not allowed",
            !source.isSystemDotViews() && !target.isSystemDotViews());

    uassert(ErrorCodes::IllegalOperation,
            "renaming system.js collection or renaming to system.js is not allowed",
            !source.isSystemDotJavascript() && !target.isSystemDotJavascript());

    if (!source.isOutTmpBucketsCollection() && source.isTimeseriesBucketsCollection()) {
        uassert(ErrorCodes::IllegalOperation,
                "Renaming system.buckets collections is not allowed",
                AuthorizationSession::get(opCtx->getClient())
                    ->isAuthorizedForActionsOnResource(
                        ResourcePattern::forClusterResource(target.tenantId()),
                        ActionType::setUserWriteBlockMode));

        uassert(ErrorCodes::IllegalOperation,
                str::stream() << "Cannot rename time-series buckets collection {"
                              << source.toStringForErrorMsg()
                              << "} to a non-time-series buckets namespace {"
                              << target.toStringForErrorMsg() << "}",
                target.isTimeseriesBucketsCollection());
    }
}

void validateAndRunRenameCollection(OperationContext* opCtx,
                                    const NamespaceString& source,
                                    const NamespaceString& target,
                                    const RenameCollectionOptions& options) {
    invariant(source != target, "Can't rename a collection to itself");

    validateNamespacesForRenameCollection(opCtx, source, target, options);

    OperationShardingState::ScopedAllowImplicitCollectionCreate_UNSAFE unsafeCreateCollection(
        opCtx);
    uassertStatusOK(renameCollection(opCtx, source, target, options));
}

Status renameCollection(OperationContext* opCtx,
                        const NamespaceString& source,
                        const NamespaceString& target,
                        const RenameCollectionOptions& options) {
    if (source.isDropPendingNamespace()) {
        return Status(ErrorCodes::NamespaceNotFound,
                      str::stream() << "renameCollection() cannot accept a source "
                                       "collection that is in a drop-pending state: "
                                    << source.toStringForErrorMsg());
    }

    if (source.isSystemDotViews() || target.isSystemDotViews()) {
        return Status(
            ErrorCodes::IllegalOperation,
            "renaming system.views collection or renaming to system.views is not allowed");
    }

    if (source.isSystemDotJavascript() || target.isSystemDotJavascript()) {
        return Status(ErrorCodes::IllegalOperation,
                      "renaming system.js collection or renaming to system.js is not allowed");
    }

    if (source.tenantId() != target.tenantId()) {
        return Status(ErrorCodes::IllegalOperation,
                      "renaming a collection across tenants is not allowed");
    }

    StringData dropTargetMsg = options.dropTarget ? "yes"_sd : "no"_sd;
    LOGV2(20400,
          "renameCollectionForCommand: rename {source} to {target}{dropTargetMsg}",
          "renameCollectionForCommand",
          "sourceNamespace"_attr = source,
          "targetNamespace"_attr = target,
          "dropTarget"_attr = dropTargetMsg);

    if (source.isEqualDb(target))
        return renameCollectionWithinDB(opCtx, source, target, options);
    else {
        return renameCollectionAcrossDatabases(opCtx, source, target, options);
    }
}

Status renameCollectionForApplyOps(OperationContext* opCtx,
                                   const boost::optional<UUID>& uuidToRename,
                                   const boost::optional<TenantId>& tid,
                                   const BSONObj& cmd,
                                   const repl::OpTime& renameOpTime) {

    // A valid 'renameOpTime' is not allowed when writes are replicated.
    if (!renameOpTime.isNull() && opCtx->writesAreReplicated()) {
        return Status(
            ErrorCodes::BadValue,
            "renameCollection() cannot accept a rename optime when writes are replicated.");
    }

    const auto sourceNsElt = cmd["renameCollection"];
    const auto targetNsElt = cmd["to"];

    NamespaceString sourceNss{NamespaceStringUtil::deserialize(tid, sourceNsElt.valueStringData())};
    NamespaceString targetNss{NamespaceStringUtil::deserialize(tid, targetNsElt.valueStringData())};

    // TODO: not needed once we are no longer parsing for prefixed tenantIds
    uassert(ErrorCodes::IllegalOperation,
            "moving a collection between tenants is not allowed",
            sourceNss.tenantId() == targetNss.tenantId());

    if (uuidToRename) {
        auto nss = CollectionCatalog::get(opCtx)->lookupNSSByUUID(opCtx, uuidToRename.value());
        if (nss)
            sourceNss = *nss;
    }

    RenameCollectionOptions options;
    options.dropTarget = cmd["dropTarget"].trueValue();
    options.stayTemp = cmd["stayTemp"].trueValue();

    boost::optional<UUID> uuidToDrop;
    if (cmd["dropTarget"].type() == BinData) {
        auto uuid = uassertStatusOK(UUID::parse(cmd["dropTarget"]));
        uuidToDrop = uuid;
    }

    // Check that the target namespace is in the correct form, "database.collection".
    auto targetStatus = userAllowedCreateNS(opCtx, targetNss);
    if (!targetStatus.isOK()) {
        return Status(targetStatus.code(),
                      str::stream() << "error with target namespace: " << targetStatus.reason());
    }

    if (!repl::ReplicationCoordinator::get(opCtx)->getSettings().isReplSet() &&
        targetNss.isOplog()) {
        return Status(ErrorCodes::IllegalOperation,
                      str::stream() << "Cannot rename collection to the oplog");
    }

    // Take strong database and collection locks in order to avoid upgrading later.
    AutoGetDb sourceDb(opCtx, sourceNss.dbName(), MODE_X);
    AutoGetCollection sourceColl(
        opCtx,
        sourceNss,
        MODE_X,
        AutoGetCollection::Options{}.viewMode(auto_get_collection::ViewMode::kViewsPermitted));

    if (sourceNss.isDropPendingNamespace() || !sourceColl) {
        boost::optional<NamespaceString> dropTargetNss;

        if (options.dropTarget)
            dropTargetNss = targetNss;

        if (uuidToDrop)
            dropTargetNss = getNamespaceFromUUID(opCtx, uuidToDrop.value());

        // Downgrade renameCollection to dropCollection.
        if (dropTargetNss) {
            return dropCollectionForApplyOps(
                opCtx,
                *dropTargetNss,
                renameOpTime,
                DropCollectionSystemCollectionMode::kAllowSystemCollectionDrops);
        }

        return Status(ErrorCodes::NamespaceNotFound,
                      str::stream()
                          << "renameCollection() cannot accept a source "
                             "collection that does not exist or is in a drop-pending state: "
                          << sourceNss.toStringForErrorMsg());
    }

    const std::string uuidToDropString = uuidToDrop ? uuidToDrop->toString() : "<none>";
    const std::string uuidString = uuidToRename ? uuidToRename->toString() : "UUID unknown";
    LOGV2(20401,
          "renameCollectionForApplyOps: rename {sourceNss} ({uuidString}) to "
          "{targetNss}{dropTargetMsg}",
          "renameCollectionForApplyOps",
          "sourceNamespace"_attr = sourceNss,
          "uuid"_attr = uuidString,
          "targetNamespace"_attr = targetNss,
          "uuidToDrop"_attr = uuidToDropString);

    if (sourceNss.isEqualDb(targetNss)) {
        return renameCollectionWithinDBForApplyOps(
            opCtx, sourceNss, targetNss, uuidToDrop, renameOpTime, options);
    } else {
        return renameCollectionAcrossDatabases(opCtx, sourceNss, targetNss, options);
    }
}

Status renameCollectionForRollback(OperationContext* opCtx,
                                   const NamespaceString& target,
                                   const UUID& uuid) {
    // If the UUID we're targeting already exists, rename from there no matter what.
    auto source = getNamespaceFromUUID(opCtx, uuid);
    invariant(source);
    invariant(source->isEqualDb(target),
              str::stream() << "renameCollectionForRollback: source and target namespaces must "
                               "have the same database. source: "
                            << (*source).toStringForErrorMsg()
                            << ". target: " << target.toStringForErrorMsg());

    LOGV2(20402,
          "renameCollectionForRollback: rename {source} ({uuid}) to {target}.",
          "renameCollectionForRollback",
          "source"_attr = *source,
          "uuid"_attr = uuid,
          "target"_attr = target);

    return renameCollectionWithinDB(opCtx, *source, target, {});
}

}  // namespace mongo
