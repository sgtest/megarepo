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

#include <boost/optional.hpp>
#include <memory>
#include <string>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/repl/tenant_migration_access_blocker.h"
#include "mongo/db/repl/tenant_migration_access_blocker_registry.h"
#include "mongo/db/repl/tenant_migration_access_blocker_util.h"
#include "mongo/db/repl/tenant_migration_decoration.h"
#include "mongo/db/repl/tenant_migration_donor_access_blocker.h"
#include "mongo/db/repl/tenant_migration_donor_op_observer.h"
#include "mongo/db/repl/tenant_migration_state_machine_gen.h"
#include "mongo/db/serverless/serverless_operation_lock_registry.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/tenant_id.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kReplication

namespace mongo {
namespace repl {

namespace {

MONGO_FAIL_POINT_DEFINE(donorOpObserverFailAfterOnInsert);
MONGO_FAIL_POINT_DEFINE(donorOpObserverFailAfterOnUpdate);

/**
 * Initializes the TenantMigrationDonorAccessBlocker for the tenant migration denoted by the given
 * state doc.
 */
void onTransitionToAbortingIndexBuilds(OperationContext* opCtx,
                                       const TenantMigrationDonorDocument& donorStateDoc) {
    invariant(donorStateDoc.getState() == TenantMigrationDonorStateEnum::kAbortingIndexBuilds);

    ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
        .acquireLock(ServerlessOperationLockRegistry::LockType::kTenantDonor,
                     donorStateDoc.getId());
    opCtx->recoveryUnit()->onRollback(
        [migrationId = donorStateDoc.getId()](OperationContext* opCtx) {
            ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
                .releaseLock(ServerlessOperationLockRegistry::LockType::kTenantDonor, migrationId);
        });

    auto mtab = std::make_shared<TenantMigrationDonorAccessBlocker>(opCtx->getServiceContext(),
                                                                    donorStateDoc.getId());
    if (donorStateDoc.getProtocol().value_or(MigrationProtocolEnum::kMultitenantMigrations) ==
        MigrationProtocolEnum::kMultitenantMigrations) {
        invariant(donorStateDoc.getTenantId());
        const auto tenantId = TenantId::parseFromString(*donorStateDoc.getTenantId());
        TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext()).add(tenantId, mtab);
    } else {
        tassert(6448702,
                "Bad protocol",
                donorStateDoc.getProtocol() == MigrationProtocolEnum::kShardMerge);
        invariant(donorStateDoc.getTenantIds());

        auto& registry = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext());

        // Add global access blocker to avoid any tenant creation during shard merge.
        registry.addGlobalDonorAccessBlocker(mtab);
        for (const auto& tenantId : *donorStateDoc.getTenantIds()) {
            registry.add(tenantId,
                         std::make_shared<TenantMigrationDonorAccessBlocker>(
                             opCtx->getServiceContext(), donorStateDoc.getId()));
        }
    }

    opCtx->recoveryUnit()->onRollback([donorStateDoc](OperationContext* opCtx) {
        TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
            .removeAccessBlockersForMigration(donorStateDoc.getId(),
                                              TenantMigrationAccessBlocker::BlockerType::kDonor);
    });
}

/**
 * Transitions the TenantMigrationDonorAccessBlocker to the blocking state.
 */
void onTransitionToBlocking(OperationContext* opCtx,
                            const TenantMigrationDonorDocument& donorStateDoc) {
    invariant(donorStateDoc.getState() == TenantMigrationDonorStateEnum::kBlocking);
    invariant(donorStateDoc.getBlockTimestamp());

    auto mtabVector = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                          .getDonorAccessBlockersForMigration(donorStateDoc.getId());
    invariant(!mtabVector.empty());

    if (!opCtx->writesAreReplicated()) {
        // A primary calls startBlockingWrites on the TenantMigrationDonorAccessBlocker before
        // reserving the OpTime for the "start blocking" write, so only secondaries call
        // startBlockingWrites on the TenantMigrationDonorAccessBlocker in the op observer.
        for (auto& mtab : mtabVector) {
            mtab->startBlockingWrites();
        }
    }

    // Both primaries and secondaries call startBlockingReadsAfter in the op observer, since
    // startBlockingReadsAfter just needs to be called before the "start blocking" write's oplog
    // hole is filled.
    for (auto& mtab : mtabVector) {
        mtab->startBlockingReadsAfter(donorStateDoc.getBlockTimestamp().value());
    }
}

/**
 * Transitions the TenantMigrationDonorAccessBlocker to the committed state.
 */
void onTransitionToCommitted(OperationContext* opCtx,
                             const TenantMigrationDonorDocument& donorStateDoc) {
    invariant(donorStateDoc.getState() == TenantMigrationDonorStateEnum::kCommitted);
    invariant(donorStateDoc.getCommitOrAbortOpTime());

    auto mtabVector = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                          .getDonorAccessBlockersForMigration(donorStateDoc.getId());
    invariant(!mtabVector.empty());

    for (auto& mtab : mtabVector) {
        mtab->setCommitOpTime(opCtx, donorStateDoc.getCommitOrAbortOpTime().value());
    }
}

/**
 * Transitions the TenantMigrationDonorAccessBlocker to the aborted state.
 */
void onTransitionToAborted(OperationContext* opCtx,
                           const TenantMigrationDonorDocument& donorStateDoc) {
    invariant(donorStateDoc.getState() == TenantMigrationDonorStateEnum::kAborted);
    invariant(donorStateDoc.getCommitOrAbortOpTime());

    auto mtabVector = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                          .getDonorAccessBlockersForMigration(donorStateDoc.getId());
    invariant(!mtabVector.empty());

    for (auto& mtab : mtabVector) {
        mtab->setAbortOpTime(opCtx, donorStateDoc.getCommitOrAbortOpTime().value());
    }
}

/**
 * Used to update the TenantMigrationDonorAccessBlocker for the migration denoted by the donor's
 * state doc once the write for updating the doc is committed.
 */
class TenantMigrationDonorCommitOrAbortHandler final : public RecoveryUnit::Change {
public:
    TenantMigrationDonorCommitOrAbortHandler(const TenantMigrationDonorDocument donorStateDoc)
        : _donorStateDoc(std::move(donorStateDoc)) {}

