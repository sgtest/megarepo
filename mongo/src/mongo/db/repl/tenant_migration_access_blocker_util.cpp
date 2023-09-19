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


#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <boost/smart_ptr.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <fmt/format.h>
#include <memory>
#include <tuple>
#include <type_traits>
#include <utility>
#include <vector>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/oid.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/multitenancy_gen.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/persistent_task_store.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/repl/read_concern_level.h"
#include "mongo/db/repl/repl_settings.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/tenant_migration_access_blocker.h"
#include "mongo/db/repl/tenant_migration_access_blocker_registry.h"
#include "mongo/db/repl/tenant_migration_access_blocker_util.h"
#include "mongo/db/repl/tenant_migration_conflict_info.h"
#include "mongo/db/repl/tenant_migration_decoration.h"
#include "mongo/db/repl/tenant_migration_donor_access_blocker.h"
#include "mongo/db/repl/tenant_migration_recipient_access_blocker.h"
#include "mongo/db/repl/tenant_migration_shard_merge_util.h"
#include "mongo/db/repl/tenant_migration_state_machine_gen.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/serverless/shard_split_state_machine_gen.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/executor/task_executor.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/compiler.h"
#include "mongo/transport/session.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/cancellation.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/future_util.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTenantMigration


namespace mongo {

// Failpoint that will cause recoverTenantMigrationAccessBlockers to return early.
MONGO_FAIL_POINT_DEFINE(skipRecoverTenantMigrationAccessBlockers);

// Signals that we have checked that we can build an index.
MONGO_FAIL_POINT_DEFINE(haveCheckedIfIndexBuildableDuringTenantMigration);

namespace tenant_migration_access_blocker {

namespace {
using MtabType = TenantMigrationAccessBlocker::BlockerType;

bool noDataHasBeenCopiedByRecipient(const TenantMigrationRecipientDocument& doc) {
    // We always set recipientPrimaryStartingFCV before copying any data. If it is not set, it means
    // no data has been copied during the current instance's lifetime.
    return !doc.getRecipientPrimaryStartingFCV();
}

bool recoverTenantMigrationRecipientAccessBlockers(OperationContext* opCtx,
                                                   const TenantMigrationRecipientDocument& doc) {
    // Do not create the mtab when:
    // 1) The migration was forgotten before receiving a 'recipientSyncData'.
    // 2) A delayed 'recipientForgetMigration' was received after the state doc was deleted.
    if ((doc.getState() == TenantMigrationRecipientStateEnum::kDone ||
         doc.getState() == TenantMigrationRecipientStateEnum::kAborted ||
         doc.getState() == TenantMigrationRecipientStateEnum::kCommitted) &&
        noDataHasBeenCopiedByRecipient(doc)) {
        return true;
    }

    auto mtab = std::make_shared<TenantMigrationRecipientAccessBlocker>(opCtx->getServiceContext(),
                                                                        doc.getId());
    auto protocol = doc.getProtocol().value_or(MigrationProtocolEnum::kMultitenantMigrations);
    switch (protocol) {
        case MigrationProtocolEnum::kShardMerge:
            invariant(doc.getTenantIds());
            TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                .add(*doc.getTenantIds(), mtab);
            break;
        case MigrationProtocolEnum::kMultitenantMigrations: {
            const auto tenantId = TenantId::parseFromString(doc.getTenantId());
            TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                .add(tenantId, mtab);
            break;
        }
        default:
            MONGO_UNREACHABLE;
    }

    switch (doc.getState()) {
        case TenantMigrationRecipientStateEnum::kStarted:
        case TenantMigrationRecipientStateEnum::kLearnedFilenames:
            invariant(!doc.getRejectReadsBeforeTimestamp());
            break;
        case TenantMigrationRecipientStateEnum::kConsistent:
        case TenantMigrationRecipientStateEnum::kDone:
        case TenantMigrationRecipientStateEnum::kCommitted:
        case TenantMigrationRecipientStateEnum::kAborted:
            if (doc.getRejectReadsBeforeTimestamp()) {
                mtab->startRejectingReadsBefore(doc.getRejectReadsBeforeTimestamp().get());
            }
            break;
        case TenantMigrationRecipientStateEnum::kUninitialized:
            MONGO_UNREACHABLE;
    }

    return true;
}

bool recoverTenantMigrationDonorAccessBlockers(OperationContext* opCtx,
                                               const TenantMigrationDonorDocument& doc) {
    // Skip creating a TenantMigrationDonorAccessBlocker for aborted migrations that have been
    // marked as garbage collected.
    if (doc.getExpireAt() && doc.getState() == TenantMigrationDonorStateEnum::kAborted) {
        return true;
    }

    std::vector<std::shared_ptr<TenantMigrationDonorAccessBlocker>> mtabVector{
        std::make_shared<TenantMigrationDonorAccessBlocker>(opCtx->getServiceContext(),
                                                            doc.getId())};

    auto& registry = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext());
    auto protocol = doc.getProtocol().value_or(MigrationProtocolEnum::kMultitenantMigrations);
    switch (protocol) {
        case MigrationProtocolEnum::kMultitenantMigrations: {
            invariant(doc.getTenantId());
            const auto tenantId = TenantId::parseFromString(*doc.getTenantId());
            registry.add(tenantId, mtabVector.back());
        } break;
        case MigrationProtocolEnum::kShardMerge:
            invariant(doc.getTenantIds());
            // Add global access blocker to avoid any tenant creation during shard merge.
            registry.addGlobalDonorAccessBlocker(mtabVector.back());
            for (const auto& tenantId : *doc.getTenantIds()) {
                mtabVector.push_back(std::make_shared<TenantMigrationDonorAccessBlocker>(
                    opCtx->getServiceContext(), doc.getId()));
                registry.add(tenantId, mtabVector.back());
            }
            break;
        default:
            MONGO_UNREACHABLE;
    }

