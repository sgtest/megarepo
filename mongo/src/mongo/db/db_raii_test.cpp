/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <functional>
#include <string>

#include <boost/move/utility_core.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/timestamp.h"
#include "mongo/crypto/encryption_fields_gen.h"
#include "mongo/db/catalog/catalog_test_fixture.h"
#include "mongo/db/catalog/clustered_collection_options_gen.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/locker_impl.h"
#include "mongo/db/curop.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/matcher/expression_parser.h"
#include "mongo/db/matcher/extensions_callback_noop.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/query/canonical_query.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/find_common.h"
#include "mongo/db/query/get_executor.h"
#include "mongo/db/query/plan_executor.h"
#include "mongo/db/query/query_request_helper.h"
#include "mongo/db/query/tailable_mode_gen.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/snapshot_manager.h"
#include "mongo/db/storage/storage_engine.h"
#include "mongo/db/timeseries/timeseries_gen.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/death_test.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTest

namespace mongo {
namespace {

class DBRAIITestFixture : public CatalogTestFixture {
public:
    typedef std::pair<ServiceContext::UniqueClient, ServiceContext::UniqueOperationContext>
        ClientAndCtx;

    ClientAndCtx makeClientWithLocker(const std::string& clientName) {
        auto client = getServiceContext()->getService()->makeClient(clientName);
        auto opCtx = client->makeOperationContext();
        client->swapLockState(std::make_unique<LockerImpl>(opCtx->getServiceContext()));
        return std::make_pair(std::move(client), std::move(opCtx));
    }

    const NamespaceString nss = NamespaceString::createNamespaceString_forTest("test", "coll");
    const Milliseconds timeoutMs = Seconds(1);
    const ClientAndCtx client1 = makeClientWithLocker("client1");
    const ClientAndCtx client2 = makeClientWithLocker("client2");
};

std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> makeTailableQueryPlan(
    OperationContext* opCtx, const CollectionPtr& collection) {
    auto findCommand = std::make_unique<FindCommandRequest>(collection->ns());
    query_request_helper::setTailableMode(TailableModeEnum::kTailableAndAwaitData,
                                          findCommand.get());

    awaitDataState(opCtx).shouldWaitForInserts = true;
    awaitDataState(opCtx).waitForInsertsDeadline =
        opCtx->getServiceContext()->getPreciseClockSource()->now() + Seconds(1);
    CurOp::get(opCtx)->ensureStarted();

    auto cq = std::make_unique<CanonicalQuery>(CanonicalQueryParams{
        .expCtx = makeExpressionContext(opCtx, *findCommand),
        .parsedFind = ParsedFindCommandParams{.findCommand = std::move(findCommand),
                                              .allowedFeatures =
                                                  MatchExpressionParser::kBanAllSpecialFeatures}});
    bool permitYield = true;
    auto swExec = getExecutorFind(opCtx,
                                  &collection,
                                  std::move(cq),
                                  nullptr /* extractAndAttachPipelineStages */,
                                  permitYield);
    ASSERT_OK(swExec.getStatus());
    return std::move(swExec.getValue());
}

void failsWithLockTimeout(std::function<void()> func, Milliseconds timeoutMillis) {
    Date_t t1 = Date_t::now();
    try {
        func();
        FAIL("Should have gotten an exception due to timeout");
    } catch (const ExceptionFor<ErrorCodes::LockTimeout>& ex) {
        LOGV2(20578, "{ex}", "ex"_attr = ex);
        Date_t t2 = Date_t::now();
        ASSERT_GTE(t2 - t1, timeoutMillis);
    }
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadCollLockDeadline) {
    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));
    Lock::CollectionLock collLock1(client1.second.get(), nss, MODE_X);
    ASSERT(client1.second->lockState()->isCollectionLockedForMode(nss, MODE_X));
    failsWithLockTimeout(
        [&] {
            AutoGetCollectionForRead acfr(
                client2.second.get(),
                nss,
                AutoGetCollection::Options{}.deadline(Date_t::now() + timeoutMs));
        },
        timeoutMs);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadDBLockDeadline) {
    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_X);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_X));
    failsWithLockTimeout(
        [&] {
            AutoGetCollectionForRead coll(
                client2.second.get(),
                nss,
                AutoGetCollection::Options{}.deadline(Date_t::now() + timeoutMs));
        },
        timeoutMs);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadGlobalLockDeadline) {
    Lock::GlobalLock gLock1(client1.second.get(), MODE_X);
    ASSERT(client1.second->lockState()->isLocked());
    failsWithLockTimeout(
        [&] {
            AutoGetCollectionForRead coll(
                client2.second.get(),
                nss,
                AutoGetCollection::Options{}.deadline(Date_t::now() + timeoutMs));
        },
        timeoutMs);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadDeadlineNow) {
    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));
    Lock::CollectionLock collLock1(client1.second.get(), nss, MODE_X);
    ASSERT(client1.second->lockState()->isCollectionLockedForMode(nss, MODE_X));

    failsWithLockTimeout(
        [&] {
            AutoGetCollectionForRead coll(
                client2.second.get(), nss, AutoGetCollection::Options{}.deadline(Date_t::now()));
        },
        Milliseconds(0));
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadDeadlineMin) {
    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));
    Lock::CollectionLock collLock1(client1.second.get(), nss, MODE_X);
    ASSERT(client1.second->lockState()->isCollectionLockedForMode(nss, MODE_X));

    failsWithLockTimeout(
        [&] {
            AutoGetCollectionForRead coll(
                client2.second.get(), nss, AutoGetCollection::Options{}.deadline(Date_t()));
        },
        Milliseconds(0));
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadDBLockCompatibleXNoCollection) {
    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));

    AutoGetCollectionForRead coll(client2.second.get(), nss);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadDBLockCompatibleXCollectionExists) {
    CollectionOptions defaultCollectionOptions;
    ASSERT_OK(
        storageInterface()->createCollection(client1.second.get(), nss, defaultCollectionOptions));

    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));

    AutoGetCollectionForRead coll(client2.second.get(), nss);
    ASSERT(coll.getCollection());
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadDBLockCompatibleXCollectionExistsReadSource) {
    CollectionOptions defaultCollectionOptions;
    ASSERT_OK(
        storageInterface()->createCollection(client1.second.get(), nss, defaultCollectionOptions));

    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));
    auto opCtx = client2.second.get();
    opCtx->recoveryUnit()->setTimestampReadSource(RecoveryUnit::ReadSource::kProvided,
                                                  Timestamp(1, 2));

    // We can instantiate AutoGetCollectionForRead but not find a collection at the provided
    // timestamp
    AutoGetCollectionForRead coll(opCtx, nss);
    ASSERT(!coll.getCollection());
}


