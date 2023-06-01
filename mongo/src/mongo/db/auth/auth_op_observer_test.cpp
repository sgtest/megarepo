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

#include "mongo/db/auth/auth_op_observer.h"
#include "mongo/db/auth/authorization_manager.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/keys_collection_client_sharded.h"
#include "mongo/db/keys_collection_manager.h"
#include "mongo/db/logical_time_validator.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/oplog_interface_local.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/storage_interface_mock.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/db/session/session_catalog_mongod.h"
#include "mongo/db/transaction/transaction_participant.h"
#include "mongo/unittest/death_test.h"
#include "mongo/util/clock_source_mock.h"

namespace mongo {
namespace {

using repl::OplogEntry;
using unittest::assertGet;

class AuthOpObserverTest : public ServiceContextMongoDTest {
public:
    void setUp() override {
        // Set up mongod.
        ServiceContextMongoDTest::setUp();

        auto service = getServiceContext();
        auto opCtx = cc().makeOperationContext();
        repl::StorageInterface::set(service, std::make_unique<repl::StorageInterfaceMock>());

        // Set up ReplicationCoordinator and create oplog.
        repl::ReplicationCoordinator::set(
            service,
            std::make_unique<repl::ReplicationCoordinatorMock>(service, createReplSettings()));
        repl::createOplog(opCtx.get());

        // Ensure that we are primary.
        auto replCoord = repl::ReplicationCoordinator::get(opCtx.get());
        ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_PRIMARY));

        // Create test collection
        writeConflictRetry(opCtx.get(), "createColl", _nss, [&] {
            opCtx->recoveryUnit()->setTimestampReadSource(RecoveryUnit::ReadSource::kNoTimestamp);
            opCtx->recoveryUnit()->abandonSnapshot();

            WriteUnitOfWork wunit(opCtx.get());
            AutoGetCollection collRaii(opCtx.get(), _nss, MODE_X);

            auto db = collRaii.ensureDbExists(opCtx.get());
            invariant(db->createCollection(opCtx.get(), _nss, {}));
            wunit.commit();
        });
    }

    NamespaceString _nss = NamespaceString::createNamespaceString_forTest("test", "coll");

private:
    // Creates a reasonable set of ReplSettings for most tests.  We need to be able to
    // override this to create a larger oplog.
    virtual repl::ReplSettings createReplSettings() {
        repl::ReplSettings settings;
        settings.setOplogSizeBytes(5 * 1024 * 1024);
        settings.setReplSetString("mySet/node1:12345");
        return settings;
    }
};

TEST_F(AuthOpObserverTest, OnRollbackInvalidatesAuthCacheWhenAuthNamespaceRolledBack) {
    AuthOpObserver opObserver;
    auto opCtx = cc().makeOperationContext();
    auto authMgr = AuthorizationManager::get(getServiceContext());
    auto initCacheGen = authMgr->getCacheGeneration();

    // Verify that the rollback op observer invalidates the user cache for each auth namespace by
    // checking that the cache generation changes after a call to the rollback observer method.
    OpObserver::RollbackObserverInfo rbInfo;
    rbInfo.rollbackNamespaces = {NamespaceString::kAdminRolesNamespace};
    opObserver.onReplicationRollback(opCtx.get(), rbInfo);
    ASSERT_NE(initCacheGen, authMgr->getCacheGeneration());

    initCacheGen = authMgr->getCacheGeneration();
    rbInfo.rollbackNamespaces = {NamespaceString::kAdminUsersNamespace};
    opObserver.onReplicationRollback(opCtx.get(), rbInfo);
    ASSERT_NE(initCacheGen, authMgr->getCacheGeneration());

    initCacheGen = authMgr->getCacheGeneration();
    rbInfo.rollbackNamespaces = {NamespaceString::kServerConfigurationNamespace};
    opObserver.onReplicationRollback(opCtx.get(), rbInfo);
    ASSERT_NE(initCacheGen, authMgr->getCacheGeneration());
}

TEST_F(AuthOpObserverTest, OnRollbackDoesntInvalidateAuthCacheWhenNoAuthNamespaceRolledBack) {
    AuthOpObserver opObserver;
    auto opCtx = cc().makeOperationContext();
    auto authMgr = AuthorizationManager::get(getServiceContext());
    auto initCacheGen = authMgr->getCacheGeneration();

    // Verify that the rollback op observer doesn't invalidate the user cache.
    OpObserver::RollbackObserverInfo rbInfo;
    opObserver.onReplicationRollback(opCtx.get(), rbInfo);
    auto newCacheGen = authMgr->getCacheGeneration();
    ASSERT_EQ(newCacheGen, initCacheGen);
}

TEST_F(AuthOpObserverTest, MultipleAboutToDeleteAndOnDelete) {
    AuthOpObserver opObserver;
    auto opCtx = cc().makeOperationContext();
    NamespaceString nss = NamespaceString::createNamespaceString_forTest("test", "coll");
    WriteUnitOfWork wunit(opCtx.get());
    AutoGetCollection autoColl(opCtx.get(), nss, MODE_IX);
    OplogDeleteEntryArgs args;
    opObserver.aboutToDelete(opCtx.get(), *autoColl, BSON("_id" << 1), &args);
    opObserver.onDelete(opCtx.get(), *autoColl, {}, args);
    opObserver.aboutToDelete(opCtx.get(), *autoColl, BSON("_id" << 1), &args);
    opObserver.onDelete(opCtx.get(), *autoColl, {}, args);
}

DEATH_TEST_F(AuthOpObserverTest, AboutToDeleteMustPreceedOnDelete, "invariant") {
    AuthOpObserver opObserver;
    auto opCtx = cc().makeOperationContext();
    NamespaceString nss = NamespaceString::createNamespaceString_forTest("test", "coll");
    AutoGetCollection autoColl(opCtx.get(), nss, MODE_IX);
    OplogDeleteEntryArgs args;
    opObserver.onDelete(opCtx.get(), *autoColl, {}, args);
}

DEATH_TEST_F(AuthOpObserverTest, EachOnDeleteRequiresAboutToDelete, "invariant") {
    AuthOpObserver opObserver;
    auto opCtx = cc().makeOperationContext();
    AutoGetCollection autoColl(opCtx.get(), _nss, MODE_IX);
    OplogDeleteEntryArgs args;
    opObserver.aboutToDelete(opCtx.get(), *autoColl, {}, &args);
    opObserver.onDelete(opCtx.get(), *autoColl, {}, args);
    opObserver.onDelete(opCtx.get(), *autoColl, {}, args);
}

}  // namespace
}  // namespace mongo
