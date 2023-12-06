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

#include "mongo/db/storage/wiredtiger/wiredtiger_recovery_unit.h"

#include <cstring>
#include <string>
#include <utility>
#include <wiredtiger.h>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/checked_cast.h"
#include "mongo/base/init.h"  // IWYU pragma: keep
#include "mongo/base/initializer.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/global_settings.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/repl_settings.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/key_format.h"
#include "mongo/db/storage/record_data.h"
#include "mongo/db/storage/record_store.h"
#include "mongo/db/storage/recovery_unit_test_harness.h"
#include "mongo/db/storage/snapshot_manager.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_cursor_helpers.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_kv_engine.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_record_store.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_session_cache.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_util.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/death_test.h"
#include "mongo/unittest/framework.h"
#include "mongo/unittest/temp_dir.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/clock_source_mock.h"
#include "mongo/util/decorable.h"
#include "mongo/util/str.h"

namespace mongo {
namespace {

class WiredTigerRecoveryUnitHarnessHelper final : public RecoveryUnitHarnessHelper {
public:
    WiredTigerRecoveryUnitHarnessHelper()
        : _dbpath("wt_test"),
          _engine(Client::getCurrent()->makeOperationContext().get(),
                  kWiredTigerEngineName,  // .canonicalName
                  _dbpath.path(),         // .path
                  &_cs,                   // .cs
                  "",                     // .extraOpenOptions
                  1,                      // .cacheSizeMB
                  0,                      // .maxCacheOverflowFileSizeMB
                  false,                  // .ephemeral
                  false                   // .repair
          ) {
        // Use a replica set so that writes to replicated collections are not journaled and thus
        // retain their timestamps.
        repl::ReplSettings replSettings;
        replSettings.setReplSetString("rs");
        setGlobalReplSettings(replSettings);
        repl::ReplicationCoordinator::set(getGlobalServiceContext(),
                                          std::make_unique<repl::ReplicationCoordinatorMock>(
                                              getGlobalServiceContext(), replSettings));
        _engine.notifyStartupComplete(Client::getCurrent()->makeOperationContext().get());
    }

    ~WiredTigerRecoveryUnitHarnessHelper() {}

    virtual std::unique_ptr<RecoveryUnit> newRecoveryUnit() final {
        return std::unique_ptr<RecoveryUnit>(_engine.newRecoveryUnit());
    }

    virtual std::unique_ptr<RecordStore> createRecordStore(OperationContext* opCtx,
                                                           const std::string& ns) final {
        std::string ident = ns;
        NamespaceString nss = NamespaceString::createNamespaceString_forTest(ns);
        std::string uri = WiredTigerKVEngine::kTableUriPrefix + ns;
        StatusWith<std::string> result =
            WiredTigerRecordStore::generateCreateString(kWiredTigerEngineName,
                                                        nss,
                                                        ident,
                                                        CollectionOptions(),
                                                        "",
                                                        KeyFormat::Long,
                                                        WiredTigerUtil::useTableLogging(nss));
        ASSERT_TRUE(result.isOK());
        std::string config = result.getValue();

        {
            WriteUnitOfWork uow(opCtx);
            WiredTigerRecoveryUnit* ru =
                checked_cast<WiredTigerRecoveryUnit*>(opCtx->recoveryUnit());
            WT_SESSION* s = ru->getSession()->getSession();
            invariantWTOK(s->create(s, uri.c_str(), config.c_str()), s);
            uow.commit();
        }

        WiredTigerRecordStore::Params params;
        params.nss = nss;
        params.ident = ident;
        params.engineName = kWiredTigerEngineName;
        params.isCapped = false;
        params.keyFormat = KeyFormat::Long;
        params.overwrite = true;
        params.isEphemeral = false;
        params.isLogged = WiredTigerUtil::useTableLogging(nss);
        params.sizeStorer = nullptr;
        params.tracksSizeAdjustments = true;
        params.forceUpdateWithFullDocument = false;

        auto ret = std::make_unique<StandardWiredTigerRecordStore>(&_engine, opCtx, params);
        ret->postConstructorInit(opCtx, nss);
        return std::move(ret);
    }

    WiredTigerKVEngine* getEngine() {
        return &_engine;
    }

private:
    unittest::TempDir _dbpath;
    ClockSourceMock _cs;
    WiredTigerKVEngine _engine;
};

std::unique_ptr<RecoveryUnitHarnessHelper> makeWTRUHarnessHelper() {
    return std::make_unique<WiredTigerRecoveryUnitHarnessHelper>();
}

MONGO_INITIALIZER(RegisterHarnessFactory)(InitializerContext* const) {
    mongo::registerRecoveryUnitHarnessHelperFactory(makeWTRUHarnessHelper);
}

class WiredTigerRecoveryUnitTestFixture : public unittest::Test {
public:
    typedef std::pair<ServiceContext::UniqueClient, ServiceContext::UniqueOperationContext>
        ClientAndCtx;

