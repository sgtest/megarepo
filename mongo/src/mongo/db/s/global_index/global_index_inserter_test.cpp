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

#include "mongo/db/s/global_index/global_index_inserter.h"

// IWYU pragma: no_include "cxxabi.h"
#include <future>
#include <ostream>
#include <system_error>
#include <utility>

#include "mongo/base/error_codes.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/database_name.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/ops/write_ops_gen.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/s/global_index/global_index_util.h"
#include "mongo/db/s/shard_server_test_fixture.h"
#include "mongo/db/s/transaction_coordinator_service.h"
#include "mongo/db/session/logical_session_cache.h"
#include "mongo/db/session/logical_session_cache_noop.h"
#include "mongo/db/session/session_catalog_mongod.h"
#include "mongo/executor/network_connection_hook.h"
#include "mongo/executor/network_interface_factory.h"
#include "mongo/executor/thread_pool_task_executor.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/rpc/metadata/metadata_hook.h"
#include "mongo/s/request_types/sharded_ddl_commands_gen.h"
#include "mongo/stdx/future.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/concurrency/thread_pool.h"
#include "mongo/util/fail_point.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTest

namespace mongo {
namespace global_index {
namespace {

class GlobalIndexInserterTest : public ShardServerTestFixture {
public:
    void setUp() override {
        ShardServerTestFixture::setUp();

        // Create config.transactions collection
        auto opCtx = operationContext();
        DBDirectClient client(opCtx);
        client.createCollection(NamespaceString::kSessionTransactionsTableNamespace);
        client.createIndexes(NamespaceString::kSessionTransactionsTableNamespace,
                             {MongoDSessionCatalog::getConfigTxnPartialIndexSpec()});

        LogicalSessionCache::set(getServiceContext(), std::make_unique<LogicalSessionCacheNoop>());

        // Note: needed to initialize txn coordinator because first thing commit command does is
        // cancelling coordinator if in a sharded environment.
        TransactionCoordinatorService::get(operationContext())
            ->onShardingInitialization(operationContext(), true);

        // Use our own executor since the executor from the fixture is using NetworkInterfaceMock
        // that uses a ClockSourceMock. This means that tasks that are scheduled to be run in the
        // future will not run unless the clock is advanced manually.
        _executor = makeTaskExecutorForCloner();

        CreateGlobalIndex createGlobalIndex(_indexUUID);
        createGlobalIndex.setDbName(DatabaseName::kAdmin);
        BSONObj cmdResult;
        auto success =
            client.runCommand(DatabaseName::kAdmin, createGlobalIndex.toBSON({}), cmdResult);
        ASSERT(success) << "createGlobalIndex cmd failed with result: " << cmdResult;
    }

    void tearDown() override {
        _executor->shutdown();
        _executor->join();

        TransactionCoordinatorService::get(operationContext())->onStepDown();
        ShardServerTestFixture::tearDown();
    }

    const NamespaceString& nss() const {
        return _nss;
    }

    const std::string& indexName() const {
        return _indexName;
    }

    const UUID& indexUUID() const {
        return _indexUUID;
    }

    NamespaceString skipIdNss() const {
        return global_index::skipIdNss(_nss, _indexName);
    }

    NamespaceString globalIndexNss() const {
        return NamespaceString::makeGlobalIndexNSS(indexUUID());
    }

    std::shared_ptr<executor::ThreadPoolTaskExecutor> getExecutor() {
        return _executor;
    }

private:
    std::shared_ptr<executor::ThreadPoolTaskExecutor> makeTaskExecutorForCloner() {
        ThreadPool::Options threadPoolOptions;
        threadPoolOptions.maxThreads = 1;
        threadPoolOptions.threadNamePrefix = "TestGlobalIndexCloner-";
        threadPoolOptions.poolName = "TestGlobalIndexClonerThreadPool";

        auto executor = std::make_shared<executor::ThreadPoolTaskExecutor>(
            std::make_unique<ThreadPool>(std::move(threadPoolOptions)),
            executor::makeNetworkInterface("TestGlobalIndexClonerNetwork", nullptr, nullptr));
        executor->startup();

        return executor;
    }