    switch (doc.getState()) {
        case TenantMigrationDonorStateEnum::kAbortingIndexBuilds:
        case TenantMigrationDonorStateEnum::kDataSync:
            break;
        case TenantMigrationDonorStateEnum::kBlocking:
            invariant(doc.getBlockTimestamp());
            for (auto& mtab : mtabVector) {
                mtab->startBlockingWrites();
                mtab->startBlockingReadsAfter(doc.getBlockTimestamp().value());
            }
            break;
        case TenantMigrationDonorStateEnum::kCommitted:
            invariant(doc.getBlockTimestamp());
            invariant(doc.getCommitOrAbortOpTime());
            for (auto& mtab : mtabVector) {
                mtab->startBlockingWrites();
                mtab->startBlockingReadsAfter(doc.getBlockTimestamp().value());
                mtab->setCommitOpTime(opCtx, doc.getCommitOrAbortOpTime().value());
            }
            break;
        case TenantMigrationDonorStateEnum::kAborted:
            invariant(doc.getCommitOrAbortOpTime());
            for (auto& mtab : mtabVector) {
                if (doc.getBlockTimestamp()) {
                    mtab->startBlockingWrites();
                    mtab->startBlockingReadsAfter(doc.getBlockTimestamp().value());
                }
                mtab->setAbortOpTime(opCtx, doc.getCommitOrAbortOpTime().value());
            }
            break;
        case TenantMigrationDonorStateEnum::kUninitialized:
            MONGO_UNREACHABLE;
    }
    return true;
}

bool recoverShardMergeRecipientAccessBlockers(OperationContext* opCtx,
                                              const ShardMergeRecipientDocument& doc) {
    auto replCoord = repl::ReplicationCoordinator::get(getGlobalServiceContext());
    invariant(replCoord && replCoord->getSettings().isReplSet());

    // If the initial syncing node (both FCBIS and logical initial sync) syncs from a sync source
    // that's in the middle of file copy/import phase of shard merge, it can cause the initial
    // syncing node to have only partial donor data. And, if this node went into initial sync (i.e,
    // resync) after it sent `recipientVoteImportedFiles` to the recipient primary, the primary
    // can commit the migration and cause permanent data loss on this node.
    if (replCoord->getMemberState().startup2() && !doc.getExpireAt()) {
        assertOnUnsafeInitialSync(doc.getId());
    }

    // Do not create mtab for following cases. Otherwise, we can get into potential race
    // causing recovery procedure to fail with `ErrorCodes::ConflictingServerlessOperation`.
    // 1) The migration was skipped.
    if (doc.getStartGarbageCollect()) {
        invariant(doc.getState() == ShardMergeRecipientStateEnum::kAborted ||
                  doc.getState() == ShardMergeRecipientStateEnum::kCommitted);
        return true;
    }
    // 2) Aborted state doc marked as garbage collectable.
    if (doc.getState() == ShardMergeRecipientStateEnum::kAborted && doc.getExpireAt()) {
        return true;
    }

    auto mtab = std::make_shared<TenantMigrationRecipientAccessBlocker>(opCtx->getServiceContext(),
                                                                        doc.getId());
    TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
        .add(doc.getTenantIds(), mtab);

    switch (doc.getState()) {
        case ShardMergeRecipientStateEnum::kStarted:
        case ShardMergeRecipientStateEnum::kLearnedFilenames:
            break;
        case ShardMergeRecipientStateEnum::kConsistent:
            repl::shard_merge_utils::assertImportDoneMarkerLocalCollExistsOnMergeConsistent(
                opCtx, doc.getId());
            FMT_FALLTHROUGH;
        case ShardMergeRecipientStateEnum::kCommitted:
            if (doc.getExpireAt()) {
                mtab->stopBlockingTTL();
            }
            FMT_FALLTHROUGH;
        case ShardMergeRecipientStateEnum::kAborted:
            if (auto rejectTs = doc.getRejectReadsBeforeTimestamp()) {
                mtab->startRejectingReadsBefore(*rejectTs);
            }
            break;
        default:
            MONGO_UNREACHABLE;
    }

    return true;
}
}  // namespace

void assertOnUnsafeInitialSync(const UUID& migrationId) {
    LOGV2_FATAL_CONTINUE(
        7219900,
        "Terminating this node as it not safe to run initial sync when shard merge is "
        "active. Otherwise, it can lead to data loss.",
        "migrationId"_attr = migrationId);
    uasserted(ErrorCodes::TenantMigrationInProgress,
              "Illegal to run initial sync when shard merge is active");
}

void validateNssIsBeingMigrated(const boost::optional<TenantId>& tenantId,
                                const NamespaceString& nss,
                                const UUID& migrationId) {
    if (!tenantId) {
        uassert(ErrorCodes::InvalidTenantId,
                str::stream() << "Failed to extract a valid tenant from namespace '"
                              << nss.toStringForErrorMsg() << "'.",
                nss.isOnInternalDb());
        return;
    }

    auto mtab = TenantMigrationAccessBlockerRegistry::get(getGlobalServiceContext())
                    .getTenantMigrationAccessBlockerForTenantId(
                        *tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
    uassert(ErrorCodes::InvalidTenantId,
            str::stream() << "The collection '" << nss.toStringForErrorMsg()
                          << "' does not belong to a tenant being migrated.",
            mtab);

    uassert(ErrorCodes::InvalidTenantId,
            str::stream() << "The collection '" << nss.toStringForErrorMsg()
                          << "' is not being migrated in migration " << migrationId,
            mtab->getMigrationId() == migrationId);
}

TenantMigrationDonorDocument parseDonorStateDocument(const BSONObj& doc) {
    auto donorStateDoc =
        TenantMigrationDonorDocument::parse(IDLParserContext("donorStateDoc"), doc);

    if (donorStateDoc.getExpireAt()) {
        uassert(ErrorCodes::BadValue,
                "contains \"expireAt\" but the migration has not committed or aborted",
                donorStateDoc.getState() == TenantMigrationDonorStateEnum::kCommitted ||
                    donorStateDoc.getState() == TenantMigrationDonorStateEnum::kAborted);
    }

    const std::string errmsg = str::stream() << "invalid donor state doc " << doc;

    switch (donorStateDoc.getState()) {
        case TenantMigrationDonorStateEnum::kUninitialized:
            break;
        case TenantMigrationDonorStateEnum::kAbortingIndexBuilds:
            uassert(ErrorCodes::BadValue,
                    errmsg,
                    !donorStateDoc.getBlockTimestamp() && !donorStateDoc.getCommitOrAbortOpTime() &&
                        !donorStateDoc.getAbortReason() &&
                        !donorStateDoc.getStartMigrationDonorTimestamp());
            break;
        case TenantMigrationDonorStateEnum::kDataSync:
            uassert(ErrorCodes::BadValue,
                    errmsg,
                    !donorStateDoc.getBlockTimestamp() && !donorStateDoc.getCommitOrAbortOpTime() &&
                        !donorStateDoc.getAbortReason());
            break;
        case TenantMigrationDonorStateEnum::kBlocking:
            uassert(ErrorCodes::BadValue,
                    errmsg,
                    donorStateDoc.getBlockTimestamp() && !donorStateDoc.getCommitOrAbortOpTime() &&
                        !donorStateDoc.getAbortReason());
            break;
        case TenantMigrationDonorStateEnum::kCommitted:
            uassert(ErrorCodes::BadValue,
                    errmsg,
                    donorStateDoc.getBlockTimestamp() && donorStateDoc.getCommitOrAbortOpTime() &&
                        !donorStateDoc.getAbortReason());
            break;
        case TenantMigrationDonorStateEnum::kAborted:
            uassert(ErrorCodes::BadValue, errmsg, donorStateDoc.getAbortReason());
            break;
        default:
            MONGO_UNREACHABLE;
    }

    return donorStateDoc;
}

SemiFuture<void> checkIfCanRunCommandOrBlock(OperationContext* opCtx,
                                             const DatabaseName& dbName,
                                             const OpMsgRequest& request) {
    if (!repl::ReplicationCoordinator::get(opCtx)->getSettings().isServerless()) {
        return Status::OK();
    }

    // We need to check both donor and recipient access blockers in the case where two
    // migrations happen back-to-back before the old recipient state (from the first
    // migration) is garbage collected.
    auto& blockerRegistry = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext());
    auto mtabPair = blockerRegistry.getAccessBlockersForDbName(dbName);

    if (!mtabPair) {
        return Status::OK();
    }

    // Source to cancel the timeout if the operation completed in time.
    CancellationSource cancelTimeoutSource;
    // Source to cancel waiting on the canRunCommandFuture's.
    CancellationSource cancelCanRunCommandSource(opCtx->getCancellationToken());
    const auto donorMtab = mtabPair->getDonorAccessBlocker();
    const auto recipientMtab = mtabPair->getRecipientAccessBlocker();
    // A vector of futures where the donor access blocker's 'getCanRunCommandFuture' will always
    // precede the recipient's.
    std::vector<ExecutorFuture<void>> futures;
    std::shared_ptr<executor::TaskExecutor> executor;
    if (donorMtab) {
        auto canRunCommandFuture =
            donorMtab->getCanRunCommandFuture(opCtx, request.getCommandName());
        if (canRunCommandFuture.isReady()) {
            auto status = canRunCommandFuture.getNoThrow();
            donorMtab->recordTenantMigrationError(status);
            if (!recipientMtab) {
                return status;
            }
        }
        executor = blockerRegistry.getAsyncBlockingOperationsExecutor();
        futures.emplace_back(std::move(canRunCommandFuture).semi().thenRunOn(executor));
    }
    if (recipientMtab) {
        auto canRunCommandFuture =
            recipientMtab->getCanRunCommandFuture(opCtx, request.getCommandName());
        if (canRunCommandFuture.isReady()) {
            auto status = canRunCommandFuture.getNoThrow();
            recipientMtab->recordTenantMigrationError(status);
            if (!donorMtab) {
                return status;
            }
        }
        executor = blockerRegistry.getAsyncBlockingOperationsExecutor();
        futures.emplace_back(std::move(canRunCommandFuture).semi().thenRunOn(executor));
    }

    if (opCtx->hasDeadline()) {
        // Cancel waiting for operations if we timeout.
        executor->sleepUntil(opCtx->getDeadline(), cancelTimeoutSource.token())
            .getAsync(
                [cancelCanRunCommandSource](auto) mutable { cancelCanRunCommandSource.cancel(); });
    }

    return future_util::withCancellation(whenAll(std::move(futures)),
                                         cancelCanRunCommandSource.token())
        .thenRunOn(executor)
        .then([cancelTimeoutSource, donorMtab, recipientMtab](std::vector<Status> results) mutable {
            cancelTimeoutSource.cancel();
            auto resultIter = results.begin();

            if (donorMtab) {
                auto donorMtabStatus = *resultIter++;
                if (!donorMtabStatus.isOK()) {
                    donorMtab->recordTenantMigrationError(donorMtabStatus);
                    LOGV2(5519301,
                          "Received error while waiting on donor access blocker",
                          "error"_attr = donorMtabStatus);
                    return donorMtabStatus;
                }
            }

            if (recipientMtab) {
                auto recipientMtabStatus = *resultIter;
                if (!recipientMtabStatus.isOK()) {
                    recipientMtab->recordTenantMigrationError(recipientMtabStatus);
                    LOGV2(5519302,
                          "Received error while waiting on recipient access blocker",
                          "error"_attr = recipientMtabStatus);
                    return recipientMtabStatus;
                }
            }

            return Status::OK();
        })
        .onError<ErrorCodes::CallbackCanceled>([cancelTimeoutSource,
                                                cancelCanRunCommandSource,
                                                donorMtab,
                                                recipientMtab,
                                                timeoutError = opCtx->getTimeoutError()](
                                                   Status status) mutable {
            auto isCanceledDueToTimeout = cancelTimeoutSource.token().isCanceled();

            if (!isCanceledDueToTimeout) {
                cancelTimeoutSource.cancel();
            }

            if (isCanceledDueToTimeout) {
                return Status(timeoutError,
                              "Blocked command timed out waiting for an internal data migration "
                              "to commit or abort");
            }

            return status.withContext("Canceled command blocked by internal data migration");
        })
        .semi();  // To require continuation in the user executor.
}

void checkIfLinearizableReadWasAllowedOrThrow(OperationContext* opCtx, const DatabaseName& dbName) {
    if (!repl::ReplicationCoordinator::get(opCtx)->getSettings().isServerless()) {
        return;
    }

    if (repl::ReadConcernArgs::get(opCtx).getLevel() ==
        repl::ReadConcernLevel::kLinearizableReadConcern) {
        // Only the donor access blocker will block linearizable reads.
        if (auto mtab = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                            .getTenantMigrationAccessBlockerForDbName(dbName, MtabType::kDonor)) {
            auto status = mtab->checkIfLinearizableReadWasAllowed(opCtx);
            mtab->recordTenantMigrationError(status);
            uassertStatusOK(status);
        }
    }
}

void checkIfCanWriteOrThrow(OperationContext* opCtx,
                            const DatabaseName& dbName,
                            Timestamp writeTs) {
    if (!repl::ReplicationCoordinator::get(opCtx)->getSettings().isServerless()) {
        return;
    }

    // The migration protocol guarantees the recipient will not get writes until the migration
    // is committed.
    auto mtab = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .getTenantMigrationAccessBlockerForDbName(dbName, MtabType::kDonor);

    if (mtab) {
        auto status = mtab->checkIfCanWrite(writeTs);
        mtab->recordTenantMigrationError(status);
        uassertStatusOK(status);
    }
}

Status checkIfCanBuildIndex(OperationContext* opCtx, const DatabaseName& dbName) {
    if (!repl::ReplicationCoordinator::get(opCtx)->getSettings().isServerless()) {
        return Status::OK();
    }

    // We only block index builds on the donor.
    auto mtab = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .getTenantMigrationAccessBlockerForDbName(dbName, MtabType::kDonor);

    if (mtab) {
        // This log is included for synchronization of the tenant migration buildindex jstests.
        auto status = mtab->checkIfCanBuildIndex();
        mtab->recordTenantMigrationError(status);

        if (MONGO_unlikely(haveCheckedIfIndexBuildableDuringTenantMigration.shouldFail())) {
            LOGV2(5835300,
                  "haveCheckedIfIndexBuildableDuringTenantMigration failpoint enabled",
                  logAttrs(dbName),
                  "status"_attr = status);
        }

        return status;
    }
    return Status::OK();
}

void assertCanOpenChangeStream(OperationContext* opCtx, const DatabaseName& dbName) {
    // We only block opening change streams on the donor.
    auto mtab = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .getTenantMigrationAccessBlockerForDbName(dbName, MtabType::kDonor);
    if (mtab) {
        auto status = mtab->checkIfCanOpenChangeStream();
        mtab->recordTenantMigrationError(status);
        uassertStatusOK(status);
    }
}

void assertCanGetMoreChangeStream(OperationContext* opCtx, const DatabaseName& dbName) {
    if (!repl::ReplicationCoordinator::get(opCtx)->getSettings().isServerless()) {
        return;
    }

    // We only block change stream getMores on the donor.
    auto mtab = TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .getTenantMigrationAccessBlockerForDbName(dbName, MtabType::kDonor);
    if (mtab) {
        auto status = mtab->checkIfCanGetMoreChangeStream();
        mtab->recordTenantMigrationError(status);
        uassertStatusOK(status);
    }
}

bool hasActiveTenantMigration(OperationContext* opCtx, const DatabaseName& dbName) {
    if (dbName.size() == 0) {
        return false;
    }

    if (!repl::ReplicationCoordinator::get(opCtx)->getSettings().isServerless()) {
        return false;
    }

    return bool(TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                    .getAccessBlockersForDbName(dbName));
}

void recoverTenantMigrationAccessBlockers(OperationContext* opCtx) {
    TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext()).clear();