    ClientAndCtx makeClientAndOpCtx(RecoveryUnitHarnessHelper* harnessHelper,
                                    const std::string& clientName) {
        auto sc = harnessHelper->serviceContext();
        auto client = sc->getService()->makeClient(clientName);
        auto opCtx = client->makeOperationContext();
        opCtx->setRecoveryUnit(harnessHelper->newRecoveryUnit(),
                               WriteUnitOfWork::RecoveryUnitState::kNotInUnitOfWork);
        return std::make_pair(std::move(client), std::move(opCtx));
    }

    void getCursor(WiredTigerRecoveryUnit* ru, WT_CURSOR** cursor) {
        WT_SESSION* wt_session = ru->getSession()->getSession();
        invariantWTOK(wt_session->create(wt_session, wt_uri, wt_config), wt_session);
        invariantWTOK(wt_session->open_cursor(wt_session, wt_uri, nullptr, nullptr, cursor),
                      wt_session);
    }

    void setUp() override {
        harnessHelper = std::make_unique<WiredTigerRecoveryUnitHarnessHelper>();
        clientAndCtx1 = makeClientAndOpCtx(harnessHelper.get(), "writer");
        clientAndCtx2 = makeClientAndOpCtx(harnessHelper.get(), "reader");
        ru1 = checked_cast<WiredTigerRecoveryUnit*>(clientAndCtx1.second->recoveryUnit());
        ru1->setOperationContext(clientAndCtx1.second.get());
        ru2 = checked_cast<WiredTigerRecoveryUnit*>(clientAndCtx2.second->recoveryUnit());
        ru2->setOperationContext(clientAndCtx2.second.get());
        snapshotManager = dynamic_cast<WiredTigerSnapshotManager*>(
            harnessHelper->getEngine()->getSnapshotManager());
    }

