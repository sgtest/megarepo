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

#include <memory>
#include <string>

#include "mongo/client/remote_command_targeter_mock.h"
#include "mongo/db/s/sharding_mongod_test_fixture.h"
#include "mongo/db/shard_id.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/catalog_cache_loader.h"
#include "mongo/s/catalog_cache_loader_mock.h"
#include "mongo/s/catalog_cache_mock.h"
#include "mongo/util/net/hostandport.h"

namespace mongo {

/**
 * Test fixture for shard components, as opposed to config or mongos components. Provides a mock
 * network via ShardingMongoDTestFixture.
 */
class ShardServerTestFixture : public ShardingMongoDTestFixture {
protected:
    ShardServerTestFixture(Options options = {}, bool setUpMajorityReads = true);
    ~ShardServerTestFixture();

    void setUp() override;

    std::unique_ptr<ShardingCatalogClient> makeShardingCatalogClient() override;

    void setCatalogCacheLoader(std::unique_ptr<CatalogCacheLoader> loader);

    /**
     * Returns the mock targeter for the config server. Useful to use like so,
     *
     *     configTargeterMock()->setFindHostReturnValue(HostAndPort);
     *     configTargeterMock()->setFindHostReturnValue({ErrorCodes::InternalError, "can't target"})
     *
     * Remote calls always need to resolve a host with RemoteCommandTargeterMock::findHost, so it
     * must be set.
     */
    std::shared_ptr<RemoteCommandTargeterMock> configTargeterMock();

    const HostAndPort kConfigHostAndPort{"dummy", 123};
    const ShardId kMyShardName{"myShardName"};

    service_context_test::ShardRoleOverride _shardRole;

    std::unique_ptr<CatalogCacheLoader> _catalogCacheLoader;
};

class ShardServerTestFixtureWithCatalogCacheMock : public ShardServerTestFixture {
protected:
    void setUp() override;
    virtual std::unique_ptr<CatalogCache> makeCatalogCache() override;
    CatalogCacheMock* getCatalogCacheMock();
    CatalogCacheLoaderMock* getCatalogCacheLoaderMock();

private:
    CatalogCacheLoaderMock* _cacheLoaderMock;
};

class ShardServerTestFixtureWithCatalogCacheLoaderMock : public ShardServerTestFixture {
protected:
    void setUp() override;
    CatalogCacheMock* getCatalogCacheMock();
    CatalogCacheLoaderMock* getCatalogCacheLoaderMock();

private:
    CatalogCacheLoaderMock* _cacheLoaderMock;
};

}  // namespace mongo
