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

#include <cstdint>
#include <memory>
#include <utility>

#include <boost/preprocessor/control/iif.hpp>

#include "mongo/bson/bsonobj.h"
#include "mongo/db/curop.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/storage_stats.h"
#include "mongo/db/transaction/server_transactions_metrics.h"
#include "mongo/db/transaction/transaction_metrics_observer.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/duration.h"

namespace mongo {

void TransactionMetricsObserver::onStart(ServerTransactionsMetrics* serverTransactionsMetrics,
                                         bool isAutoCommit,
                                         TickSource* tickSource,
                                         Date_t curWallClockTime,
                                         Date_t expireDate) {
    //
    // Per transaction metrics.
    //
    _singleTransactionStats.setStartTime(tickSource->getTicks(), curWallClockTime);
    _singleTransactionStats.setAutoCommit(isAutoCommit);
    _singleTransactionStats.setExpireDate(expireDate);

    //
    // Server wide transactions metrics.
    //
    serverTransactionsMetrics->incrementTotalStarted();
    serverTransactionsMetrics->incrementCurrentOpen();
    serverTransactionsMetrics->incrementCurrentInactive();
}

void TransactionMetricsObserver::onChooseReadTimestamp(Timestamp readTimestamp) {
    _singleTransactionStats.setReadTimestamp(readTimestamp);
}

void TransactionMetricsObserver::onStash(ServerTransactionsMetrics* serverTransactionsMetrics,
                                         TickSource* tickSource) {
    //
    // Per transaction metrics.
    //
    invariant(_singleTransactionStats.isActive());
    _singleTransactionStats.setInactive(tickSource, tickSource->getTicks());

    //
    // Server wide transactions metrics.
    //
    // We accept possible slight inaccuracies in these counters from non-atomicity.
    serverTransactionsMetrics->decrementCurrentActive();
    serverTransactionsMetrics->incrementCurrentInactive();
}

void TransactionMetricsObserver::onUnstash(ServerTransactionsMetrics* serverTransactionsMetrics,
                                           TickSource* tickSource) {
    //
    // Per transaction metrics.
    //
    invariant(!_singleTransactionStats.isActive());
    _singleTransactionStats.setActive(tickSource->getTicks());

    //
    // Server wide transactions metrics.
    //
    // We accept possible slight inaccuracies in these counters from non-atomicity.
    serverTransactionsMetrics->incrementCurrentActive();
    serverTransactionsMetrics->decrementCurrentInactive();
}

void TransactionMetricsObserver::onCommit(OperationContext* opCtx,
                                          ServerTransactionsMetrics* serverTransactionsMetrics,
                                          TickSource* tickSource,
                                          Top* top,
                                          size_t operationCount,
                                          size_t oplogOperationBytes) {
    //
    // Per transaction metrics.
    //
    // After the transaction has been committed, we must update the end time and mark it as
    // inactive. We use the same "now" time to prevent skew in the time-related metrics.
    auto curTick = tickSource->getTicks();
    _singleTransactionStats.setEndTime(curTick);

    invariant(_singleTransactionStats.isActive());
    _singleTransactionStats.setInactive(tickSource, curTick);

    //
    // Server wide transactions metrics.
    //
    serverTransactionsMetrics->incrementTotalCommitted();
    serverTransactionsMetrics->decrementCurrentOpen();
    serverTransactionsMetrics->decrementCurrentActive();

    if (_singleTransactionStats.isPrepared()) {
        serverTransactionsMetrics->incrementTotalPreparedThenCommitted();
        serverTransactionsMetrics->decrementCurrentPrepared();
    }

    serverTransactionsMetrics->updateLastTransaction(
        operationCount,
        oplogOperationBytes,
        opCtx->getWriteConcern().usedDefaultConstructedWC ? BSONObj()
                                                          : opCtx->getWriteConcern().toBSON());

    auto duration =
        durationCount<Microseconds>(_singleTransactionStats.getDuration(tickSource, curTick));
    top->incrementGlobalTransactionLatencyStats(opCtx, static_cast<uint64_t>(duration));
}

void TransactionMetricsObserver::_onAbortActive(
    OperationContext* opCtx,
    ServerTransactionsMetrics* serverTransactionsMetrics,
    TickSource* tickSource,
    Top* top) {

    auto curTick = tickSource->getTicks();
    invariant(_singleTransactionStats.isActive());
    _onAbort(opCtx, serverTransactionsMetrics, curTick, tickSource, top);
    //
    // Per transaction metrics.
    //
    _singleTransactionStats.setInactive(tickSource, curTick);

    //
    // Server wide transactions metrics.
    //
    serverTransactionsMetrics->decrementCurrentActive();
}

void TransactionMetricsObserver::_onAbortInactive(
    OperationContext* opCtx,
    ServerTransactionsMetrics* serverTransactionsMetrics,
    TickSource* tickSource,
    Top* top) {
    auto curTick = tickSource->getTicks();
    invariant(!_singleTransactionStats.isActive());
    _onAbort(opCtx, serverTransactionsMetrics, curTick, tickSource, top);

    //
    // Server wide transactions metrics.
    //
    serverTransactionsMetrics->decrementCurrentInactive();
}

void TransactionMetricsObserver::onAbort(OperationContext* opCtx,
                                         ServerTransactionsMetrics* serverTransactionsMetrics,
                                         TickSource* tickSource,
                                         Top* top) {
    if (_singleTransactionStats.isActive()) {
        _onAbortActive(opCtx, serverTransactionsMetrics, tickSource, top);
    } else {
        _onAbortInactive(opCtx, serverTransactionsMetrics, tickSource, top);
    }
}

void TransactionMetricsObserver::onTransactionOperation(OperationContext* opCtx,
                                                        OpDebug::AdditiveMetrics additiveMetrics,
                                                        bool isPrepared) {
    // Add the latest operation stats to the aggregate OpDebug::AdditiveMetrics object stored in the
    // SingleTransactionStats instance on the TransactionMetricsObserver.
    _singleTransactionStats.getOpDebug()->additiveMetrics.add(additiveMetrics);

    // If there are valid storage statistics for this operation, put those in the
    // SingleTransactionStats instance either by creating a new storageStats instance or by adding
    // into an existing storageStats instance stored in SingleTransactionStats.
    // WiredTiger doesn't let storage statistics be collected when transaction is prepared.
    if (!isPrepared) {
        std::unique_ptr<StorageStats> storageStats =
            opCtx->recoveryUnit()->computeOperationStatisticsSinceLastCall();
        if (storageStats) {
            if (!_singleTransactionStats.getOpDebug()->storageStats) {
                _singleTransactionStats.getOpDebug()->storageStats = storageStats->clone();
            } else {
                *_singleTransactionStats.getOpDebug()->storageStats += *storageStats;
            }
            CurOp::get(opCtx)->debug().storageStats = std::move(storageStats);
        }
    }

    // Update the LastClientInfo object stored in the SingleTransactionStats instance on the
    // TransactionMetricsObserver with this Client's information. This is the last client that ran a
    // transaction operation on the txnParticipant.
    _singleTransactionStats.updateLastClientInfo(opCtx->getClient());
}

void TransactionMetricsObserver::_onAbort(OperationContext* opCtx,
                                          ServerTransactionsMetrics* serverTransactionsMetrics,
                                          TickSource::Tick curTick,
                                          TickSource* tickSource,
                                          Top* top) {
    //
    // Per transaction metrics.
    //
    _singleTransactionStats.setEndTime(curTick);

    //
    // Server wide transactions metrics.
    //
    serverTransactionsMetrics->incrementTotalAborted();
    serverTransactionsMetrics->decrementCurrentOpen();

    if (_singleTransactionStats.isPrepared()) {
        serverTransactionsMetrics->incrementTotalPreparedThenAborted();
        serverTransactionsMetrics->decrementCurrentPrepared();
    }

    auto latency =
        durationCount<Microseconds>(_singleTransactionStats.getDuration(tickSource, curTick));
    top->incrementGlobalTransactionLatencyStats(opCtx, static_cast<uint64_t>(latency));
}

void TransactionMetricsObserver::onPrepare(ServerTransactionsMetrics* serverTransactionsMetrics,
                                           TickSource::Tick curTick) {
    //
    // Per transaction metrics.
    //
    _singleTransactionStats.setPreparedStartTime(curTick);

    serverTransactionsMetrics->incrementCurrentPrepared();
    serverTransactionsMetrics->incrementTotalPrepared();
}

}  // namespace mongo
