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

#include <fstream>  // IWYU pragma: keep
#include <iterator>
#include <memory>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/status_with.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db/client.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/op_observer/op_observer_impl.h"
#include "mongo/db/op_observer/op_observer_registry.h"
#include "mongo/db/op_observer/operation_logger_mock.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/primary_only_service.h"
#include "mongo/db/repl/primary_only_service_op_observer.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/tenant_migration_donor_service.h"
#include "mongo/db/repl/tenant_migration_state_machine_gen.h"
#include "mongo/db/repl/wait_for_majority_service.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/clock_source_mock.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/net/ssl_options.h"
#include "mongo/util/net/ssl_util.h"

namespace mongo {
namespace repl {

#ifdef MONGO_CONFIG_SSL

class TenantMigrationDonorServiceTest : public ServiceContextMongoDTest {
    void setUp() override {
        ServiceContextMongoDTest::setUp();
        auto serviceContext = getServiceContext();

        WaitForMajorityService::get(getServiceContext()).startup(getServiceContext());

        {
            auto opCtx = cc().makeOperationContext();
            auto replCoord = std::make_unique<ReplicationCoordinatorMock>(serviceContext);
            ReplicationCoordinator::set(serviceContext, std::move(replCoord));

            repl::createOplog(opCtx.get());
            // Set up OpObserver so that repl::logOp() will store the oplog entry's optime in
            // ReplClientInfo.
            OpObserverRegistry* opObserverRegistry =
                dynamic_cast<OpObserverRegistry*>(serviceContext->getOpObserver());
            opObserverRegistry->addObserver(
                std::make_unique<OpObserverImpl>(std::make_unique<OperationLoggerMock>()));
            opObserverRegistry->addObserver(
                std::make_unique<PrimaryOnlyServiceOpObserver>(serviceContext));

            _registry = repl::PrimaryOnlyServiceRegistry::get(getServiceContext());
            std::unique_ptr<TenantMigrationDonorService> service =
                std::make_unique<TenantMigrationDonorService>(getServiceContext());
            _registry->registerService(std::move(service));
            _registry->onStartup(opCtx.get());
        }
        stepUp();

        _service = _registry->lookupServiceByName(TenantMigrationDonorService::kServiceName);
        ASSERT(_service);

        // Set the sslMode to allowSSL to avoid validation error.
        sslGlobalParams.sslMode.store(SSLParams::SSLMode_allowSSL);
    }

    void tearDown() override {
        // Unset the sslMode.
        sslGlobalParams.sslMode.store(SSLParams::SSLMode_disabled);

        WaitForMajorityService::get(getServiceContext()).shutDown();

        _registry->onShutdown();
        _service = nullptr;

        ServiceContextMongoDTest::tearDown();
    }

    void stepDown() {
        ASSERT_OK(ReplicationCoordinator::get(getServiceContext())
                      ->setFollowerMode(MemberState::RS_SECONDARY));
        _registry->onStepDown();
    }

    void stepUp() {
        auto opCtx = cc().makeOperationContext();
        auto replCoord = ReplicationCoordinator::get(getServiceContext());

        // Advance term
        _term++;

        ASSERT_OK(replCoord->setFollowerMode(MemberState::RS_PRIMARY));
        ASSERT_OK(replCoord->updateTerm(opCtx.get(), _term));
        replCoord->setMyLastAppliedOpTimeAndWallTimeForward(
            OpTimeAndWallTime(OpTime(Timestamp(1, 1), _term), Date_t()));

        _registry->onStepUpComplete(opCtx.get(), _term);
    }

protected:
    TenantMigrationDonorServiceTest() : ServiceContextMongoDTest(Options{}.useMockClock(true)) {}

    PrimaryOnlyServiceRegistry* _registry;
    PrimaryOnlyService* _service;
    ClockSourceMock _clkSource;
    long long _term = 0;
};

TEST_F(TenantMigrationDonorServiceTest, CheckSettingMigrationStartDate) {
    // Advance the clock by some arbitrary amount of time so we are not starting at 0 seconds.
    _clkSource.advance(Milliseconds(10000));

    auto taskFp =
        globalFailPointRegistry().find("pauseTenantMigrationAfterPersistingInitialDonorStateDoc");
    auto initialTimesEntered = taskFp->setMode(FailPoint::alwaysOn);

    const UUID migrationUUID = UUID::gen();
    const TenantId kTenantId = TenantId(OID::gen());
    TenantMigrationDonorDocument initialStateDocument(
        migrationUUID,
        "donor-rs/localhost:12345",
        ReadPreferenceSetting(ReadPreference::PrimaryOnly, TagSet::primaryOnly()));
    initialStateDocument.setTenantId(boost::make_optional<StringData>(kTenantId.toString()));
    initialStateDocument.setProtocol(MigrationProtocolEnum::kMultitenantMigrations);

    // Create and start the instance.
    auto opCtx = makeOperationContext();
    auto instance = TenantMigrationDonorService::Instance::getOrCreate(
        opCtx.get(), _service, initialStateDocument.toBSON());
    ASSERT(instance.get());

    taskFp->waitForTimesEntered(initialTimesEntered + 1);

    auto currOpObj =
        instance->reportForCurrentOp(MongoProcessInterface::CurrentOpConnectionsMode::kExcludeIdle,
                                     MongoProcessInterface::CurrentOpSessionsMode::kExcludeIdle);
    ASSERT_EQ(currOpObj->getField("migrationStart").Date(),
              getServiceContext()->getFastClockSource()->now());

    taskFp->setMode(FailPoint::off);
}

#else

TEST(TenantMigrationServiceNoSSL, NoopTestCaseForNosslVariant) {
    // Without this test case, running this test binary on nossl variant will fail with a "no suites
    // registered." error.
}

#endif
}  // namespace repl
}  // namespace mongo
