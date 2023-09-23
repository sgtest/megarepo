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


#include "mongo/db/repl/tenant_migration_recipient_op_observer.h"

#include <fmt/format.h>
#include <memory>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/repl/tenant_migration_access_blocker.h"
#include "mongo/db/repl/tenant_migration_access_blocker_registry.h"
#include "mongo/db/repl/tenant_migration_access_blocker_util.h"
#include "mongo/db/repl/tenant_migration_decoration.h"
#include "mongo/db/repl/tenant_migration_recipient_access_blocker.h"
#include "mongo/db/repl/tenant_migration_state_machine_gen.h"
#include "mongo/db/serverless/serverless_operation_lock_registry.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/tenant_id.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kReplication

namespace mongo {
namespace repl {
using namespace fmt;
namespace {

void addTenantMigrationRecipientAccessBlocker(ServiceContext* serviceContext,
                                              const StringData& tenantId,
                                              const UUID& migrationId) {
    auto& registry = TenantMigrationAccessBlockerRegistry::get(serviceContext);
    TenantId tid = TenantId::parseFromString(tenantId);

    if (registry.getTenantMigrationAccessBlockerForTenantId(
            tid, TenantMigrationAccessBlocker::BlockerType::kRecipient)) {
        return;
    }

    auto mtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(serviceContext, migrationId);
    registry.add(tid, mtab);
}


/**
 * Transitions the TenantMigrationRecipientAccessBlocker to the rejectBefore state.
 */
void onSetRejectReadsBeforeTimestamp(OperationContext* opCtx,
                                     const TenantMigrationRecipientDocument& recipientStateDoc) {
    invariant(recipientStateDoc.getState() == TenantMigrationRecipientStateEnum::kConsistent);
    invariant(recipientStateDoc.getRejectReadsBeforeTimestamp());

    auto mtabVector = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                          .getRecipientAccessBlockersForMigration(recipientStateDoc.getId());
    invariant(!mtabVector.empty());
    for (auto& mtab : mtabVector) {
        invariant(mtab);
        mtab->startRejectingReadsBefore(recipientStateDoc.getRejectReadsBeforeTimestamp().value());
    }
}

void handleStateChange(OperationContext* opCtx,
                       const TenantMigrationRecipientDocument& recipientStateDoc) {
    auto state = recipientStateDoc.getState();

    switch (state) {
        case TenantMigrationRecipientStateEnum::kUninitialized:
            break;
        case TenantMigrationRecipientStateEnum::kStarted:
            addTenantMigrationRecipientAccessBlocker(opCtx->getServiceContext(),
                                                     recipientStateDoc.getTenantId(),
                                                     recipientStateDoc.getId());
            break;
        case TenantMigrationRecipientStateEnum::kConsistent:
            if (recipientStateDoc.getRejectReadsBeforeTimestamp()) {
                onSetRejectReadsBeforeTimestamp(opCtx, recipientStateDoc);
            }
            break;
        case TenantMigrationRecipientStateEnum::kDone:
        case TenantMigrationRecipientStateEnum::kCommitted:
        case TenantMigrationRecipientStateEnum::kAborted:
            break;
        default:
            MONGO_UNREACHABLE_TASSERT(6112900);
    }
}
}  // namespace

void TenantMigrationRecipientOpObserver::onInserts(
    OperationContext* opCtx,
    const CollectionPtr& coll,
    std::vector<InsertStatement>::const_iterator first,
    std::vector<InsertStatement>::const_iterator last,
    std::vector<bool> fromMigrate,
    bool defaultFromMigrate,
    OpStateAccumulator* opAccumulator) {
    if (coll->ns() == NamespaceString::kTenantMigrationRecipientsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        for (auto it = first; it != last; it++) {
            auto recipientStateDoc = TenantMigrationRecipientDocument::parse(
                IDLParserContext("recipientStateDoc"), it->doc);
            if (!recipientStateDoc.getExpireAt()) {
                ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
                    .acquireLock(ServerlessOperationLockRegistry::LockType::kTenantRecipient,
                                 recipientStateDoc.getId());
                opCtx->recoveryUnit()->onRollback([migrationId = recipientStateDoc.getId()](
                                                      OperationContext* opCtx) {
                    ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
                        .releaseLock(ServerlessOperationLockRegistry::LockType::kTenantRecipient,
                                     migrationId);
                });
            }
        }
    }
}

void TenantMigrationRecipientOpObserver::onUpdate(OperationContext* opCtx,
                                                  const OplogUpdateEntryArgs& args,
                                                  OpStateAccumulator* opAccumulator) {
    if (args.coll->ns() == NamespaceString::kTenantMigrationRecipientsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        auto recipientStateDoc = TenantMigrationRecipientDocument::parse(
            IDLParserContext("recipientStateDoc"), args.updateArgs->updatedDoc);

        opCtx->recoveryUnit()->onCommit(
            [recipientStateDoc](OperationContext* opCtx, boost::optional<Timestamp>) {
                if (recipientStateDoc.getExpireAt()) {
                    ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
                        .releaseLock(ServerlessOperationLockRegistry::LockType::kTenantRecipient,
                                     recipientStateDoc.getId());

                    bool shouldCleanAccessBlockers = false;
                    auto cleanUpBlockerIfGarbage =
                        [&](const TenantId& tenantId,
                            std::shared_ptr<TenantMigrationAccessBlocker>& mtab) {
                            if (recipientStateDoc.getId() != mtab->getMigrationId()) {
                                return;
                            }

                            auto recipientMtab =
                                checked_pointer_cast<TenantMigrationRecipientAccessBlocker>(mtab);
                            if (recipientMtab->inStateReject()) {
                                // The TenantMigrationRecipientAccessBlocker entry needs to be
                                // removed to re-allow reads and future migrations with the same
                                // tenantId as this migration has already been aborted and
                                // forgotten.
                                shouldCleanAccessBlockers = true;
                                return;
                            }
                            // Once the state doc is marked garbage collectable the TTL deletions
                            // should be unblocked.
                            recipientMtab->stopBlockingTTL();
                        };

                    TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                        .applyAll(TenantMigrationAccessBlocker::BlockerType::kRecipient,
                                  cleanUpBlockerIfGarbage);

                    if (shouldCleanAccessBlockers) {
                        TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                            .removeAccessBlockersForMigration(
                                recipientStateDoc.getId(),
                                TenantMigrationAccessBlocker::BlockerType::kRecipient);
                    }
                }

                handleStateChange(opCtx, recipientStateDoc);
            });
    }
}

void TenantMigrationRecipientOpObserver::aboutToDelete(OperationContext* opCtx,
                                                       const CollectionPtr& coll,
                                                       BSONObj const& doc,
                                                       OplogDeleteEntryArgs* args,
                                                       OpStateAccumulator* opAccumulator) {
    if (coll->ns() == NamespaceString::kTenantMigrationRecipientsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        auto recipientStateDoc =
            TenantMigrationRecipientDocument::parse(IDLParserContext("recipientStateDoc"), doc);
        uassert(ErrorCodes::IllegalOperation,
                str::stream() << "cannot delete a recipient's state document " << doc
                              << " since it has not been marked as garbage collectable",
                recipientStateDoc.getExpireAt());

        // TenantMigrationRecipientAccessBlocker is created at the start of a migration (in this
        // case the recipient state will be kStarted). If the recipient primary receives
        // recipientForgetMigration before receiving recipientSyncData, we set recipient state to
        // kDone in order to avoid creating an unnecessary TenantMigrationRecipientAccessBlocker.
        // In this case, the TenantMigrationRecipientAccessBlocker will not exist for a given
        // tenant.
        tenantMigrationInfo(opCtx) =
            boost::make_optional(TenantMigrationInfo(recipientStateDoc.getId()));
    }
}

void TenantMigrationRecipientOpObserver::onDelete(OperationContext* opCtx,
                                                  const CollectionPtr& coll,
                                                  StmtId stmtId,
                                                  const BSONObj& doc,
                                                  const OplogDeleteEntryArgs& args,
                                                  OpStateAccumulator* opAccumulator) {
    if (coll->ns() == NamespaceString::kTenantMigrationRecipientsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        auto tmi = tenantMigrationInfo(opCtx);
        if (!tmi) {
            return;
        }

        auto migrationId = tmi->uuid;
        opCtx->recoveryUnit()->onCommit(
            [migrationId](OperationContext* opCtx, boost::optional<Timestamp>) {
                LOGV2_INFO(6114101,
                           "Removing expired migration access blocker",
                           "migrationId"_attr = migrationId);
                TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .removeAccessBlockersForMigration(
                        migrationId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
            });
    }
}

repl::OpTime TenantMigrationRecipientOpObserver::onDropCollection(
    OperationContext* opCtx,
    const NamespaceString& collectionName,
    const UUID& uuid,
    std::uint64_t numRecords,
    const CollectionDropType dropType,
    bool markFromMigrate) {
    if (collectionName == NamespaceString::kTenantMigrationRecipientsNamespace) {
        opCtx->recoveryUnit()->onCommit([](OperationContext* opCtx, boost::optional<Timestamp>) {
            TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                .removeAll(TenantMigrationAccessBlocker::BlockerType::kRecipient);

            ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
                .onDropStateCollection(ServerlessOperationLockRegistry::LockType::kTenantRecipient);
        });
    }
    return {};
}

}  // namespace repl
}  // namespace mongo
