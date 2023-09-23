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

#include <utility>


#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/db/baton.h"
#include "mongo/db/operation_context.h"
#include "mongo/executor/remote_command_response.h"
#include "mongo/s/multi_statement_transaction_requests_sender.h"
#include "mongo/s/transaction_router.h"
#include "mongo/s/transaction_router_resource_yielder.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/database_name_util.h"

namespace mongo {

namespace {

std::vector<AsyncRequestsSender::Request> attachTxnDetails(
    OperationContext* opCtx, const std::vector<AsyncRequestsSender::Request>& requests) {
    auto txnRouter = TransactionRouter::get(opCtx);
    if (!txnRouter) {
        return requests;
    }

    std::vector<AsyncRequestsSender::Request> newRequests;
    newRequests.reserve(requests.size());

    for (const auto& request : requests) {
        newRequests.emplace_back(
            request.shardId,
            txnRouter.attachTxnFieldsIfNeeded(opCtx, request.shardId, request.cmdObj));
    }

    return newRequests;
}

void processReplyMetadata(OperationContext* opCtx, const AsyncRequestsSender::Response& response) {
    auto txnRouter = TransactionRouter::get(opCtx);
    if (!txnRouter) {
        return;
    }

    if (!response.swResponse.isOK()) {
        return;
    }

    txnRouter.processParticipantResponse(
        opCtx, response.shardId, response.swResponse.getValue().data);
}

}  // unnamed namespace

MultiStatementTransactionRequestsSender::MultiStatementTransactionRequestsSender(
    OperationContext* opCtx,
    std::shared_ptr<executor::TaskExecutor> executor,
    const DatabaseName& dbName,
    const std::vector<AsyncRequestsSender::Request>& requests,
    const ReadPreferenceSetting& readPreference,
    Shard::RetryPolicy retryPolicy,
    AsyncRequestsSender::ShardHostMap designatedHostsMap)
    : _opCtx(opCtx),
      _ars(std::make_unique<AsyncRequestsSender>(
          opCtx,
          std::move(executor),
          dbName,
          attachTxnDetails(opCtx, requests),
          readPreference,
          retryPolicy,
          TransactionRouterResourceYielder::makeForRemoteCommand(),
          designatedHostsMap)) {}

MultiStatementTransactionRequestsSender::~MultiStatementTransactionRequestsSender() {
    invariant(_opCtx);
    auto baton = _opCtx->getBaton();
    invariant(baton);
    // Delegate the destruction of `_ars` to the `_opCtx` baton to potentially move the cost off of
    // the critical path. The assumption is that postponing the destruction is safe so long as the
    // `_opCtx` that corresponds to `_ars` remains alive.
    baton->schedule([ars = std::move(_ars)](Status) mutable { ars.reset(); });
}

bool MultiStatementTransactionRequestsSender::done() {
    return _ars->done();
}

AsyncRequestsSender::Response MultiStatementTransactionRequestsSender::next() {
    auto response = _ars->next();
    processReplyMetadata(_opCtx, response);
    return response;
}

void MultiStatementTransactionRequestsSender::stopRetrying() {
    _ars->stopRetrying();
}

}  // namespace mongo
