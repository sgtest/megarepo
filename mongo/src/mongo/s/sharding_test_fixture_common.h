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

#include <boost/preprocessor/control/iif.hpp>
#include <memory>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/client/remote_command_targeter_factory_mock.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/service_context.h"
#include "mongo/db/service_context_test_fixture.h"
#include "mongo/executor/network_interface_mock.h"
#include "mongo/executor/network_test_env.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/grid.h"
#include "mongo/transport/session.h"
#include "mongo/unittest/temp_dir.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/time_support.h"

namespace mongo {

/**
 * Contains common functionality and tools, which apply to both mongos and mongod unit-tests.
 */
class ShardingTestFixtureCommon : public virtual ServiceContextTest {
public:
    /**
     * Constructs a standalone RoutingTableHistory object (i.e., not linked to any CatalogCache),
     * which can be used to pass to ChunkManager for tests, which specifically target the behaviour
     * of the ChunkManager.
     */
    static RoutingTableHistoryValueHandle makeStandaloneRoutingTableHistory(RoutingTableHistory rt);

protected:
    ShardingTestFixtureCommon();
    ~ShardingTestFixtureCommon();

    void setUp() override;

    void tearDown() override;

    OperationContext* operationContext() const {
        invariant(_opCtxHolder,
                  "ShardingTestFixtureCommon::setUp() must have been called before this method");
        return _opCtxHolder.get();
    }

    template <typename Lambda>
    executor::NetworkTestEnv::FutureHandle<typename std::invoke_result<Lambda>::type> launchAsync(
        Lambda&& func) const {
        return _networkTestEnv->launchAsync(std::forward<Lambda>(func));
    }

    executor::NetworkInterfaceMock* network() const {
        invariant(_mockNetwork);
        return _mockNetwork;
    }

    RemoteCommandTargeterFactoryMock* targeterFactory() const {
        invariant(_targeterFactory);
        return _targeterFactory;
    }

    /**
     * Blocking methods, which receive one message from the network and respond using the responses
     * returned from the input function. This is a syntactic sugar for simple, single request +
     * response or find tests.
     */
    void onCommand(executor::NetworkTestEnv::OnCommandFunction func);
    void onCommands(std::vector<executor::NetworkTestEnv::OnCommandFunction> funcs);
    void onCommandWithMetadata(executor::NetworkTestEnv::OnCommandWithMetadataFunction func);
    void onFindCommand(executor::NetworkTestEnv::OnFindCommandFunction func);
    void onFindWithMetadataCommand(
        executor::NetworkTestEnv::OnFindCommandWithMetadataFunction func);

    /**
     * Waits for an operation which creates a capped config collection with the specified name and
     * capped size.
     */
    void expectConfigCollectionCreate(const HostAndPort& configHost,
                                      StringData collName,
                                      int cappedSize,
                                      const BSONObj& response);

    /**
     * Wait for a single insert in one of the change or action log collections with the specified
     * contents and return a successful response.
     */
    void expectConfigCollectionInsert(const HostAndPort& configHost,
                                      StringData collName,
                                      Date_t timestamp,
                                      const std::string& what,
                                      const std::string& ns,
                                      const BSONObj& detail);

    virtual std::unique_ptr<ShardingCatalogClient> makeShardingCatalogClient() {
        return nullptr;
    }

    // Since a NetworkInterface is a private member of a TaskExecutor, we store a raw pointer to the
    // fixed TaskExecutor's NetworkInterface here.
    //
    // TODO(Esha): Currently, some fine-grained synchronization of the network and task executor is
    // outside of NetworkTestEnv's capabilities. If all control of the network is done through
    // _networkTestEnv, storing this raw pointer is not necessary.
    executor::NetworkInterfaceMock* _mockNetwork{nullptr};

    // Allows for processing tasks through the NetworkInterfaceMock/ThreadPoolMock subsystem
    std::unique_ptr<executor::NetworkTestEnv> _networkTestEnv;

    // Since the RemoteCommandTargeterFactory is currently a private member of ShardFactory, we
    // store a raw pointer to it here.
    RemoteCommandTargeterFactoryMock* _targeterFactory = nullptr;

private:
    // Keeps the lifetime of the operation context
    ServiceContext::UniqueOperationContext _opCtxHolder;

    // The temporary dbpath for the tests in this fixture.
    unittest::TempDir _tempDir;
};

}  // namespace mongo