    std::unique_ptr<WiredTigerRecoveryUnitHarnessHelper> harnessHelper;
    ClientAndCtx clientAndCtx1, clientAndCtx2;
    WiredTigerRecoveryUnit *ru1, *ru2;
    WiredTigerSnapshotManager* snapshotManager;

private:
    const char* wt_uri = "table:prepare_transaction";
    const char* wt_config = "key_format=S,value_format=S,log=(enabled=false)";
};

TEST_F(WiredTigerRecoveryUnitTestFixture, SetReadSource) {
    // Storage engine operations require at least Global IS.
    Lock::GlobalLock lk(clientAndCtx1.second.get(), MODE_IS);
    ru1->setTimestampReadSource(RecoveryUnit::ReadSource::kProvided, Timestamp(1, 1));
    ASSERT_EQ(RecoveryUnit::ReadSource::kProvided, ru1->getTimestampReadSource());
    ASSERT_EQ(Timestamp(1, 1), ru1->getPointInTimeReadTimestamp(clientAndCtx1.second.get()));
}

TEST_F(WiredTigerRecoveryUnitTestFixture, NoOverlapReadSource) {
    OperationContext* opCtx1 = clientAndCtx1.second.get();
    OperationContext* opCtx2 = clientAndCtx2.second.get();

    // Hold the global locks throughout the test to avoid having the global lock destructor
    // prematurely abandon snapshots.
    Lock::GlobalLock globalLock1(opCtx1, MODE_IX);
    Lock::GlobalLock globalLock2(opCtx2, MODE_IX);

    std::unique_ptr<RecordStore> rs(harnessHelper->createRecordStore(opCtx1, "a.b"));

    const std::string str = str::stream() << "test";
    const Timestamp ts1{1, 1};
    const Timestamp ts2{1, 2};
    const Timestamp ts3{1, 2};

    RecordId rid1;
    {
        WriteUnitOfWork wuow(opCtx1);
        StatusWith<RecordId> res = rs->insertRecord(opCtx1, str.c_str(), str.size() + 1, ts1);
        ASSERT_OK(res);
        wuow.commit();
        rid1 = res.getValue();
        snapshotManager->setLastApplied(ts1);
    }

    // Read without a timestamp. The write should be visible.
    ASSERT_EQ(opCtx1->recoveryUnit()->getTimestampReadSource(),
              RecoveryUnit::ReadSource::kNoTimestamp);
    RecordData unused;
    ASSERT_TRUE(rs->findRecord(opCtx1, rid1, &unused));

    // Read with kNoOverlap. The write should be visible.
    opCtx1->recoveryUnit()->abandonSnapshot();
    opCtx1->recoveryUnit()->setTimestampReadSource(RecoveryUnit::ReadSource::kNoOverlap);
    ASSERT_TRUE(rs->findRecord(opCtx1, rid1, &unused));

    RecordId rid2, rid3;
    {
        // Start, but do not commit a transaction with opCtx2. This sets a timestamp at ts2, which
        // creates a hole. kNoOverlap, which is a function of all_durable, will only be able to read
        // at the time immediately before.
        WriteUnitOfWork wuow(opCtx2);
        StatusWith<RecordId> res =
            rs->insertRecord(opCtx2, str.c_str(), str.size() + 1, Timestamp());
        ASSERT_OK(opCtx2->recoveryUnit()->setTimestamp(ts2));
        ASSERT_OK(res);
        rid2 = res.getValue();

        // While holding open a transaction with opCtx2, perform an insert at ts3 with opCtx1. This
        // creates a "hole".
        {
            WriteUnitOfWork wuow(opCtx1);
            StatusWith<RecordId> res = rs->insertRecord(opCtx1, str.c_str(), str.size() + 1, ts3);
            ASSERT_OK(res);
            wuow.commit();
            rid3 = res.getValue();
            snapshotManager->setLastApplied(ts3);
        }

        // Read without a timestamp, and we should see the first and third records.
        opCtx1->recoveryUnit()->abandonSnapshot();
        opCtx1->recoveryUnit()->setTimestampReadSource(RecoveryUnit::ReadSource::kNoTimestamp);
        ASSERT_TRUE(rs->findRecord(opCtx1, rid1, &unused));
        ASSERT_FALSE(rs->findRecord(opCtx1, rid2, &unused));
        ASSERT_TRUE(rs->findRecord(opCtx1, rid3, &unused));

        // Now read at kNoOverlap. Since the transaction at ts2 has not committed, all_durable is
        // held back to ts1. LastApplied has advanced to ts3, but because kNoOverlap is the minimum,
        // we should only see one record.
        opCtx1->recoveryUnit()->abandonSnapshot();
        opCtx1->recoveryUnit()->setTimestampReadSource(RecoveryUnit::ReadSource::kNoOverlap);
        ASSERT_TRUE(rs->findRecord(opCtx1, rid1, &unused));
        ASSERT_FALSE(rs->findRecord(opCtx1, rid2, &unused));
        ASSERT_FALSE(rs->findRecord(opCtx1, rid3, &unused));

        wuow.commit();
    }

    // Now that the hole has been closed, kNoOverlap should see all 3 records.
    opCtx1->recoveryUnit()->abandonSnapshot();
    opCtx1->recoveryUnit()->setTimestampReadSource(RecoveryUnit::ReadSource::kNoOverlap);
    ASSERT_TRUE(rs->findRecord(opCtx1, rid1, &unused));
    ASSERT_TRUE(rs->findRecord(opCtx1, rid2, &unused));
    ASSERT_TRUE(rs->findRecord(opCtx1, rid3, &unused));
}

TEST_F(WiredTigerRecoveryUnitTestFixture,
       LocalReadOnADocumentBeingPreparedWithoutIgnoringPreparedTriggersPrepareConflict) {
    // Prepare but don't commit a transaction
    ru1->beginUnitOfWork(clientAndCtx1.second->readOnly());
    WT_CURSOR* cursor;
    getCursor(ru1, &cursor);
    cursor->set_key(cursor, "key");
    cursor->set_value(cursor, "value");
    invariantWTOK(
        wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(clientAndCtx1.second.get()), cursor),
        cursor->session);
    ru1->setPrepareTimestamp({1, 1});
    ru1->prepareUnitOfWork();

    // The transaction read default enforces prepare conflicts and triggers a WT_PREPARE_CONFLICT.
    ru2->beginUnitOfWork(clientAndCtx2.second->readOnly());
    getCursor(ru2, &cursor);
    cursor->set_key(cursor, "key");
    int ret = cursor->search(cursor);
    ASSERT_EQ(WT_PREPARE_CONFLICT, ret);