    if (MONGO_unlikely(skipRecoverTenantMigrationAccessBlockers.shouldFail())) {
        return;
    }

    // Recover TenantMigrationDonorAccessBlockers.
    PersistentTaskStore<TenantMigrationDonorDocument> donorStore(
        NamespaceString::kTenantMigrationDonorsNamespace);

    donorStore.forEach(opCtx, {}, [&](const TenantMigrationDonorDocument& doc) {
        return recoverTenantMigrationDonorAccessBlockers(opCtx, doc);
    });

    // Recover TenantMigrationRecipientAccessBlockers.
    PersistentTaskStore<TenantMigrationRecipientDocument> recipientStore(
        NamespaceString::kTenantMigrationRecipientsNamespace);

    recipientStore.forEach(opCtx, {}, [&](const TenantMigrationRecipientDocument& doc) {
        return recoverTenantMigrationRecipientAccessBlockers(opCtx, doc);
    });

    PersistentTaskStore<ShardMergeRecipientDocument> mergeRecipientStore(
        NamespaceString::kShardMergeRecipientsNamespace);

    mergeRecipientStore.forEach(opCtx, {}, [&](const ShardMergeRecipientDocument& doc) {
        return recoverShardMergeRecipientAccessBlockers(opCtx, doc);
    });

