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

#include <fstream>  // IWYU pragma: keep
#include <iterator>
#include <memory>
#include <string>

#include <boost/move/utility_core.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/read_preference.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db/client.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/storage_interface_impl.h"
#include "mongo/db/repl/tenant_migration_recipient_entry_helpers.h"
#include "mongo/db/repl/tenant_migration_state_machine_gen.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/net/ssl_util.h"
#include "mongo/util/time_support.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace repl {

using namespace tenantMigrationRecipientEntryHelpers;

const Timestamp kDefaultStartMigrationTimestamp(1, 1);

class TenantMigrationRecipientEntryHelpersTest : public ServiceContextMongoDTest {
public:
    void setUp() override {
        ServiceContextMongoDTest::setUp();
        auto serviceContext = getServiceContext();

        auto opCtx = cc().makeOperationContext();
        ReplicationCoordinator::set(serviceContext,
                                    std::make_unique<ReplicationCoordinatorMock>(serviceContext));
        StorageInterface::set(serviceContext, std::make_unique<StorageInterfaceImpl>());

        repl::createOplog(opCtx.get());

        // Step up the node.
        long long term = 1;
        auto replCoord = ReplicationCoordinator::get(getServiceContext());
        ASSERT_OK(replCoord->setFollowerMode(MemberState::RS_PRIMARY));
        ASSERT_OK(replCoord->updateTerm(opCtx.get(), term));
        replCoord->setMyLastAppliedOpTimeAndWallTimeForward(
            OpTimeAndWallTime(OpTime(Timestamp(1, 1), term), Date_t()));
    }

    void tearDown() override {
        ServiceContextMongoDTest::tearDown();
    }

protected:
    bool checkStateDocPersisted(OperationContext* opCtx,
                                const TenantMigrationRecipientDocument& stateDoc) {
        auto persistedStateDocWithStatus = getStateDoc(opCtx, stateDoc.getId());

        auto status = persistedStateDocWithStatus.getStatus();
        if (status == ErrorCodes::NoMatchingDocument) {
            return false;
        }
        ASSERT_OK(status);

        ASSERT_BSONOBJ_EQ(stateDoc.toBSON(), persistedStateDocWithStatus.getValue().toBSON());
        return true;
    }
};

#ifdef MONGO_CONFIG_SSL
TEST_F(TenantMigrationRecipientEntryHelpersTest, AddTenantMigrationRecipientStateDoc) {
    auto opCtx = cc().makeOperationContext();

    const UUID migrationUUID = UUID::gen();
    TenantMigrationRecipientDocument activeTenantAStateDoc(
        migrationUUID,
        "donor-rs0/localhost:12345",
        "tenantA",
        kDefaultStartMigrationTimestamp,
        ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    activeTenantAStateDoc.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    ASSERT_OK(insertStateDoc(opCtx.get(), activeTenantAStateDoc));
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), activeTenantAStateDoc));

    // Same migration uuid and same tenant id.
    TenantMigrationRecipientDocument stateDoc1(migrationUUID,
                                               "donor-rs1/localhost:12345",
                                               "tenantA",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc1.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    auto status = insertStateDoc(opCtx.get(), stateDoc1);
    ASSERT_EQUALS(ErrorCodes::ConflictingOperationInProgress, status.code());
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), activeTenantAStateDoc));

    // Same migration uuid and different tenant id.
    TenantMigrationRecipientDocument stateDoc2(migrationUUID,
                                               "donor-rs0/localhost:12345",
                                               "tenantB",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc2.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    ASSERT_THROWS_CODE(
        insertStateDoc(opCtx.get(), stateDoc2), DBException, ErrorCodes::DuplicateKey);
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), activeTenantAStateDoc));

    // Different migration uuid and same tenant id.
    TenantMigrationRecipientDocument stateDoc3(UUID::gen(),
                                               "donor-rs0/localhost:12345",
                                               "tenantA",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc3.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    status = insertStateDoc(opCtx.get(), stateDoc3);
    ASSERT_EQUALS(ErrorCodes::ConflictingOperationInProgress, status.code());
    ASSERT_FALSE(checkStateDocPersisted(opCtx.get(), stateDoc3));

    // Different migration uuid and different tenant id.
    TenantMigrationRecipientDocument stateDoc4(UUID::gen(),
                                               "donor-rs0/localhost:12345",
                                               "tenantB",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc4.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    ASSERT_OK(insertStateDoc(opCtx.get(), stateDoc4));
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), stateDoc4));
}

TEST_F(TenantMigrationRecipientEntryHelpersTest,
       AddTenantMigrationRecipientStateDoc_GarbageCollect) {
    auto opCtx = cc().makeOperationContext();

    const UUID migrationUUID = UUID::gen();
    TenantMigrationRecipientDocument inactiveTenantAStateDoc(
        migrationUUID,
        "donor-rs0/localhost:12345",
        "tenantA",
        kDefaultStartMigrationTimestamp,
        ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    inactiveTenantAStateDoc.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    inactiveTenantAStateDoc.setExpireAt(Date_t::now());
    ASSERT_OK(insertStateDoc(opCtx.get(), inactiveTenantAStateDoc));
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), inactiveTenantAStateDoc));

    // Same migration uuid and same tenant id.
    TenantMigrationRecipientDocument stateDoc1(migrationUUID,
                                               "donor-rs1/localhost:12345",
                                               "tenantA",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc1.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    ASSERT_THROWS_CODE(
        insertStateDoc(opCtx.get(), stateDoc1), DBException, ErrorCodes::DuplicateKey);
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), inactiveTenantAStateDoc));

    // Same migration uuid and different tenant id.
    TenantMigrationRecipientDocument stateDoc2(migrationUUID,
                                               "donor-rs0/localhost:12345",
                                               "tenantB",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc2.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    ASSERT_THROWS_CODE(
        insertStateDoc(opCtx.get(), stateDoc2), DBException, ErrorCodes::DuplicateKey);
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), inactiveTenantAStateDoc));

    // Different migration uuid and same tenant id.
    TenantMigrationRecipientDocument stateDoc3(UUID::gen(),
                                               "donor-rs0/localhost:12345",
                                               "tenantA",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc3.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    ASSERT_OK(insertStateDoc(opCtx.get(), stateDoc3));
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), stateDoc3));

    // Different migration uuid and different tenant id.
    TenantMigrationRecipientDocument stateDoc4(UUID::gen(),
                                               "donor-rs0/localhost:12345",
                                               "tenantC",
                                               kDefaultStartMigrationTimestamp,
                                               ReadPreferenceSetting(ReadPreference::PrimaryOnly));
    stateDoc4.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);
    ASSERT_OK(insertStateDoc(opCtx.get(), stateDoc4));
    ASSERT_TRUE(checkStateDocPersisted(opCtx.get(), stateDoc4));
}
#endif

}  // namespace repl
}  // namespace mongo