    ru1->abortUnitOfWork();
    ru2->abortUnitOfWork();
}

TEST_F(WiredTigerRecoveryUnitTestFixture,
       LocalReadOnADocumentBeingPreparedDoesntTriggerPrepareConflict) {
    // Prepare but don't commit a transaction
    ru1->beginUnitOfWork(clientAndCtx1.second->readOnly());
    WT_CURSOR* cursor;
    getCursor(ru1, &cursor);
    cursor->set_key(cursor, "key");
    cursor->set_value(cursor, "value");
    invariantWTOK(
        wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(clientAndCtx1.second.get()), cursor),
        cursor->session);
    ru1->setPrepareTimestamp({1, 1});
    ru1->prepareUnitOfWork();

    // A transaction that chooses to ignore prepare conflicts does not see the record instead of
    // returning a prepare conflict.
    ru2->beginUnitOfWork(clientAndCtx2.second->readOnly());
    ru2->setPrepareConflictBehavior(PrepareConflictBehavior::kIgnoreConflicts);
    getCursor(ru2, &cursor);
    cursor->set_key(cursor, "key");
    int ret = cursor->search(cursor);
    ASSERT_EQ(WT_NOTFOUND, ret);

    ru1->abortUnitOfWork();
    ru2->abortUnitOfWork();
}

TEST_F(WiredTigerRecoveryUnitTestFixture, WriteAllowedWhileIgnorePrepareFalse) {
    // Prepare but don't commit a transaction
    ru1->beginUnitOfWork(clientAndCtx1.second->readOnly());
    WT_CURSOR* cursor;
    getCursor(ru1, &cursor);
    cursor->set_key(cursor, "key1");
    cursor->set_value(cursor, "value1");
    invariantWTOK(
        wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(clientAndCtx1.second.get()), cursor),
        cursor->session);
    ru1->setPrepareTimestamp({1, 1});
    ru1->prepareUnitOfWork();

    // A transaction that chooses to ignore prepare conflicts with kIgnoreConflictsAllowWrites does
    // not see the record
    ru2->beginUnitOfWork(clientAndCtx2.second->readOnly());
    ru2->setPrepareConflictBehavior(PrepareConflictBehavior::kIgnoreConflictsAllowWrites);

    // The prepared write is not visible.
    getCursor(ru2, &cursor);
    cursor->set_key(cursor, "key1");
    ASSERT_EQ(WT_NOTFOUND, cursor->search(cursor));

    getCursor(ru2, &cursor);
    cursor->set_key(cursor, "key2");
    cursor->set_value(cursor, "value2");

    // The write is allowed.
    invariantWTOK(
        wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(clientAndCtx2.second.get()), cursor),
        cursor->session);

    ru1->abortUnitOfWork();
    ru2->abortUnitOfWork();
}

TEST_F(WiredTigerRecoveryUnitTestFixture, WriteOnADocumentBeingPreparedTriggersWTRollback) {
    // Prepare but don't commit a transaction
    ru1->beginUnitOfWork(clientAndCtx1.second->readOnly());
    WT_CURSOR* cursor;
    getCursor(ru1, &cursor);
    cursor->set_key(cursor, "key");
    cursor->set_value(cursor, "value");
    invariantWTOK(
        wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(clientAndCtx1.second.get()), cursor),
        cursor->session);
    ru1->setPrepareTimestamp({1, 1});
    ru1->prepareUnitOfWork();

    // Another transaction with write triggers WT_ROLLBACK
    ru2->beginUnitOfWork(clientAndCtx2.second->readOnly());
    getCursor(ru2, &cursor);
    cursor->set_key(cursor, "key");
    cursor->set_value(cursor, "value2");
    int ret =
        wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(clientAndCtx2.second.get()), cursor);
    ASSERT_EQ(WT_ROLLBACK, ret);

    ru1->abortUnitOfWork();
    ru2->abortUnitOfWork();
}