    // Recover TenantMigrationDonorAccessBlockers for ShardSplit.
    PersistentTaskStore<ShardSplitDonorDocument> shardSplitDonorStore(
        NamespaceString::kShardSplitDonorsNamespace);

    shardSplitDonorStore.forEach(opCtx, {}, [&](const ShardSplitDonorDocument& doc) {
        // Skip creating a TenantMigrationDonorAccessBlocker for terminal shard split that have been
        // marked as garbage collected.
        if (doc.getExpireAt() &&
            (doc.getState() == ShardSplitDonorStateEnum::kCommitted ||
             doc.getState() == ShardSplitDonorStateEnum::kAborted)) {
            return true;
        }

        auto optionalTenants = doc.getTenantIds();
        invariant(optionalTenants);
        for (const auto& tenantId : optionalTenants.value()) {
            auto mtab = std::make_shared<TenantMigrationDonorAccessBlocker>(
                opCtx->getServiceContext(), doc.getId());
            TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                .add(tenantId, mtab);

            switch (doc.getState()) {
                case ShardSplitDonorStateEnum::kAbortingIndexBuilds:
                    break;
                case ShardSplitDonorStateEnum::kBlocking:
                case ShardSplitDonorStateEnum::kRecipientCaughtUp:
                    invariant(doc.getBlockOpTime());
                    mtab->startBlockingWrites();
                    mtab->startBlockingReadsAfter(doc.getBlockOpTime()->getTimestamp());
                    break;
                case ShardSplitDonorStateEnum::kCommitted:
                    invariant(doc.getBlockOpTime());
                    mtab->startBlockingWrites();
                    mtab->startBlockingReadsAfter(doc.getBlockOpTime()->getTimestamp());
                    mtab->setCommitOpTime(opCtx, doc.getCommitOrAbortOpTime().value());
                    break;
                case ShardSplitDonorStateEnum::kAborted:
                    if (doc.getBlockOpTime()) {
                        mtab->startBlockingWrites();
                        mtab->startBlockingReadsAfter(doc.getBlockOpTime()->getTimestamp());
                    }
                    mtab->setAbortOpTime(opCtx, doc.getCommitOrAbortOpTime().value());
                    break;
                case ShardSplitDonorStateEnum::kUninitialized:
                    MONGO_UNREACHABLE;
            }
        }
        return true;
    });
}

