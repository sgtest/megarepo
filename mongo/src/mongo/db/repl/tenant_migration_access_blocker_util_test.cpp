/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include <memory>
#include <utility>
#include <vector>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/oid.h"
#include "mongo/client/read_preference.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/create_collection.h"
#include "mongo/db/commands/create_gen.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/repl/read_concern_level.h"
#include "mongo/db/repl/repl_settings.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/storage_interface_impl.h"
#include "mongo/db/repl/tenant_migration_access_blocker.h"
#include "mongo/db/repl/tenant_migration_access_blocker_registry.h"
#include "mongo/db/repl/tenant_migration_access_blocker_util.h"
#include "mongo/db/repl/tenant_migration_conflict_info.h"
#include "mongo/db/repl/tenant_migration_donor_access_blocker.h"
#include "mongo/db/repl/tenant_migration_recipient_access_blocker.h"
#include "mongo/db/repl/tenant_migration_shard_merge_util.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/service_context.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/db/service_context_test_fixture.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/death_test.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"

namespace mongo {

namespace {
const Timestamp kDefaultStartMigrationTimestamp(1, 1);
static const std::string kDefaultDonorConnStr = "donor-rs/localhost:12345";
static const std::string kDefaultRecipientConnStr = "recipient-rs/localhost:56789";
static const std::string kDefaultEmptyTenantStr = "";
static const UUID kMigrationId = UUID::gen();

}  // namespace

class TenantMigrationAccessBlockerUtilTest : public ServiceContextTest {
public:
    const TenantId kTenantId = TenantId(OID::gen());
    const DatabaseName kTenantDB =
        DatabaseName::createDatabaseName_forTest(boost::none, kTenantId.toString() + "_ db");

    void setUp() override {
        _opCtx = makeOperationContext();
        auto service = getServiceContext();

        repl::ReplicationCoordinator::set(
            service, std::make_unique<repl::ReplicationCoordinatorMock>(service, _replSettings));

        TenantMigrationAccessBlockerRegistry::get(getServiceContext()).startup();
    }

    void tearDown() override {
        TenantMigrationAccessBlockerRegistry::get(getServiceContext()).shutDown();
    }

    OperationContext* opCtx() const {
        return _opCtx.get();
    }

private:
    ServiceContext::UniqueOperationContext _opCtx;
    const repl::ReplSettings _replSettings = repl::createServerlessReplSettings();
};


TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveTenantMigrationInitiallyFalse) {
    ASSERT_FALSE(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveTenantMigrationTrueWithDonor) {
    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, donorMtab);

    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveShardMergeTrueWithDonor) {
    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .addGlobalDonorAccessBlocker(donorMtab);
    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kLocal));
    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveTenantMigrationTrueWithRecipient) {
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);

    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveTenantMigrationTrueWithBoth) {
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);

    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, donorMtab);

    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveShardMergeTrueWithBoth) {
    auto uuid = UUID::gen();
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), uuid);
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);

    auto donorMtab = std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), uuid);
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .addGlobalDonorAccessBlocker(donorMtab);
    // Access blocker do not impact ns without tenants.
    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kConfig));
    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveTenantMigrationDonorFalseForNoDbName) {
    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, donorMtab);

    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kEmpty));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveShardMergeDonorFalseForNoDbName) {
    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .addGlobalDonorAccessBlocker(donorMtab);
    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kEmpty));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveShardMergeRecipientFalseForNoDbName) {
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);
    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kEmpty));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveTenantMigrationFalseForUnrelatedDb) {
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);

    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), UUID::gen());
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, donorMtab);

    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kConfig));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveTenantMigrationFalseAfterRemoveWithBoth) {
    auto recipientId = UUID::gen();
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), recipientId);
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);

    auto donorId = UUID::gen();
    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), donorId);
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, donorMtab);

    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));

    // Remove donor, should still be a migration.
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .removeAccessBlockersForMigration(donorId,
                                          TenantMigrationAccessBlocker::BlockerType::kDonor);
    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));

    // Remove recipient, there should be no migration.
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .removeAccessBlockersForMigration(recipientId,
                                          TenantMigrationAccessBlocker::BlockerType::kRecipient);
    ASSERT_FALSE(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, HasActiveShardMergeFalseAfterRemoveWithBoth) {
    auto migrationId = UUID::gen();
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), migrationId);
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);

    auto donorMtab =
        std::make_shared<TenantMigrationDonorAccessBlocker>(getServiceContext(), migrationId);
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .addGlobalDonorAccessBlocker(donorMtab);

    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kAdmin));

    // Remove donor, should still be a migration for the tenants migrating to the recipient.
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .removeAccessBlockersForMigration(migrationId,
                                          TenantMigrationAccessBlocker::BlockerType::kDonor);
    ASSERT(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kAdmin));

    // Remove recipient, there should be no migration.
    TenantMigrationAccessBlockerRegistry::get(getServiceContext())
        .removeAccessBlockersForMigration(migrationId,
                                          TenantMigrationAccessBlocker::BlockerType::kRecipient);
    ASSERT_FALSE(tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), kTenantDB));
    ASSERT_FALSE(
        tenant_migration_access_blocker::hasActiveTenantMigration(opCtx(), DatabaseName::kAdmin));
}