TEST_F(WiredTigerRecoveryUnitTestFixture,
       ChangeIsPassedEmptyLastTimestampSetOnCommitWithNoTimestamp) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        wuow.commit();
    }
    ASSERT(!commitTs);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, ChangeIsPassedLastTimestampSetOnCommit) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    Timestamp ts2(6, 6);
    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts1));
        ASSERT(!commitTs);
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts2));
        ASSERT(!commitTs);
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts1));
        ASSERT(!commitTs);
        wuow.commit();
        ASSERT_EQ(*commitTs, ts1);
    }
    ASSERT_EQ(*commitTs, ts1);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, ChangeIsNotPassedLastTimestampSetOnAbort) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts1));
        ASSERT(!commitTs);
    }
    ASSERT(!commitTs);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, ChangeIsPassedCommitTimestamp) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);

    opCtx->recoveryUnit()->setCommitTimestamp(ts1);
    ASSERT(!commitTs);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
        wuow.commit();
        ASSERT_EQ(*commitTs, ts1);
    }
    ASSERT_EQ(*commitTs, ts1);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, ChangeIsNotPassedCommitTimestampIfCleared) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);

    opCtx->recoveryUnit()->setCommitTimestamp(ts1);
    ASSERT(!commitTs);
    opCtx->recoveryUnit()->clearCommitTimestamp();
    ASSERT(!commitTs);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
        wuow.commit();
    }
    ASSERT(!commitTs);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, ChangeIsPassedNewestCommitTimestamp) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    Timestamp ts2(6, 6);

    opCtx->recoveryUnit()->setCommitTimestamp(ts2);
    ASSERT(!commitTs);
    opCtx->recoveryUnit()->clearCommitTimestamp();
    ASSERT(!commitTs);
    opCtx->recoveryUnit()->setCommitTimestamp(ts1);
    ASSERT(!commitTs);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
        wuow.commit();
        ASSERT_EQ(*commitTs, ts1);
    }
    ASSERT_EQ(*commitTs, ts1);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, ChangeIsNotPassedCommitTimestampOnAbort) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);

    opCtx->recoveryUnit()->setCommitTimestamp(ts1);
    ASSERT(!commitTs);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
    }
    ASSERT(!commitTs);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CommitTimestampBeforeSetTimestampOnCommit) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    Timestamp ts2(6, 6);

    opCtx->recoveryUnit()->setCommitTimestamp(ts2);
    ASSERT(!commitTs);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
        wuow.commit();
        ASSERT_EQ(*commitTs, ts2);
    }
    ASSERT_EQ(*commitTs, ts2);
    opCtx->recoveryUnit()->clearCommitTimestamp();

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts1));
        ASSERT_EQ(*commitTs, ts2);
        wuow.commit();
        ASSERT_EQ(*commitTs, ts1);
    }
    ASSERT_EQ(*commitTs, ts1);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CommitTimestampAfterSetTimestampOnCommit) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    Timestamp ts2(6, 6);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts2));
        ASSERT(!commitTs);
        wuow.commit();
        ASSERT_EQ(*commitTs, ts2);
    }
    ASSERT_EQ(*commitTs, ts2);

    opCtx->recoveryUnit()->setCommitTimestamp(ts1);
    ASSERT_EQ(*commitTs, ts2);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT_EQ(*commitTs, ts2);
        wuow.commit();
        ASSERT_EQ(*commitTs, ts1);
    }
    ASSERT_EQ(*commitTs, ts1);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CommitTimestampBeforeSetTimestampOnAbort) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    Timestamp ts2(6, 6);

    opCtx->recoveryUnit()->setCommitTimestamp(ts2);
    ASSERT(!commitTs);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
    }
    ASSERT(!commitTs);
    opCtx->recoveryUnit()->clearCommitTimestamp();

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts1));
        ASSERT(!commitTs);
    }
    ASSERT(!commitTs);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CommitTimestampAfterSetTimestampOnAbort) {
    boost::optional<Timestamp> commitTs = boost::none;
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    Timestamp ts2(6, 6);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
        ASSERT_OK(opCtx->recoveryUnit()->setTimestamp(ts2));
        ASSERT(!commitTs);
    }
    ASSERT(!commitTs);

    opCtx->recoveryUnit()->setCommitTimestamp(ts1);
    ASSERT(!commitTs);

    {
        WriteUnitOfWork wuow(opCtx);
        opCtx->recoveryUnit()->onCommit(
            [&](OperationContext*, boost::optional<Timestamp> commitTime) {
                commitTs = commitTime;
            });
        ASSERT(!commitTs);
    }
    ASSERT(!commitTs);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CheckpointCursorNotChanged) {
    auto opCtx1 = clientAndCtx1.second.get();
    auto opCtx2 = clientAndCtx2.second.get();

    // Hold the global lock throughout the test to avoid having the global lock destructor
    // prematurely abandon snapshots.
    Lock::GlobalLock globalLock(opCtx1, MODE_IX);
    Lock::GlobalLock globalLock2(opCtx2, MODE_IX);
    auto ru = WiredTigerRecoveryUnit::get(opCtx1);
    auto ru2 = WiredTigerRecoveryUnit::get(opCtx2);

    std::unique_ptr<RecordStore> rs(
        harnessHelper->createRecordStore(opCtx1, "test.checkpoint_stable"));

    WiredTigerKVEngine* engine = harnessHelper->getEngine();

    // Insert a record.
    RecordId rid1;
    {
        WriteUnitOfWork wuow(opCtx1);
        StatusWith<RecordId> s1 = rs->insertRecord(opCtx1, "data", 4, Timestamp());
        ASSERT_TRUE(s1.isOK());
        ASSERT_EQUALS(1, rs->numRecords(opCtx1));
        rid1 = s1.getValue();
        wuow.commit();
    }
    // Force a checkpoint.
    engine->flushAllFiles(opCtx1, /*callerHoldsReadLock*/ false);

    // Test 1: Open a checkpoint cursor and ensure it has the first record.
    ru2->setTimestampReadSource(WiredTigerRecoveryUnit::ReadSource::kCheckpoint);
    auto originalCheckpointCursor = rs->getCursor(opCtx2, true);
    ASSERT(originalCheckpointCursor->seekExact(rid1));

    // Insert a new record.
    RecordId rid2;
    {
        WriteUnitOfWork wuow(opCtx1);
        StatusWith<RecordId> s2 = rs->insertRecord(opCtx1, "data_2", 6, Timestamp());
        ASSERT_TRUE(s2.isOK());
        ASSERT_EQUALS(2, rs->numRecords(opCtx1));
        rid2 = s2.getValue();
        wuow.commit();
    }

    // Test 2: New record does not appear in original checkpoint cursor.
    ASSERT(!originalCheckpointCursor->seekExact(rid2));
    ASSERT(originalCheckpointCursor->seekExact(rid1));

    // Test 3: New record does not appear in new checkpoint cursor since no new checkpoint was
    // created.
    ru->setTimestampReadSource(WiredTigerRecoveryUnit::ReadSource::kCheckpoint);
    auto checkpointCursor = rs->getCursor(opCtx1, true);
    ASSERT(!checkpointCursor->seekExact(rid2));

    // Force a checkpoint.
    engine->flushAllFiles(opCtx1, /*callerHoldsReadLock*/ false);

    // Test 4: Old and new record should appear in new checkpoint cursor. Only old record
    // should appear in the original checkpoint cursor
    ru->setTimestampReadSource(WiredTigerRecoveryUnit::ReadSource::kCheckpoint);
    auto newCheckpointCursor = rs->getCursor(opCtx1, true);
    ASSERT(newCheckpointCursor->seekExact(rid1));
    ASSERT(newCheckpointCursor->seekExact(rid2));
    ASSERT(originalCheckpointCursor->seekExact(rid1));
    ASSERT(!originalCheckpointCursor->seekExact(rid2));
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CheckpointCursorGetId) {
    auto opCtx1 = clientAndCtx1.second.get();
    auto opCtx2 = clientAndCtx2.second.get();

    // Hold the global lock throughout the test to avoid having the global lock destructor
    // prematurely abandon snapshots.
    Lock::GlobalLock globalLock(opCtx1, MODE_IX);
    Lock::GlobalLock globalLock2(opCtx2, MODE_IX);
    auto ru = WiredTigerRecoveryUnit::get(opCtx1);
    auto ru2 = WiredTigerRecoveryUnit::get(opCtx2);

    std::unique_ptr<RecordStore> rs(harnessHelper->createRecordStore(opCtx1, "test.checkpoint_id"));

    WiredTigerKVEngine* engine = harnessHelper->getEngine();

    // Force a checkpoint.
    engine->flushAllFiles(opCtx1, /*callerHoldsReadLock*/ false);

    // Open a checkpoint cursor and check its id.
    ru2->setTimestampReadSource(WiredTigerRecoveryUnit::ReadSource::kCheckpoint);
    auto originalCheckpointCursor = rs->getCursor(opCtx2, true);
    auto firstCheckpointId = originalCheckpointCursor->getCheckpointId();
    ASSERT(firstCheckpointId > 0);

    // Insert a record and force a checkpoint.
    RecordId rid1;
    {
        WriteUnitOfWork wuow(opCtx1);
        StatusWith<RecordId> s1 = rs->insertRecord(opCtx1, "data", 4, Timestamp());
        ASSERT_TRUE(s1.isOK());
        ASSERT_EQUALS(1, rs->numRecords(opCtx1));
        rid1 = s1.getValue();
        wuow.commit();
    }
    engine->flushAllFiles(opCtx1, /*callerHoldsReadLock*/ false);

    // Open another checkpoint cursor and check its new id.
    ru->setTimestampReadSource(WiredTigerRecoveryUnit::ReadSource::kCheckpoint);
    auto newCheckpointCursor = rs->getCursor(opCtx1, true);
    ASSERT(newCheckpointCursor->getCheckpointId() > firstCheckpointId);
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CommitWithDurableTimestamp) {
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(3, 3);
    Timestamp ts2(5, 5);

    opCtx->recoveryUnit()->setCommitTimestamp(ts1);
    opCtx->recoveryUnit()->setDurableTimestamp(ts2);
    auto durableTs = opCtx->recoveryUnit()->getDurableTimestamp();
    ASSERT_EQ(ts2, durableTs);

    {
        WriteUnitOfWork wuow(opCtx);
        wuow.commit();
    }
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CommitWithoutDurableTimestamp) {
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(5, 5);
    opCtx->recoveryUnit()->setCommitTimestamp(ts1);

    {
        WriteUnitOfWork wuow(opCtx);
        wuow.commit();
    }
}

