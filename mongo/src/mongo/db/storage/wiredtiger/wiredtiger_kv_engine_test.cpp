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

#include <boost/filesystem/fstream.hpp>
#include <boost/filesystem/operations.hpp>
#include <boost/filesystem/path.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <ostream>
#include <utility>
// IWYU pragma: no_include "boost/system/detail/error_code.hpp"

#include <fmt/format.h>

#include "mongo/base/error_codes.h"
#include "mongo/base/init.h"  // IWYU pragma: keep
#include "mongo/base/initializer.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/global_settings.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/repl_settings.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/server_options.h"
#include "mongo/db/service_context.h"
#include "mongo/db/service_context_test_fixture.h"
#include "mongo/db/storage/checkpointer.h"
#include "mongo/db/storage/kv/kv_engine_test_harness.h"
#include "mongo/db/storage/record_data.h"
#include "mongo/db/storage/storage_engine_impl.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_kv_engine.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_record_store.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_recovery_unit.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/log_severity.h"
#include "mongo/platform/atomic_proxy.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/unittest/log_test.h"
#include "mongo/unittest/temp_dir.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/clock_source_mock.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/time_support.h"
#include "mongo/util/version/releases.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTest

namespace mongo {
namespace {

class WiredTigerKVHarnessHelper : public KVHarnessHelper {
public:
    WiredTigerKVHarnessHelper(ServiceContext* svcCtx, bool forRepair = false)
        : _svcCtx(svcCtx), _dbpath("wt-kv-harness"), _forRepair(forRepair) {
        // Faitfhully simulate being in replica set mode for timestamping tests which requires
        // parity for journaling settings.
        repl::ReplSettings replSettings;
        replSettings.setReplSetString("i am a replica set");
        setGlobalReplSettings(replSettings);
        repl::ReplicationCoordinator::set(
            svcCtx, std::make_unique<repl::ReplicationCoordinatorMock>(svcCtx, replSettings));
        _svcCtx->setStorageEngine(makeEngine());
        getWiredTigerKVEngine()->notifyStartupComplete();
    }

    ~WiredTigerKVHarnessHelper() {
        getWiredTigerKVEngine()->cleanShutdown();
    }

    virtual KVEngine* restartEngine() override {
        getEngine()->cleanShutdown();
        _svcCtx->clearStorageEngine();
        _svcCtx->setStorageEngine(makeEngine());
        getEngine()->notifyStartupComplete();
        return getEngine();
    }

    virtual KVEngine* getEngine() override {
        return _svcCtx->getStorageEngine()->getEngine();
    }

    virtual WiredTigerKVEngine* getWiredTigerKVEngine() {
        return static_cast<WiredTigerKVEngine*>(_svcCtx->getStorageEngine()->getEngine());
    }

private:
    std::unique_ptr<StorageEngine> makeEngine() {
        // Use a small journal for testing to account for the unlikely event that the underlying
        // filesystem does not support fast allocation of a file of zeros.
        std::string extraStrings = "log=(file_max=1m,prealloc=false)";
        auto client = _svcCtx->getService()->makeClient("opCtx");
        auto opCtx = client->makeOperationContext();
        auto kv = std::make_unique<WiredTigerKVEngine>(opCtx.get(),
                                                       kWiredTigerEngineName,
                                                       _dbpath.path(),
                                                       _cs.get(),
                                                       extraStrings,
                                                       1,
                                                       0,
                                                       false,
                                                       _forRepair);
        StorageEngineOptions options;
        return std::make_unique<StorageEngineImpl>(opCtx.get(), std::move(kv), options);
    }

    ServiceContext* _svcCtx;
    const std::unique_ptr<ClockSource> _cs = std::make_unique<ClockSourceMock>();
    unittest::TempDir _dbpath;
    bool _forRepair;
};

class WiredTigerKVEngineTest : public ServiceContextTest {
public:
    WiredTigerKVEngineTest(bool repair = false) : _helper(getServiceContext(), repair) {}

protected:
    ServiceContext::UniqueOperationContext _makeOperationContext() {
        auto opCtx = makeOperationContext();
        opCtx->setRecoveryUnit(
            std::unique_ptr<RecoveryUnit>(_helper.getEngine()->newRecoveryUnit()),
            WriteUnitOfWork::RecoveryUnitState::kNotInUnitOfWork);
        return opCtx;
    }

