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

#include <utility>

#include <boost/move/utility_core.hpp>

#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/client.h"
#include "mongo/db/repl/primary_only_service_test_fixture.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/storage_interface_mock.h"
#include "mongo/db/s/config/configsvr_coordinator.h"
#include "mongo/db/s/config/configsvr_coordinator_service.h"
#include "mongo/db/s/config/set_cluster_parameter_coordinator_document_gen.h"
#include "mongo/db/s/config/set_user_write_block_mode_coordinator_document_gen.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future.h"


namespace mongo {
namespace {

class ConfigsvrCoordinatorServiceTest : public repl::PrimaryOnlyServiceMongoDTest {

public:
    std::unique_ptr<repl::PrimaryOnlyService> makeService(ServiceContext* serviceContext) override {
        return std::make_unique<ConfigsvrCoordinatorService>(serviceContext);
    }

    void setUp() override {
        repl::PrimaryOnlyServiceMongoDTest::setUp();

        auto serviceContext = getServiceContext();
        auto storageMock = std::make_unique<repl::StorageInterfaceMock>();
        repl::StorageInterface::set(serviceContext, std::move(storageMock));
    }

    void tearDown() override {
        _service->shutdown();
        repl::PrimaryOnlyServiceMongoDTest::tearDown();
    }
};

TEST_F(ConfigsvrCoordinatorServiceTest, CoordinatorsOfSameTypeCanExist) {
    auto opCtx = cc().makeOperationContext();

    auto* service = dynamic_cast<ConfigsvrCoordinatorService*>(_service);

    std::vector<std::shared_ptr<ConfigsvrCoordinator>> instances;
    {
        // Ensure that the new coordinators we create won't actually run.
        FailPointEnableBlock fp("hangAndEndBeforeRunningConfigsvrCoordinatorInstance");

        SetClusterParameterCoordinatorDocument coordinatorDoc;
        ConfigsvrCoordinatorId cid(ConfigsvrCoordinatorTypeEnum::kSetClusterParameter);
        cid.setSubId("0"_sd);
        coordinatorDoc.setConfigsvrCoordinatorMetadata({cid});
        coordinatorDoc.setParameter(BSON("a" << 1));

        SetClusterParameterCoordinatorDocument coordinatorDocSameSubId;
        coordinatorDocSameSubId.setConfigsvrCoordinatorMetadata({cid});
        coordinatorDocSameSubId.setParameter(BSON("b" << 2));

        SetClusterParameterCoordinatorDocument coordinatorDocDiffSubId;
        ConfigsvrCoordinatorId cid1(ConfigsvrCoordinatorTypeEnum::kSetClusterParameter);
        cid1.setSubId("1"_sd);
        coordinatorDocDiffSubId.setConfigsvrCoordinatorMetadata({cid1});
        coordinatorDocDiffSubId.setParameter(BSON("a" << 1));

        SetUserWriteBlockModeCoordinatorDocument coordinatorDocDiffType;
        ConfigsvrCoordinatorId cid2(ConfigsvrCoordinatorTypeEnum::kSetUserWriteBlockMode);
        cid2.setSubId("0"_sd);
        coordinatorDocDiffType.setConfigsvrCoordinatorMetadata({cid2});
        coordinatorDocDiffType.setBlock(true);

        // Trying to create a second coordinator with exact same fields will just get current
        // coordinator.
        auto coord1 = service->getOrCreateService(opCtx.get(), coordinatorDoc.toBSON());
        auto coord1_copy = service->getOrCreateService(opCtx.get(), coordinatorDoc.toBSON());
        ASSERT(coord1);
        // Note that this is pointer equality, so there is only one real instance.
        ASSERT_EQUALS(coord1, coord1_copy);

        // Trying to create a second coordinator with same type and subId but different fields will
        // fail due to conflict.
        ASSERT_THROWS(service->getOrCreateService(opCtx.get(), coordinatorDocSameSubId.toBSON()),
                      AssertionException);

        // We can create a second coordinator of the same type but different subId.
        auto coord2 = service->getOrCreateService(opCtx.get(), coordinatorDocDiffSubId.toBSON());
        ASSERT(coord2);
        ASSERT_NOT_EQUALS(coord1, coord2);

        // We can create a coordinator with different type and same (or different) subId.
        auto coord3 = service->getOrCreateService(opCtx.get(), coordinatorDocDiffType.toBSON());
        ASSERT(coord3);
        ASSERT_NOT_EQUALS(coord1, coord3);
        ASSERT_NOT_EQUALS(coord2, coord3);

        // Ensure all instances start before we disable the failpoint.
        fp->waitForTimesEntered(fp.initialTimesEntered() + 5);
        instances = {coord1, coord2, coord3};
    }

    for (const auto& instance : instances) {
        instance->getCompletionFuture().wait();
    }
}

}  // namespace
}  // namespace mongo
