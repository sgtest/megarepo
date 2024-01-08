/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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
#include "mongo/db/query/search/mongot_cursor.h"

#include "mongo/db/query/search/mongot_options.h"
#include "mongo/db/query/search/search_task_executors.h"
#include "mongo/logv2/log.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery

namespace mongo::mongot_cursor {
MONGO_FAIL_POINT_DEFINE(shardedSearchOpCtxDisconnect);

namespace {
executor::TaskExecutorCursor::Options getSearchCursorOptions(
    bool preFetchNextBatch,
    std::function<void(BSONObjBuilder& bob)> augmentGetMore,
    std::unique_ptr<PlanYieldPolicy> yieldPolicy) {
    executor::TaskExecutorCursor::Options opts;
    opts.yieldPolicy = std::move(yieldPolicy);
    // If we are pushing down a limit to mongot, then we should avoid prefetching the next
    // batch. We optimistically assume that we will only need a single batch and attempt to
    // avoid doing unnecessary work on mongot. If $idLookup filters out enough documents
    // such that we are not able to satisfy the limit, then we will fetch the next batch
    // syncronously on the subsequent 'getNext()' call.
    opts.preFetchNextBatch = preFetchNextBatch;
    if (!opts.preFetchNextBatch) {
        // Only set this function if we will not be prefetching.
        opts.getMoreAugmentationWriter = augmentGetMore;
    }
    return opts;
}

executor::RemoteCommandRequest getRemoteCommandRequestForSearchQuery(
    OperationContext* opCtx,
    const NamespaceString& nss,
    const boost::optional<UUID>& uuid,
    const boost::optional<ExplainOptions::Verbosity>& explain,
    const BSONObj& query,
    const boost::optional<long long> docsRequested,
    const bool requiresSearchSequenceToken = false,
    const boost::optional<int> protocolVersion = boost::none) {
    BSONObjBuilder cmdBob;
    cmdBob.append(kSearchField, nss.coll());
    uassert(
        6584801,
        str::stream() << "A uuid is required for a search query, but was missing. Got namespace "
                      << nss.toStringForErrorMsg(),
        uuid);
    uuid.value().appendToBuilder(&cmdBob, kCollectionUuidField);
    cmdBob.append(kQueryField, query);
    if (explain) {
        cmdBob.append(kExplainField,
                      BSON(kVerbosityField << ExplainOptions::verbosityString(*explain)));
    }
    if (protocolVersion) {
        cmdBob.append(kIntermediateField, *protocolVersion);
    }
    // (Ignore FCV check): This feature is enabled on an earlier FCV.
    const auto needsSetDocsRequested =
        feature_flags::gFeatureFlagSearchBatchSizeLimit.isEnabledAndIgnoreFCVUnsafe() &&
        docsRequested.has_value();
    if (needsSetDocsRequested || requiresSearchSequenceToken) {
        BSONObjBuilder cursorOptionsBob(cmdBob.subobjStart(kCursorOptionsField));
        if (needsSetDocsRequested) {
            cursorOptionsBob.append(kDocsRequestedField, docsRequested.get());
        }
        if (requiresSearchSequenceToken) {
            // Indicate to mongot that the user wants to paginate so mongot returns pagination
            // tokens alongside the _id values.
            cursorOptionsBob.append(kRequiresSearchSequenceToken, true);
        }
        cursorOptionsBob.doneFast();
    }


    return getRemoteCommandRequest(opCtx, nss, cmdBob.obj());
}

void doThrowIfNotRunningWithMongotHostConfigured() {
    uassert(
        ErrorCodes::SearchNotEnabled,
        str::stream()
            << "Using $search and $vectorSearch aggregation stages requires additional "
            << "configuration. Please connect to Atlas or an AtlasCLI local deployment to enable."
            << "For more information on how to connect, see "
            << "https://dochub.mongodb.org/core/atlas-cli-deploy-local-reqs.",
        globalMongotParams.enabled);
}
}  // namespace

executor::RemoteCommandRequest getRemoteCommandRequest(OperationContext* opCtx,
                                                       const NamespaceString& nss,
                                                       const BSONObj& cmdObj) {
    doThrowIfNotRunningWithMongotHostConfigured();
    auto swHostAndPort = HostAndPort::parse(globalMongotParams.host);
    // This host and port string is configured and validated at startup.
    invariant(swHostAndPort.getStatus().isOK());
    executor::RemoteCommandRequest rcr(
        executor::RemoteCommandRequest(swHostAndPort.getValue(), nss.dbName(), cmdObj, opCtx));
    rcr.sslMode = transport::ConnectSSLMode::kDisableSSL;
    return rcr;
}

std::vector<executor::TaskExecutorCursor> establishCursors(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const executor::RemoteCommandRequest& command,
    std::shared_ptr<executor::TaskExecutor> taskExecutor,
    bool preFetchNextBatch,
    std::function<void(BSONObjBuilder& bob)> augmentGetMore,
    std::unique_ptr<PlanYieldPolicy> yieldPolicy) {
    std::vector<executor::TaskExecutorCursor> cursors;
    auto initialCursor = makeTaskExecutorCursor(
        expCtx->opCtx,
        taskExecutor,
        command,
        getSearchCursorOptions(preFetchNextBatch, augmentGetMore, std::move(yieldPolicy)),
        makeRetryOnNetworkErrorPolicy());

    auto additionalCursors = initialCursor.releaseAdditionalCursors();
    cursors.push_back(std::move(initialCursor));
    // Preserve cursor order. Expect cursors to be labeled, so this may not be necessary.
    for (auto& thisCursor : additionalCursors) {
        cursors.push_back(std::move(thisCursor));
    }

    return cursors;
}

std::vector<executor::TaskExecutorCursor> establishSearchCursors(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const BSONObj& query,
    std::shared_ptr<executor::TaskExecutor> taskExecutor,
    boost::optional<long long> docsRequested,
    std::function<void(BSONObjBuilder& bob)> augmentGetMore,
    const boost::optional<int>& protocolVersion,
    bool requiresSearchSequenceToken,
    std::unique_ptr<PlanYieldPolicy> yieldPolicy) {
    // UUID is required for mongot queries. If not present, no results for the query as the
    // collection has not been created yet.
    if (!expCtx->uuid) {
        return {};
    }

    return establishCursors(expCtx,
                            getRemoteCommandRequestForSearchQuery(expCtx->opCtx,
                                                                  expCtx->ns,
                                                                  expCtx->uuid,
                                                                  expCtx->explain,
                                                                  query,
                                                                  docsRequested,
                                                                  requiresSearchSequenceToken,
                                                                  protocolVersion),
                            taskExecutor,
                            !docsRequested.has_value(),
                            augmentGetMore,
                            std::move(yieldPolicy));
}


BSONObj getExplainResponse(const ExpressionContext* expCtx,
                           const executor::RemoteCommandRequest& request,
                           executor::TaskExecutor* taskExecutor) {
    auto [promise, future] = makePromiseFuture<executor::TaskExecutor::RemoteCommandCallbackArgs>();
    auto promisePtr = std::make_shared<Promise<executor::TaskExecutor::RemoteCommandCallbackArgs>>(
        std::move(promise));
    auto scheduleResult = taskExecutor->scheduleRemoteCommand(
        std::move(request), [promisePtr](const auto& args) { promisePtr->emplaceValue(args); });
    if (!scheduleResult.isOK()) {
        // Since the command failed to be scheduled, the callback above did not and will not run.
        // Thus, it is safe to fulfill the promise here without worrying about synchronizing access
        // with the executor's thread.
        promisePtr->setError(scheduleResult.getStatus());
    }
    auto response = future.getNoThrow(expCtx->opCtx);
    uassertStatusOK(response.getStatus());
    uassertStatusOK(response.getValue().response.status);
    BSONObj responseData = response.getValue().response.data;
    uassertStatusOK(getStatusFromCommandResult(responseData));
    auto explain = responseData["explain"];
    uassert(4895000,
            "Response must contain an 'explain' field that is of type 'Object'",
            explain.type() == BSONType::Object);
    return explain.embeddedObject().getOwned();
}

BSONObj getSearchExplainResponse(const ExpressionContext* expCtx,
                                 const BSONObj& query,
                                 executor::TaskExecutor* taskExecutor) {
    const auto request = getRemoteCommandRequestForSearchQuery(
        expCtx->opCtx, expCtx->ns, expCtx->uuid, expCtx->explain, query, boost::none);
    return getExplainResponse(expCtx, request, taskExecutor);
}

executor::RemoteCommandResponse runSearchCommandWithRetries(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const BSONObj& cmdObj,
    std::function<bool(Status)> retryPolicy) {
    using namespace fmt::literals;
    auto taskExecutor = executor::getMongotTaskExecutor(expCtx->opCtx->getServiceContext());
    executor::RemoteCommandResponse response =
        Status(ErrorCodes::InternalError, "Internal error running search command");
    for (;;) {
        Status err = Status::OK();
        do {
            auto swCbHnd = taskExecutor->scheduleRemoteCommand(
                getRemoteCommandRequest(expCtx->opCtx, expCtx->ns, cmdObj),
                [&](const auto& args) { response = args.response; });
            err = swCbHnd.getStatus();
            if (!err.isOK()) {
                // scheduling error
                err.addContext("Failed to execute search command: {}"_format(cmdObj.toString()));
                break;
            }
            if (MONGO_likely(shardedSearchOpCtxDisconnect.shouldFail())) {
                expCtx->opCtx->markKilled();
            }
            // It is imperative to wrap the wait() call in a try/catch. If an exception is thrown
            // and not caught, planShardedSearch will exit and all stack-allocated variables will be
            // destroyed. Then later when the executor thread tries to run the callbackFn of
            // scheduleRemoteCommand (the lambda above), it will try to access the `response` var,
            // which had been captured by reference and thus lived on the stack and therefore
            // destroyed as part of stack unwinding, and the server will segfault.

            // By catching the exception and then wait-ing for the callbackFn to run, we
            // ensure that planShardedSearch isn't exited (and the `response` object isn't
            // destroyed) before the callbackFn (which has a reference to `response`) executes.
            try {
                taskExecutor->wait(swCbHnd.getValue(), expCtx->opCtx);
            } catch (const DBException& exception) {
                LOGV2_ERROR(8049900,
                            "An interruption occured while the MongotTaskExecutor was waiting for "
                            "a response",
                            "error"_attr = exception.toStatus());
                // If waiting for the response is interrupted, like by a ClientDisconnectError, then
                // we still have a callback-handle out and registered with the TaskExecutor to run
                // when the response finally does come back.

                // Since the callback-handle references local state, cbResponse, it would
                // be invalid for the callback-handle to run after leaving the this function.
                // Therefore, cancel() stops any work associated with the callback handle (eg
                // network work in the case of scheduleRemoteCommand).

                // The contract for executor::scheduleRemoteCommand(....., callbackFn) requires that
                // callbackFn (the lambda in our case) is always run. Thefore after the cancel(), we
                // wait() for the callbackFn to be run with a not-ok status to inform the executor
                // that the original callback handle call (scheduleRemoteCommand) was canceled.
                taskExecutor->cancel(swCbHnd.getValue());
                taskExecutor->wait(swCbHnd.getValue());
                throw;
            }
            err = response.status;
            if (!err.isOK()) {
                // Local error running the command.
                err.addContext("Failed to execute search command: {}"_format(cmdObj.toString()));
                break;
            }
            err = getStatusFromCommandResult(response.data);
            if (!err.isOK()) {
                // Mongot ran the command and returned an error.
                err.addContext("mongot returned an error");
                break;
            }
        } while (0);

        if (err.isOK())
            return response;
        if (!retryPolicy(err))
            uassertStatusOK(err);
    }
}

void throwIfNotRunningWithMongotHostConfigured(
    const boost::intrusive_ptr<ExpressionContext>& expCtx) {
    // We must validate if a mongot is configured. However, we might just be parsing or validating
    // the query without executing it. In this scenario, there is no reason to check if we are
    // running with a mongot configured, since we will never make a call to the mongot host. For
    // example, if we are in query analysis, performing pipeline-style updates, or creating query
    // shapes. Additionally, it would be an error to validate this inside query analysis, since
    // query analysis doesn't have access to the mongot host.
    //
    // This validation should occur before parsing so in the case of a parse and configuration
    // error, the configuration error is thrown.
    if (expCtx->mongoProcessInterface->isExpectedToExecuteQueries()) {
        doThrowIfNotRunningWithMongotHostConfigured();
    }
}
}  // namespace mongo::mongot_cursor