template <typename MigrationConflictInfoType>
Status _handleTenantMigrationConflict(OperationContext* opCtx, const Status& status) {
    auto migrationConflictInfo = status.extraInfo<MigrationConflictInfoType>();
    invariant(migrationConflictInfo);
    auto mtab = migrationConflictInfo->getTenantMigrationAccessBlocker();
    invariant(mtab);
    auto migrationStatus = mtab->waitUntilCommittedOrAborted(opCtx);
    mtab->recordTenantMigrationError(migrationStatus);
    return migrationStatus;
}

Status handleTenantMigrationConflict(OperationContext* opCtx, Status status) {
    if (status == ErrorCodes::NonRetryableTenantMigrationConflict) {
        auto migrationStatus =
            _handleTenantMigrationConflict<NonRetryableTenantMigrationConflictInfo>(opCtx, status);

        // Some operations, like multi updates, can't safely be automatically retried so we return a
        // non retryable error instead of TenantMigrationCommitted/TenantMigrationAborted. If
        // waiting failed for a different reason, e.g. MaxTimeMS expiring, propagate that to the
        // user unchanged.
        if (ErrorCodes::isTenantMigrationError(migrationStatus)) {
            return kNonRetryableTenantMigrationStatus;
        }
        return migrationStatus;
    }

    return _handleTenantMigrationConflict<TenantMigrationConflictInfo>(opCtx, status);
}