TEST_F(WiredTigerRecoveryUnitTestFixture, MultiTimestampConstraintsInternalState) {
    Timestamp ts1(1, 1);
    Timestamp ts2(2, 2);

    OperationContext* opCtx = clientAndCtx1.second.get();
    ru1->beginUnitOfWork(opCtx->readOnly());

    // Perform an non timestamped write.
    WT_CURSOR* cursor;
    getCursor(ru1, &cursor);
    cursor->set_key(cursor, "key");
    cursor->set_value(cursor, "value");
    invariantWTOK(wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(opCtx), cursor),
                  cursor->session);

    // Perform a write at ts1.
    cursor->set_key(cursor, "key2");
    cursor->set_value(cursor, "value");
    ASSERT_OK(ru1->setTimestamp(ts1));
    invariantWTOK(wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(opCtx), cursor),
                  cursor->session);

    // Setting the timestamp again to the same value should not fail.
    ASSERT_OK(ru1->setTimestamp(ts1));

    // Committing the unit of work should reset the internal state for the multi timestamp
    // constraint checks.
    ru1->commitUnitOfWork();
    ru1->beginUnitOfWork(opCtx->readOnly());

    // Perform a write at ts2.
    cursor->set_key(cursor, "key3");
    cursor->set_value(cursor, "value");
    ASSERT_OK(ru1->setTimestamp(ts2));
    invariantWTOK(wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(opCtx), cursor),
                  cursor->session);

    ru1->commitUnitOfWork();
}

