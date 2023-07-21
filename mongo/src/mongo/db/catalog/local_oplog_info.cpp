/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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


#include "mongo/db/catalog/local_oplog_info.h"

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
// IWYU pragma: no_include "ext/alloc_traits.h"
#include <mutex>
#include <utility>

#include "mongo/db/curop.h"
#include "mongo/db/logical_time.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/storage/flow_control.h"
#include "mongo/db/storage/record_store.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/vector_clock_mutable.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/timer.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kCatalog


namespace mongo {
namespace {

const auto localOplogInfo = ServiceContext::declareDecoration<LocalOplogInfo>();

}  // namespace

// static
LocalOplogInfo* LocalOplogInfo::get(ServiceContext& service) {
    return get(&service);
}

// static
LocalOplogInfo* LocalOplogInfo::get(ServiceContext* service) {
    return &localOplogInfo(service);
}

// static
LocalOplogInfo* LocalOplogInfo::get(OperationContext* opCtx) {
    return get(opCtx->getServiceContext());
}

const Collection* LocalOplogInfo::getCollection() const {
    return _oplog;
}

void LocalOplogInfo::setCollection(const Collection* oplog) {
    _oplog = oplog;
}

void LocalOplogInfo::resetCollection() {
    _oplog = nullptr;
}

void LocalOplogInfo::setNewTimestamp(ServiceContext* service, const Timestamp& newTime) {
    VectorClockMutable::get(service)->tickClusterTimeTo(LogicalTime(newTime));
}

std::vector<OplogSlot> LocalOplogInfo::getNextOpTimes(OperationContext* opCtx, std::size_t count) {
    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    long long term = repl::OpTime::kUninitializedTerm;

    // Fetch term out of the newOpMutex.
    if (replCoord->getSettings().isReplSet()) {
        // Current term. If we're not a replset of pv=1, it remains kOldProtocolVersionTerm.
        term = replCoord->getTerm();
    }

    Timestamp ts;
    // Provide a sample to FlowControl after the `oplogInfo.newOpMutex` is released.
    ON_BLOCK_EXIT([opCtx, &ts, count] {
        auto flowControl = FlowControl::get(opCtx);
        if (flowControl) {
            flowControl->sample(ts, count);
        }
    });

    // Allow the storage engine to start the transaction outside the critical section.
    opCtx->recoveryUnit()->preallocateSnapshot();
    {
        stdx::lock_guard<Latch> lk(_newOpMutex);

        ts = VectorClockMutable::get(opCtx)->tickClusterTime(count).asTimestamp();
        const bool orderedCommit = false;

        // The local oplog collection pointer must already be established by this point.
        // We can't establish it here because that would require locking the local database, which
        // would be a lock order violation.
        invariant(_oplog);
        fassert(28560, _oplog->getRecordStore()->oplogDiskLocRegister(opCtx, ts, orderedCommit));
    }

    Timer oplogSlotDurationTimer;
    std::vector<OplogSlot> oplogSlots(count);
    for (std::size_t i = 0; i < count; i++) {
        oplogSlots[i] = {Timestamp(ts.asULL() + i), term};
    }

    // If we abort a transaction that has reserved an optime, we should make sure to update the
    // stable timestamp if necessary, since this oplog hole may have been holding back the stable
    // timestamp.
    opCtx->recoveryUnit()->onRollback([replCoord, oplogSlotDurationTimer](OperationContext* opCtx) {
        replCoord->attemptToAdvanceStableTimestamp();
        // Sum the oplog slot durations. An operation may participate in multiple transactions.
        CurOp::get(opCtx)->debug().totalOplogSlotDurationMicros +=
            Microseconds(oplogSlotDurationTimer.elapsed());
    });

    opCtx->recoveryUnit()->onCommit(
        [oplogSlotDurationTimer](OperationContext* opCtx, boost::optional<Timestamp>) {
            // Sum the oplog slot durations. An operation may participate in multiple transactions.
            CurOp::get(opCtx)->debug().totalOplogSlotDurationMicros +=
                Microseconds(oplogSlotDurationTimer.elapsed());
        });

    return oplogSlots;
}

}  // namespace mongo
