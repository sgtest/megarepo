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

#include "mongo/db/mongod_options.h"
#include "mongo/db/server_options.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/unittest/unittest.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/options_parser/environment.h"
#include "mongo/util/options_parser/value.h"

namespace mongo {
namespace {

class MongodOptionsTest : public unittest::Test {
public:
    class Environment : public moe::Environment {
    public:
        auto& setPort(int port) {
            _set("net.port", port);
            return *this;
        }

        auto& setRouterPort(int port = ServerGlobalParams::RouterPort) {
            _set("net.routerPort", port);
            return *this;
        }

        auto& setClusterRole(std::string role) {
            _set("sharding.clusterRole", role);
            return *this;
        }

        auto& setReplicaSet(std::string rs) {
            _set("replication.replSet", rs);
            return *this;
        }

    private:
        template <typename K, typename V>
        void _set(K key, V value) {
            uassertStatusOK(set(moe::Key(key), moe::Value(value)));
        }
    };

    void setUp() override {
        // Note that the tests are currently only focused on the cluster role and listening ports,
        // so we only reset the relevant states between runs.
        ServerGlobalParams defaults;
        serverGlobalParams.port = defaults.port;
        serverGlobalParams.clusterRole = defaults.clusterRole;
        serverGlobalParams.routerPort = defaults.routerPort;

        env = Environment{};
    }

    Environment env;

private:
    RAIIServerParameterControllerForTest _scopedFeature{"featureFlagEmbeddedRouter", true};
};

TEST_F(MongodOptionsTest, Base) {
    ASSERT_OK(storeMongodOptions(env));
}

TEST_F(MongodOptionsTest, RouterAndShardServerWithDefaultPorts) {
    env.setClusterRole("shardsvr").setReplicaSet("myRS").setRouterPort();
    ASSERT_OK(storeMongodOptions(env));

    ASSERT_EQ(serverGlobalParams.port, ServerGlobalParams::ShardServerPort);
    ASSERT(serverGlobalParams.routerPort);
    ASSERT_EQ(*serverGlobalParams.routerPort, ServerGlobalParams::RouterPort);
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::RouterServer));
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::ShardServer));
}

TEST_F(MongodOptionsTest, RouterAndShardServerWithCustomPorts) {
    env.setClusterRole("shardsvr").setReplicaSet("myRS").setPort(123).setRouterPort(456);
    ASSERT_OK(storeMongodOptions(env));

    ASSERT_EQ(serverGlobalParams.port, 123);
    ASSERT(serverGlobalParams.routerPort);
    ASSERT_EQ(*serverGlobalParams.routerPort, 456);
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::RouterServer));
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::ShardServer));
}

TEST_F(MongodOptionsTest, RouterAndConfigServerWithDefaultPorts) {
    env.setClusterRole("configsvr").setReplicaSet("myRS").setRouterPort();
    ASSERT_OK(storeMongodOptions(env));

    ASSERT_EQ(serverGlobalParams.port, ServerGlobalParams::ConfigServerPort);
    ASSERT(serverGlobalParams.routerPort);
    ASSERT_EQ(*serverGlobalParams.routerPort, ServerGlobalParams::RouterPort);
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::RouterServer));
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::ConfigServer));
}

TEST_F(MongodOptionsTest, RouterAndConfigServerWithCustomPorts) {
    env.setClusterRole("configsvr").setReplicaSet("myRS").setPort(123).setRouterPort(456);
    ASSERT_OK(storeMongodOptions(env));

    ASSERT_EQ(serverGlobalParams.port, 123);
    ASSERT(serverGlobalParams.routerPort);
    ASSERT_EQ(*serverGlobalParams.routerPort, 456);
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::RouterServer));
    ASSERT_TRUE(serverGlobalParams.clusterRole.has(ClusterRole::ConfigServer));
}

}  // namespace
}  // namespace mongo
