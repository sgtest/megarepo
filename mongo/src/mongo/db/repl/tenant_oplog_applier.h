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

#pragma once

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <cstddef>
#include <map>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/abstract_async_component.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/oplog_buffer.h"
#include "mongo/db/repl/oplog_entry.h"
#include "mongo/db/repl/oplog_entry_or_grouped_inserts.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/tenant_oplog_batcher.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/service_context.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/session/session_txn_record_gen.h"
#include "mongo/executor/task_executor.h"
#include "mongo/platform/mutex.h"
#include "mongo/stdx/unordered_set.h"
#include "mongo/util/concurrency/with_lock.h"
#include "mongo/util/future.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/uuid.h"

namespace mongo {
class ThreadPool;

namespace repl {

/**
 * This class reads oplog entries from a tenant migration, then applies those entries to the
 * (real) oplog, then writes out no-op entries corresponding to the original oplog entries
 * from the oplog buffer.  Applier will not apply, but will write no-op entries for,
 * entries before the applyFromOpTime.
 *
 */
class TenantOplogApplier : public AbstractAsyncComponent,
                           public std::enable_shared_from_this<TenantOplogApplier> {
public:
    struct OpTimePair {
        OpTimePair() = default;
        OpTimePair(OpTime in_donorOpTime, OpTime in_recipientOpTime)
            : donorOpTime(in_donorOpTime), recipientOpTime(in_recipientOpTime) {}
        bool operator<(const OpTimePair& other) const {
            if (donorOpTime == other.donorOpTime)
                return recipientOpTime < other.recipientOpTime;
            return donorOpTime < other.donorOpTime;
        }
        std::string toString() const {
            return BSON("donorOpTime" << donorOpTime << "recipientOpTime" << recipientOpTime)
                .toString();
        }
        OpTime donorOpTime;
        OpTime recipientOpTime;
    };

    /**
     * Used to configure behavior of this TenantOplogApplier.
     **/
    struct Options {
        explicit Options(OplogApplication::Mode inputMode)
            : mode(inputMode),
              allowNamespaceNotFoundErrorsOnCrudOps(inputMode !=
                                                    OplogApplication::Mode::kSecondary),
              isDataConsistent(inputMode == OplogApplication::Mode::kSecondary) {

            // Safety rail to prevent incorrect values for 'isDataConsistent' &
            // 'allowNamespaceNotFoundErrorsOnCrudOps' for future oplog application modes.
            invariant(mode == OplogApplication::Mode::kInitialSync ||
                      mode == OplogApplication::Mode::kSecondary);
        }

        const OplogApplication::Mode mode;
        const bool allowNamespaceNotFoundErrorsOnCrudOps;
        const bool isDataConsistent;
    };

    TenantOplogApplier(const UUID& migrationUuid,
                       const MigrationProtocolEnum& protocol,
                       const OpTime& StartApplyingAfterOpTime,
                       const OpTime& cloneFinishedRecipientOpTime,
                       boost::optional<std::string> tenantId,
                       RandomAccessOplogBuffer* oplogBuffer,
                       std::shared_ptr<executor::TaskExecutor> executor,
                       ThreadPool* writerPool,
                       Timestamp resumeBatchingTs = Timestamp());

    virtual ~TenantOplogApplier();

    /**
     * Return a future which will be notified when that optime has been reached.  Future will
     * contain donor and recipient optime of last oplog entry in batch where donor optime is greater
     * than passed-in time. To be noted, recipient optime returned in the future can be null if the
     * tenant oplog applier has never applied any tenant oplog entries (i.e., non resume token no-op
     * entries) till that batch.
     */
    SemiFuture<OpTimePair> getNotificationForOpTime(OpTime donorOpTime);

    size_t getNumOpsApplied() {
        stdx::lock_guard lk(_mutex);
        return _numOpsApplied;
    }

    /**
     * Returns the optime the applier will start applying from.
     */
    OpTime getStartApplyingAfterOpTime() const;

    /**
     * Returns the timestamp the applier will resume batching from.
     */
    Timestamp getResumeBatchingTs() const;

private:
    void _doStartup_inlock() final;
    void _doShutdown_inlock() noexcept final;
    void _preJoin() noexcept final;
    void _finishShutdown(WithLock lk, Status status);

    void _applyLoop(TenantOplogBatch batch);
    bool _shouldStopApplying(Status status);
    // Indicates an oplog entry should be ignored and not applied.
    bool _shouldIgnore(const OplogEntry& entry);

    void _applyOplogBatch(TenantOplogBatch* batch);
    Status _applyOplogBatchPerWorker(std::vector<ApplierOperation>* ops);
    void _checkNsAndUuidsBelongToTenant(OperationContext* opCtx, const TenantOplogBatch& batch);
    void _writeTransactionEntryNoOp(OperationContext* opCtx,
                                    MutableOplogEntry& noopEntry,
                                    const OplogEntry& entry);
    void _writeRetryableWriteEntryNoOp(OperationContext* opCtx,
                                       MutableOplogEntry& noopEntry,
                                       const OplogEntry& entry,
                                       const boost::optional<MutableOplogEntry>& prePostImageEntry,
                                       const OpTime& originalPrePostImageOpTime);
    void _writeSessionNoOp(OperationContext* opCtx,
                           MutableOplogEntry& noopEntry,
                           boost::optional<SessionTxnRecord> sessionTxnRecord = boost::none,
                           std::vector<StmtId> stmtIds = {},
                           boost::optional<MutableOplogEntry> prePostImageEntry = boost::none);
    OpTimePair _writeNoOpEntries(OperationContext* opCtx, const TenantOplogBatch& batch);

    using TenantNoOpEntry = std::pair<ApplierOperation, std::vector<OplogSlot>::iterator>;
    void _writeNoOpsForRange(OpObserver* opObserver,
                             std::vector<TenantNoOpEntry>::const_iterator begin,
                             std::vector<TenantNoOpEntry>::const_iterator end);
    void _writeSessionNoOpsForRange(std::vector<TenantNoOpEntry>::const_iterator begin,
                                    std::vector<TenantNoOpEntry>::const_iterator end);

    Status _applyOplogEntryOrGroupedInserts(OperationContext* opCtx,
                                            const OplogEntryOrGroupedInserts& entryOrGroupedInserts,
                                            OplogApplication::Mode oplogApplicationMode,
                                            bool isDataConsistent);
    std::vector<std::vector<ApplierOperation>> _fillWriterVectors(OperationContext* opCtx,
                                                                  TenantOplogBatch* batch);

    /**
     * Acquires Intent Exclusive (IX) lock for each tenant referred to by oplog entries [entryBegin;
     * entryEnd) and returns lock objects.
     */
    std::vector<Lock::TenantLock> _acquireIntentExclusiveTenantLocks(
        OperationContext* opCtx,
        std::vector<TenantNoOpEntry>::const_iterator entryBegin,
        std::vector<TenantNoOpEntry>::const_iterator entryEnd) const;

    /**
     * Sets the _finalStatus to the new status if and only if the old status is "OK".
     */
    void _setFinalStatusIfOk(WithLock, Status newStatus);

    Mutex* _getMutex() noexcept final {
        return &_mutex;
    }

    Mutex _mutex = MONGO_MAKE_LATCH("TenantOplogApplier::_mutex");
    // All member variables are labeled with one of the following codes indicating the
    // synchronization rules for accessing them.
    //
    // (R)  Read-only in concurrent operation; no synchronization required.
    // (S)  Self-synchronizing; access according to class's own rules.
    // (M)  Reads and writes guarded by _mutex
    // (X)  Access only allowed from the main flow of control called from run() or constructor.

    // Handles consuming oplog entries from the OplogBuffer for oplog application.
    std::shared_ptr<TenantOplogBatcher> _oplogBatcher;  // (R)
    const UUID _migrationUuid;                          // (R)
    const MigrationProtocolEnum _protocol;              // (R)
    const OpTime _startApplyingAfterOpTime;             // (R)
    // All no-op entries written by this migration should have OpTime greater than this
    // OpTime.
    const OpTime _cloneFinishedRecipientOpTime;  // (R)
    // For multi-tenant migration protocol, _tenantId is set.
    // But, for shard merge protcol, _tenantId is empty.
    const boost::optional<std::string> _tenantId;  // (R)

    RandomAccessOplogBuffer* _oplogBuffer;  // (R)
    std::shared_ptr<executor::TaskExecutor>
        _executor;  // (R)
                    // Pool of worker threads for writing ops to the databases.
    // Not owned by us.
    ThreadPool* const _writerPool;  // (S)
    // Keeps track of last applied donor and recipient optimes by the tenant oplog applier.
    // This gets updated only on batch boundaries.
    OpTimePair _lastAppliedOpTimesUpToLastBatch;  // (M)

    // The timestamp to resume batching from. A null timestamp indicates that the oplog applier
    // is starting fresh (not a retry), and will start batching from the beginning of the oplog
    // buffer.
    const Timestamp _resumeBatchingTs;                                    // (R)
    std::map<OpTime, SharedPromise<OpTimePair>> _opTimeNotificationList;  // (M)
    Status _finalStatus = Status::OK();                                   // (M)
    stdx::unordered_set<UUID, UUID::Hash> _knownGoodUuids;                // (X)
    bool _applyLoopApplyingBatch = false;                                 // (M)
    size_t _numOpsApplied{0};                                             // (M)
    const Options _options;                                               // (R)
};

/**
 * Creates the default thread pool for writer tasks.
 */
std::unique_ptr<ThreadPool> makeTenantMigrationWriterPool();
std::unique_ptr<ThreadPool> makeTenantMigrationWriterPool(int threadCount);

}  // namespace repl
}  // namespace mongo
