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

#include <functional>
#include <memory>
#include <string>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/connection_string.h"
#include "mongo/client/mongo_uri.h"
#include "mongo/db/catalog/create_collection.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/commands/create_gen.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/storage_interface_mock.h"
#include "mongo/db/repl/tenant_migration_access_blocker.h"
#include "mongo/db/repl/tenant_migration_access_blocker_registry.h"
#include "mongo/db/repl/tenant_migration_donor_access_blocker.h"
#include "mongo/db/serverless/serverless_operation_lock_registry.h"
#include "mongo/db/serverless/shard_split_donor_op_observer.h"
#include "mongo/db/serverless/shard_split_state_machine_gen.h"
#include "mongo/db/serverless/shard_split_test_utils.h"
#include "mongo/db/serverless/shard_split_utils.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/tenant_id.h"
#include "mongo/dbtests/mock/mock_replica_set.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/time_support.h"

namespace mongo {
namespace {

class ShardSplitDonorOpObserverTest : public ServiceContextMongoDTest {
public:
    void setUp() override {
        ServiceContextMongoDTest::setUp();

        auto service = getServiceContext();
        {
            auto opCtx = cc().makeOperationContext();
            repl::StorageInterface::set(service, std::make_unique<repl::StorageInterfaceMock>());

            // Set up ReplicationCoordinator and create oplog.
            auto coordinatorMock = std::make_unique<repl::ReplicationCoordinatorMock>(
                service, repl::createServerlessReplSettings());
            _replicationCoordinatorMock = coordinatorMock.get();

            repl::ReplicationCoordinator::set(service, std::move(coordinatorMock));
            repl::createOplog(opCtx.get());

            // Ensure that we are primary.
            auto replCoord = repl::ReplicationCoordinator::get(opCtx.get());
            ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_PRIMARY));
        }

        _observer = std::make_unique<ShardSplitDonorOpObserver>();
        _opCtx = makeOperationContext();
        _oplogSlot = 0;

        ASSERT_OK(createCollection(_opCtx.get(), CreateCommand(_nss)));
    }

    void tearDown() override {
        _observer.reset();
        _opCtx.reset();

        ServiceContextMongoDTest::tearDown();
    }

protected:
    void runInsertTestCase(
        ShardSplitDonorDocument stateDocument,
        const std::vector<TenantId>& tenants,
        std::function<void(std::shared_ptr<TenantMigrationAccessBlocker>)> mtabVerifier) {

        std::vector<InsertStatement> inserts;
        inserts.emplace_back(_oplogSlot++, stateDocument.toBSON());

        {
            AutoGetCollection autoColl(_opCtx.get(), _nss, MODE_IX);
            WriteUnitOfWork wow(_opCtx.get());
            _observer->onInserts(_opCtx.get(),
                                 *autoColl,
                                 inserts.begin(),
                                 inserts.end(),
                                 /*fromMigrate=*/std::vector<bool>(inserts.size(), false),
                                 /*defaultFromMigrate=*/false);
            wow.commit();
        }

        verifyAndRemoveMtab(tenants, mtabVerifier);
    }

    void runUpdateTestCase(
        ShardSplitDonorDocument stateDocument,
        const std::vector<TenantId>& tenants,
        std::function<void(std::shared_ptr<TenantMigrationAccessBlocker>)> mtabVerifier) {

        // If there's an exception, aborting without removing the access blocker will trigger an
        // invariant. This creates a confusing error log in the test output.
        test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, _opCtx.get());

        const auto criteria = BSON("_id" << stateDocument.getId());
        auto preImageDoc = defaultStateDocument();
        preImageDoc.setState(ShardSplitDonorStateEnum::kBlocking);
        preImageDoc.setBlockOpTime(repl::OpTime(Timestamp(1, 1), 1));

