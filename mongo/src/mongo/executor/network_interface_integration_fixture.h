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
#pragma once

#include <cstddef>
#include <functional>
#include <memory>
#include <mutex>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/client/connection_string.h"
#include "mongo/db/baton.h"
#include "mongo/executor/connection_pool.h"
#include "mongo/executor/network_connection_hook.h"
#include "mongo/executor/network_interface.h"
#include "mongo/executor/remote_command_request.h"
#include "mongo/executor/remote_command_response.h"
#include "mongo/executor/task_executor.h"
#include "mongo/platform/mutex.h"
#include "mongo/platform/random.h"
#include "mongo/stdx/condition_variable.h"
#include "mongo/transport/transport_layer.h"
#include "mongo/unittest/framework.h"
#include "mongo/unittest/unittest.h"
#include "mongo/util/duration.h"
#include "mongo/util/future.h"

namespace mongo {

class PseudoRandom;

namespace executor {

/**
 * A mock class mimicking TaskExecutor::CallbackState, does nothing.
 */
class MockCallbackState final : public TaskExecutor::CallbackState {
public:
    MockCallbackState() = default;
    void cancel() override {}
    void waitForCompletion() override {}
    bool isCanceled() const override {
        return false;
    }
};

inline TaskExecutor::CallbackHandle makeCallbackHandle() {
    return TaskExecutor::CallbackHandle(std::make_shared<MockCallbackState>());
}

using StartCommandCB = std::function<void(const RemoteCommandResponse&)>;

class NetworkInterfaceIntegrationFixture : public mongo::unittest::Test {
public:
    void createNet(std::unique_ptr<NetworkConnectionHook> connectHook = nullptr,
                   ConnectionPool::Options options = {});
    void startNet(std::unique_ptr<NetworkConnectionHook> connectHook = nullptr);
    void tearDown() override;

    NetworkInterface& net();

    ConnectionString fixture();

    void setRandomNumberGenerator(PseudoRandom* generator);

    void resetIsInternalClient(bool isInternalClient);

    PseudoRandom* getRandomNumberGenerator();

    void startCommand(const TaskExecutor::CallbackHandle& cbHandle,
                      RemoteCommandRequest& request,
                      StartCommandCB onFinish);

    Future<RemoteCommandResponse> runCommand(const TaskExecutor::CallbackHandle& cbHandle,
                                             RemoteCommandRequestOnAny rcroa);

    Future<RemoteCommandOnAnyResponse> runCommandOnAny(const TaskExecutor::CallbackHandle& cbHandle,
                                                       RemoteCommandRequestOnAny request);

    Future<void> startExhaustCommand(
        const TaskExecutor::CallbackHandle& cbHandle,
        RemoteCommandRequest request,
        std::function<void(const RemoteCommandResponse&)> exhaustUtilCB,
        const BatonHandle& baton = nullptr);

    RemoteCommandResponse runCommandSync(RemoteCommandRequest& request);

    void assertCommandOK(const DatabaseName& db,
                         const BSONObj& cmd,
                         Milliseconds timeoutMillis = Minutes(5),
                         transport::ConnectSSLMode sslMode = transport::kGlobalSSLMode);
    void assertCommandFailsOnClient(const DatabaseName& db,
                                    const BSONObj& cmd,
                                    ErrorCodes::Error reason,
                                    Milliseconds timeoutMillis = Minutes(5));

    void assertCommandFailsOnServer(const DatabaseName& db,
                                    const BSONObj& cmd,
                                    ErrorCodes::Error reason,
                                    Milliseconds timeoutMillis = Minutes(5));

    void assertWriteError(const DatabaseName& db,
                          const BSONObj& cmd,
                          ErrorCodes::Error reason,
                          Milliseconds timeoutMillis = Minutes(5));

    size_t getInProgress() {
        auto lk = stdx::unique_lock(_mutex);
        return _workInProgress;
    }

private:
    void _onSchedulingCommand();
    void _onCompletingCommand();

    std::unique_ptr<NetworkInterface> _net;
    PseudoRandom* _rng = nullptr;

    size_t _workInProgress = 0;
    stdx::condition_variable _fixtureIsIdle;
    mutable Mutex _mutex = MONGO_MAKE_LATCH("NetworkInterfaceIntegrationFixture::_mutex");
};

}  // namespace executor
}  // namespace mongo
