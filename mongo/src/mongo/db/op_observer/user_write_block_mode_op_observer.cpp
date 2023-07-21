/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <set>
#include <utility>

#include "mongo/bson/timestamp.h"
#include "mongo/db/catalog/collection_operation_source.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/op_observer/user_write_block_mode_op_observer.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/s/global_user_write_block_state.h"
#include "mongo/db/s/user_writes_critical_section_document_gen.h"
#include "mongo/db/s/user_writes_recoverable_critical_section_service.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/decorable.h"

namespace mongo {
namespace {

const auto documentIdDecoration = OplogDeleteEntryArgs::declareDecoration<BSONObj>();

bool isStandaloneOrPrimary(OperationContext* opCtx, const NamespaceString& nss) {
    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    return replCoord->canAcceptWritesFor(opCtx, nss);
}

}  // namespace

void UserWriteBlockModeOpObserver::onInserts(OperationContext* opCtx,
                                             const CollectionPtr& coll,
                                             std::vector<InsertStatement>::const_iterator first,
                                             std::vector<InsertStatement>::const_iterator last,
                                             std::vector<bool> fromMigrate,
                                             bool defaultFromMigrate,
                                             OpStateAccumulator* opAccumulator) {
    const auto& nss = coll->ns();

    if (!defaultFromMigrate) {
        _checkWriteAllowed(opCtx, nss);
    }

    if (nss == NamespaceString::kUserWritesCriticalSectionsNamespace &&
        !user_writes_recoverable_critical_section_util::inRecoveryMode(opCtx)) {
        for (auto it = first; it != last; ++it) {
            const auto& insertedDoc = it->doc;

            const auto collCSDoc = UserWriteBlockingCriticalSectionDocument::parse(
                IDLParserContext("UserWriteBlockOpObserver"), insertedDoc);
            opCtx->recoveryUnit()->onCommit(
                [blockShardedDDL = collCSDoc.getBlockNewUserShardedDDL(),
                 blockWrites = collCSDoc.getBlockUserWrites(),
                 insertedNss = collCSDoc.getNss()](OperationContext* opCtx,
                                                   boost::optional<Timestamp>) {
                    invariant(insertedNss.isEmpty());
                    boost::optional<Lock::GlobalLock> globalLockIfNotPrimary;
                    if (!isStandaloneOrPrimary(
                            opCtx, NamespaceString::kUserWritesCriticalSectionsNamespace)) {
                        globalLockIfNotPrimary.emplace(opCtx, MODE_IX);
                    }

                    if (blockShardedDDL) {
                        GlobalUserWriteBlockState::get(opCtx)->enableUserShardedDDLBlocking(opCtx);
                    }

                    if (blockWrites) {
                        GlobalUserWriteBlockState::get(opCtx)->enableUserWriteBlocking(opCtx);
                    }
                });
        }
    }
}

void UserWriteBlockModeOpObserver::onUpdate(OperationContext* opCtx,
                                            const OplogUpdateEntryArgs& args,
                                            OpStateAccumulator* opAccumulator) {
    const auto& nss = args.coll->ns();

    if (args.updateArgs->source != OperationSource::kFromMigrate) {
        _checkWriteAllowed(opCtx, nss);
    }

    if (nss == NamespaceString::kUserWritesCriticalSectionsNamespace &&
        !user_writes_recoverable_critical_section_util::inRecoveryMode(opCtx)) {
        const auto collCSDoc = UserWriteBlockingCriticalSectionDocument::parse(
            IDLParserContext("UserWriteBlockOpObserver"), args.updateArgs->updatedDoc);

        opCtx->recoveryUnit()->onCommit([updatedNss = collCSDoc.getNss(),
                                         blockShardedDDL = collCSDoc.getBlockNewUserShardedDDL(),
                                         blockWrites = collCSDoc.getBlockUserWrites(),
                                         insertedNss = collCSDoc.getNss()](
                                            OperationContext* opCtx, boost::optional<Timestamp>) {
            invariant(updatedNss.isEmpty());
            boost::optional<Lock::GlobalLock> globalLockIfNotPrimary;
            if (!isStandaloneOrPrimary(opCtx,
                                       NamespaceString::kUserWritesCriticalSectionsNamespace)) {
                globalLockIfNotPrimary.emplace(opCtx, MODE_IX);
            }

            if (blockShardedDDL) {
                GlobalUserWriteBlockState::get(opCtx)->enableUserShardedDDLBlocking(opCtx);
            } else {
                GlobalUserWriteBlockState::get(opCtx)->disableUserShardedDDLBlocking(opCtx);
            }

            if (blockWrites) {
                GlobalUserWriteBlockState::get(opCtx)->enableUserWriteBlocking(opCtx);
            } else {
                GlobalUserWriteBlockState::get(opCtx)->disableUserWriteBlocking(opCtx);
            }
        });
    }
}

void UserWriteBlockModeOpObserver::aboutToDelete(OperationContext* opCtx,
                                                 const CollectionPtr& coll,
                                                 BSONObj const& doc,
                                                 OplogDeleteEntryArgs* args,
                                                 OpStateAccumulator* opAccumulator) {
    if (coll->ns() == NamespaceString::kUserWritesCriticalSectionsNamespace) {
        documentIdDecoration(args) = doc;
    }
}

void UserWriteBlockModeOpObserver::onDelete(OperationContext* opCtx,
                                            const CollectionPtr& coll,
                                            StmtId stmtId,
                                            const OplogDeleteEntryArgs& args,
                                            OpStateAccumulator* opAccumulator) {
    const auto& nss = coll->ns();
    if (!args.fromMigrate) {
        _checkWriteAllowed(opCtx, nss);
    }

    if (nss == NamespaceString::kUserWritesCriticalSectionsNamespace &&
        !user_writes_recoverable_critical_section_util::inRecoveryMode(opCtx)) {
        auto& documentId = documentIdDecoration(args);
        invariant(!documentId.isEmpty());

        const auto& deletedDoc = documentId;
        const auto collCSDoc = UserWriteBlockingCriticalSectionDocument::parse(
            IDLParserContext("UserWriteBlockOpObserver"), deletedDoc);

        opCtx->recoveryUnit()->onCommit([deletedNss = collCSDoc.getNss()](
                                            OperationContext* opCtx, boost::optional<Timestamp> _) {
            invariant(deletedNss.isEmpty());
            boost::optional<Lock::GlobalLock> globalLockIfNotPrimary;
            if (!isStandaloneOrPrimary(opCtx,
                                       NamespaceString::kUserWritesCriticalSectionsNamespace)) {
                globalLockIfNotPrimary.emplace(opCtx, MODE_IX);
            }

            GlobalUserWriteBlockState::get(opCtx)->disableUserShardedDDLBlocking(opCtx);
            GlobalUserWriteBlockState::get(opCtx)->disableUserWriteBlocking(opCtx);
        });
    }
}

void UserWriteBlockModeOpObserver::onReplicationRollback(OperationContext* opCtx,
                                                         const RollbackObserverInfo& rbInfo) {
    if (rbInfo.rollbackNamespaces.find(NamespaceString::kUserWritesCriticalSectionsNamespace) !=
        rbInfo.rollbackNamespaces.end()) {
        UserWritesRecoverableCriticalSectionService::get(opCtx)->recoverRecoverableCriticalSections(
            opCtx);
    }
}

void UserWriteBlockModeOpObserver::onCreateIndex(OperationContext* opCtx,
                                                 const NamespaceString& nss,
                                                 const UUID& uuid,
                                                 BSONObj indexDoc,
                                                 bool fromMigrate) {
    _checkWriteAllowed(opCtx, nss);
}

void UserWriteBlockModeOpObserver::onStartIndexBuild(OperationContext* opCtx,
                                                     const NamespaceString& nss,
                                                     const UUID& collUUID,
                                                     const UUID& indexBuildUUID,
                                                     const std::vector<BSONObj>& indexes,
                                                     bool fromMigrate) {
    _checkWriteAllowed(opCtx, nss);
}

void UserWriteBlockModeOpObserver::onStartIndexBuildSinglePhase(OperationContext* opCtx,
                                                                const NamespaceString& nss) {
    _checkWriteAllowed(opCtx, nss);
}

void UserWriteBlockModeOpObserver::onCreateCollection(OperationContext* opCtx,
                                                      const CollectionPtr& coll,
                                                      const NamespaceString& collectionName,
                                                      const CollectionOptions& options,
                                                      const BSONObj& idIndex,
                                                      const OplogSlot& createOpTime,
                                                      bool fromMigrate) {
    _checkWriteAllowed(opCtx, collectionName);
}

void UserWriteBlockModeOpObserver::onCollMod(OperationContext* opCtx,
                                             const NamespaceString& nss,
                                             const UUID& uuid,
                                             const BSONObj& collModCmd,
                                             const CollectionOptions& oldCollOptions,
                                             boost::optional<IndexCollModInfo> indexInfo) {
    _checkWriteAllowed(opCtx, nss);
}

void UserWriteBlockModeOpObserver::onDropDatabase(OperationContext* opCtx,
                                                  const DatabaseName& dbName) {
    _checkWriteAllowed(opCtx, NamespaceString(dbName));
}

repl::OpTime UserWriteBlockModeOpObserver::onDropCollection(OperationContext* opCtx,
                                                            const NamespaceString& collectionName,
                                                            const UUID& uuid,
                                                            std::uint64_t numRecords,
                                                            CollectionDropType dropType,
                                                            bool markFromMigrate) {
    _checkWriteAllowed(opCtx, collectionName);
    return repl::OpTime();
}

void UserWriteBlockModeOpObserver::onDropIndex(OperationContext* opCtx,
                                               const NamespaceString& nss,
                                               const UUID& uuid,
                                               const std::string& indexName,
                                               const BSONObj& indexInfo) {
    _checkWriteAllowed(opCtx, nss);
}

repl::OpTime UserWriteBlockModeOpObserver::preRenameCollection(
    OperationContext* opCtx,
    const NamespaceString& fromCollection,
    const NamespaceString& toCollection,
    const UUID& uuid,
    const boost::optional<UUID>& dropTargetUUID,
    std::uint64_t numRecords,
    bool stayTemp,
    bool markFromMigrate) {
    _checkWriteAllowed(opCtx, fromCollection);
    _checkWriteAllowed(opCtx, toCollection);
    return repl::OpTime();
}

void UserWriteBlockModeOpObserver::onRenameCollection(OperationContext* opCtx,
                                                      const NamespaceString& fromCollection,
                                                      const NamespaceString& toCollection,
                                                      const UUID& uuid,
                                                      const boost::optional<UUID>& dropTargetUUID,
                                                      std::uint64_t numRecords,
                                                      bool stayTemp,
                                                      bool markFromMigrate) {
    _checkWriteAllowed(opCtx, fromCollection);
    _checkWriteAllowed(opCtx, toCollection);
}

void UserWriteBlockModeOpObserver::onImportCollection(OperationContext* opCtx,
                                                      const UUID& importUUID,
                                                      const NamespaceString& nss,
                                                      long long numRecords,
                                                      long long dataSize,
                                                      const BSONObj& catalogEntry,
                                                      const BSONObj& storageMetadata,
                                                      bool isDryRun) {
    _checkWriteAllowed(opCtx, nss);
}

void UserWriteBlockModeOpObserver::_checkWriteAllowed(OperationContext* opCtx,
                                                      const NamespaceString& nss) {
    // Evaluate write blocking only on replica set primaries.
    const auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    if (replCoord->getSettings().isReplSet() && replCoord->canAcceptWritesFor(opCtx, nss)) {
        GlobalUserWriteBlockState::get(opCtx)->checkUserWritesAllowed(opCtx, nss);
    }
}

}  // namespace mongo