        CollectionUpdateArgs updateArgs{preImageDoc.toBSON()};
        updateArgs.criteria = criteria;
        updateArgs.stmtIds = {};
        updateArgs.updatedDoc = stateDocument.toBSON();
        updateArgs.update =
            BSON("$set" << BSON(ShardSplitDonorDocument::kStateFieldName
                                << ShardSplitDonorState_serializer(stateDocument.getState())));
        AutoGetCollection autoColl(_opCtx.get(), _nss, MODE_IX);
        OplogUpdateEntryArgs update(&updateArgs, *autoColl);

        WriteUnitOfWork wuow(_opCtx.get());
        _observer->onUpdate(_opCtx.get(), update);
        wuow.commit();

        verifyAndRemoveMtab(tenants, mtabVerifier);
        scopedTenants.dismiss();
    }

    std::vector<std::shared_ptr<TenantMigrationDonorAccessBlocker>>
    createAccessBlockerAndStartBlockingWrites(const UUID& migrationId,
                                              const std::vector<TenantId>& tenants,
                                              OperationContext* opCtx,
                                              bool isSecondary = false) {

        std::vector<std::shared_ptr<TenantMigrationDonorAccessBlocker>> result;
        for (const auto& tenantId : tenants) {
            auto mtab = std::make_shared<TenantMigrationDonorAccessBlocker>(
                _opCtx->getServiceContext(), migrationId);

            if (!isSecondary) {
                mtab->startBlockingWrites();
            }

            TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                .add(tenantId, mtab);
            result.push_back(mtab);
        }

        return result;
    }

    ShardSplitDonorDocument defaultStateDocument() const {
        auto shardSplitStateDoc = ShardSplitDonorDocument::parse(
            IDLParserContext{"donor.document"},
            BSON("_id" << _uuid << "recipientTagName" << _recipientTagName << "recipientSetName"
                       << _recipientSetName));
        shardSplitStateDoc.setTenantIds(_tenantIds);
        return shardSplitStateDoc;
    }

protected:
    MockReplicaSet _replSet =
        MockReplicaSet("donorSet", 3, true /* hasPrimary */, true /* dollarPrefixHosts */);
    MockReplicaSet _recipientReplSet =
        MockReplicaSet("recipientSet", 3, true /* hasPrimary */, true /* dollarPrefixHosts */);
    const NamespaceString _nss = NamespaceString::kShardSplitDonorsNamespace;
    std::vector<TenantId> _tenantIds = {TenantId(OID::gen()), TenantId(OID::gen())};
    UUID _uuid = UUID::gen();
    std::string _recipientTagName{"$recipientNode"};
    std::string _recipientSetName{_replSet.getURI().getSetName()};

    std::unique_ptr<ShardSplitDonorOpObserver> _observer;
    std::shared_ptr<OperationContext> _opCtx;
    repl::ReplicationCoordinatorMock* _replicationCoordinatorMock;
    int _oplogSlot;

private:
    void verifyAndRemoveMtab(
        const std::vector<TenantId>& tenants,
        const std::function<void(std::shared_ptr<TenantMigrationAccessBlocker>)>& mtabVerifier) {
        for (const auto& tenantId : tenants) {
            auto mtab = TenantMigrationAccessBlockerRegistry::get(_opCtx->getServiceContext())
                            .getTenantMigrationAccessBlockerForTenantId(
                                tenantId, TenantMigrationAccessBlocker::BlockerType::kDonor);
            mtabVerifier(mtab);
        }

        TenantMigrationAccessBlockerRegistry::get(_opCtx->getServiceContext())
            .removeAccessBlockersForMigration(_uuid,
                                              TenantMigrationAccessBlocker::BlockerType::kDonor);
    }
};

TEST_F(ShardSplitDonorOpObserverTest, InsertWrongType) {
    std::vector<InsertStatement> inserts1;
    inserts1.emplace_back(0,
                          BSON("_id" << 0 << "data"
                                     << "x"));
    inserts1.emplace_back(1,
                          BSON("_id" << 1 << "data"
                                     << "y"));

    AutoGetCollection autoColl(_opCtx.get(), _nss, MODE_IX);
    ASSERT_THROWS_CODE(
        _observer->onInserts(_opCtx.get(),
                             *autoColl,
                             inserts1.begin(),
                             inserts1.end(),
                             /*fromMigrate=*/std::vector<bool>(inserts1.size(), false),
                             /*defaultFromMigrate=*/false),
        DBException,
        ErrorCodes::TypeMismatch);
}