TEST_F(DBRAIITestFixture,
       AutoGetCollectionForReadDBLockCompatibleXCollectionExistsSecondaryNoLastApplied) {
    CollectionOptions defaultCollectionOptions;
    ASSERT_OK(
        storageInterface()->createCollection(client1.second.get(), nss, defaultCollectionOptions));
    ASSERT_OK(repl::ReplicationCoordinator::get(client1.second.get())
                  ->setFollowerMode(repl::MemberState::RS_SECONDARY));
    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));

    // Simulate using a DBDirectClient to test this behavior for user reads.
    client2.first->setInDirectClient(true);
    AutoGetCollectionForRead coll(client2.second.get(), nss);
}

TEST_F(DBRAIITestFixture,
       AutoGetCollectionForReadDBLockCompatibleXCollectionExistsSecondaryLastApplied) {
    auto replCoord = repl::ReplicationCoordinator::get(client1.second.get());
    CollectionOptions defaultCollectionOptions;
    ASSERT_OK(
        storageInterface()->createCollection(client1.second.get(), nss, defaultCollectionOptions));
    ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_SECONDARY));

    // Don't call into the ReplicationCoordinator to update lastApplied because it is only a mock
    // class and does not update the correct state in the SnapshotManager.
    auto snapshotManager =
        client1.second.get()->getServiceContext()->getStorageEngine()->getSnapshotManager();
    snapshotManager->setLastApplied(replCoord->getMyLastAppliedOpTime().getTimestamp());
    Lock::DBLock dbLock1(client1.second.get(), nss.dbName(), MODE_IX);
    ASSERT(client1.second->lockState()->isDbLockedForMode(nss.dbName(), MODE_IX));

    // Simulate using a DBDirectClient to test this behavior for user reads.
    client2.first->setInDirectClient(true);
    AutoGetCollectionForRead coll(client2.second.get(), nss);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadLastAppliedConflict) {
    // This test simulates a situation where we read from the last applied timestamp but that point
    // in time is earlier than the catalog change. We expect to reconstruct the state from that
    // point in time in order to read at last applied.
    auto replCoord = repl::ReplicationCoordinator::get(client1.second.get());
    CollectionOptions defaultCollectionOptions;
    ASSERT_OK(
        storageInterface()->createCollection(client1.second.get(), nss, defaultCollectionOptions));
    ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_SECONDARY));

    // Note that when the collection was created, above, the system chooses a minimum snapshot time
    // for the collection.  If we now manually set our last applied time to something very early, we
    // will be guaranteed to hit the logic that triggers when the minimum snapshot time is greater
    // than the read-at time, since we default to reading at last-applied when in SECONDARY state.

    // Don't call into the ReplicationCoordinator to update lastApplied because it is only a mock
    // class and does not update the correct state in the SnapshotManager.
    repl::OpTime opTime(Timestamp(2, 1), 1);
    auto snapshotManager =
        client1.second.get()->getServiceContext()->getStorageEngine()->getSnapshotManager();
    snapshotManager->setLastApplied(opTime.getTimestamp());

    // Simulate using a DBDirectClient to test this behavior for user reads.
    client1.first->setInDirectClient(true);

    // We can perform the lock acquisition even though lastApplied is earlier than the minimum valid
    // time on the namespace.
    AutoGetCollectionForRead coll(client1.second.get(), nss);
    ASSERT_EQ(client1.second.get()->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kLastApplied);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadLastAppliedUnavailable) {
    // This test simulates a situation where AutoGetCollectionForRead reads without a timestamp
    // even though lastApplied is not available.
    auto replCoord = repl::ReplicationCoordinator::get(client1.second.get());
    CollectionOptions defaultCollectionOptions;
    ASSERT_OK(
        storageInterface()->createCollection(client1.second.get(), nss, defaultCollectionOptions));
    ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_SECONDARY));

    // Note that when the collection was created, above, the system chooses a minimum snapshot time
    // for the collection. Since last-applied isn't available, we default to read without a
    // timestamp.
    auto snapshotManager =
        client1.second.get()->getServiceContext()->getStorageEngine()->getSnapshotManager();
    ASSERT_FALSE(snapshotManager->getLastApplied());

    // Simulate using a DBDirectClient to test this behavior for user reads.
    client1.first->setInDirectClient(true);
    AutoGetCollectionForRead coll(client1.second.get(), nss);

    ASSERT_EQ(client1.second.get()->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kLastApplied);
    ASSERT_FALSE(
        client1.second.get()->recoveryUnit()->getPointInTimeReadTimestamp(client1.second.get()));
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadOplogOnSecondary) {
    // This test simulates a situation where AutoGetCollectionForRead reads at lastApplied on a
    // secondary.
    auto replCoord = repl::ReplicationCoordinator::get(client1.second.get());
    ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_SECONDARY));

    // Ensure the default ReadSource is used.
    ASSERT_EQ(client1.second.get()->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kNoTimestamp);

    // Don't call into the ReplicationCoordinator to update lastApplied because it is only a mock
    // class and does not update the correct state in the SnapshotManager.
    repl::OpTime opTime(Timestamp(2, 1), 1);
    auto snapshotManager =
        client1.second.get()->getServiceContext()->getStorageEngine()->getSnapshotManager();
    snapshotManager->setLastApplied(opTime.getTimestamp());

    // Simulate using a DBDirectClient to test this behavior for user reads.
    client1.first->setInDirectClient(true);
    AutoGetCollectionForRead coll(client1.second.get(), NamespaceString::kRsOplogNamespace);

    ASSERT_EQ(client1.second.get()->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kLastApplied);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadUsesLastAppliedOnSecondary) {
    auto opCtx = client1.second.get();

    // Use a tailable query on a capped collection because we can anticipate it automatically
    // yielding locks when it reaches the end of a capped collection.
    CollectionOptions options;
    options.capped = true;
    ASSERT_OK(storageInterface()->createCollection(opCtx, nss, options));

    // Simulate using a DBDirectClient to test this behavior for user reads.
    opCtx->getClient()->setInDirectClient(true);
    AutoGetCollectionForRead autoColl(opCtx, nss);
    auto exec = makeTailableQueryPlan(opCtx, autoColl.getCollection());

    // The collection scan should use the default ReadSource on a primary.
    ASSERT_EQ(RecoveryUnit::ReadSource::kNoTimestamp,
              opCtx->recoveryUnit()->getTimestampReadSource());

    // When the tailable query recovers from its yield, it should discover that the node is
    // secondary and change its read source.
    ASSERT_OK(
        repl::ReplicationCoordinator::get(opCtx)->setFollowerMode(repl::MemberState::RS_SECONDARY));
    BSONObj unused;
    auto state = exec->getNext(&unused, nullptr);
    ASSERT_EQ(state, PlanExecutor::ExecState::IS_EOF);

    // After restoring, the collection scan should now be reading with kLastApplied, the default on
    // secondaries.
    ASSERT_EQ(RecoveryUnit::ReadSource::kLastApplied,
              opCtx->recoveryUnit()->getTimestampReadSource());
    ASSERT_EQUALS(PlanExecutor::IS_EOF, exec->getNext(&unused, nullptr));
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadChangedReadSourceAfterStepUp) {
    auto opCtx = client1.second.get();

    // Use a tailable query on a capped collection because we can anticipate it automatically
    // yielding locks when it reaches the end of a capped collection.
    CollectionOptions options;
    options.capped = true;
    ASSERT_OK(storageInterface()->createCollection(opCtx, nss, options));
    ASSERT_OK(
        repl::ReplicationCoordinator::get(opCtx)->setFollowerMode(repl::MemberState::RS_SECONDARY));

    // Simulate using a DBDirectClient to test this behavior for user reads.
    opCtx->getClient()->setInDirectClient(true);
    AutoGetCollectionForRead autoColl(opCtx, nss);
    auto exec = makeTailableQueryPlan(opCtx, autoColl.getCollection());

    // The collection scan should use the default ReadSource on a secondary.
    ASSERT_EQ(RecoveryUnit::ReadSource::kLastApplied,
              opCtx->recoveryUnit()->getTimestampReadSource());

    // When the tailable query recovers from its yield, it should discover that the node is primary
    // and change its ReadSource.
    ASSERT_OK(
        repl::ReplicationCoordinator::get(opCtx)->setFollowerMode(repl::MemberState::RS_PRIMARY));
    BSONObj unused;
    auto state = exec->getNext(&unused, nullptr);
    ASSERT_EQ(state, PlanExecutor::ExecState::IS_EOF);

    // After restoring, the collection scan should now be reading with kUnset, the default on
    // primaries.
    ASSERT_EQ(RecoveryUnit::ReadSource::kNoTimestamp,
              opCtx->recoveryUnit()->getTimestampReadSource());
    ASSERT_EQUALS(PlanExecutor::IS_EOF, exec->getNext(&unused, nullptr));
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadSecondaryReadSource) {
    auto opCtx = client1.second.get();

    ASSERT_OK(storageInterface()->createCollection(opCtx, nss, {}));
    ASSERT_OK(
        repl::ReplicationCoordinator::get(opCtx)->setFollowerMode(repl::MemberState::RS_SECONDARY));
    ASSERT_EQ(opCtx->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kNoTimestamp);

    AutoGetCollectionForRead autoColl(opCtx, nss);

    // The AutoGetCollectionForRead changes the read source to be last applied.
    ASSERT_EQ(opCtx->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kLastApplied);
}

TEST_F(DBRAIITestFixture, AutoGetCollectionForReadSecondaryReadSourceNotEnforcingConstraints) {
    auto opCtx = client1.second.get();

    ASSERT_OK(storageInterface()->createCollection(opCtx, nss, {}));
    ASSERT_OK(
        repl::ReplicationCoordinator::get(opCtx)->setFollowerMode(repl::MemberState::RS_SECONDARY));
    ASSERT_EQ(opCtx->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kNoTimestamp);

    opCtx->setEnforceConstraints(false);
    AutoGetCollectionForRead autoColl(opCtx, nss);

    // The AutoGetCollectionForRead does not change the read source since we are not enforcing
    // constraints.
    ASSERT_EQ(opCtx->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kNoTimestamp);
}

}  // namespace
}  // namespace mongo
