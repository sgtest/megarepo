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


// IWYU pragma: no_include "cxxabi.h"
#include <string>
#include <system_error>

#include "mongo/base/error_codes.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/commands.h"
#include "mongo/db/s/shard_server_test_fixture.h"
#include "mongo/db/s/sharding_logging.h"
#include "mongo/executor/network_interface_mock.h"
#include "mongo/executor/network_test_env.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/text.h"  // IWYU pragma: keep

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding


namespace mongo {
namespace {

using unittest::assertGet;

class InfoLoggingTest : public ShardServerTestFixture {
public:
    enum CollType { ActionLog, ChangeLog };

    InfoLoggingTest(CollType configCollType, int cappedSize)
        : _configCollType(configCollType), _cappedSize(cappedSize) {}

protected:
    void noRetryAfterSuccessfulCreate() {
        auto future = launchAsync([this] {
            log("moved a chunk", "foo.bar", BSON("min" << 3 << "max" << 4)).transitional_ignore();
        });

        expectConfigCollectionCreate(
            kConfigHostAndPort, getConfigCollName(), _cappedSize, BSON("ok" << 1));
        expectConfigCollectionInsert(kConfigHostAndPort,
                                     getConfigCollName(),
                                     network()->now(),
                                     "moved a chunk",
                                     "foo.bar",
                                     BSON("min" << 3 << "max" << 4));

        // Now wait for the logChange call to return
        future.default_timed_get();

        // Now log another change and confirm that we don't re-attempt to create the collection
        future = launchAsync([this] {
            log("moved a second chunk", "foo.bar", BSON("min" << 4 << "max" << 5))
                .transitional_ignore();
        });

        expectConfigCollectionInsert(kConfigHostAndPort,
                                     getConfigCollName(),
                                     network()->now(),
                                     "moved a second chunk",
                                     "foo.bar",
                                     BSON("min" << 4 << "max" << 5));

        // Now wait for the logChange call to return
        future.default_timed_get();
    }

    void noRetryCreateIfAlreadyExists() {
        auto future = launchAsync([this] {
            log("moved a chunk", "foo.bar", BSON("min" << 3 << "max" << 4)).transitional_ignore();
        });

        BSONObjBuilder createResponseBuilder;
        CommandHelpers::appendCommandStatusNoThrow(
            createResponseBuilder, Status(ErrorCodes::NamespaceExists, "coll already exists"));
        expectConfigCollectionCreate(
            kConfigHostAndPort, getConfigCollName(), _cappedSize, createResponseBuilder.obj());
        expectConfigCollectionInsert(kConfigHostAndPort,
                                     getConfigCollName(),
                                     network()->now(),
                                     "moved a chunk",
                                     "foo.bar",
                                     BSON("min" << 3 << "max" << 4));

        // Now wait for the logAction call to return
        future.default_timed_get();

        // Now log another change and confirm that we don't re-attempt to create the collection
        future = launchAsync([this] {
            log("moved a second chunk", "foo.bar", BSON("min" << 4 << "max" << 5))
                .transitional_ignore();
        });

        expectConfigCollectionInsert(kConfigHostAndPort,
                                     getConfigCollName(),
                                     network()->now(),
                                     "moved a second chunk",
                                     "foo.bar",
                                     BSON("min" << 4 << "max" << 5));

        // Now wait for the logChange call to return
        future.default_timed_get();
    }

    void createFailure() {
        auto future = launchAsync([this] {
            log("moved a chunk", "foo.bar", BSON("min" << 3 << "max" << 4)).transitional_ignore();
        });

        BSONObjBuilder createResponseBuilder;
        CommandHelpers::appendCommandStatusNoThrow(
            createResponseBuilder, Status(ErrorCodes::Interrupted, "operation interrupted"));
        expectConfigCollectionCreate(
            kConfigHostAndPort, getConfigCollName(), _cappedSize, createResponseBuilder.obj());

        // Now wait for the logAction call to return
        future.default_timed_get();

        // Now log another change and confirm that we *do* attempt to create the collection
        future = launchAsync([this] {
            log("moved a second chunk", "foo.bar", BSON("min" << 4 << "max" << 5))
                .transitional_ignore();
        });

        expectConfigCollectionCreate(
            kConfigHostAndPort, getConfigCollName(), _cappedSize, BSON("ok" << 1));
        expectConfigCollectionInsert(kConfigHostAndPort,
                                     getConfigCollName(),
                                     network()->now(),
                                     "moved a second chunk",
                                     "foo.bar",
                                     BSON("min" << 4 << "max" << 5));

        // Now wait for the logChange call to return
        future.default_timed_get();
    }

    std::string getConfigCollName() const {
        return (_configCollType == ChangeLog ? "changelog" : "actionlog");
    }

    Status log(const std::string& what, const std::string& ns, const BSONObj& detail) {
        if (_configCollType == ChangeLog) {
            return ShardingLogging::get(operationContext())
                ->logChangeChecked(operationContext(),
                                   what,
                                   ns,
                                   detail,
                                   ShardingCatalogClient::kMajorityWriteConcern);
        } else {
            return ShardingLogging::get(operationContext())
                ->logAction(operationContext(), what, ns, detail);
        }
    }

    const CollType _configCollType;
    const int _cappedSize;
};

class ActionLogTest : public InfoLoggingTest {
public:
    ActionLogTest() : InfoLoggingTest(ActionLog, 20 * 1024 * 1024) {}
};

class ChangeLogTest : public InfoLoggingTest {
public:
    ChangeLogTest() : InfoLoggingTest(ChangeLog, 200 * 1024 * 1024) {}
};

TEST_F(ActionLogTest, NoRetryAfterSuccessfulCreate) {
    noRetryAfterSuccessfulCreate();
}
TEST_F(ChangeLogTest, NoRetryAfterSuccessfulCreate) {
    noRetryAfterSuccessfulCreate();
}

TEST_F(ActionLogTest, NoRetryCreateIfAlreadyExists) {
    noRetryCreateIfAlreadyExists();
}
TEST_F(ChangeLogTest, NoRetryCreateIfAlreadyExists) {
    noRetryCreateIfAlreadyExists();
}

TEST_F(ActionLogTest, CreateFailure) {
    createFailure();
}
TEST_F(ChangeLogTest, CreateFailure) {
    createFailure();
}

}  // namespace
}  // namespace mongo