TEST_F(ShardSplitDonorOpObserverTest, InitialInsertInvalidState) {
    std::vector<ShardSplitDonorStateEnum> states = {ShardSplitDonorStateEnum::kAborted,
                                                    ShardSplitDonorStateEnum::kBlocking,
                                                    ShardSplitDonorStateEnum::kUninitialized,
                                                    ShardSplitDonorStateEnum::kCommitted};

    for (auto state : states) {
        auto stateDocument = defaultStateDocument();
        stateDocument.setState(state);

        auto mtabVerifier = [](std::shared_ptr<TenantMigrationAccessBlocker>) {
        };

        ASSERT_THROWS(runInsertTestCase(stateDocument, _tenantIds, mtabVerifier), DBException);
    }
}

TEST_F(ShardSplitDonorOpObserverTest, InsertValidAbortedDocument) {
    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAborted);

    Status status(ErrorCodes::CallbackCanceled, "Split has been aborted");
    BSONObjBuilder bob;
    status.serializeErrorToBSON(&bob);
    stateDocument.setAbortReason(bob.obj());

    stateDocument.setCommitOrAbortOpTime(repl::OpTime(Timestamp(1), 1));

    std::vector<InsertStatement> inserts;
    inserts.emplace_back(_oplogSlot++, stateDocument.toBSON());

    {
        AutoGetCollection autoColl(_opCtx.get(), _nss, MODE_IX);
        WriteUnitOfWork wow(_opCtx.get());
        _observer->onInserts(_opCtx.get(),
                             *autoColl,
                             inserts.begin(),
                             inserts.end(),
                             /*fromMigrate=*/std::vector<bool>(inserts.size(), false),
                             /*defaultFromMigrate=*/false);
        wow.commit();
    }

    for (const auto& tenant : _tenantIds) {
        ASSERT_FALSE(TenantMigrationAccessBlockerRegistry::get(_opCtx->getServiceContext())
                         .getTenantMigrationAccessBlockerForTenantId(
                             tenant, TenantMigrationAccessBlocker::BlockerType::kDonor));
    }
}

TEST_F(ShardSplitDonorOpObserverTest, InsertAbortingIndexDocumentPrimary) {
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientReplSet.getHosts());

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAbortingIndexBuilds);
    stateDocument.setRecipientConnectionString(mongo::serverless::makeRecipientConnectionString(
        repl::ReplicationCoordinator::get(_opCtx.get())->getConfig(),
        _recipientTagName,
        _recipientSetName));

    auto mtabVerifier = [opCtx = _opCtx.get()](std::shared_ptr<TenantMigrationAccessBlocker> mtab) {
        ASSERT_TRUE(mtab);
        // The OpObserver does not set the mtab to blocking for primaries.
        ASSERT_OK(mtab->checkIfCanWrite(Timestamp(1, 1)));
        ASSERT_OK(mtab->checkIfCanWrite(Timestamp(1, 3)));
        ASSERT_OK(mtab->checkIfLinearizableReadWasAllowed(opCtx));
        ASSERT_EQ(mtab->checkIfCanBuildIndex().code(), ErrorCodes::TenantMigrationConflict);
    };

    runInsertTestCase(stateDocument, _tenantIds, mtabVerifier);
}