TEST_F(WiredTigerRecoveryUnitTestFixture, AbandonSnapshotAbortMode) {
    ru1->setAbandonSnapshotMode(RecoveryUnit::AbandonSnapshotMode::kAbort);

    OperationContext* opCtx = clientAndCtx1.second.get();
    const char* const key = "key";

    {
        ru1->beginUnitOfWork(opCtx->readOnly());

        WT_CURSOR* cursor;
        getCursor(ru1, &cursor);
        cursor->set_key(cursor, key);
        cursor->set_value(cursor, "value");
        invariantWTOK(wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(opCtx), cursor),
                      cursor->session);

        ru1->commitUnitOfWork();
    }

    // Create a cursor. We will check that once positioned, the cursor is reset by a call to
    // abandonSnapshot() on the associated RecoveryUnit.
    WT_CURSOR* cursor;
    getCursor(ru1, &cursor);
    cursor->set_key(cursor, key);
    ASSERT_EQ(0, cursor->search(cursor));

    ru1->abandonSnapshot();

    // The WT transaction should have been aborted and the cursor reset.

    // Advancing to the "next" record now that the cursor has been reset should give us the first
    // record again.
    ASSERT_EQ(0, cursor->next(cursor));

    const char* returnedKey = nullptr;
    ASSERT_EQ(0, cursor->get_key(cursor, &returnedKey));
    ASSERT_EQ(0, strncmp(key, returnedKey, strlen(key)));
}

class SnapshotTestDecoration {
public:
    void hit() {
        _hits++;
    }

    int getHits() {
        return _hits;
    }

private:
    int _hits = 0;
};

const RecoveryUnit::Snapshot::Decoration<SnapshotTestDecoration> getSnapshotDecoration =
    RecoveryUnit::Snapshot::declareDecoration<SnapshotTestDecoration>();

TEST_F(WiredTigerRecoveryUnitTestFixture, AbandonSnapshotChange) {
    ASSERT(ru1->getSession());

    getSnapshotDecoration(ru1->getSnapshot()).hit();
    ASSERT_EQ(1, getSnapshotDecoration(ru1->getSnapshot()).getHits());

    ru1->abandonSnapshot();

    // A snapshot is closed, reconstructing our decoration.
    ASSERT_EQ(0, getSnapshotDecoration(ru1->getSnapshot()).getHits());
}