TEST_F(TenantMigrationAccessBlockerUtilTest, TestValidateNssBeingMigrated) {
    auto migrationId = UUID::gen();
    auto recipientMtab =
        std::make_shared<TenantMigrationRecipientAccessBlocker>(getServiceContext(), migrationId);
    TenantMigrationAccessBlockerRegistry::get(getServiceContext()).add(kTenantId, recipientMtab);

    // No tenantId should work for an adminDB.
    tenant_migration_access_blocker::validateNssIsBeingMigrated(
        boost::none,
        NamespaceString::createNamespaceString_forTest(DatabaseName::kAdmin, "test"),
        UUID::gen());

    // No tenantId will throw if it's not an adminDB.
    ASSERT_THROWS_CODE(tenant_migration_access_blocker::validateNssIsBeingMigrated(
                           boost::none,
                           NamespaceString::createNamespaceString_forTest("foo", "test"),
                           migrationId),
                       DBException,
                       ErrorCodes::InvalidTenantId);

    // A different tenantId will throw.
    ASSERT_THROWS_CODE(tenant_migration_access_blocker::validateNssIsBeingMigrated(
                           TenantId(OID::gen()),
                           NamespaceString::createNamespaceString_forTest("foo", "test"),
                           migrationId),
                       DBException,
                       ErrorCodes::InvalidTenantId);

    // A different migrationId will throw.
    ASSERT_THROWS_CODE(
        tenant_migration_access_blocker::validateNssIsBeingMigrated(
            kTenantId, NamespaceString::createNamespaceString_forTest("foo", "test"), UUID::gen()),
        DBException,
        ErrorCodes::InvalidTenantId);

    // Finally everything works.
    tenant_migration_access_blocker::validateNssIsBeingMigrated(
        kTenantId,
        NamespaceString::createNamespaceString_forTest(DatabaseName::kAdmin, "test"),
        migrationId);
}

class RecoverAccessBlockerTest : public ServiceContextMongoDTest {
public:
    void setUp() override {
        ServiceContextMongoDTest::setUp();

        auto serviceContext = getServiceContext();
        // Need real (non-mock) storage to insert state doc.
        repl::StorageInterface::set(serviceContext, std::make_unique<repl::StorageInterfaceImpl>());

        auto replCoord =
            std::make_unique<repl::ReplicationCoordinatorMock>(serviceContext, _replSettings);
        ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_PRIMARY));
        _replMock = replCoord.get();
        repl::ReplicationCoordinator::set(serviceContext, std::move(replCoord));

        _opCtx = makeOperationContext();
        TenantMigrationAccessBlockerRegistry::get(getServiceContext()).startup();

        repl::createOplog(_opCtx.get());
    }

    void tearDown() override {
        TenantMigrationAccessBlockerRegistry::get(getServiceContext()).shutDown();
    }

    OperationContext* opCtx() const {
        return _opCtx.get();
    }

