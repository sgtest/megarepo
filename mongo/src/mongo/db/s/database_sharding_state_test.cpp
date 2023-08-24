/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include <boost/none.hpp>
#include <fmt/format.h>
#include <memory>
#include <string>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/connection_string.h"
#include "mongo/client/remote_command_targeter_factory_mock.h"
#include "mongo/client/remote_command_targeter_mock.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/cluster_role.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/repl/optime_with.h"
#include "mongo/db/repl/read_concern_level.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/wait_for_majority_service.h"
#include "mongo/db/s/database_sharding_state.h"
#include "mongo/db/s/shard_filtering_metadata_refresh.h"
#include "mongo/db/s/shard_server_test_fixture.h"
#include "mongo/db/s/sharding_mongod_test_fixture.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/server_options.h"
#include "mongo/db/shard_id.h"
#include "mongo/db/tenant_id.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/catalog/sharding_catalog_client_mock.h"
#include "mongo/s/catalog/type_collection.h"
#include "mongo/s/catalog/type_shard.h"
#include "mongo/s/catalog_cache_loader.h"
#include "mongo/s/catalog_cache_loader_mock.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace {

class DatabaseShardingStateTestWithMockedLoader
    : public ShardServerTestFixtureWithCatalogCacheLoaderMock {
public:
    const DatabaseName kDbName = DatabaseName::createDatabaseName_forTest(boost::none, "test");

    const HostAndPort kConfigHostAndPort{"DummyConfig", 12345};
    const std::vector<ShardType> kShardList = {ShardType(_myShardName.toString(), "Host0:12345")};

    void setUp() override {
        ShardServerTestFixtureWithCatalogCacheLoaderMock::setUp();

        WaitForMajorityService::get(getServiceContext()).startup(getServiceContext());

        for (const auto& shard : kShardList) {
            std::unique_ptr<RemoteCommandTargeterMock> targeter(
                std::make_unique<RemoteCommandTargeterMock>());
            HostAndPort host(shard.getHost());
            targeter->setConnectionStringReturnValue(ConnectionString(host));
            targeter->setFindHostReturnValue(host);
            targeterFactory()->addTargeterToReturn(ConnectionString(host), std::move(targeter));
        }
    }

    void tearDown() override {
        WaitForMajorityService::get(getServiceContext()).shutDown();

        ShardServerTestFixtureWithCatalogCacheLoaderMock::tearDown();
    }

    class StaticCatalogClient final : public ShardingCatalogClientMock {
    public:
        StaticCatalogClient(std::vector<ShardType> shards) : _shards(std::move(shards)) {}

        StatusWith<repl::OpTimeWith<std::vector<ShardType>>> getAllShards(
            OperationContext* opCtx, repl::ReadConcernLevel readConcern) override {
            return repl::OpTimeWith<std::vector<ShardType>>(_shards);
        }

        std::vector<CollectionType> getCollections(OperationContext* opCtx,
                                                   StringData dbName,
                                                   repl::ReadConcernLevel readConcernLevel,
                                                   const BSONObj& sort) override {
            return _colls;
        }

        void setCollections(std::vector<CollectionType> colls) {
            _colls = std::move(colls);
        }

    private:
        const std::vector<ShardType> _shards;
        std::vector<CollectionType> _colls;
    };

    std::unique_ptr<ShardingCatalogClient> makeShardingCatalogClient() override {
        return std::make_unique<StaticCatalogClient>(kShardList);
    }

    DatabaseType createDatabase(const UUID& uuid, const Timestamp& timestamp) {
        return DatabaseType(
            kDbName.toString_forTest(), kShardList[0].getName(), DatabaseVersion(uuid, timestamp));
    }
};

TEST_F(DatabaseShardingStateTestWithMockedLoader, OnDbVersionMismatch) {
    const auto oldDb = createDatabase(UUID::gen(), Timestamp(1));
    const auto newDb = createDatabase(UUID::gen(), Timestamp(2));

    auto checkOnDbVersionMismatch = [&](const auto& newDb, bool expectRefresh) {
        const auto newDbVersion = newDb.getVersion();
        auto opCtx = operationContext();
        auto getActiveDbVersion = [&] {
            AutoGetDb autoDb(opCtx, kDbName, MODE_IS);
            const auto scopedDss =
                DatabaseShardingState::assertDbLockedAndAcquireShared(opCtx, kDbName);
            return scopedDss->getDbVersion(opCtx);
        };

        getCatalogCacheLoaderMock()->setDatabaseRefreshReturnValue(newDb);
        ASSERT_OK(onDbVersionMismatchNoExcept(opCtx, kDbName, newDbVersion));

        auto activeDbVersion = getActiveDbVersion();
        ASSERT_TRUE(activeDbVersion);
        if (expectRefresh) {
            ASSERT_EQUALS(newDbVersion.getTimestamp(), activeDbVersion->getTimestamp());
        }
    };

    checkOnDbVersionMismatch(oldDb, true);
    checkOnDbVersionMismatch(newDb, true);
    checkOnDbVersionMismatch(oldDb, false);
}

TEST_F(DatabaseShardingStateTestWithMockedLoader, ForceDatabaseRefresh) {
    const auto uuid = UUID::gen();

    const auto oldDb = createDatabase(uuid, Timestamp(1));
    const auto newDb = createDatabase(uuid, Timestamp(2));

    auto checkForceDatabaseRefresh = [&](const auto& newDb, bool expectRefresh) {
        const auto newDbVersion = newDb.getVersion();
        auto opCtx = operationContext();

        getCatalogCacheLoaderMock()->setDatabaseRefreshReturnValue(newDb);
        ASSERT_OK(onDbVersionMismatchNoExcept(opCtx, kDbName, boost::none));

        boost::optional<DatabaseVersion> activeDbVersion = [&] {
            AutoGetDb autoDb(opCtx, kDbName, MODE_IS);
            const auto scopedDss =
                DatabaseShardingState::assertDbLockedAndAcquireShared(opCtx, kDbName);
            return scopedDss->getDbVersion(opCtx);
        }();
        ASSERT_TRUE(activeDbVersion);
        if (expectRefresh) {
            ASSERT_EQ(newDbVersion.getTimestamp(), activeDbVersion->getTimestamp());
        }
    };

    checkForceDatabaseRefresh(oldDb, true);
    checkForceDatabaseRefresh(newDb, true);
    checkForceDatabaseRefresh(oldDb, false);
}

}  // namespace
}  // namespace mongo