TEST_F(WiredTigerRecoveryUnitTestFixture, CommitSnapshotChange) {
    ru1->beginUnitOfWork(/*readOnly=*/false);

    getSnapshotDecoration(ru1->getSnapshot()).hit();
    ASSERT_EQ(1, getSnapshotDecoration(ru1->getSnapshot()).getHits());

    ASSERT(ru1->getSession());

    ASSERT_EQ(1, getSnapshotDecoration(ru1->getSnapshot()).getHits());

    ru1->commitUnitOfWork();

    // A snapshot is closed, reconstructing our decoration.
    ASSERT_EQ(0, getSnapshotDecoration(ru1->getSnapshot()).getHits());
}

TEST_F(WiredTigerRecoveryUnitTestFixture, AbortSnapshotChange) {
    // A snapshot is already open from when the RU was constructed.
    ASSERT(ru1->getSession());
    getSnapshotDecoration(ru1->getSnapshot()).hit();
    ASSERT_EQ(1, getSnapshotDecoration(ru1->getSnapshot()).getHits());

    ru1->beginUnitOfWork(/*readOnly=*/false);
    ASSERT_EQ(1, getSnapshotDecoration(ru1->getSnapshot()).getHits());

    ru1->abortUnitOfWork();

    // A snapshot is closed, reconstructing our decoration.
    ASSERT_EQ(0, getSnapshotDecoration(ru1->getSnapshot()).getHits());
}

DEATH_TEST_REGEX_F(WiredTigerRecoveryUnitTestFixture,
                   MultiTimestampConstraints,
                   "Fatal assertion.*4877100") {
    Timestamp ts1(1, 1);
    Timestamp ts2(2, 2);

    OperationContext* opCtx = clientAndCtx1.second.get();
    ru1->beginUnitOfWork(opCtx->readOnly());

    auto writeTest = [&]() {
        // Perform an non timestamped write.
        WT_CURSOR* cursor;
        getCursor(ru1, &cursor);
        cursor->set_key(cursor, "key");
        cursor->set_value(cursor, "value");
        invariantWTOK(wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(opCtx), cursor),
                      cursor->session);

        // Perform a write at ts1.
        cursor->set_key(cursor, "key2");
        cursor->set_value(cursor, "value");
        ASSERT_OK(ru1->setTimestamp(ts1));
        invariantWTOK(wiredTigerCursorInsert(*WiredTigerRecoveryUnit::get(opCtx), cursor),
                      cursor->session);

        // Setting the timestamp again to a different value should detect that we're trying to set
        // multiple timestamps with the first write being non timestamped.
        ASSERT_OK(ru1->setTimestamp(ts2));
        ru1->commitUnitOfWork();
    };

    try {
        writeTest();
    } catch (StorageUnavailableException const&) {
        // It's expected to get a WCE the first time we try this, due to the multi-timestamp
        // constraint. We'll try again and it will fassert and print out extra debug info.
    }
    writeTest();
}

DEATH_TEST_F(WiredTigerRecoveryUnitTestFixture,
             SetDurableTimestampTwice,
             "Trying to reset durable timestamp when it was already set.") {
    auto opCtx = clientAndCtx1.second.get();
    Timestamp ts1(3, 3);
    Timestamp ts2(5, 5);
    opCtx->recoveryUnit()->setDurableTimestamp(ts1);
    opCtx->recoveryUnit()->setDurableTimestamp(ts2);
}

DEATH_TEST_F(WiredTigerRecoveryUnitTestFixture,
             RollbackHandlerAbortsOnTxnOpen,
             "rollback handler reopened transaction") {
    auto opCtx = clientAndCtx1.second.get();
    auto ru = WiredTigerRecoveryUnit::get(opCtx);
    ASSERT(ru->getSession());
    {
        WriteUnitOfWork wuow(opCtx);
        ru->assertInActiveTxn();
        ru->onRollback([ru](OperationContext*) { ru->getSession(); });
    }
}

DEATH_TEST_F(WiredTigerRecoveryUnitTestFixture,
             MayNotChangeReadSourceWhilePinned,
             "Cannot change ReadSource as it is pinned.") {

    // Storage engine operations require at least Global IS.
    Lock::GlobalLock lk(clientAndCtx1.second.get(), MODE_IS);
    ru1->pinReadSource();
    ru1->setTimestampReadSource(RecoveryUnit::ReadSource::kNoOverlap);
}

}  // namespace
}  // namespace mongo