protected:
    void insertStateDocument(const NamespaceString& nss, const BSONObj& obj) {
        auto storage = repl::StorageInterface::get(opCtx());
        ASSERT_OK(storage->createCollection(opCtx(), nss, CollectionOptions()));
        ASSERT_OK(storage->putSingleton(opCtx(), nss, {obj, Timestamp(100, 1)}));
    }

    std::vector<TenantId> _tenantIds{TenantId{OID::gen()}, TenantId{OID::gen()}};
    repl::ReplicationCoordinatorMock* _replMock;

private:
    ServiceContext::UniqueOperationContext _opCtx;
    const repl::ReplSettings _replSettings = repl::createServerlessReplSettings();
};

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientBlockerStarted) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kStarted);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(mtab);
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_THROWS_CODE_AND_WHAT(
            cmdFuture.get(),
            DBException,
            ErrorCodes::IllegalOperation,
            "Tenant command 'dummyCmd' is not allowed before migration completes");
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientAbortedBeforeDataCopy) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kAborted);
    recipientDoc.setStartGarbageCollect(true);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(!mtab);
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientAbortedAfterDataCopy) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kAborted);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(mtab);
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_THROWS_CODE_AND_WHAT(
            cmdFuture.get(),
            DBException,
            ErrorCodes::IllegalOperation,
            "Tenant command 'dummyCmd' is not allowed before migration completes");
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientCommittedWithoutDataCopy) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kCommitted);
    recipientDoc.setStartGarbageCollect(true);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(!mtab);
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientCommittedAfterDataCopy) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kCommitted);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(mtab);
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_THROWS_CODE_AND_WHAT(
            cmdFuture.get(),
            DBException,
            ErrorCodes::IllegalOperation,
            "Tenant command 'dummyCmd' is not allowed before migration completes");
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientLearnedFiles) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kLearnedFilenames);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(mtab);
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_THROWS_CODE_AND_WHAT(
            cmdFuture.get(),
            DBException,
            ErrorCodes::IllegalOperation,
            "Tenant command 'dummyCmd' is not allowed before migration completes");
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientConsistent) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kConsistent);
    // Create the import done marker collection.
    ASSERT_OK(createCollection(
        opCtx(), CreateCommand(repl::shard_merge_utils::getImportDoneMarkerNs(kMigrationId))));

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(mtab);
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_THROWS_CODE_AND_WHAT(
            cmdFuture.get(),
            DBException,
            ErrorCodes::IllegalOperation,
            "Tenant command 'dummyCmd' is not allowed before migration completes");
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeRecipientRejectBeforeTimestamp) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kCommitted);
    recipientDoc.setRejectReadsBeforeTimestamp(Timestamp{20, 1});

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kRecipient);
        ASSERT(mtab);

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kMajorityReadConcern);
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_OK(cmdFuture.getNoThrow());

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kSnapshotReadConcern);
        repl::ReadConcernArgs::get(opCtx()).setArgsAtClusterTimeForSnapshot(Timestamp{15, 1});
        auto cmdFutureAtClusterTime = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFutureAtClusterTime.isReady());
        ASSERT_THROWS_CODE_AND_WHAT(
            cmdFutureAtClusterTime.get(),
            DBException,
            ErrorCodes::SnapshotTooOld,
            "Tenant command 'dummyCmd' is not allowed before migration completes");
    }
}

TEST_F(RecoverAccessBlockerTest, InitialSyncUsingSyncSourceRunningShardMergeImportAsserts) {
    ShardMergeRecipientDocument recipientDoc(UUID::gen(),
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kLearnedFilenames);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    // Simulate the node is in initial sync.
    ASSERT_OK(_replMock->setFollowerMode(repl::MemberState::RS_STARTUP2));

    ASSERT_THROWS_CODE_AND_WHAT(
        tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx()),
        DBException,
        ErrorCodes::TenantMigrationInProgress,
        "Illegal to run initial sync when shard merge is active");
}