TEST_F(ShardSplitDonorOpObserverTest, UpdateBlockingDocumentPrimary) {
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientReplSet.getHosts());

    createAccessBlockerAndStartBlockingWrites(_uuid, _tenantIds, _opCtx.get());

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kBlocking);
    stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 1), 1));

    auto mtabVerifier = [opCtx = _opCtx.get()](std::shared_ptr<TenantMigrationAccessBlocker> mtab) {
        ASSERT_TRUE(mtab);
        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp(1, 1)).code(),
                  ErrorCodes::TenantMigrationConflict);
        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp(1, 3)).code(),
                  ErrorCodes::TenantMigrationConflict);
        ASSERT_OK(mtab->checkIfLinearizableReadWasAllowed(opCtx));
        ASSERT_EQ(mtab->checkIfCanBuildIndex().code(), ErrorCodes::TenantMigrationConflict);
    };

    runUpdateTestCase(stateDocument, _tenantIds, mtabVerifier);
}

TEST_F(ShardSplitDonorOpObserverTest, UpdateBlockingDocumentSecondary) {
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientReplSet.getHosts());

    // This indicates the instance is secondary for the OpObserver.
    repl::UnreplicatedWritesBlock setSecondary(_opCtx.get());
    createAccessBlockerAndStartBlockingWrites(_uuid, _tenantIds, _opCtx.get(), true);

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kBlocking);
    stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 1), 1));

    auto mtabVerifier = [opCtx = _opCtx.get()](std::shared_ptr<TenantMigrationAccessBlocker> mtab) {
        ASSERT_TRUE(mtab);
        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp(1, 1)).code(),
                  ErrorCodes::TenantMigrationConflict);
        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp(1, 3)).code(),
                  ErrorCodes::TenantMigrationConflict);
        ASSERT_OK(mtab->checkIfLinearizableReadWasAllowed(opCtx));
        ASSERT_EQ(mtab->checkIfCanBuildIndex().code(), ErrorCodes::TenantMigrationConflict);
    };

    runUpdateTestCase(stateDocument, _tenantIds, mtabVerifier);
}

TEST_F(ShardSplitDonorOpObserverTest, TransitionToAbortingIndexBuildsFail) {
    // This indicates the instance is secondary for the OpObserver.
    repl::UnreplicatedWritesBlock setSecondary(_opCtx.get());

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAbortingIndexBuilds);

    const auto criteria = BSON("_id" << stateDocument.getId());
    const auto preImageDoc = criteria;
    CollectionUpdateArgs updateArgs{preImageDoc};
    updateArgs.criteria = criteria;
    updateArgs.stmtIds = {};
    updateArgs.updatedDoc = stateDocument.toBSON();
    updateArgs.update =
        BSON("$set" << BSON(ShardSplitDonorDocument::kStateFieldName
                            << ShardSplitDonorState_serializer(stateDocument.getState())));
    AutoGetCollection autoColl(_opCtx.get(), _nss, MODE_IX);
    OplogUpdateEntryArgs update(&updateArgs, *autoColl);

    auto update_lambda = [&]() {
        WriteUnitOfWork wuow(_opCtx.get());
        _observer->onUpdate(_opCtx.get(), update);
        wuow.commit();
    };

    ASSERT_THROWS_CODE(update_lambda(), DBException, ErrorCodes::IllegalOperation);
}

TEST_F(ShardSplitDonorOpObserverTest, TransitionToCommit) {
    // Transition to commit needs a commitOpTime in the OpLog
    auto commitOpTime = mongo::repl::OpTime(Timestamp(1, 3), 2);
    _replicationCoordinatorMock->setCurrentCommittedSnapshotOpTime(commitOpTime);

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kCommitted);
    stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 2), 1));
    stateDocument.setCommitOrAbortOpTime(commitOpTime);

    auto mtabVector = createAccessBlockerAndStartBlockingWrites(_uuid, _tenantIds, _opCtx.get());
    for (auto& mtab : mtabVector) {
        mtab->startBlockingReadsAfter(Timestamp(1));
    }

    auto mtabVerifier = [opCtx = _opCtx.get()](std::shared_ptr<TenantMigrationAccessBlocker> mtab) {
        ASSERT_TRUE(mtab);
        // For primary instance, the ShardSplitDonorService will set the mtab to blocking, not
        // the OpObserver
        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp(1)).code(), ErrorCodes::TenantMigrationCommitted);
        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp(3)).code(), ErrorCodes::TenantMigrationCommitted);
        ASSERT_EQ(mtab->checkIfLinearizableReadWasAllowed(opCtx),
                  ErrorCodes::TenantMigrationCommitted);
        ASSERT_EQ(mtab->checkIfCanBuildIndex().code(), ErrorCodes::TenantMigrationCommitted);
    };

    runUpdateTestCase(stateDocument, _tenantIds, mtabVerifier);
}

