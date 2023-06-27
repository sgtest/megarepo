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

#include <string>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/shim.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/s/transaction_coordinator_service.h"
#include "mongo/db/service_context.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/transaction/transaction_participant_gen.h"
#include "mongo/idl/mutable_observer_registry.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/duration.h"
#include "mongo/util/time_support.h"

namespace mongo {
namespace {

void createTransactionCoordinatorImpl(OperationContext* opCtx,
                                      TxnNumber clientTxnNumber,
                                      boost::optional<TxnRetryCounter> clientTxnRetryCounter) {
    auto clientLsid = opCtx->getLogicalSessionId().value();
    auto clockSource = opCtx->getServiceContext()->getFastClockSource();

    // If this shard has been selected as the coordinator, set up the coordinator state
    // to be ready to receive votes.
    TransactionCoordinatorService::get(opCtx)->createCoordinator(
        opCtx,
        clientLsid,
        {clientTxnNumber, clientTxnRetryCounter ? *clientTxnRetryCounter : 0},
        clockSource->now() + Seconds(gTransactionLifetimeLimitSeconds.load()));
}

auto createTransactionCoordinatorRegistration = MONGO_WEAK_FUNCTION_REGISTRATION(
    createTransactionCoordinator, createTransactionCoordinatorImpl);

}  // namespace
}  // namespace mongo
