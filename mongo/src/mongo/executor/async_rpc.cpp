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

#include "mongo/executor/async_rpc.h"

#include <boost/smart_ptr.hpp>
#include <string>
#include <tuple>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/executor/remote_command_request.h"
#include "mongo/executor/task_executor.h"
#include "mongo/rpc/metadata.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/database_name_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/net/hostandport.h"

namespace mongo::async_rpc {
namespace detail {
namespace {
const auto getRCRImpl = ServiceContext::declareDecoration<std::unique_ptr<AsyncRPCRunner>>();
}  // namespace

MONGO_FAIL_POINT_DEFINE(pauseAsyncRPCAfterNetworkResponse);
MONGO_FAIL_POINT_DEFINE(pauseScheduleCallWithCancelTokenUntilCanceled);

class AsyncRPCRunnerImpl : public AsyncRPCRunner {
public:
    /**
     * Executes the BSON command asynchronously on the given target.
     *
     * Do not call directly - this is not part of the public API.
     */
    ExecutorFuture<AsyncRPCInternalResponse> _sendCommand(
        std::shared_ptr<TaskExecutor> exec,
        CancellationToken token,
        OperationContext* opCtx,
        Targeter* targeter,
        const DatabaseName& dbName,
        BSONObj cmdBSON,
        BatonHandle baton,
        boost::optional<UUID> clientOperationKey) final {
        auto proxyExec = std::make_shared<ProxyingExecutor>(exec, baton);
        return targeter->resolve(token)
            .thenRunOn(proxyExec)
            .then([dbName,
                   cmdBSON,
                   opCtx,
                   exec = std::move(exec),
                   token,
                   baton = std::move(baton),
                   clientOperationKey](std::vector<HostAndPort> targets) {
                invariant(targets.size(),
                          "Successful targeting implies there are hosts to target.");
                executor::RemoteCommandRequestOnAny executorRequest(
                    targets,
                    dbName,
                    cmdBSON,
                    rpc::makeEmptyMetadata(),
                    opCtx,
                    executor::RemoteCommandRequest::kNoTimeout,
                    {},
                    clientOperationKey);

                // Fail point to make this method to wait until the token is canceled.
                if (!token.isCanceled()) {
                    try {
                        pauseScheduleCallWithCancelTokenUntilCanceled.pauseWhileSetAndNotCanceled(
                            Interruptible::notInterruptible(), token);
                    } catch (ExceptionFor<ErrorCodes::Interrupted>&) {
                        // Swallow the interrupted exception that arrives from canceling a
                        // failpoint.
                    }
                }

                auto [p, f] = makePromiseFuture<TaskExecutor::RemoteCommandOnAnyCallbackArgs>();
                auto swCallbackHandle = exec->scheduleRemoteCommandOnAny(
                    executorRequest,
                    [p = std::make_shared<Promise<TaskExecutor::RemoteCommandOnAnyCallbackArgs>>(
                         std::move(p))](
                        const TaskExecutor::RemoteCommandOnAnyCallbackArgs& cbData) {
                        pauseAsyncRPCAfterNetworkResponse.pauseWhileSet();
                        p->emplaceValue(cbData);
                    },
                    std::move(baton));
                uassertStatusOK(swCallbackHandle);
                token.onCancel()
                    .unsafeToInlineFuture()
                    .then(
                        [exec, callbackHandle = std::move(swCallbackHandle.getValue())]() mutable {
                            exec->cancel(callbackHandle);
                        })
                    .getAsync([](auto) {});
                return std::move(f);
            })
            .onError([](Status s)
                         -> StatusWith<TaskExecutor::TaskExecutor::RemoteCommandOnAnyCallbackArgs> {
                // If there was a scheduling error or other local error before the
                // command was accepted by the executor.
                return Status{AsyncRPCErrorInfo(s, {}), "Remote command execution failed"};
            })
            .then([targeter](TaskExecutor::RemoteCommandOnAnyCallbackArgs cbargs) {
                auto r = cbargs.response;
                auto s = makeErrorIfNeeded(r, r.target);
                // Update targeter for errors.
                if (!s.isOK() && s.code() == ErrorCodes::RemoteCommandExecutionError && r.target) {
                    auto extraInfo = s.extraInfo<AsyncRPCErrorInfo>();
                    if (extraInfo->isLocal()) {
                        targeter->onRemoteCommandError(*(r.target), extraInfo->asLocal()).get();
                    } else {
                        targeter
                            ->onRemoteCommandError(*(r.target),
                                                   extraInfo->asRemote().getRemoteCommandResult())
                            .get();
                    }
                }
                uassertStatusOK(s);
                return AsyncRPCInternalResponse{r.data, r.target.get(), *r.elapsed};
            });
    }
};

const auto implRegisterer =
    ServiceContext::ConstructorActionRegisterer{"RemoteCommmandRunner", [](ServiceContext* ctx) {
                                                    getRCRImpl(ctx) =
                                                        std::make_unique<AsyncRPCRunnerImpl>();
                                                }};

AsyncRPCRunner* AsyncRPCRunner::get(ServiceContext* svcCtx) {
    return getRCRImpl(svcCtx).get();
}

void AsyncRPCRunner::set(ServiceContext* svcCtx, std::unique_ptr<AsyncRPCRunner> theRunner) {
    getRCRImpl(svcCtx) = std::move(theRunner);
}
}  // namespace detail
}  // namespace mongo::async_rpc