TEST_F(ShardSplitDonorOpObserverTest, TransitionToAbort) {
    // Transition to abort needs a commitOpTime in the OpLog
    auto abortOpTime = mongo::repl::OpTime(Timestamp(1, 3), 2);
    _replicationCoordinatorMock->setCurrentCommittedSnapshotOpTime(abortOpTime);

    Status status(ErrorCodes::CallbackCanceled, "Split has been aborted");
    BSONObjBuilder bob;
    status.serializeErrorToBSON(&bob);

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAborted);
    stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 2), 1));
    stateDocument.setCommitOrAbortOpTime(abortOpTime);
    stateDocument.setAbortReason(bob.obj());

    auto mtabVector = createAccessBlockerAndStartBlockingWrites(_uuid, _tenantIds, _opCtx.get());
    for (auto& mtab : mtabVector) {
        mtab->startBlockingReadsAfter(Timestamp(1));
    }

    auto mtabVerifier = [opCtx = _opCtx.get()](std::shared_ptr<TenantMigrationAccessBlocker> mtab) {
        ASSERT_TRUE(mtab);
        // For primary instance, the ShardSplitDonorService will set the mtab to blocking, not
        // the OpObserver
        ASSERT_OK(mtab->checkIfCanWrite(Timestamp(1)).code());
        ASSERT_OK(mtab->checkIfCanWrite(Timestamp(3)).code());
        ASSERT_OK(mtab->checkIfLinearizableReadWasAllowed(opCtx));
        ASSERT_OK(mtab->checkIfCanBuildIndex().code());
    };

    runUpdateTestCase(stateDocument, _tenantIds, mtabVerifier);
}

TEST_F(ShardSplitDonorOpObserverTest, SetExpireAtForAbortedRemoveBlockers) {
    // Transition to abort needs an abortOpTime in the OpLog
    auto abortOpTime = mongo::repl::OpTime(Timestamp(1, 3), 2);
    _replicationCoordinatorMock->setCurrentCommittedSnapshotOpTime(abortOpTime);

    Status status(ErrorCodes::CallbackCanceled, "Split has been aborted");
    BSONObjBuilder bob;
    status.serializeErrorToBSON(&bob);

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAborted);
    stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 2), 1));
    stateDocument.setCommitOrAbortOpTime(abortOpTime);
    stateDocument.setAbortReason(bob.obj());
    stateDocument.setExpireAt(mongo::Date_t::fromMillisSinceEpoch(1000));

    auto mtabVector = createAccessBlockerAndStartBlockingWrites(_uuid, _tenantIds, _opCtx.get());
    for (auto& mtab : mtabVector) {
        mtab->startBlockingReadsAfter(Timestamp(1));
        mtab->setAbortOpTime(_opCtx.get(), *stateDocument.getCommitOrAbortOpTime());
    }

    auto mtabVerifier = [opCtx = _opCtx.get()](std::shared_ptr<TenantMigrationAccessBlocker> mtab) {
        ASSERT_FALSE(mtab);
    };

    ServerlessOperationLockRegistry::get(_opCtx->getServiceContext())
        .acquireLock(ServerlessOperationLockRegistry::LockType::kShardSplit, _uuid);

    runUpdateTestCase(stateDocument, _tenantIds, mtabVerifier);

    ASSERT_FALSE(ServerlessOperationLockRegistry::get(_opCtx->getServiceContext())
                     .getActiveOperationType_forTest());
}

