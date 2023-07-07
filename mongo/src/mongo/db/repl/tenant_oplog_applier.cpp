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

#include "mongo/db/repl/tenant_oplog_applier.h"

#include <absl/container/node_hash_map.h>
#include <boost/cstdint.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <boost/smart_ptr.hpp>
#include <cstdint>
// IWYU pragma: no_include "ext/alloc_traits.h"
#include <algorithm>
#include <cstddef>
#include <mutex>
#include <set>
#include <tuple>
#include <type_traits>

#include "mongo/base/error_codes.h"
#include "mongo/bson/oid.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/cloner_utils.h"
#include "mongo/db/repl/oplog_applier.h"
#include "mongo/db/repl/oplog_applier_utils.h"
#include "mongo/db/repl/oplog_entry_gen.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/repl/repl_server_parameters_gen.h"
#include "mongo/db/repl/session_update_tracker.h"
#include "mongo/db/repl/tenant_migration_access_blocker_util.h"
#include "mongo/db/repl/tenant_migration_decoration.h"
#include "mongo/db/repl/tenant_migration_recipient_service.h"
#include "mongo/db/repl/tenant_oplog_batcher.h"
#include "mongo/db/session/logical_session_id_gen.h"
#include "mongo/db/session/logical_session_id_helpers.h"
#include "mongo/db/session/session_catalog_mongod.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/tenant_id.h"
#include "mongo/db/transaction/transaction_participant.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/concurrency/thread_pool.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTenantMigration

namespace mongo {
namespace repl {

MONGO_FAIL_POINT_DEFINE(hangInTenantOplogApplication);
MONGO_FAIL_POINT_DEFINE(fpBeforeTenantOplogApplyingBatch);

enum OplogEntryType {
    kOplogEntryTypeTransaction,
    kOplogEntryTypePartialTransaction,
    kOplogEntryTypeRetryableWrite,
    kOplogEntryTypeRetryableWritePrePostImage,
    kOplogEntryTypePreviouslyWrappedRetryableWrite,
};
OplogEntryType getOplogEntryType(const OplogEntry& entry) {
    // Final applyOp for a transaction.
    if (entry.getTxnNumber() && !entry.isPartialTransaction() &&
        (entry.getCommandType() == repl::OplogEntry::CommandType::kCommitTransaction ||
         entry.getCommandType() == repl::OplogEntry::CommandType::kApplyOps)) {
        return OplogEntryType::kOplogEntryTypeTransaction;
    }

    // If it has a statement id but isn't a transaction, it's a retryable write.
    const auto isRetryableWriteEntry =
        !entry.getStatementIds().empty() && !SessionUpdateTracker::isTransactionEntry(entry);

    // There are two types of no-ops we expect here. One is pre/post image, which will have an empty
    // o2 field. The other is previously transformed retryable write entries from earlier
    // migrations, which we should avoid re-wrapping.
    if (isRetryableWriteEntry && entry.getOpType() == repl::OpTypeEnum::kNoop) {
        if (entry.getObject2()) {
            return OplogEntryType::kOplogEntryTypePreviouslyWrappedRetryableWrite;
        }

        return OplogEntryType::kOplogEntryTypeRetryableWritePrePostImage;
    }

    if (isRetryableWriteEntry) {
        return OplogEntryType::kOplogEntryTypeRetryableWrite;
    }

    return OplogEntryType::kOplogEntryTypePartialTransaction;
};

TenantOplogApplier::TenantOplogApplier(const UUID& migrationUuid,
                                       const MigrationProtocolEnum& protocol,
                                       const OpTime& startApplyingAfterOpTime,
                                       const OpTime& cloneFinishedRecipientOpTime,
                                       boost::optional<std::string> tenantId,
                                       RandomAccessOplogBuffer* oplogBuffer,
                                       std::shared_ptr<executor::TaskExecutor> executor,
                                       ThreadPool* writerPool,
                                       Timestamp resumeBatchingTs)
    : AbstractAsyncComponent(executor.get(),
                             std::string("TenantOplogApplier_") + migrationUuid.toString()),
      _migrationUuid(migrationUuid),
      _protocol(protocol),
      _startApplyingAfterOpTime(startApplyingAfterOpTime),
      _cloneFinishedRecipientOpTime(cloneFinishedRecipientOpTime),
      _tenantId(tenantId),
      _oplogBuffer(oplogBuffer),
      _executor(std::move(executor)),
      _writerPool(writerPool),
      _resumeBatchingTs(resumeBatchingTs),
      _options([&] {
          switch (protocol) {
              case MigrationProtocolEnum::kMultitenantMigrations:
                  // Since multi-tenant migration uses logical cloning, the oplog entries will be
                  // applied on a inconsistent copy of donor data. Hence, using
                  // OplogApplication::Mode::kInitialSync.
                  return OplogApplication::Mode::kInitialSync;
              case MigrationProtocolEnum::kShardMerge:
                  // Since shard merge  uses backup cursor for database cloning and tenant oplog
                  // catchup phase is not resumable on failovers, the oplog entries will be applied
                  // on a consistent copy of donor data. Hence, using
                  // OplogApplication::Mode::kSecondary.
                  return OplogApplication::Mode::kSecondary;
              default:
                  MONGO_UNREACHABLE;
          }
      }()) {
    invariant(!_cloneFinishedRecipientOpTime.isNull());
    if (_protocol != MigrationProtocolEnum::kShardMerge) {
        invariant(_tenantId);
    } else {
        invariant(!_tenantId);
    }
}

TenantOplogApplier::~TenantOplogApplier() {
    shutdown();
    join();
}

SemiFuture<TenantOplogApplier::OpTimePair> TenantOplogApplier::getNotificationForOpTime(
    OpTime donorOpTime) {
    stdx::lock_guard lk(_mutex);
    // If we're not running, return a future with the status we shut down with.
    if (!_isActive_inlock()) {
        return SemiFuture<OpTimePair>::makeReady(_finalStatus);
    }
    // If this optime has already passed, just return a ready future.
    if (_lastAppliedOpTimesUpToLastBatch.donorOpTime >= donorOpTime ||
        _startApplyingAfterOpTime >= donorOpTime) {
        return SemiFuture<OpTimePair>::makeReady(_lastAppliedOpTimesUpToLastBatch);
    }

    // This will pull a new future off the existing promise for this time if it exists, otherwise
    // it constructs a new promise and pulls a future off of it.
    auto [iter, isNew] = _opTimeNotificationList.try_emplace(donorOpTime);
    return iter->second.getFuture().semi();
}

OpTime TenantOplogApplier::getStartApplyingAfterOpTime() const {
    return _startApplyingAfterOpTime;
}

Timestamp TenantOplogApplier::getResumeBatchingTs() const {
    return _resumeBatchingTs;
}

void TenantOplogApplier::_doStartup_inlock() {
    _oplogBatcher = std::make_shared<TenantOplogBatcher>(
        _migrationUuid, _oplogBuffer, _executor, _resumeBatchingTs, _startApplyingAfterOpTime);
    uassertStatusOK(_oplogBatcher->startup());
    auto fut = _oplogBatcher->getNextBatch(
        TenantOplogBatcher::BatchLimits(std::size_t(tenantApplierBatchSizeBytes.load()),
                                        std::size_t(tenantApplierBatchSizeOps.load())));
    std::move(fut)
        .thenRunOn(_executor)
        .then([this, self = shared_from_this()](TenantOplogBatch batch) {
            _applyLoop(std::move(batch));
        })
        .onError([this, self = shared_from_this()](Status status) {
            invariant(_shouldStopApplying(status));
        })
        .getAsync([](auto status) {});
}

void TenantOplogApplier::_setFinalStatusIfOk(WithLock, Status newStatus) {
    if (_finalStatus.isOK()) {
        _finalStatus = newStatus;
    }
}

void TenantOplogApplier::_doShutdown_inlock() noexcept {
    // Shutting down the oplog batcher will make the _applyLoop stop with an error future, thus
    // shutting down the applier.
    _oplogBatcher->shutdown();
    // Oplog applier executor can shutdown before executing _applyLoop() and shouldStopApplying().
    // This can cause the applier to miss notifying the waiters in _opTimeNotificationList. So,
    // shutdown() is responsible to notify those waiters when _applyLoop() is not running.
    if (!_applyLoopApplyingBatch) {
        // We actually hold the required lock, but the lock object itself is not passed through.
        _finishShutdown(WithLock::withoutLock(),
                        {ErrorCodes::CallbackCanceled, "Tenant oplog applier shut down"});
    }
}

void TenantOplogApplier::_preJoin() noexcept {
    if (_oplogBatcher) {
        _oplogBatcher->join();
    }
}

void TenantOplogApplier::_applyLoop(TenantOplogBatch batch) {
    {
        stdx::lock_guard lk(_mutex);
        // Applier is not active as someone might have called shutdown().
        if (!_isActive_inlock())
            return;
        _applyLoopApplyingBatch = true;
    }

    // Getting the future for the next batch here means the batcher can retrieve the next batch
    // while the applier is processing the current one.
    auto nextBatchFuture = _oplogBatcher->getNextBatch(
        TenantOplogBatcher::BatchLimits(std::size_t(tenantApplierBatchSizeBytes.load()),
                                        std::size_t(tenantApplierBatchSizeOps.load())));

    Status applyStatus{Status::OK()};
    try {
        _applyOplogBatch(&batch);
    } catch (const DBException& e) {
        applyStatus = e.toStatus();
    }

    if (_shouldStopApplying(applyStatus)) {
        return;
    }

    std::move(nextBatchFuture)
        .thenRunOn(_executor)
        .then([this, self = shared_from_this()](TenantOplogBatch batch) {
            _applyLoop(std::move(batch));
        })
        .onError([this, self = shared_from_this()](Status status) {
            invariant(_shouldStopApplying(status));
        })
        .getAsync([](auto status) {});
}

bool TenantOplogApplier::_shouldStopApplying(Status status) {
    {
        stdx::lock_guard lk(_mutex);
        _applyLoopApplyingBatch = false;

        if (!_isActive_inlock()) {
            return true;
        }

        if (_isShuttingDown_inlock()) {
            _finishShutdown(lk,
                            {ErrorCodes::CallbackCanceled, "Tenant oplog applier shutting down"});
            return true;
        }

        dassert(_finalStatus.isOK());
        // Set the _finalStatus. This guarantees that the shutdown() called after releasing
        // the mutex will signal donor opTime waiters with the 'status' error code and not with
        // ErrorCodes::CallbackCanceled.
        _setFinalStatusIfOk(lk, status);
        if (_finalStatus.isOK()) {
            return false;
        }
    }
    shutdown();
    return true;
}

bool TenantOplogApplier::_shouldIgnore(const OplogEntry& entry) {
    if (_protocol == MigrationProtocolEnum::kMultitenantMigrations) {
        return false;
    }

    const auto tenantId =
        tenant_migration_access_blocker::parseTenantIdFromDatabaseName(entry.getNss().dbName());
    tenant_migration_access_blocker::validateNssIsBeingMigrated(
        tenantId, entry.getNss(), _migrationUuid);

    return !tenantId;
}

void TenantOplogApplier::_finishShutdown(WithLock lk, Status status) {
    // shouldStopApplying() might have already set the final status. So, don't mask the original
    // error.
    _setFinalStatusIfOk(lk, status);
    LOGV2_DEBUG(4886005,
                1,
                "TenantOplogApplier::_finishShutdown",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "error"_attr = redact(_finalStatus));

    invariant(!_finalStatus.isOK());
    // Any unfulfilled notifications are errored out.
    for (auto& listEntry : _opTimeNotificationList) {
        listEntry.second.setError(_finalStatus);
    }
    _opTimeNotificationList.clear();
    _transitionToComplete_inlock();
}

void TenantOplogApplier::_applyOplogBatch(TenantOplogBatch* batch) {
    LOGV2_DEBUG(4886004,
                1,
                "Tenant Oplog Applier starting to apply batch",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "firstDonorOptime"_attr = batch->ops.front().entry.getOpTime(),
                "lastDonorOptime"_attr = batch->ops.back().entry.getOpTime());
    auto opCtx = cc().makeOperationContext();
    _checkNsAndUuidsBelongToTenant(opCtx.get(), *batch);
    auto writerVectors = _fillWriterVectors(opCtx.get(), batch);
    std::vector<Status> statusVector(writerVectors.size(), Status::OK());
    for (size_t i = 0; i < writerVectors.size(); i++) {
        if (writerVectors[i].empty())
            continue;

        _writerPool->schedule([this, &writer = writerVectors.at(i), &status = statusVector.at(i)](
                                  auto scheduleStatus) {
            if (!scheduleStatus.isOK()) {
                status = scheduleStatus;
            } else {
                status = _applyOplogBatchPerWorker(&writer);
            }
        });
    }
    _writerPool->waitForIdle();

    // Make sure all the workers succeeded.
    for (const auto& status : statusVector) {
        if (!status.isOK()) {
            LOGV2_ERROR(4886012,
                        "Failed to apply operation in tenant migration",
                        "protocol"_attr = _protocol,
                        "migrationId"_attr = _migrationUuid,
                        "error"_attr = redact(status));
        }
        uassertStatusOK(status);
    }

    fpBeforeTenantOplogApplyingBatch.pauseWhileSet();

    LOGV2_DEBUG(4886011,
                1,
                "Tenant Oplog Applier starting to write no-ops",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid);
    auto lastBatchCompletedOpTimes = _writeNoOpEntries(opCtx.get(), *batch);
    stdx::lock_guard lk(_mutex);
    _lastAppliedOpTimesUpToLastBatch.donorOpTime = lastBatchCompletedOpTimes.donorOpTime;
    // If the batch contains only resume token no-ops, then the last batch completed
    // recipient optime returned will be null.
    if (!lastBatchCompletedOpTimes.recipientOpTime.isNull()) {
        _lastAppliedOpTimesUpToLastBatch.recipientOpTime =
            lastBatchCompletedOpTimes.recipientOpTime;
    }

    _numOpsApplied += batch->ops.size();

    LOGV2_DEBUG(4886002,
                1,
                "Tenant Oplog Applier finished applying batch",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "lastBatchCompletedOpTimes"_attr = lastBatchCompletedOpTimes);

    // Notify all the waiters on optimes before and including _lastAppliedOpTimesUpToLastBatch.
    auto firstUnexpiredIter =
        _opTimeNotificationList.upper_bound(_lastAppliedOpTimesUpToLastBatch.donorOpTime);
    for (auto iter = _opTimeNotificationList.begin(); iter != firstUnexpiredIter; iter++) {
        iter->second.emplaceValue(_lastAppliedOpTimesUpToLastBatch);
    }
    _opTimeNotificationList.erase(_opTimeNotificationList.begin(), firstUnexpiredIter);

    hangInTenantOplogApplication.executeIf(
        [&](const BSONObj& data) {
            LOGV2(
                5272315,
                "hangInTenantOplogApplication failpoint enabled -- blocking until it is disabled.",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "lastBatchCompletedOpTimes"_attr = lastBatchCompletedOpTimes);
            hangInTenantOplogApplication.pauseWhileSet(opCtx.get());
        },
        [&](const BSONObj& data) { return !lastBatchCompletedOpTimes.recipientOpTime.isNull(); });
}

void TenantOplogApplier::_checkNsAndUuidsBelongToTenant(OperationContext* opCtx,
                                                        const TenantOplogBatch& batch) {

    // Shard merge protocol checks the namespace and UUID when ops are assigned to writer pool.
    if (_protocol == MigrationProtocolEnum::kShardMerge)
        return;

    auto checkNsAndUuid = [&](const OplogEntry& op) {
        if (!op.getNss().isEmpty() && !ClonerUtils::isNamespaceForTenant(op.getNss(), *_tenantId)) {
            LOGV2_ERROR(4886015,
                        "Namespace does not belong to tenant being migrated",
                        "tenant"_attr = *_tenantId,
                        "migrationId"_attr = _migrationUuid,
                        logAttrs(op.getNss()));
            uasserted(4886016, "Namespace does not belong to tenant being migrated");
        }
        if (!op.getUuid())
            return;
        if (_knownGoodUuids.find(*op.getUuid()) != _knownGoodUuids.end())
            return;
        try {
            auto nss = OplogApplierUtils::parseUUIDOrNs(opCtx, op);
            if (!ClonerUtils::isNamespaceForTenant(nss, *_tenantId)) {
                LOGV2_ERROR(4886013,
                            "UUID does not belong to tenant being migrated",
                            "tenant"_attr = *_tenantId,
                            "migrationId"_attr = _migrationUuid,
                            "UUID"_attr = *op.getUuid(),
                            logAttrs(nss));
                uasserted(4886014, "UUID does not belong to tenant being migrated");
            }
            _knownGoodUuids.insert(*op.getUuid());
        } catch (const ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
            LOGV2_DEBUG(4886017,
                        2,
                        "UUID for tenant being migrated does not exist",
                        "tenant"_attr = *_tenantId,
                        "migrationId"_attr = _migrationUuid,
                        "UUID"_attr = *op.getUuid(),
                        logAttrs(op.getNss()));
        }
    };

    for (const auto& op : batch.ops) {
        if (op.expansionsEntry < 0 && !op.entry.isPartialTransaction())
            checkNsAndUuid(op.entry);
    }
    for (const auto& expansion : batch.expansions) {
        for (const auto& op : expansion) {
            checkNsAndUuid(op);
        }
    }
}

namespace {
bool isResumeTokenNoop(const OplogEntry& entry) {
    if (entry.getOpType() != OpTypeEnum::kNoop) {
        return false;
    }
    if (!entry.getObject().hasField("msg")) {
        return false;
    }
    if (entry.getObject().getStringField("msg") != TenantMigrationRecipientService::kNoopMsg) {
        return false;
    }
    return true;
}
}  // namespace

void TenantOplogApplier::_writeRetryableWriteEntryNoOp(
    OperationContext* opCtx,
    MutableOplogEntry& noopEntry,
    const OplogEntry& entry,
    const boost::optional<MutableOplogEntry>& prePostImageEntry,
    const OpTime& originalPrePostImageOpTime) {

    auto sessionId = *entry.getSessionId();
    auto txnNumber = *entry.getTxnNumber();
    auto stmtIds = entry.getStatementIds();
    LOGV2_DEBUG(5351000,
                2,
                "Tenant Oplog Applier processing retryable write",
                "entry"_attr = redact(entry.toBSONForLogging()),
                "sessionId"_attr = sessionId,
                "txnNumber"_attr = txnNumber,
                "statementIds"_attr = stmtIds,
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid);

    const auto hasPreOrPostImageOpTime = entry.getPreImageOpTime() || entry.getPostImageOpTime();
    if (prePostImageEntry && entry.getPreImageOpTime()) {
        uassert(5351002,
                str::stream()
                    << "Tenant oplog application cannot apply retryable write with txnNumber  "
                    << txnNumber << " statementNumber " << stmtIds.front() << " on session "
                    << sessionId << " because the preImage op time "
                    << originalPrePostImageOpTime.toString()
                    << " does not match the expected optime "
                    << entry.getPreImageOpTime()->toString(),
                originalPrePostImageOpTime == entry.getPreImageOpTime());
        noopEntry.setPreImageOpTime(prePostImageEntry->getOpTime());
    } else if (prePostImageEntry && entry.getPostImageOpTime()) {
        uassert(5351007,
                str::stream()
                    << "Tenant oplog application cannot apply retryable write with txnNumber  "
                    << txnNumber << " statementNumber " << stmtIds.front() << " on session "
                    << sessionId << " because the postImage op time "
                    << originalPrePostImageOpTime.toString()
                    << " does not match the expected optime "
                    << entry.getPostImageOpTime()->toString(),
                originalPrePostImageOpTime == entry.getPostImageOpTime());
        noopEntry.setPostImageOpTime(prePostImageEntry->getOpTime());
    } else if (!prePostImageEntry && hasPreOrPostImageOpTime) {
        LOGV2(5535302,
              "Tenant Oplog Applier omitting pre- or post- image for findAndModify",
              "entry"_attr = redact(entry.toBSONForLogging()),
              "protocol"_attr = _protocol,
              "migrationId"_attr = _migrationUuid);
    }

    auto txnParticipant = TransactionParticipant::get(opCtx);
    uassert(5350900,
            str::stream() << "Tenant oplog application failed to get retryable write "
                             "for transaction "
                          << txnNumber << " on session " << sessionId,
            txnParticipant);

    TxnNumberAndRetryCounter txnNumberAndRetryCounter{txnNumber};
    if (txnParticipant.getLastWriteOpTime() > _cloneFinishedRecipientOpTime) {
        // Out-of-order processing within a migration lifetime is not possible,
        // except in recipient failovers. However, merge and tenant migration
        // are not resilient to recipient failovers. If attempted, beginOrContinue()
        // will throw ErrorCodes::TransactionTooOld.
        txnParticipant.beginOrContinue(opCtx,
                                       txnNumberAndRetryCounter,
                                       boost::none /* autocommit */,
                                       boost::none /* startTransaction */);
        noopEntry.setPrevWriteOpTimeInTransaction(txnParticipant.getLastWriteOpTime());
    } else {
        // We can end up here under the following circumstances:
        // 1) LastWriteOpTime is not null.
        //    - During a back-to-back migration (rs0->rs1->rs0) or a migration retry,
        //      when 'txnNum'== txnParticipant.o().activeTxnNumber and rs0 already has
        //      the oplog chain.
        //
        // 2) LastWriteOpTime is null.
        //    - During a back-to-back migration (rs0->rs1->rs0) when
        //      'txnNum' < txnParticipant.o().activeTxnNumber and last activeTxnNumber corresponds
        //      to a no-op session write, like, no-op retryable update, read transaction, etc.
        //    - New session with no transaction started yet on this node (this will be a no-op).
        LOGV2_DEBUG(5709800,
                    2,
                    "Tenant oplog applier resetting existing retryable write state",
                    "lastWriteOpTime"_attr = txnParticipant.getLastWriteOpTime(),
                    "lastActiveTxnNumber"_attr =
                        txnParticipant.getActiveTxnNumberAndRetryCounter().toBSON());

        // Reset the statements executed list in the txnParticipant.
        txnParticipant.invalidate(opCtx);
        txnParticipant.refreshFromStorageIfNeededNoOplogEntryFetch(opCtx);

        txnParticipant.beginOrContinue(opCtx,
                                       txnNumberAndRetryCounter,
                                       boost::none /* autocommit */,
                                       boost::none /* startTransaction */);

        // Reset the retryable write history chain.
        noopEntry.setPrevWriteOpTimeInTransaction(OpTime());
    }

    // We should never process the same donor statement twice, except in failover
    // cases where we'll also have "forgotten" the statement was executed.
    uassert(5350902,
            str::stream() << "Tenant oplog application processed same retryable write "
                             "twice for transaction "
                          << txnNumber << " statement " << stmtIds.front() << " on session "
                          << sessionId,
            !txnParticipant.checkStatementExecutedNoOplogEntryFetch(opCtx, stmtIds.front()));

    // Set sessionId, txnNumber, and statementId for all ops in a retryable write.
    noopEntry.setSessionId(sessionId);
    noopEntry.setTxnNumber(txnNumber);
    noopEntry.setStatementIds(stmtIds);

    // set fromMigrate on the no-op so the session update tracker recognizes it.
    noopEntry.setFromMigrate(true);

    // Use the same wallclock time as the noop entry.  The lastWriteOpTime will be filled
    // in after the no-op is written.
    auto sessionTxnRecord =
        SessionTxnRecord{sessionId, txnNumber, OpTime(), noopEntry.getWallClockTime()};

    // If we have a prePostImage no-op without the original entry, do not write it. This can
    // happen in some very unlikely rollback situations.
    auto isValidPrePostImageEntry = prePostImageEntry && hasPreOrPostImageOpTime;

    _writeSessionNoOp(opCtx,
                      noopEntry,
                      sessionTxnRecord,
                      stmtIds,
                      isValidPrePostImageEntry ? prePostImageEntry : boost::none);
}

void TenantOplogApplier::_writeTransactionEntryNoOp(OperationContext* opCtx,
                                                    MutableOplogEntry& noopEntry,
                                                    const OplogEntry& entry) {
    auto sessionId = *entry.getSessionId();
    auto txnNumber = *entry.getTxnNumber();
    auto optTxnRetryCounter = entry.getOperationSessionInfo().getTxnRetryCounter();
    uassert(ErrorCodes::InvalidOptions,
            "txnRetryCounter is only supported in sharded clusters",
            !optTxnRetryCounter.has_value());

    LOGV2_DEBUG(5351502,
                1,
                "Tenant Oplog Applier committing transaction",
                "sessionId"_attr = sessionId,
                "txnNumber"_attr = txnNumber,
                "txnRetryCounter"_attr = optTxnRetryCounter,
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "op"_attr = redact(entry.toBSONForLogging()));

    auto txnParticipant = TransactionParticipant::get(opCtx);
    uassert(5351500,
            str::stream() << "Tenant oplog application failed to get transaction participant "
                             "for transaction "
                          << txnNumber << " on session " << sessionId,
            txnParticipant);
    // We should only write the noop entry for this transaction commit once.
    uassert(5351501,
            str::stream() << "Tenant oplog application cannot apply transaction " << txnNumber
                          << " on session " << sessionId
                          << " because the transaction with txnNumberAndRetryCounter "
                          << txnParticipant.getActiveTxnNumberAndRetryCounter().toBSON()
                          << " has already started",
            txnParticipant.getActiveTxnNumberAndRetryCounter().getTxnNumber() < txnNumber);
    txnParticipant.beginOrContinueTransactionUnconditionally(opCtx,
                                                             {txnNumber, optTxnRetryCounter});

    // Only set sessionId, txnNumber and txnRetryCounter for the final applyOp in a
    // transaction.
    noopEntry.setSessionId(sessionId);
    noopEntry.setTxnNumber(txnNumber);
    noopEntry.getOperationSessionInfo().setTxnRetryCounter(optTxnRetryCounter);

    // Write a fake applyOps with the tenantId as the namespace so that this will be picked
    // up by the committed transaction prefetch pipeline in subsequent migrations.
    //
    // Unlike MTM, shard merge copies all tenants from the donor. This means that merge does
    // not need to filter prefetched committed transactions by tenantId. As a result,
    // setting a nss containing the tenantId for the fake transaction applyOps entry isn't
    // necessary.
    if (_protocol != MigrationProtocolEnum::kShardMerge) {
        noopEntry.setObject(
            BSON("applyOps" << BSON_ARRAY(BSON(OplogEntry::kNssFieldName
                                               << NamespaceString(*_tenantId + "_", "").ns()))));
    }

    // Use the same wallclock time as the noop entry.
    auto sessionTxnRecord =
        SessionTxnRecord{sessionId, txnNumber, OpTime(), noopEntry.getWallClockTime()};
    sessionTxnRecord.setState(DurableTxnStateEnum::kCommitted);
    sessionTxnRecord.setTxnRetryCounter(optTxnRetryCounter);

    _writeSessionNoOp(opCtx, noopEntry, sessionTxnRecord);
}

TenantOplogApplier::OpTimePair TenantOplogApplier::_writeNoOpEntries(
    OperationContext* opCtx, const TenantOplogBatch& batch) {
    auto* opObserver = cc().getServiceContext()->getOpObserver();

    // Group donor oplog entries from the same session together.
    LogicalSessionIdMap<std::vector<TenantNoOpEntry>> sessionOps;
    // All other oplog entries.
    std::vector<TenantNoOpEntry> nonSessionOps;

    // The 'opCtx' must be interruptible on stepdown and stepup to avoid a deadlock situation with
    // the RSTL.
    opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();

    // Prevent the node from being able to change state when reserving oplog slots and writing
    // entries.
    AutoGetOplog oplogWrite(opCtx, OplogAccessMode::kWrite);

    // We start WriteUnitOfWork only to reserve oplog slots. So, it's ok to abort the
    // WriteUnitOfWork when it goes out of scope.
    WriteUnitOfWork wuow(opCtx);
    // Reserve oplog slots for all entries.  This allows us to write them in parallel.
    auto oplogSlots = repl::getNextOpTimes(opCtx, batch.ops.size());
    // Keep track of the greatest oplog slot actually used, ignoring resume token noops. This is
    // what we want to return from this function.
    auto greatestOplogSlotUsed = OpTime();
    auto slotIter = oplogSlots.begin();
    for (const auto& op : batch.ops) {
        if (isResumeTokenNoop(op.entry) || op.ignore) {
            // Since we won't apply resume token noop oplog entries and internal collection
            // oplog entries (for shard merge protocol), we do not want to set the recipient optime
            // for them.
            invariant(!op.ignore || _protocol == MigrationProtocolEnum::kShardMerge);
            slotIter++;
            continue;
        }
        // Group oplog entries from the same session for noop writes.
        if (auto sessionId = op.entry.getOperationSessionInfo().getSessionId()) {
            uassert(
                ErrorCodes::RetryableInternalTransactionNotSupported,
                str::stream() << "Retryable internal transactions are not supported. Protocol:: "
                              << MigrationProtocol_serializer(_protocol)
                              << ", SessionId:: " << sessionId->toBSON(),
                !isInternalSessionForRetryableWrite(*sessionId));
            sessionOps[*sessionId].emplace_back(&op.entry, slotIter);
        } else {
            nonSessionOps.emplace_back(&op.entry, slotIter);
        }
        greatestOplogSlotUsed = *slotIter++;
    }

    const size_t numOplogThreads = _writerPool->getStats().options.maxThreads;
    const size_t numOpsPerThread = std::max(std::size_t(minOplogEntriesPerThread.load()),
                                            (nonSessionOps.size() / numOplogThreads));
    LOGV2_DEBUG(4886003,
                1,
                "Tenant Oplog Applier scheduling no-ops ",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "firstDonorOptime"_attr = batch.ops.front().entry.getOpTime(),
                "lastDonorOptime"_attr = batch.ops.back().entry.getOpTime(),
                "numOplogThreads"_attr = numOplogThreads,
                "numOpsPerThread"_attr = numOpsPerThread,
                "numOplogEntries"_attr = batch.ops.size(),
                "numSessionsInBatch"_attr = sessionOps.size());

    // Vector to store errors from each writer thread. The first numOplogThreads entries store
    // errors from the noop writes for non-session oplog entries. And the rest store errors from the
    // noop writes for each session in the batch.
    std::vector<Status> statusVector(numOplogThreads + sessionOps.size(), Status::OK());

    // Dispatch noop writes for non-session oplog entries into numOplogThreads writer threads.
    auto opsIter = nonSessionOps.begin();
    size_t numOpsRemaining = nonSessionOps.size();
    for (size_t thread = 0; thread < numOplogThreads && opsIter != nonSessionOps.end(); thread++) {
        auto numOps = std::min(numOpsPerThread, numOpsRemaining);
        if (thread == numOplogThreads - 1) {
            numOps = numOpsRemaining;
        }
        _writerPool->schedule([=, this, &status = statusVector.at(thread)](auto scheduleStatus) {
            if (!scheduleStatus.isOK()) {
                status = scheduleStatus;
            } else {
                try {
                    _writeNoOpsForRange(opObserver, opsIter, opsIter + numOps);
                } catch (const DBException& e) {
                    status = e.toStatus();
                }
            }
        });
        opsIter += numOps;
        numOpsRemaining -= numOps;
    }
    invariant(opsIter == nonSessionOps.end());

    // Dispatch noop writes for oplog entries from the same session into the same writer thread.
    size_t sessionThreadNum = 0;
    for (const auto& s : sessionOps) {
        _writerPool->schedule(
            [=, this, &status = statusVector.at(numOplogThreads + sessionThreadNum)](
                auto scheduleStatus) {
                if (!scheduleStatus.isOK()) {
                    status = scheduleStatus;
                } else {
                    try {
                        _writeSessionNoOpsForRange(s.second.begin(), s.second.end());
                    } catch (const DBException& e) {
                        status = e.toStatus();
                    }
                }
            });
        sessionThreadNum++;
    }

    _writerPool->waitForIdle();

    // Make sure all the workers succeeded.
    for (const auto& status : statusVector) {
        if (!status.isOK()) {
            LOGV2_ERROR(5333900,
                        "Failed to write noop in tenant migration",
                        "protocol"_attr = _protocol,
                        "migrationId"_attr = _migrationUuid,
                        "error"_attr = redact(status));
        }
        uassertStatusOK(status);
    }
    return {batch.ops.back().entry.getOpTime(), greatestOplogSlotUsed};
}

void TenantOplogApplier::_writeSessionNoOp(OperationContext* opCtx,
                                           MutableOplogEntry& noopEntry,
                                           boost::optional<SessionTxnRecord> sessionTxnRecord,
                                           std::vector<StmtId> stmtIds,
                                           boost::optional<MutableOplogEntry> prePostImageEntry) {
    LOGV2_DEBUG(5535700,
                2,
                "Tenant Oplog Applier writing session no-op",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "op"_attr = redact(noopEntry.toBSON()));

    AutoGetOplog oplogWrite(opCtx, OplogAccessMode::kWrite);
    boost::optional<Lock::TenantLock> tenantLock;
    if (auto tid = noopEntry.getTid()) {
        tenantLock.emplace(opCtx, *tid, MODE_IX);
    }

    writeConflictRetry(opCtx, "writeTenantNoOps", NamespaceString::kRsOplogNamespace, [&] {
        WriteUnitOfWork wuow(opCtx);

        // Write the pre/post image entry, if it exists.
        if (prePostImageEntry)
            repl::logOp(opCtx, &*prePostImageEntry);
        // Write the noop entry and update config.transactions.
        auto oplogOpTime = repl::logOp(opCtx, &noopEntry);
        if (sessionTxnRecord) {
            sessionTxnRecord->setLastWriteOpTime(oplogOpTime);
            TransactionParticipant::get(opCtx).onWriteOpCompletedOnPrimary(
                opCtx, {stmtIds}, *sessionTxnRecord);
        }

        wuow.commit();
    });
}

void TenantOplogApplier::_writeSessionNoOpsForRange(
    std::vector<TenantNoOpEntry>::const_iterator begin,
    std::vector<TenantNoOpEntry>::const_iterator end) {
    auto opCtx = cc().makeOperationContext();
    tenantMigrationInfo(opCtx.get()) = boost::make_optional<TenantMigrationInfo>(_migrationUuid);

    // Since the client object persists across each noop write call and the same writer thread could
    // be reused to write noop entries with older optime, we need to clear the lastOp associated
    // with the client to avoid the invariant in replClientInfo::setLastOp that the optime only goes
    // forward.
    repl::ReplClientInfo::forClient(opCtx->getClient()).clearLastOp();

    opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();

    // All the ops will have the same session, so we can retain the scopedSession throughout
    // the loop, except when invalidated by multi-document transactions. This allows us to
    // track the statements in a retryable write.
    std::unique_ptr<MongoDSessionCatalog::Session> scopedSession;

    // Make sure a partial session doesn't escape.
    ON_BLOCK_EXIT([this, &scopedSession, &opCtx] {
        if (scopedSession) {
            auto txnParticipant = TransactionParticipant::get(opCtx.get());
            invariant(txnParticipant);
            txnParticipant.invalidate(opCtx.get());
        }
    });

    boost::optional<MutableOplogEntry> prePostImageEntry = boost::none;
    OpTime originalPrePostImageOpTime;
    for (auto iter = begin; iter != end; iter++) {
        const auto& entry = *iter->first;
        invariant(!isResumeTokenNoop(entry));
        invariant(entry.getSessionId());

        MutableOplogEntry noopEntry;
        noopEntry.setOpType(repl::OpTypeEnum::kNoop);
        noopEntry.setNss(entry.getNss());
        noopEntry.setUuid(entry.getUuid());
        noopEntry.setObject({});  // Empty 'o' field.
        noopEntry.setObject2(entry.getEntry().toBSON());
        noopEntry.setOpTime(*iter->second);
        noopEntry.setWallClockTime(opCtx->getServiceContext()->getFastClockSource()->now());

        boost::optional<TenantId> tenantId = [&]() -> boost::optional<TenantId> {
            if (_protocol == MigrationProtocolEnum::kMultitenantMigrations && _tenantId) {
                return TenantId{OID::createFromString(*_tenantId)};
            }
            if (_protocol == MigrationProtocolEnum::kShardMerge && entry.getTid()) {
                return *entry.getTid();
            }
            return boost::none;
        }();
        noopEntry.setTid(tenantId);

        switch (getOplogEntryType(entry)) {
            case OplogEntryType::kOplogEntryTypeRetryableWritePrePostImage: {
                // entry.getEntry().toBSON() is the pre- or post-image in BSON format.
                prePostImageEntry =
                    uassertStatusOK(MutableOplogEntry::parse(entry.getEntry().toBSON()));
                originalPrePostImageOpTime = entry.getOpTime();
                prePostImageEntry->setOpTime(*iter->second);
                prePostImageEntry->setWallClockTime(
                    opCtx->getServiceContext()->getFastClockSource()->now());
                prePostImageEntry->setFromMigrate(true);
                // Clear the old tenant migration UUID.
                prePostImageEntry->setFromTenantMigration(boost::none);
                // Don't write the no-op entry, both the no-op entry and prePostImage entry will be
                // written on the next iteration.
                continue;
            }
            case OplogEntryType::kOplogEntryTypePreviouslyWrappedRetryableWrite: {
                uassert(5351003,
                        str::stream() << "Tenant Oplog Applier received unexpected Empty o2 "
                                         "field (original oplog entry) in migrated noop: "
                                      << redact(entry.toBSONForLogging()),
                        !entry.getObject2()->isEmpty());
                // entry.getEntry().toBSON() is the original migrated no-op in BSON format.
                noopEntry = uassertStatusOK(MutableOplogEntry::parse(entry.getEntry().toBSON()));
                noopEntry.setOpTime(*iter->second);
                noopEntry.setWallClockTime(opCtx->getServiceContext()->getFastClockSource()->now());
                // Clear the old tenant migration UUID.
                noopEntry.setFromTenantMigration(boost::none);

                // Set the inner 'o2' optime to the donor entry's optime because the recipient
                // uses the timestamp in 'o2' to determine where to resume applying from.
                auto o2Entry = uassertStatusOK(MutableOplogEntry::parse(*entry.getObject2()));
                o2Entry.setOpTime(entry.getOpTime());
                o2Entry.setWallClockTime(entry.getWallClockTime());
                noopEntry.setObject2(o2Entry.toBSON());

                // Handle as for kOplogEntryTypeRetryableWrite after extracting original op.
                [[fallthrough]];
            }
            case OplogEntryType::kOplogEntryTypeRetryableWrite: {
                {
                    auto lk = stdx::lock_guard(*opCtx->getClient());
                    opCtx->setLogicalSessionId(*entry.getSessionId());
                    opCtx->setTxnNumber(*entry.getTxnNumber());
                }

                if (!scopedSession) {
                    auto mongoDSessionCatalog = MongoDSessionCatalog::get(opCtx.get());
                    scopedSession =
                        mongoDSessionCatalog->checkOutSessionWithoutOplogRead(opCtx.get());
                }

                _writeRetryableWriteEntryNoOp(
                    opCtx.get(), noopEntry, entry, prePostImageEntry, originalPrePostImageOpTime);
                break;
            }
            case OplogEntryType::kOplogEntryTypePartialTransaction: {
                _writeSessionNoOp(opCtx.get(), noopEntry);
                break;
            }
            case OplogEntryType::kOplogEntryTypeTransaction: {
                {
                    auto lk = stdx::lock_guard(*opCtx->getClient());
                    opCtx->setLogicalSessionId(*entry.getSessionId());
                    opCtx->setTxnNumber(*entry.getTxnNumber());
                    opCtx->setInMultiDocumentTransaction();
                }

                // Check out the session.
                if (!scopedSession) {
                    auto mongoDSessionCatalog = MongoDSessionCatalog::get(opCtx.get());
                    scopedSession =
                        mongoDSessionCatalog->checkOutSessionWithoutOplogRead(opCtx.get());
                }

                _writeTransactionEntryNoOp(opCtx.get(), noopEntry, entry);
                break;
            }
            default:
                MONGO_UNREACHABLE;
        }

        // If we have a prePostImage no-op here that hasn't already been logged, it is orphaned;
        // this can happen in some very unlikely rollback situations. Otherwise, the image entry
        // should have been written at this point so we need to reset it for the next iteration.
        prePostImageEntry = boost::none;

        // Invalidate in-memory state so that the next time the session is checked out, it
        // would reload the transaction state from config.transactions.
        if (opCtx->inMultiDocumentTransaction()) {
            auto txnParticipant = TransactionParticipant::get(opCtx.get());
            invariant(txnParticipant);
            txnParticipant.invalidate(opCtx.get());
            opCtx->resetMultiDocumentTransactionState();
            scopedSession = {};
        }
    }
}

void TenantOplogApplier::_writeNoOpsForRange(OpObserver* opObserver,
                                             std::vector<TenantNoOpEntry>::const_iterator begin,
                                             std::vector<TenantNoOpEntry>::const_iterator end) {
    auto opCtx = cc().makeOperationContext();
    tenantMigrationInfo(opCtx.get()) = boost::make_optional<TenantMigrationInfo>(_migrationUuid);

    // Since the client object persists across each noop write call and the same writer thread could
    // be reused to write noop entries with older optime, we need to clear the lastOp associated
    // with the client to avoid the invariant in replClientInfo::setLastOp that the optime only goes
    // forward.
    repl::ReplClientInfo::forClient(opCtx->getClient()).clearLastOp();

    opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();

    AutoGetOplog oplogWrite(opCtx.get(), OplogAccessMode::kWrite);
    auto tenantLocks = _acquireIntentExclusiveTenantLocks(opCtx.get(), begin, end);

    writeConflictRetry(opCtx.get(), "writeTenantNoOps", NamespaceString::kRsOplogNamespace, [&] {
        WriteUnitOfWork wuow(opCtx.get());
        for (auto iter = begin; iter != end; iter++) {
            const auto& entry = *iter->first;
            if (isResumeTokenNoop(entry)) {
                // We don't want to write noops for resume token noop oplog entries. They would
                // not be applied in a change stream anyways.
                continue;
            }
            // We don't need to link no-ops entries for operations done outside of a session.
            const boost::optional<OpTime> preImageOpTime = boost::none;
            const boost::optional<OpTime> postImageOpTime = boost::none;
            const boost::optional<OpTime> prevWriteOpTimeInTransaction = boost::none;
            opObserver->onInternalOpMessage(
                opCtx.get(),
                entry.getNss(),
                entry.getUuid(),
                {},  // Empty 'o' field.
                entry.getEntry().toBSON(),
                // We link the no-ops together by recipient op time the same way the actual ops
                // were linked together by donor op time.  This is to allow retryable writes
                // and changestreams to find the ops they need.
                preImageOpTime,
                postImageOpTime,
                prevWriteOpTimeInTransaction,
                *iter->second);
        }
        wuow.commit();
    });
}
std::vector<Lock::TenantLock> TenantOplogApplier::_acquireIntentExclusiveTenantLocks(
    OperationContext* opCtx,
    std::vector<TenantNoOpEntry>::const_iterator entryBegin,
    std::vector<TenantNoOpEntry>::const_iterator entryEnd) const {
    // Determine all involved tenants.
    std::set<TenantId> tenantIds = [&] {
        std::set<TenantId> tenantIds;
        if (_tenantId) {
            tenantIds.emplace(OID::createFromString(*_tenantId));
        } else {
            for (auto iter = entryBegin; iter != entryEnd; ++iter) {
                const auto& oplogEntry = *iter->first;
                if (oplogEntry.getTid()) {
                    tenantIds.insert(*oplogEntry.getTid());
                }
            }
        }
        return tenantIds;
    }();

    // Acquire a lock for each tenant.
    std::vector<Lock::TenantLock> tenantLocks;
    tenantLocks.reserve(tenantIds.size());
    for (auto&& tenantId : tenantIds) {
        tenantLocks.emplace_back(opCtx, tenantId, MODE_IX);
    }
    return tenantLocks;
}
std::vector<std::vector<ApplierOperation>> TenantOplogApplier::_fillWriterVectors(
    OperationContext* opCtx, TenantOplogBatch* batch) {
    std::vector<std::vector<ApplierOperation>> writerVectors(
        _writerPool->getStats().options.maxThreads);
    CachedCollectionProperties collPropertiesCache;

    for (auto&& op : batch->ops) {
        // If the operation's optime is before or the same as the startApplyingAfterOpTime we don't
        // want to apply it, so don't include it in writerVectors.
        if (op.entry.getOpTime() <= _startApplyingAfterOpTime)
            continue;
        uassert(4886006,
                "Tenant oplog application does not support prepared transactions.",
                !op.entry.shouldPrepare());
        uassert(4886007,
                "Tenant oplog application does not support prepared transactions.",
                !op.entry.isPreparedCommit());

        // We never need to apply no-ops or partial transactions.
        if (op.entry.getOpType() == OpTypeEnum::kNoop || op.entry.isPartialTransaction())
            continue;

        if (op.expansionsEntry >= 0) {
            // This is an applyOps or transaction; add the expansions to the writer vectors.

            auto isTransactionWithCommand = false;
            auto expansions = &batch->expansions[op.expansionsEntry];
            bool tenantOp = false;
            for (auto&& entry : *expansions) {
                if (_shouldIgnore(entry)) {
                    uassert(6114521,
                            "Can't have a transaction with operations on both tenant and internal "
                            "collections.",
                            !tenantOp);
                    op.ignore = true;
                    continue;
                }

                uassert(6114522,
                        "Can't have a transaction with operations on both tenant and internal "
                        "collections.",
                        !op.ignore);
                tenantOp = true;
                if (entry.isCommand()) {
                    // If the transaction contains a command, serialize the operations.
                    isTransactionWithCommand = true;
                }
            }

            if (op.ignore) {
                continue;
            }

            OplogApplierUtils::addDerivedOps(opCtx,
                                             expansions,
                                             &writerVectors,
                                             &collPropertiesCache,
                                             isTransactionWithCommand /* serial */);
        } else {
            if (_shouldIgnore(op.entry)) {
                op.ignore = true;
                continue;
            }
            // Add a single op to the writer vectors.
            OplogApplierUtils::addToWriterVector(
                opCtx, &op.entry, &writerVectors, &collPropertiesCache);
        }
    }
    return writerVectors;
}

Status TenantOplogApplier::_applyOplogEntryOrGroupedInserts(
    OperationContext* opCtx,
    const OplogEntryOrGroupedInserts& entryOrGroupedInserts,
    OplogApplication::Mode oplogApplicationMode,
    const bool isDataConsistent) {
    // We must ensure the opCtx uses replicated writes, because that will ensure we get a
    // NotWritablePrimary error if a stepdown occurs.
    invariant(opCtx->writesAreReplicated());

    auto op = entryOrGroupedInserts.getOp();
    if (op->isIndexCommandType() &&
        op->getCommandType() != OplogEntry::CommandType::kCreateIndexes &&
        op->getCommandType() != OplogEntry::CommandType::kDropIndexes) {
        LOGV2_ERROR(488610,
                    "Index creation, except createIndex on empty collections, is not supported in "
                    "tenant migration",
                    "protocol"_attr = _protocol,
                    "migrationId"_attr = _migrationUuid,
                    "op"_attr = redact(op->toBSONForLogging()));

        uasserted(5434700,
                  "Index creation, except createIndex on empty collections, is not supported in "
                  "tenant migration");
    }
    if (op->getCommandType() == OplogEntry::CommandType::kCreateIndexes) {
        auto uuid = op->getUuid();
        uassert(5652700, "Missing UUID from createIndex oplog entry", uuid);
        try {
            AutoGetCollectionForRead autoColl(opCtx, {op->getNss().db().toString(), *uuid});
            uassert(ErrorCodes::NamespaceNotFound, "Collection does not exist", autoColl);
            // During tenant migration oplog application, we only need to apply createIndex on empty
            // collections. Otherwise, the index is guaranteed to be dropped after. This is because
            // we block index builds on the donor for the duration of the tenant migration.
            if (!Helpers::findOne(opCtx, autoColl.getCollection(), BSONObj()).isNull()) {
                LOGV2_DEBUG(5652701,
                            2,
                            "Tenant migration ignoring createIndex for non-empty collection",
                            "op"_attr = redact(op->toBSONForLogging()),
                            "protocol"_attr = _protocol,
                            "migrationId"_attr = _migrationUuid);
                return Status::OK();
            }
        } catch (const ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
            // If the collection doesn't exist, it is safe to ignore.
            return Status::OK();
        }
    }
    // We don't count tenant application in the ops applied stats.
    auto incrementOpsAppliedStats = [] {
    };
    auto status = OplogApplierUtils::applyOplogEntryOrGroupedInsertsCommon(opCtx,
                                                                           entryOrGroupedInserts,
                                                                           oplogApplicationMode,
                                                                           isDataConsistent,
                                                                           incrementOpsAppliedStats,
                                                                           nullptr /* opCounters*/);
    LOGV2_DEBUG(4886009,
                2,
                "Applied tenant operation",
                "protocol"_attr = _protocol,
                "migrationId"_attr = _migrationUuid,
                "error"_attr = status,
                "op"_attr = redact(op->toBSONForLogging()));
    return status;
}

Status TenantOplogApplier::_applyOplogBatchPerWorker(std::vector<ApplierOperation>* ops) {
    auto opCtx = cc().makeOperationContext();
    opCtx->setEnforceConstraints(false);
    tenantMigrationInfo(opCtx.get()) = boost::make_optional<TenantMigrationInfo>(_migrationUuid);

    // Set this to satisfy low-level locking invariants.
    opCtx->lockState()->setShouldConflictWithSecondaryBatchApplication(false);

    auto status = OplogApplierUtils::applyOplogBatchCommon(
        opCtx.get(),
        ops,
        _options.mode,
        _options.allowNamespaceNotFoundErrorsOnCrudOps,
        _options.isDataConsistent,
        [this](OperationContext* opCtx,
               const OplogEntryOrGroupedInserts& opOrInserts,
               OplogApplication::Mode mode,
               const bool isDataConsistent) {
            return _applyOplogEntryOrGroupedInserts(opCtx, opOrInserts, mode, isDataConsistent);
        });

    if (!status.isOK()) {
        LOGV2_ERROR(4886008,
                    "Tenant migration writer worker batch application failed",
                    "protocol"_attr = _protocol,
                    "migrationId"_attr = _migrationUuid,
                    "error"_attr = redact(status));
    }
    return status;
}

std::unique_ptr<ThreadPool> makeTenantMigrationWriterPool() {
    return makeTenantMigrationWriterPool(tenantApplierThreadCount);
}

std::unique_ptr<ThreadPool> makeTenantMigrationWriterPool(int threadCount) {
    return makeReplWriterPool(
        threadCount, "TenantMigrationWriter"_sd, true /*  isKillableByStepdown */);
}

}  // namespace repl
}  // namespace mongo