TEST_F(RecoverAccessBlockerTest, SyncSourceCompletesShardMergeBeforeInitialSyncStart) {
    ShardMergeRecipientDocument recipientDoc(kMigrationId,
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kCommitted);
    recipientDoc.setExpireAt(opCtx()->getServiceContext()->getFastClockSource()->now());

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    // Simulate the node is in initial sync.
    ASSERT_OK(_replMock->setFollowerMode(repl::MemberState::RS_STARTUP2));

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());
}

DEATH_TEST_REGEX_F(RecoverAccessBlockerTest,
                   ShardMergeRecipientConsistentStateWithoutImportDoneMarkerCollectionFasserts,
                   "Fatal assertion.*7219902") {
    ShardMergeRecipientDocument recipientDoc(UUID::gen(),
                                             kDefaultDonorConnStr,
                                             _tenantIds,
                                             kDefaultStartMigrationTimestamp,
                                             ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    recipientDoc.setState(ShardMergeRecipientStateEnum::kConsistent);

    insertStateDocument(NamespaceString::kShardMergeRecipientsNamespace, recipientDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());
}

TEST_F(RecoverAccessBlockerTest, ShardMergeDonorAbortingIndex) {
    TenantMigrationDonorDocument donorDoc(
        kMigrationId,
        kDefaultRecipientConnStr,
        mongo::ReadPreferenceSetting(ReadPreference::PrimaryOnly));

    donorDoc.setProtocol(MigrationProtocolEnum::kShardMerge);
    donorDoc.setTenantIds(_tenantIds);
    donorDoc.setState(TenantMigrationDonorStateEnum::kAbortingIndexBuilds);

    insertStateDocument(NamespaceString::kTenantMigrationDonorsNamespace, donorDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kDonor);
        ASSERT(mtab);

        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_OK(cmdFuture.getNoThrow());

        ASSERT_OK(mtab->checkIfCanWrite(Timestamp{10, 1}));

        auto indexStatus = mtab->checkIfCanBuildIndex();
        ASSERT_EQ(indexStatus.code(), ErrorCodes::TenantMigrationConflict);
        auto migrationConflictInfo = indexStatus.extraInfo<TenantMigrationConflictInfo>();
        ASSERT_EQ(migrationConflictInfo->getMigrationId(), kMigrationId);
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeDonorBlocking) {
    TenantMigrationDonorDocument donorDoc(
        kMigrationId,
        kDefaultRecipientConnStr,
        mongo::ReadPreferenceSetting(ReadPreference::PrimaryOnly));

    donorDoc.setProtocol(MigrationProtocolEnum::kShardMerge);
    donorDoc.setTenantIds(_tenantIds);
    donorDoc.setState(TenantMigrationDonorStateEnum::kBlocking);
    donorDoc.setBlockTimestamp(Timestamp{100, 1});

    insertStateDocument(NamespaceString::kTenantMigrationDonorsNamespace, donorDoc.toBSON());

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kDonor);
        ASSERT(mtab);

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kMajorityReadConcern);
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_OK(cmdFuture.getNoThrow());

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kSnapshotReadConcern);
        repl::ReadConcernArgs::get(opCtx()).setArgsAtClusterTimeForSnapshot(Timestamp{101, 1});
        auto afterCmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_FALSE(afterCmdFuture.isReady());

        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp{101, 1}).code(),
                  ErrorCodes::TenantMigrationConflict);

        auto indexStatus = mtab->checkIfCanBuildIndex();
        ASSERT_EQ(indexStatus.code(), ErrorCodes::TenantMigrationConflict);
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeDonorCommitted) {
    TenantMigrationDonorDocument donorDoc(
        kMigrationId,
        kDefaultRecipientConnStr,
        mongo::ReadPreferenceSetting(ReadPreference::PrimaryOnly));

    donorDoc.setProtocol(MigrationProtocolEnum::kShardMerge);
    donorDoc.setTenantIds(_tenantIds);
    donorDoc.setState(TenantMigrationDonorStateEnum::kCommitted);
    donorDoc.setBlockTimestamp(Timestamp{100, 1});
    donorDoc.setCommitOrAbortOpTime(repl::OpTime{Timestamp{101, 1}, 2});

    insertStateDocument(NamespaceString::kTenantMigrationDonorsNamespace, donorDoc.toBSON());
    _replMock->setCurrentCommittedSnapshotOpTime(repl::OpTime{Timestamp{101, 1}, 2});

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kDonor);
        ASSERT(mtab);

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kSnapshotReadConcern);
        repl::ReadConcernArgs::get(opCtx()).setArgsAtClusterTimeForSnapshot(Timestamp{90, 1});
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_OK(cmdFuture.getNoThrow());

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kSnapshotReadConcern);
        repl::ReadConcernArgs::get(opCtx()).setArgsAtClusterTimeForSnapshot(Timestamp{102, 1});
        auto afterCmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(afterCmdFuture.isReady());
        ASSERT_EQ(afterCmdFuture.getNoThrow().code(), ErrorCodes::TenantMigrationCommitted);

        ASSERT_EQ(mtab->checkIfCanWrite(Timestamp{102, 1}).code(),
                  ErrorCodes::TenantMigrationCommitted);

        auto indexStatus = mtab->checkIfCanBuildIndex();
        ASSERT_EQ(indexStatus.code(), ErrorCodes::TenantMigrationCommitted);
    }
}

