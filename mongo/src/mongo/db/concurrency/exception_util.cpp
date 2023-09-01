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

#include "mongo/db/concurrency/exception_util.h"

#include <cstddef>

#include "mongo/db/commands/server_status_metric.h"
#include "mongo/db/concurrency/exception_util_gen.h"
#include "mongo/db/namespace_string.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/log_severity.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/duration.h"
#include "mongo/util/log_and_backoff.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kControl

namespace mongo {

MONGO_FAIL_POINT_DEFINE(skipWriteConflictRetries);

void logWriteConflictAndBackoff(size_t attempt,
                                StringData operation,
                                StringData reason,
                                const NamespaceStringOrUUID& nssOrUUID) {
    logAndBackoff(4640401,
                  logv2::LogComponent::kWrite,
                  logv2::LogSeverity::Debug(1),
                  static_cast<size_t>(attempt),
                  "Caught WriteConflictException",
                  "operation"_attr = operation,
                  "reason"_attr = reason,
                  "namespace"_attr = toStringForLogging(nssOrUUID));
}

namespace {

CounterMetric temporarilyUnavailableErrors{"operation.temporarilyUnavailableErrors"};
CounterMetric temporarilyUnavailableErrorsEscaped{"operation.temporarilyUnavailableErrorsEscaped"};
CounterMetric temporarilyUnavailableErrorsConvertedToWriteConflict{
    "operation.temporarilyUnavailableErrorsConvertedToWriteConflict"};

CounterMetric transactionTooLargeForCacheErrors{"operation.transactionTooLargeForCacheErrors"};
CounterMetric transactionTooLargeForCacheErrorsConvertedToWriteConflict{
    "operation.transactionTooLargeForCacheErrorsConvertedToWriteConflict"};


}  // namespace

void handleTemporarilyUnavailableException(
    OperationContext* opCtx,
    size_t tempUnavailAttempts,
    StringData opStr,
    const NamespaceStringOrUUID& nssOrUUID,
    const ExceptionFor<ErrorCodes::TemporarilyUnavailable>& e,
    size_t& writeConflictAttempts) {
    CurOp::get(opCtx)->debug().additiveMetrics.incrementTemporarilyUnavailableErrors(1);

    opCtx->recoveryUnit()->abandonSnapshot();
    temporarilyUnavailableErrors.increment(1);

    // Internal operations cannot escape a TUE to the client. Convert it to a write conflict
    // exception and handle it accordingly.
    if (!opCtx->getClient()->isFromUserConnection()) {
        temporarilyUnavailableErrorsConvertedToWriteConflict.increment(1);
        CurOp::get(opCtx)->debug().additiveMetrics.incrementWriteConflicts(1);
        logWriteConflictAndBackoff(
            writeConflictAttempts, opStr, e.reason(), NamespaceStringOrUUID(nssOrUUID));
        ++writeConflictAttempts;
        return;
    }

    invariant(opCtx->getClient()->isFromUserConnection());

    if (tempUnavailAttempts >
        static_cast<size_t>(gTemporarilyUnavailableExceptionMaxRetryAttempts.load())) {
        LOGV2_DEBUG(6083901,
                    1,
                    "Too many TemporarilyUnavailableException's, giving up",
                    "reason"_attr = e.reason(),
                    "attempts"_attr = tempUnavailAttempts,
                    "operation"_attr = opStr,
                    "namespace"_attr = toStringForLogging(nssOrUUID));
        temporarilyUnavailableErrorsEscaped.increment(1);
        throw e;
    }

    // Back off linearly with the retry attempt number.
    auto sleepFor = Milliseconds(gTemporarilyUnavailableExceptionRetryBackoffBaseMs.load()) *
        static_cast<int64_t>(tempUnavailAttempts);
    LOGV2_DEBUG(6083900,
                1,
                "Caught TemporarilyUnavailableException",
                "reason"_attr = e.reason(),
                "attempts"_attr = tempUnavailAttempts,
                "operation"_attr = opStr,
                "sleepFor"_attr = sleepFor,
                "namespace"_attr = toStringForLogging(nssOrUUID));
    opCtx->sleepFor(sleepFor);
}

void convertToWCEAndRethrow(OperationContext* opCtx,
                            StringData opStr,
                            const ExceptionFor<ErrorCodes::TemporarilyUnavailable>& e) {
    // For multi-document transactions, since WriteConflicts are tagged as
    // TransientTransactionErrors and TemporarilyUnavailable errors are not, convert the error to a
    // WriteConflict to allow users of multi-document transactions to retry without changing
    // any behavior.
    temporarilyUnavailableErrorsConvertedToWriteConflict.increment(1);
    throwWriteConflictException(e.reason());
}

void handleTransactionTooLargeForCacheException(
    OperationContext* opCtx,
    StringData opStr,
    const NamespaceStringOrUUID& nssOrUUID,
    const ExceptionFor<ErrorCodes::TransactionTooLargeForCache>& e,
    size_t& writeConflictAttempts) {
    transactionTooLargeForCacheErrors.increment(1);
    if (opCtx->writesAreReplicated()) {
        // Surface error on primaries.
        throw e;
    }
    // If an operation succeeds on primary, it should always be retried on secondaries. Secondaries
    // always retry TemporarilyUnavailableExceptions and WriteConflictExceptions indefinitely, the
    // only difference being the rate of retry. We prefer retrying faster, by converting to
    // WriteConflictException, to avoid stalling replication longer than necessary.
    transactionTooLargeForCacheErrorsConvertedToWriteConflict.increment(1);

    // Handle as write conflict.
    CurOp::get(opCtx)->debug().additiveMetrics.incrementWriteConflicts(1);
    logWriteConflictAndBackoff(
        writeConflictAttempts, opStr, e.reason(), NamespaceStringOrUUID(nssOrUUID));
    ++writeConflictAttempts;
    opCtx->recoveryUnit()->abandonSnapshot();
}

}  // namespace mongo
