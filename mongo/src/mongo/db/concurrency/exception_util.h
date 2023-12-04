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

#pragma once

#include <string>
#include <utility>

#include <fmt/format.h>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/db/client.h"
#include "mongo/db/curop.h"
#include "mongo/db/locker_api.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/fail_point.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kControl

namespace mongo {

extern FailPoint skipWriteConflictRetries;

/**
 * Will log a message if sensible and will do an exponential backoff to make sure
 * we don't hammer the same doc over and over.
 * @param attempt - what attempt is this, 1 based
 * @param operation - e.g. "update"
 */
void logWriteConflictAndBackoff(size_t attempt,
                                StringData operation,
                                StringData reason,
                                const NamespaceStringOrUUID& nssOrUUID);

/**
 * Retries the operation for a fixed number of attempts with linear backoff.
 * For internal system operations, converts the temporarily unavailable error into a write
 * conflict and handles it, because unlike user operations, the error cannot eventually escape to
 * the client.
 */
void handleTemporarilyUnavailableException(
    OperationContext* opCtx,
    size_t tempUnavailAttempts,
    StringData opStr,
    const NamespaceStringOrUUID& nssOrUUID,
    const ExceptionFor<ErrorCodes::TemporarilyUnavailable>& e,
    size_t& writeConflictAttempts);

/**
 * Convert `e` into a `WriteConflictException` and throw it.
 */
void convertToWCEAndRethrow(OperationContext* opCtx,
                            StringData opStr,
                            const ExceptionFor<ErrorCodes::TemporarilyUnavailable>& e);

void handleTransactionTooLargeForCacheException(
    OperationContext* opCtx,
    StringData opStr,
    const NamespaceStringOrUUID& nssOrUUID,
    const ExceptionFor<ErrorCodes::TransactionTooLargeForCache>& e,
    size_t& writeConflictAttempts);

namespace error_details {
/**
 * A faster alternative to `iasserted`, designed to throw exceptions for unexceptional events on the
 * critical execution path (e.g., `WriteConflict`).
 */
template <ErrorCodes::Error ec>
[[noreturn]] void throwExceptionFor(std::string reason) {
    throw ExceptionFor<ec>({ec, std::move(reason)});
}
}  // namespace error_details

/**
 * A `WriteConflictException` is thrown if during a write, two or more operations conflict with each
 * other. For example if two operations get the same version of a document, and then both try to
 * modify that document, this exception will get thrown by one of them.
 */
[[noreturn]] inline void throwWriteConflictException(StringData context) {
    using namespace fmt::literals;
    error_details::throwExceptionFor<ErrorCodes::WriteConflict>(
        "Caused by :: {} :: Please retry your operation or multi-document transaction."_format(
            context));
}

/**
 * A `TemporarilyUnavailableException` is thrown if an operation aborts due to the server being
 * temporarily unavailable, e.g. due to excessive load. For user-originating operations, this will
 * be retried internally by the `writeConflictRetry` helper a finite number of times before
 * eventually being returned.
 */
[[noreturn]] inline void throwTemporarilyUnavailableException(std::string context) {
    error_details::throwExceptionFor<ErrorCodes::TemporarilyUnavailable>(std::move(context));
}

/**
 * A `TransactionTooLargeForCache` is thrown if it has been determined that it is unlikely to
 * ever complete the operation because the configured cache is insufficient to hold all the
 * transaction state. This helps to avoid retrying, maybe indefinitely, a transaction which would
 * never be able to complete.
 */
[[noreturn]] inline void throwTransactionTooLargeForCache(std::string context) {
    error_details::throwExceptionFor<ErrorCodes::TransactionTooLargeForCache>(std::move(context));
}

/**
 * Runs the argument function f as many times as needed for f to complete or throw an exception
 * other than WriteConflictException or TemporarilyUnavailableException. For each time f throws
 * one of these exceptions, logs the error, waits a spell, cleans up, and then tries f again.
 * Imposes no upper limit on the number of times to re-try f after a WriteConflictException, so any
 * required timeout behavior must be enforced within f. When retrying a
 * TemporarilyUnavailableException, f is called a finite number of times before we eventually let
 * the error escape.
 *
 * If we are already in a WriteUnitOfWork, we assume that we are being called within a
 * WriteConflictException retry loop up the call stack. Hence, this retry loop is reduced to an
 * invocation of the argument function f without any exception handling and retry logic.
 */
template <typename F>
auto writeConflictRetry(OperationContext* opCtx,
                        StringData opStr,
                        const NamespaceStringOrUUID& nssOrUUID,
                        F&& f,
                        boost::optional<size_t> retryLimit = boost::none) {
    invariant(opCtx);
    invariant(shard_role_details::getLocker(opCtx));
    invariant(opCtx->recoveryUnit());

    // This failpoint disables exception handling for write conflicts. Only allow this exception to
    // escape user operations. Do not allow exceptions to escape internal threads, which may rely on
    // this exception handler to avoid crashing.
    bool userSkipWriteConflictRetry = MONGO_unlikely(skipWriteConflictRetries.shouldFail()) &&
        opCtx->getClient()->isFromUserConnection();
    if (shard_role_details::getLocker(opCtx)->inAWriteUnitOfWork() || userSkipWriteConflictRetry) {
        try {
            return f();
        } catch (ExceptionFor<ErrorCodes::TemporarilyUnavailable> const& e) {
            if (opCtx->inMultiDocumentTransaction()) {
                convertToWCEAndRethrow(opCtx, opStr, e);
            }
            throw;
        } catch (ExceptionFor<ErrorCodes::WriteConflict>&) {
            CurOp::get(opCtx)->debug().additiveMetrics.incrementWriteConflicts(1);
            throw;
        }
    }

    size_t writeConflictAttempts = 0;
    size_t attemptsTempUnavailable = 0;
    while (true) {
        try {
            return f();
        } catch (ExceptionFor<ErrorCodes::WriteConflict> const& e) {
            CurOp::get(opCtx)->debug().additiveMetrics.incrementWriteConflicts(1);
            logWriteConflictAndBackoff(writeConflictAttempts, opStr, e.reason(), nssOrUUID);
            ++writeConflictAttempts;
            opCtx->recoveryUnit()->abandonSnapshot();
            if (MONGO_unlikely(retryLimit && writeConflictAttempts > *retryLimit)) {
                LOGV2_ERROR(7677402,
                            "Got too many write conflicts, the server may run into problems.");
                fassert(7677401, !getTestCommandsEnabled());
            }
        } catch (ExceptionFor<ErrorCodes::TemporarilyUnavailable> const& e) {
            handleTemporarilyUnavailableException(
                opCtx, ++attemptsTempUnavailable, opStr, nssOrUUID, e, writeConflictAttempts);
        } catch (ExceptionFor<ErrorCodes::TransactionTooLargeForCache> const& e) {
            handleTransactionTooLargeForCacheException(
                opCtx, opStr, nssOrUUID, e, writeConflictAttempts);
        }
    }
}

}  // namespace mongo

#undef MONGO_LOGV2_DEFAULT_COMPONENT