TEST_F(ShardSplitDonorOpObserverTest, DeleteAbortedDocumentDoesNotRemoveBlockers) {
    // Transition to abort needs an abortOpTime in the OpLog
    auto abortOpTime = mongo::repl::OpTime(Timestamp(1, 3), 2);
    _replicationCoordinatorMock->setCurrentCommittedSnapshotOpTime(abortOpTime);

    Status status(ErrorCodes::CallbackCanceled, "Split has been aborted");
    BSONObjBuilder bob;
    status.serializeErrorToBSON(&bob);

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAborted);
    stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 2), 1));
    stateDocument.setCommitOrAbortOpTime(abortOpTime);
    stateDocument.setAbortReason(bob.obj());
    stateDocument.setExpireAt(mongo::Date_t::fromMillisSinceEpoch(1000));

    auto mtabVector = createAccessBlockerAndStartBlockingWrites(_uuid, _tenantIds, _opCtx.get());
    for (auto& mtab : mtabVector) {
        mtab->startBlockingReadsAfter(Timestamp(1));
        mtab->setAbortOpTime(_opCtx.get(), *stateDocument.getCommitOrAbortOpTime());
    }

    auto bsonDoc = stateDocument.toBSON();

    WriteUnitOfWork wuow(_opCtx.get());
    AutoGetCollection autoColl(_opCtx.get(), NamespaceString::kShardSplitDonorsNamespace, MODE_IX);
    OplogDeleteEntryArgs deleteArgs;
    _observer->aboutToDelete(_opCtx.get(), *autoColl, bsonDoc, &deleteArgs);

    deleteArgs.deletedDoc = &bsonDoc;

    _observer->onDelete(_opCtx.get(), *autoColl, 0 /* stmtId */, deleteArgs);
    wuow.commit();

    // Verify blockers have not been removed
    for (const auto& tenantId : _tenantIds) {
        ASSERT_TRUE(TenantMigrationAccessBlockerRegistry::get(_opCtx->getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kDonor));
    }
}

TEST_F(ShardSplitDonorOpObserverTest, DeleteCommittedDocumentRemovesBlockers) {
    // Transition to committed needs a commitOpTime in the OpLog
    auto commitOpTime = mongo::repl::OpTime(Timestamp(1, 3), 2);
    _replicationCoordinatorMock->setCurrentCommittedSnapshotOpTime(commitOpTime);

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kCommitted);
    stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 2), 1));
    stateDocument.setCommitOrAbortOpTime(commitOpTime);
    stateDocument.setExpireAt(mongo::Date_t::fromMillisSinceEpoch(1000));

    auto mtabVector = createAccessBlockerAndStartBlockingWrites(_uuid, _tenantIds, _opCtx.get());
    for (auto& mtab : mtabVector) {
        mtab->startBlockingReadsAfter(Timestamp(1));
        mtab->setCommitOpTime(_opCtx.get(), *stateDocument.getCommitOrAbortOpTime());
    }

    ServerlessOperationLockRegistry::get(_opCtx->getServiceContext())
        .acquireLock(ServerlessOperationLockRegistry::LockType::kShardSplit, stateDocument.getId());

    auto bsonDoc = stateDocument.toBSON();

    WriteUnitOfWork wuow(_opCtx.get());
    AutoGetCollection autoColl(_opCtx.get(), NamespaceString::kShardSplitDonorsNamespace, MODE_IX);
    OplogDeleteEntryArgs deleteArgs;
    _observer->aboutToDelete(_opCtx.get(), *autoColl, bsonDoc, &deleteArgs);

    deleteArgs.deletedDoc = &bsonDoc;

    _observer->onDelete(_opCtx.get(), *autoColl, 0 /* stmtId */, deleteArgs);
    wuow.commit();

    // Verify blockers have been removed
    for (const auto& tenantId : _tenantIds) {
        ASSERT_FALSE(TenantMigrationAccessBlockerRegistry::get(_opCtx->getServiceContext())
                         .getTenantMigrationAccessBlockerForTenantId(
                             tenantId, TenantMigrationAccessBlocker::BlockerType::kDonor));
    }
}

}  // namespace
}  // namespace mongo