    void commit(OperationContext* opCtx, boost::optional<Timestamp>) override {
        if (_donorStateDoc.getExpireAt()) {
            ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
                .releaseLock(ServerlessOperationLockRegistry::LockType::kTenantDonor,
                             _donorStateDoc.getId());

            auto mtabVector = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                                  .getDonorAccessBlockersForMigration(_donorStateDoc.getId());

            if (mtabVector.empty()) {
                // The state doc and TenantMigrationDonorAccessBlocker for this migration were
                // removed immediately after expireAt was set. This is unlikely to occur in
                // production where the garbage collection delay should be sufficiently large.
                return;
            }

            if (!opCtx->writesAreReplicated()) {
                // Setting expireAt implies that the TenantMigrationDonorAccessBlocker for this
                // migration will be removed shortly after this. However, a lagged secondary
                // might not manage to advance its majority commit point past the migration
                // commit or abort opTime and consequently transition out of the blocking state
                // before the TenantMigrationDonorAccessBlocker is removed. When this occurs,
                // blocked reads or writes will be left waiting for the migration decision
                // indefinitely. To avoid that, notify the TenantMigrationDonorAccessBlocker
                // here that the commit or abort opTime has been majority committed (guaranteed
                // to be true since by design the donor never marks its state doc as garbage
                // collectable before the migration decision is majority committed).
                for (auto& mtab : mtabVector) {
                    mtab->onMajorityCommitPointUpdate(
                        _donorStateDoc.getCommitOrAbortOpTime().value());
                }
            }

            if (_donorStateDoc.getState() == TenantMigrationDonorStateEnum::kAborted) {
                for (auto& mtab : mtabVector) {
                    invariant(mtab->inStateAborted());
                }
                // The migration durably aborted and is now marked as garbage collectable,
                // remove its TenantMigrationDonorAccessBlocker right away to allow back-to-back
                // migration retries.
                TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .removeAccessBlockersForMigration(
                        _donorStateDoc.getId(), TenantMigrationAccessBlocker::BlockerType::kDonor);
            }
            return;
        }

        switch (_donorStateDoc.getState()) {
            case TenantMigrationDonorStateEnum::kCommitted:
                onTransitionToCommitted(opCtx, _donorStateDoc);
                break;
            case TenantMigrationDonorStateEnum::kAborted:
                onTransitionToAborted(opCtx, _donorStateDoc);
                break;
            default:
                MONGO_UNREACHABLE;
        }
    }

    void rollback(OperationContext* opCtx) override {}

private:
    const TenantMigrationDonorDocument _donorStateDoc;
};

}  // namespace

void TenantMigrationDonorOpObserver::onInserts(OperationContext* opCtx,
                                               const CollectionPtr& coll,
                                               std::vector<InsertStatement>::const_iterator first,
                                               std::vector<InsertStatement>::const_iterator last,
                                               std::vector<bool> fromMigrate,
                                               bool defaultFromMigrate,
                                               OpStateAccumulator* opAccumulator) {
    if (coll->ns() == NamespaceString::kTenantMigrationDonorsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        for (auto it = first; it != last; it++) {
            auto donorStateDoc = tenant_migration_access_blocker::parseDonorStateDocument(it->doc);
            switch (donorStateDoc.getState()) {
                case TenantMigrationDonorStateEnum::kAbortingIndexBuilds:
                    onTransitionToAbortingIndexBuilds(opCtx, donorStateDoc);
                    break;
                case TenantMigrationDonorStateEnum::kDataSync:
                case TenantMigrationDonorStateEnum::kBlocking:
                case TenantMigrationDonorStateEnum::kCommitted:
                case TenantMigrationDonorStateEnum::kAborted:
                    uasserted(ErrorCodes::IllegalOperation,
                              "cannot insert a donor's state doc with 'state' other than 'aborting "
                              "index builds'");
                    break;
                default:
                    MONGO_UNREACHABLE;
            }
        }

        if (MONGO_unlikely(donorOpObserverFailAfterOnInsert.shouldFail())) {
            uasserted(ErrorCodes::InternalError, "fail donor's state doc insert");
        }
    }
}