    WiredTigerKVHarnessHelper _helper;
};

class WiredTigerKVEngineRepairTest : public WiredTigerKVEngineTest {
public:
    WiredTigerKVEngineRepairTest() : WiredTigerKVEngineTest(true /* repair */) {}
};

TEST_F(WiredTigerKVEngineRepairTest, OrphanedDataFilesCanBeRecovered) {
    auto opCtxPtr = _makeOperationContext();

    NamespaceString nss = NamespaceString::createNamespaceString_forTest("a.b");
    std::string ident = "collection-1234";
    std::string record = "abcd";
    CollectionOptions defaultCollectionOptions;

    std::unique_ptr<RecordStore> rs;
    ASSERT_OK(_helper.getWiredTigerKVEngine()->createRecordStore(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions));
    rs = _helper.getWiredTigerKVEngine()->getRecordStore(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions);
    ASSERT(rs);

    RecordId loc;
    {
        WriteUnitOfWork uow(opCtxPtr.get());
        StatusWith<RecordId> res =
            rs->insertRecord(opCtxPtr.get(), record.c_str(), record.length() + 1, Timestamp());
        ASSERT_OK(res.getStatus());
        loc = res.getValue();
        uow.commit();
    }

    const boost::optional<boost::filesystem::path> dataFilePath =
        _helper.getWiredTigerKVEngine()->getDataFilePathForIdent(ident);
    ASSERT(dataFilePath);

    ASSERT(boost::filesystem::exists(*dataFilePath));

    const boost::filesystem::path tmpFile{dataFilePath->string() + ".tmp"};
    ASSERT(!boost::filesystem::exists(tmpFile));

#ifdef _WIN32
    auto status = _helper.getWiredTigerKVEngine()->recoverOrphanedIdent(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions);
    ASSERT_EQ(ErrorCodes::CommandNotSupported, status.code());
#else

    // Dropping a collection might fail if we haven't checkpointed the data.
    _helper.getWiredTigerKVEngine()->checkpoint(opCtxPtr.get());

    // Move the data file out of the way so the ident can be dropped. This not permitted on Windows
    // because the file cannot be moved while it is open. The implementation for orphan recovery is
    // also not implemented on Windows for this reason.
    boost::system::error_code err;
    boost::filesystem::rename(*dataFilePath, tmpFile, err);
    ASSERT(!err) << err.message();

    ASSERT_OK(_helper.getWiredTigerKVEngine()->dropIdent(opCtxPtr.get()->recoveryUnit(), ident));

    // The data file is moved back in place so that it becomes an "orphan" of the storage
    // engine and the restoration process can be tested.
    boost::filesystem::rename(tmpFile, *dataFilePath, err);
    ASSERT(!err) << err.message();

    auto status = _helper.getWiredTigerKVEngine()->recoverOrphanedIdent(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions);
    ASSERT_EQ(ErrorCodes::DataModifiedByRepair, status.code());
#endif
}

TEST_F(WiredTigerKVEngineRepairTest, UnrecoverableOrphanedDataFilesAreRebuilt) {
    auto opCtxPtr = _makeOperationContext();
    Lock::GlobalLock globalLk(opCtxPtr.get(), MODE_X);

    NamespaceString nss = NamespaceString::createNamespaceString_forTest("a.b");
    std::string ident = "collection-1234";
    std::string record = "abcd";
    CollectionOptions defaultCollectionOptions;

    std::unique_ptr<RecordStore> rs;
    ASSERT_OK(_helper.getWiredTigerKVEngine()->createRecordStore(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions));
    rs = _helper.getWiredTigerKVEngine()->getRecordStore(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions);
    ASSERT(rs);

    RecordId loc;
    {
        WriteUnitOfWork uow(opCtxPtr.get());
        StatusWith<RecordId> res =
            rs->insertRecord(opCtxPtr.get(), record.c_str(), record.length() + 1, Timestamp());
        ASSERT_OK(res.getStatus());
        loc = res.getValue();
        uow.commit();
    }

    const boost::optional<boost::filesystem::path> dataFilePath =
        _helper.getWiredTigerKVEngine()->getDataFilePathForIdent(ident);
    ASSERT(dataFilePath);

    ASSERT(boost::filesystem::exists(*dataFilePath));

    // Dropping a collection might fail if we haven't checkpointed the data
    _helper.getWiredTigerKVEngine()->checkpoint(opCtxPtr.get());

    ASSERT_OK(_helper.getWiredTigerKVEngine()->dropIdent(opCtxPtr.get()->recoveryUnit(), ident));

#ifdef _WIN32
    auto status = _helper.getWiredTigerKVEngine()->recoverOrphanedIdent(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions);
    ASSERT_EQ(ErrorCodes::CommandNotSupported, status.code());
#else
    // The ident may not get immediately dropped, so ensure it is completely gone.
    boost::system::error_code err;
    boost::filesystem::remove(*dataFilePath, err);
    ASSERT(!err) << err.message();

    // Create an empty data file. The subsequent call to recreate the collection will fail because
    // it is unsalvageable.
    boost::filesystem::ofstream fileStream(*dataFilePath);
    fileStream << "";
    fileStream.close();

    ASSERT(boost::filesystem::exists(*dataFilePath));

    // This should recreate an empty data file successfully and move the old one to a name that ends
    // in ".corrupt".
    auto status = _helper.getWiredTigerKVEngine()->recoverOrphanedIdent(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions);
    ASSERT_EQ(ErrorCodes::DataModifiedByRepair, status.code()) << status.reason();

    boost::filesystem::path corruptFile = (dataFilePath->string() + ".corrupt");
    ASSERT(boost::filesystem::exists(corruptFile));

    rs = _helper.getWiredTigerKVEngine()->getRecordStore(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions);
    RecordData data;
    ASSERT_FALSE(rs->findRecord(opCtxPtr.get(), loc, &data));
#endif
}

TEST_F(WiredTigerKVEngineTest, TestOplogTruncation) {
    // To diagnose any intermittent failures, maximize logging from WiredTigerKVEngine and friends.
    auto severityGuard = unittest::MinimumLoggedSeverityGuard{logv2::LogComponent::kStorage,
                                                              logv2::LogSeverity::Debug(3)};

    // Set syncdelay before starting the checkpoint thread, otherwise it can observe the default
    // checkpoint frequency of 60 seconds, causing the test to fail due to a 10 second timeout.
    storageGlobalParams.syncdelay.store(1);

    std::unique_ptr<Checkpointer> checkpointer = std::make_unique<Checkpointer>();
    checkpointer->go();

    // If the test fails we want to ensure the checkpoint thread shuts down to avoid accessing the
    // storage engine during shutdown.
    ON_BLOCK_EXIT([&] {
        checkpointer->shutdown({ErrorCodes::ShutdownInProgress, "Test finished"});
    });

    auto opCtxPtr = _makeOperationContext();
    // The initial data timestamp has to be set to take stable checkpoints. The first stable
    // timestamp greater than this will also trigger a checkpoint. The following loop of the
    // CheckpointThread will observe the new `syncdelay` value.
    _helper.getWiredTigerKVEngine()->setInitialDataTimestamp(Timestamp(1, 1));

    // Simulate the callback that queries config.transactions for the oldest active transaction.
    boost::optional<Timestamp> oldestActiveTxnTimestamp;
    AtomicWord<bool> callbackShouldFail{false};
    auto callback = [&](Timestamp stableTimestamp) {
        using ResultType = StorageEngine::OldestActiveTransactionTimestampResult;
        if (callbackShouldFail.load()) {
            return ResultType(ErrorCodes::ExceededTimeLimit, "timeout");
        }

        return ResultType(oldestActiveTxnTimestamp);
    };

    _helper.getWiredTigerKVEngine()->setOldestActiveTransactionTimestampCallback(callback);

    // A method that will poll the WiredTigerKVEngine until it sees the amount of oplog necessary
    // for crash recovery exceeds the input.
    auto assertPinnedMovesSoon = [this](Timestamp newPinned) {
        // If the current oplog needed for rollback does not exceed the requested pinned out, we
        // cannot expect the CheckpointThread to eventually publish a sufficient crash recovery
        // value.
        auto needed = _helper.getWiredTigerKVEngine()->getOplogNeededForRollback();
        if (needed.isOK()) {
            ASSERT_TRUE(needed.getValue() >= newPinned);
        }

        // Do 100 iterations that sleep for 100 milliseconds between polls. This will wait for up
        // to 10 seconds to observe an asynchronous update that iterates once per second.
        for (auto iterations = 0; iterations < 100; ++iterations) {
            if (_helper.getWiredTigerKVEngine()->getPinnedOplog() >= newPinned) {
                ASSERT_TRUE(
                    _helper.getWiredTigerKVEngine()->getOplogNeededForCrashRecovery().value() >=
                    newPinned);
                return;
            }

            sleepmillis(100);
        }

        LOGV2(22367,
              "Expected the pinned oplog to advance.",
              "expectedValue"_attr = newPinned,
              "publishedValue"_attr =
                  _helper.getWiredTigerKVEngine()->getOplogNeededForCrashRecovery());
        FAIL("");
    };

    oldestActiveTxnTimestamp = boost::none;
    _helper.getWiredTigerKVEngine()->setStableTimestamp(Timestamp(10, 1), false);
    assertPinnedMovesSoon(Timestamp(10, 1));

    oldestActiveTxnTimestamp = Timestamp(15, 1);
    _helper.getWiredTigerKVEngine()->setStableTimestamp(Timestamp(20, 1), false);
    assertPinnedMovesSoon(Timestamp(15, 1));

    oldestActiveTxnTimestamp = Timestamp(19, 1);
    _helper.getWiredTigerKVEngine()->setStableTimestamp(Timestamp(30, 1), false);
    assertPinnedMovesSoon(Timestamp(19, 1));

    oldestActiveTxnTimestamp = boost::none;
    _helper.getWiredTigerKVEngine()->setStableTimestamp(Timestamp(30, 1), false);
    assertPinnedMovesSoon(Timestamp(30, 1));

    callbackShouldFail.store(true);
    ASSERT_NOT_OK(_helper.getWiredTigerKVEngine()->getOplogNeededForRollback());
    _helper.getWiredTigerKVEngine()->setStableTimestamp(Timestamp(40, 1), false);
    // Await a new checkpoint. Oplog needed for rollback does not advance.
    sleepmillis(1100);
    ASSERT_EQ(_helper.getWiredTigerKVEngine()->getOplogNeededForCrashRecovery().value(),
              Timestamp(30, 1));
    _helper.getWiredTigerKVEngine()->setStableTimestamp(Timestamp(30, 1), false);
    callbackShouldFail.store(false);
    assertPinnedMovesSoon(Timestamp(40, 1));
}

TEST_F(WiredTigerKVEngineTest, IdentDrop) {
#ifdef _WIN32
    // TODO SERVER-51595: to re-enable this test on Windows.
    return;
#endif

    auto opCtxPtr = _makeOperationContext();

    NamespaceString nss = NamespaceString::createNamespaceString_forTest("a.b");
    std::string ident = "collection-1234";
    CollectionOptions defaultCollectionOptions;

    std::unique_ptr<RecordStore> rs;
    ASSERT_OK(_helper.getWiredTigerKVEngine()->createRecordStore(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions));

    const boost::optional<boost::filesystem::path> dataFilePath =
        _helper.getWiredTigerKVEngine()->getDataFilePathForIdent(ident);
    ASSERT(dataFilePath);
    ASSERT(boost::filesystem::exists(*dataFilePath));

    _helper.getWiredTigerKVEngine()->dropIdentForImport(opCtxPtr.get(), ident);
    ASSERT(boost::filesystem::exists(*dataFilePath));

    // Because the underlying file was not removed, it will be renamed out of the way by WiredTiger
    // when creating a new table with the same ident.
    ASSERT_OK(_helper.getWiredTigerKVEngine()->createRecordStore(
        opCtxPtr.get(), nss, ident, defaultCollectionOptions));

    const boost::filesystem::path renamedFilePath = dataFilePath->generic_string() + ".1";
    ASSERT(boost::filesystem::exists(*dataFilePath));
    ASSERT(boost::filesystem::exists(renamedFilePath));

    ASSERT_OK(_helper.getWiredTigerKVEngine()->dropIdent(opCtxPtr.get()->recoveryUnit(), ident));

    // WiredTiger drops files asynchronously.
    for (size_t check = 0; check < 30; check++) {
        if (!boost::filesystem::exists(*dataFilePath))
            break;
        sleepsecs(1);
    }

    ASSERT(!boost::filesystem::exists(*dataFilePath));
    ASSERT(boost::filesystem::exists(renamedFilePath));
}

TEST_F(WiredTigerKVEngineTest, TestBasicPinOldestTimestamp) {
    auto opCtxRaii = _makeOperationContext();
    const Timestamp initTs = Timestamp(1, 0);

    // Initialize the oldest timestamp.
    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs, false);
    ASSERT_EQ(initTs, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Assert that advancing the oldest timestamp still succeeds.
    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs + 1, false);
    ASSERT_EQ(initTs + 1, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Error if there's a request to pin the oldest timestamp earlier than what it is already set
    // as. This error case is not exercised in this test.
    const bool roundUpIfTooOld = false;
    // Pin the oldest timestamp to "3".
    auto pinnedTs = unittest::assertGet(_helper.getWiredTigerKVEngine()->pinOldestTimestamp(
        opCtxRaii.get(), "A", initTs + 3, roundUpIfTooOld));
    // Assert that the pinning method returns the same timestamp as was requested.
    ASSERT_EQ(initTs + 3, pinnedTs);
    // Assert that pinning the oldest timestamp does not advance it.
    ASSERT_EQ(initTs + 1, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Attempt to advance the oldest timestamp to "5".
    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs + 5, false);
    // Observe the oldest timestamp was pinned at the requested "3".
    ASSERT_EQ(initTs + 3, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Unpin the oldest timestamp. Assert that unpinning does not advance the oldest timestamp.
    _helper.getWiredTigerKVEngine()->unpinOldestTimestamp("A");
    ASSERT_EQ(initTs + 3, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Now advancing the oldest timestamp to "5" succeeds.
    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs + 5, false);
    ASSERT_EQ(initTs + 5, _helper.getWiredTigerKVEngine()->getOldestTimestamp());
}

/**
 * Demonstrate that multiple actors can request different pins of the oldest timestamp. The minimum
 * of all active requests will be obeyed.
 */
TEST_F(WiredTigerKVEngineTest, TestMultiPinOldestTimestamp) {
    auto opCtxRaii = _makeOperationContext();
    const Timestamp initTs = Timestamp(1, 0);

    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs, false);
    ASSERT_EQ(initTs, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Error if there's a request to pin the oldest timestamp earlier than what it is already set
    // as. This error case is not exercised in this test.
    const bool roundUpIfTooOld = false;
    // Have "A" pin the timestamp to "1".
    auto pinnedTs = unittest::assertGet(_helper.getWiredTigerKVEngine()->pinOldestTimestamp(
        opCtxRaii.get(), "A", initTs + 1, roundUpIfTooOld));
    ASSERT_EQ(initTs + 1, pinnedTs);
    ASSERT_EQ(initTs, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Have "B" pin the timestamp to "2".
    pinnedTs = unittest::assertGet(_helper.getWiredTigerKVEngine()->pinOldestTimestamp(
        opCtxRaii.get(), "B", initTs + 2, roundUpIfTooOld));
    ASSERT_EQ(initTs + 2, pinnedTs);
    ASSERT_EQ(initTs, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Advancing the oldest timestamp to "5" will only succeed in advancing it to "1".
    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs + 5, false);
    ASSERT_EQ(initTs + 1, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // After unpinning "A" at "1", advancing the oldest timestamp will be pinned to "2".
    _helper.getWiredTigerKVEngine()->unpinOldestTimestamp("A");
    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs + 5, false);
    ASSERT_EQ(initTs + 2, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Unpinning "B" at "2" allows the oldest timestamp to advance freely.
    _helper.getWiredTigerKVEngine()->unpinOldestTimestamp("B");
    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs + 5, false);
    ASSERT_EQ(initTs + 5, _helper.getWiredTigerKVEngine()->getOldestTimestamp());
}

/**
 * Test error cases where a request to pin the oldest timestamp uses a value that's too early
 * relative to the current oldest timestamp.
 */
TEST_F(WiredTigerKVEngineTest, TestPinOldestTimestampErrors) {
    auto opCtxRaii = _makeOperationContext();
    const Timestamp initTs = Timestamp(10, 0);

    _helper.getWiredTigerKVEngine()->setOldestTimestamp(initTs, false);
    ASSERT_EQ(initTs, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    const bool roundUpIfTooOld = true;
    // The false value means using this variable will cause the method to fail on error.
    const bool failOnError = false;

    // When rounding on error, the pin will succeed, but the return value will be the current oldest
    // timestamp instead of the requested value.
    auto pinnedTs = unittest::assertGet(_helper.getWiredTigerKVEngine()->pinOldestTimestamp(
        opCtxRaii.get(), "A", initTs - 1, roundUpIfTooOld));
    ASSERT_EQ(initTs, pinnedTs);
    ASSERT_EQ(initTs, _helper.getWiredTigerKVEngine()->getOldestTimestamp());

    // Using "fail on error" will result in a not-OK return value.
    ASSERT_NOT_OK(_helper.getWiredTigerKVEngine()->pinOldestTimestamp(
        opCtxRaii.get(), "B", initTs - 1, failOnError));
    ASSERT_EQ(initTs, _helper.getWiredTigerKVEngine()->getOldestTimestamp());
}

TEST_F(WiredTigerKVEngineTest, WiredTigerDowngrade) {
    // Initializing this value to silence Coverity warning. Doesn't matter what value
    // _startupVersion is set to since shouldDowngrade() & getDowngradeString() only look at
    // _startupVersion when FCV is uninitialized. This test initializes FCV via setVersion().
    WiredTigerFileVersion version = {WiredTigerFileVersion::StartupVersion::IS_42};

    // (Generic FCV reference): When FCV is kLatest, no downgrade is necessary.
    serverGlobalParams.mutableFCV.setVersion(multiversion::GenericFCV::kLatest);
    ASSERT_FALSE(version.shouldDowngrade(/*hasRecoveryTimestamp=*/false));
    ASSERT_EQ(WiredTigerFileVersion::kLatestWTRelease, version.getDowngradeString());

    // (Generic FCV reference): When FCV is kLastContinuous or kLastLTS, a downgrade may be needed.
    serverGlobalParams.mutableFCV.setVersion(multiversion::GenericFCV::kLastContinuous);
    ASSERT_TRUE(version.shouldDowngrade(/*hasRecoveryTimestamp=*/false));
    ASSERT_EQ(WiredTigerFileVersion::kLastContinuousWTRelease, version.getDowngradeString());

    serverGlobalParams.mutableFCV.setVersion(multiversion::GenericFCV::kLastLTS);
    ASSERT_TRUE(version.shouldDowngrade(/*hasRecoveryTimestamp=*/false));
    ASSERT_EQ(WiredTigerFileVersion::kLastLTSWTRelease, version.getDowngradeString());

    // (Generic FCV reference): While we're in a semi-downgraded state, we shouldn't try downgrading
    // the WiredTiger compatibility version.
    serverGlobalParams.mutableFCV.setVersion(
        multiversion::GenericFCV::kDowngradingFromLatestToLastContinuous);
    ASSERT_FALSE(version.shouldDowngrade(/*hasRecoveryTimestamp=*/false));
    ASSERT_EQ(WiredTigerFileVersion::kLatestWTRelease, version.getDowngradeString());

    serverGlobalParams.mutableFCV.setVersion(
        multiversion::GenericFCV::kDowngradingFromLatestToLastLTS);
    ASSERT_FALSE(version.shouldDowngrade(/*hasRecoveryTimestamp=*/false));
    ASSERT_EQ(WiredTigerFileVersion::kLatestWTRelease, version.getDowngradeString());
}

TEST_F(WiredTigerKVEngineTest, TestReconfigureLog) {
    // Perform each test in their own limited scope in order to establish different
    // severity levels.

    {
        auto opCtxRaii = _makeOperationContext();
        // Set the WiredTiger Checkpoint LOGV2 component severity to the Log level.
        auto severityGuard = unittest::MinimumLoggedSeverityGuard{
            logv2::LogComponent::kWiredTigerCheckpoint, logv2::LogSeverity::Log()};
        ASSERT_EQ(logv2::LogSeverity::Log(),
                  unittest::getMinimumLogSeverity(logv2::LogComponent::kWiredTigerCheckpoint));
        ASSERT_OK(_helper.getWiredTigerKVEngine()->reconfigureLogging());
        // Perform a checkpoint. The goal here is create some activity in WiredTiger in order
        // to generate verbose messages (we don't really care about the checkpoint itself).
        startCapturingLogMessages();
        _helper.getWiredTigerKVEngine()->checkpoint(opCtxRaii.get());
        stopCapturingLogMessages();
        // In this initial case, we don't expect to capture any debug checkpoint messages. The
        // base severity for the checkpoint component should be at Log().
        bool foundWTCheckpointMessage = false;
        for (auto&& bson : getCapturedBSONFormatLogMessages()) {
            if (bson["c"].String() == "WTCHKPT" &&
                bson["attr"]["message"]["verbose_level"].String() == "DEBUG_1" &&
                bson["attr"]["message"]["category"].String() == "WT_VERB_CHECKPOINT") {
                foundWTCheckpointMessage = true;
            }
        }
        ASSERT_FALSE(foundWTCheckpointMessage);
    }
    {
        auto opCtxRaii = _makeOperationContext();
        // Set the WiredTiger Checkpoint LOGV2 component severity to the Debug(2) level.
        auto severityGuard = unittest::MinimumLoggedSeverityGuard{
            logv2::LogComponent::kWiredTigerCheckpoint, logv2::LogSeverity::Debug(2)};
        ASSERT_OK(_helper.getWiredTigerKVEngine()->reconfigureLogging());
        ASSERT_EQ(logv2::LogSeverity::Debug(2),
                  unittest::getMinimumLogSeverity(logv2::LogComponent::kWiredTigerCheckpoint));

        // Perform another checkpoint.
        startCapturingLogMessages();
        _helper.getWiredTigerKVEngine()->checkpoint(opCtxRaii.get());
        stopCapturingLogMessages();

        // This time we expect to detect WiredTiger checkpoint Debug() messages.
        bool foundWTCheckpointMessage = false;
        for (auto&& bson : getCapturedBSONFormatLogMessages()) {
            if (bson["c"].String() == "WTCHKPT" &&
                bson["attr"]["message"]["verbose_level"].String() == "DEBUG_1" &&
                bson["attr"]["message"]["category"].String() == "WT_VERB_CHECKPOINT") {
                foundWTCheckpointMessage = true;
            }
        }
        ASSERT_TRUE(foundWTCheckpointMessage);
    }
}

TEST_F(WiredTigerKVEngineTest, RollbackToStableEBUSY) {
    auto opCtxPtr = _makeOperationContext();
    _helper.getWiredTigerKVEngine()->setInitialDataTimestamp(Timestamp(1, 1));
    _helper.getWiredTigerKVEngine()->setStableTimestamp(Timestamp(1, 1), false);

    // Get a session. This will open a transaction.
    WiredTigerSession* session = WiredTigerRecoveryUnit::get(opCtxPtr.get())->getSession();
    invariant(session);

    // WT will return EBUSY due to the open transaction.
    FailPointEnableBlock failPoint("WTRollbackToStableReturnOnEBUSY");
    ASSERT_EQ(ErrorCodes::ObjectIsBusy,
              _helper.getWiredTigerKVEngine()
                  ->recoverToStableTimestamp(opCtxPtr.get())
                  .getStatus()
                  .code());

    // Close the open transaction.
    WiredTigerRecoveryUnit::get(opCtxPtr.get())->abandonSnapshot();

    // WT will no longer return EBUSY.
    ASSERT_OK(_helper.getWiredTigerKVEngine()->recoverToStableTimestamp(opCtxPtr.get()));
}

std::unique_ptr<KVHarnessHelper> makeHelper(ServiceContext* svcCtx) {
    return std::make_unique<WiredTigerKVHarnessHelper>(svcCtx);
}

MONGO_INITIALIZER(RegisterKVHarnessFactory)(InitializerContext*) {
    KVHarnessHelper::registerFactory(makeHelper);
}

}  // namespace
}  // namespace mongo