void performNoopWrite(OperationContext* opCtx, StringData msg) {
    const auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    AutoGetOplog oplogWrite(opCtx, OplogAccessMode::kWrite);
    uassert(ErrorCodes::NotWritablePrimary,
            "Not primary when performing noop write for {}"_format(msg),
            replCoord->canAcceptWritesForDatabase(opCtx, DatabaseName::kAdmin));

    writeConflictRetry(
        opCtx, "performNoopWrite", NamespaceString::kRsOplogNamespace, [&opCtx, &msg] {
            WriteUnitOfWork wuow(opCtx);
            opCtx->getClient()->getServiceContext()->getOpObserver()->onOpMessage(
                opCtx, BSON("msg" << msg));
            wuow.commit();
        });
}

bool inRecoveryMode(OperationContext* opCtx) {
    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    if (!replCoord->getSettings().isReplSet()) {
        return false;
    }

    auto memberState = replCoord->getMemberState();

    return memberState.startup() || memberState.startup2() || memberState.rollback();
}

bool shouldExclude(OperationContext* opCtx) {
    return repl::tenantMigrationInfo(opCtx) || opCtx->getClient()->isInDirectClient() ||
        (opCtx->getClient()->session() && opCtx->getClient()->isInternalClient());
}

std::string getTenantPrefix(StringData prefixedDb) {
    const auto pos = prefixedDb.find('_');
    if (pos == std::string::npos || pos == 0) {
        return "";
    }
    return prefixedDb.substr(0, pos).toString();
}

boost::optional<TenantId> parseTenantIdFromDatabaseName(const DatabaseName& dbName) {
    if (gMultitenancySupport) {
        return dbName.tenantId();
    }

    const auto tenantStr = getTenantPrefix(DatabaseNameUtil::serialize(dbName));
    if (tenantStr.empty()) {
        // Not a tenant database.
        return boost::none;
    }

    const auto statusWithOID = OID::parse(tenantStr);
    if (!statusWithOID.isOK()) {
        return boost::none;
    }
    return TenantId(statusWithOID.getValue());
}

boost::optional<std::string> extractTenantFromDatabaseName(const DatabaseName& dbName) {
    if (gMultitenancySupport) {
        if (dbName.tenantId()) {
            return dbName.tenantId()->toString();
        } else {
            return boost::none;
        }
    }

    const auto tenantStr = getTenantPrefix(DatabaseNameUtil::serialize(dbName));
    if (tenantStr.empty()) {
        // Not a tenant database.
        return boost::none;
    }
    return tenantStr;
}

}  // namespace tenant_migration_access_blocker

}  // namespace mongo