void TenantMigrationDonorOpObserver::onUpdate(OperationContext* opCtx,
                                              const OplogUpdateEntryArgs& args,
                                              OpStateAccumulator* opAccumulator) {
    if (args.coll->ns() == NamespaceString::kTenantMigrationDonorsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        auto donorStateDoc =
            tenant_migration_access_blocker::parseDonorStateDocument(args.updateArgs->updatedDoc);
        switch (donorStateDoc.getState()) {
            case TenantMigrationDonorStateEnum::kDataSync:
                break;
            case TenantMigrationDonorStateEnum::kBlocking:
                onTransitionToBlocking(opCtx, donorStateDoc);
                break;
            case TenantMigrationDonorStateEnum::kCommitted:
            case TenantMigrationDonorStateEnum::kAborted:
                opCtx->recoveryUnit()->registerChange(
                    std::make_unique<TenantMigrationDonorCommitOrAbortHandler>(donorStateDoc));
                break;
            default:
                MONGO_UNREACHABLE;
        }

        if (MONGO_unlikely(donorOpObserverFailAfterOnUpdate.shouldFail())) {
            uasserted(ErrorCodes::InternalError, "fail donor's state doc update");
        }
    }
}

void TenantMigrationDonorOpObserver::aboutToDelete(OperationContext* opCtx,
                                                   const CollectionPtr& coll,
                                                   BSONObj const& doc,
                                                   OplogDeleteEntryArgs* args,
                                                   OpStateAccumulator* opAccumulator) {
    if (coll->ns() == NamespaceString::kTenantMigrationDonorsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        auto donorStateDoc = tenant_migration_access_blocker::parseDonorStateDocument(doc);
        uassert(ErrorCodes::IllegalOperation,
                str::stream() << "cannot delete a donor's state document " << doc
                              << " since it has not been marked as garbage collectable",
                donorStateDoc.getExpireAt());

        // To support back-to-back migration retries, when a migration is aborted, we remove its
        // TenantMigrationDonorAccessBlocker as soon as its donor state doc is marked as garbage
        // collectable. So onDelete should skip removing the TenantMigrationDonorAccessBlocker for
        // aborted migrations.
        tenantMigrationInfo(opCtx) =
            donorStateDoc.getState() == TenantMigrationDonorStateEnum::kAborted
            ? boost::none
            : boost::make_optional(TenantMigrationInfo(donorStateDoc.getId()));
    }
}

void TenantMigrationDonorOpObserver::onDelete(OperationContext* opCtx,
                                              const CollectionPtr& coll,
                                              StmtId stmtId,
                                              const BSONObj& doc,
                                              const OplogDeleteEntryArgs& args,
                                              OpStateAccumulator* opAccumulator) {
    if (coll->ns() == NamespaceString::kTenantMigrationDonorsNamespace &&
        !tenant_migration_access_blocker::inRecoveryMode(opCtx)) {
        auto tmi = tenantMigrationInfo(opCtx);
        if (!tmi) {
            return;
        }

        auto migrationId = tmi->uuid;
        opCtx->recoveryUnit()->onCommit(
            [migrationId](OperationContext* opCtx, boost::optional<Timestamp>) {
                LOGV2_INFO(6461601,
                           "Removing expired migration access blocker",
                           "migrationId"_attr = migrationId);
                TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .removeAccessBlockersForMigration(
                        migrationId, TenantMigrationAccessBlocker::BlockerType::kDonor);
            });
    }
}

repl::OpTime TenantMigrationDonorOpObserver::onDropCollection(OperationContext* opCtx,
                                                              const NamespaceString& collectionName,
                                                              const UUID& uuid,
                                                              std::uint64_t numRecords,
                                                              const CollectionDropType dropType,
                                                              bool markFromMigrate) {
    if (collectionName == NamespaceString::kTenantMigrationDonorsNamespace) {
        opCtx->recoveryUnit()->onCommit([](OperationContext* opCtx, boost::optional<Timestamp>) {
            TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                .removeAll(TenantMigrationAccessBlocker::BlockerType::kDonor);

            ServerlessOperationLockRegistry::get(opCtx->getServiceContext())
                .onDropStateCollection(ServerlessOperationLockRegistry::LockType::kTenantDonor);
        });
    }
    return {};
}

void TenantMigrationDonorOpObserver::onMajorityCommitPointUpdate(
    ServiceContext* service, const repl::OpTime& newCommitPoint) {
    TenantMigrationAccessBlockerRegistry::get(service).onMajorityCommitPointUpdate(newCommitPoint);
}

}  // namespace repl
}  // namespace mongo