    const NamespaceString _nss = NamespaceString::createNamespaceString_forTest("test", "user");
    const std::string _indexName{"global_x"};
    const UUID _indexUUID{UUID::gen()};
    const RAIIServerParameterControllerForTest _enableFeature{"featureFlagGlobalIndexes", true};

    std::shared_ptr<executor::ThreadPoolTaskExecutor> _executor;
};

TEST_F(GlobalIndexInserterTest, ClonerUpdatesIndexEntryAndSkipIdCollection) {
    GlobalIndexInserter cloner(nss(), indexName(), indexUUID(), getExecutor());

    const auto indexKeyValues = BSON("x" << 34);
    const auto documentKey = BSON("_id" << 12 << "x" << 34);
    cloner.processDoc(operationContext(), indexKeyValues, documentKey);

    DBDirectClient client(operationContext());
    ASSERT_EQ(1, client.count(globalIndexNss()));

    FindCommandRequest skipIdQuery(skipIdNss());
    auto skipIdDoc = client.findOne(skipIdQuery);
    ASSERT_BSONOBJ_EQ(BSON("_id" << documentKey), skipIdDoc);
}

TEST_F(GlobalIndexInserterTest, ClonerSkipsDocumentIfInSkipCollection) {
    GlobalIndexInserter cloner(nss(), indexName(), indexUUID(), getExecutor());

    const auto indexKeyValues = BSON("x" << 34);
    const auto documentKey = BSON("_id" << 12 << "x" << 34);

    DBDirectClient client(operationContext());
    write_ops::InsertCommandRequest skipIdInsert(skipIdNss());
    skipIdInsert.setDocuments({BSON("_id" << documentKey)});
    client.insert(skipIdInsert);

    cloner.processDoc(operationContext(), indexKeyValues, documentKey);

    ASSERT_EQ(0, client.count(globalIndexNss()));
}

TEST_F(GlobalIndexInserterTest, ClonerRetriesWhenItEncountersWCE) {
    GlobalIndexInserter cloner(nss(), indexName(), indexUUID(), getExecutor());

    DBDirectClient client(operationContext());

    auto clonerThread = ([&] {
        FailPointEnableBlock fp("globalIndexInserterPauseAfterReadingSkipCollection");

        const auto indexKeyValues = BSON("x" << 34);
        const auto documentKey = BSON("_id" << 12 << "x" << 34);

        auto future = stdx::async(stdx::launch::async, [&] {
            cloner.processDoc(operationContext(), indexKeyValues, documentKey);
        });

        fp->waitForTimesEntered(1);

        write_ops::InsertCommandRequest skipIdInsert(skipIdNss());
        skipIdInsert.setDocuments({BSON("_id" << documentKey)});
        client.insert(skipIdInsert);

        return future;
    })();

    clonerThread.get();

    ASSERT_EQ(0, client.count(globalIndexNss()));
}

TEST_F(GlobalIndexInserterTest, ClonerThrowsIfIndexEntryAlreadyExists) {
    GlobalIndexInserter cloner(nss(), indexName(), indexUUID(), getExecutor());

    const auto indexKeyValues = BSON("x" << 34);
    const auto documentKey = BSON("_id" << 12 << "x" << 34);
    const auto documentKey2 = BSON("_id" << 25 << "x" << 34);

    cloner.processDoc(operationContext(), indexKeyValues, documentKey);
    ASSERT_THROWS_CODE(cloner.processDoc(operationContext(), indexKeyValues, documentKey2),
                       DBException,
                       ErrorCodes::DuplicateKey);
}

}  // namespace
}  // namespace global_index
}  // namespace mongo