TEST_F(RecoverAccessBlockerTest, ShardMergeDonorAborted) {
    TenantMigrationDonorDocument donorDoc(
        kMigrationId,
        kDefaultRecipientConnStr,
        mongo::ReadPreferenceSetting(ReadPreference::PrimaryOnly));

    donorDoc.setProtocol(MigrationProtocolEnum::kShardMerge);
    donorDoc.setTenantIds(_tenantIds);
    donorDoc.setState(TenantMigrationDonorStateEnum::kAborted);
    donorDoc.setBlockTimestamp(Timestamp{100, 1});
    donorDoc.setCommitOrAbortOpTime(repl::OpTime{Timestamp{101, 1}, 2});

    insertStateDocument(NamespaceString::kTenantMigrationDonorsNamespace, donorDoc.toBSON());
    _replMock->setCurrentCommittedSnapshotOpTime(repl::OpTime{Timestamp{101, 1}, 2});

    tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx());

    for (const auto& tenantId : _tenantIds) {
        auto mtab = TenantMigrationAccessBlockerRegistry::get(getServiceContext())
                        .getTenantMigrationAccessBlockerForTenantId(
                            tenantId, TenantMigrationAccessBlocker::BlockerType::kDonor);
        ASSERT(mtab);

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kSnapshotReadConcern);
        repl::ReadConcernArgs::get(opCtx()).setArgsAtClusterTimeForSnapshot(Timestamp{90, 1});
        auto cmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(cmdFuture.isReady());
        ASSERT_OK(cmdFuture.getNoThrow());

        repl::ReadConcernArgs::get(opCtx()) =
            repl::ReadConcernArgs(repl::ReadConcernLevel::kSnapshotReadConcern);
        repl::ReadConcernArgs::get(opCtx()).setArgsAtClusterTimeForSnapshot(Timestamp{102, 1});
        auto afterCmdFuture = mtab->getCanRunCommandFuture(opCtx(), "dummyCmd");
        ASSERT_TRUE(afterCmdFuture.isReady());
        ASSERT_OK(afterCmdFuture.getNoThrow());

        ASSERT_OK(mtab->checkIfCanWrite(Timestamp{102, 1}));

        ASSERT_OK(mtab->checkIfCanBuildIndex());
    }
}

}  // namespace mongo
