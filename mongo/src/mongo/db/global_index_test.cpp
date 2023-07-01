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

#include "mongo/db/global_index.h"

#include <memory>
#include <utility>
#include <vector>

#include <boost/move/utility_core.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/client.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/internal_plans.h"
#include "mongo/db/query/plan_executor.h"
#include "mongo/db/query/plan_yield_policy.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/storage_interface_impl.h"
#include "mongo/db/service_context.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/db/storage/key_string.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/bufreader.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kIndex

namespace mongo {
namespace {

class GlobalIndexTest : public ServiceContextMongoDTest {
public:
    explicit GlobalIndexTest(Options options = {}) : ServiceContextMongoDTest(std::move(options)) {}

    OperationContext* operationContext() {
        return _opCtx.get();
    }
    repl::StorageInterface* storageInterface() {
        return repl::StorageInterface::get(getServiceContext());
    }

protected:
    void setUp() override {
        // Set up mongod.
        ServiceContextMongoDTest::setUp();

        auto service = getServiceContext();
        repl::StorageInterface::set(service, std::make_unique<repl::StorageInterfaceImpl>());

        _opCtx = cc().makeOperationContext();

        // Set up ReplicationCoordinator and ensure that we are primary.
        auto replCoord = std::make_unique<repl::ReplicationCoordinatorMock>(service);
        ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_PRIMARY));
        repl::ReplicationCoordinator::set(service, std::move(replCoord));

        // Set up oplog collection. If the WT storage engine is used, the oplog collection is
        // expected to exist when fetching the next opTime (LocalOplogInfo::getNextOpTimes) to use
        // for a write.
        repl::createOplog(operationContext());
    }

    void tearDown() override {
        _opCtx.reset();

        // Tear down mongod.
        ServiceContextMongoDTest::tearDown();
    }

private:
    ServiceContext::UniqueOperationContext _opCtx;
};

// Verify that the index key's KeyString and optional TypeBits stored in the 'indexEntry' object
// matches the BSON 'key'.
void verifyStoredKeyMatchesIndexKey(const BSONObj& key,
                                    const BSONObj& indexEntry,
                                    bool expectTypeBits = false) {
    // The index entry's ik field stores the BinData(KeyString(key)) and the index entry's
    // 'tb' field stores the BinData(TypeBits(key)). The 'tb' field is not present if there are
    // no TypeBits.

    auto entryIndexKeySize = indexEntry[global_index::kContainerIndexKeyFieldName].size();
    const auto entryIndexKeyBinData =
        indexEntry[global_index::kContainerIndexKeyFieldName].binData(entryIndexKeySize);

    const auto hasTypeBits =
        indexEntry.hasElement(global_index::kContainerIndexKeyTypeBitsFieldName);
    ASSERT_EQ(expectTypeBits, hasTypeBits);

    auto tb = key_string::TypeBits(key_string::Version::V1);
    if (hasTypeBits) {
        auto entryTypeBitsSize =
            indexEntry[global_index::kContainerIndexKeyTypeBitsFieldName].size();
        auto entryTypeBitsBinData =
            indexEntry[global_index::kContainerIndexKeyTypeBitsFieldName].binData(
                entryTypeBitsSize);
        auto entryTypeBitsReader = BufReader(entryTypeBitsBinData, entryTypeBitsSize);
        tb = key_string::TypeBits::fromBuffer(key_string::Version::V1, &entryTypeBitsReader);
        ASSERT(!tb.isAllZeros());
    }

    const auto rehydratedKey =
        key_string::toBson(entryIndexKeyBinData, entryIndexKeySize, key_string::ALL_ASCENDING, tb);

    ASSERT_BSONOBJ_EQ(rehydratedKey, key);
    LOGV2(6789401,
          "The rehydrated index key matches the inserted index key",
          "rehydrated"_attr = rehydratedKey,
          "original"_attr = key,
          "typeBitsPresent"_attr = hasTypeBits);
}

TEST_F(GlobalIndexTest, StorageFormat) {
    const auto uuid = UUID::gen();

    global_index::createContainer(operationContext(), uuid);

    // Single field index.
    {
        const auto key = BSON(""
                              << "hola");
        const auto docKey = BSON("shk0" << 0 << "shk1" << 0 << "_id" << 0);
        const auto entryId = BSON(global_index::kContainerIndexDocKeyFieldName << docKey);
        global_index::insertKey(operationContext(), uuid, key, docKey);

        // Validate that the document key is stored in the index entry's _id field.
        StatusWith<BSONObj> status =
            storageInterface()->findById(operationContext(),
                                         NamespaceString::makeGlobalIndexNSS(uuid),
                                         entryId[global_index::kContainerIndexDocKeyFieldName]);
        ASSERT_OK(status.getStatus());
        const auto indexEntry = status.getValue();

        // Validate the index key.
        verifyStoredKeyMatchesIndexKey(key, indexEntry);
    }

    // Compound index.
    {
        const auto key = BSON(""
                              << "hola"
                              << "" << 1);
        const auto docKey = BSON("shk0" << 1 << "shk1" << 1 << "_id" << 1);
        const auto entryId = BSON(global_index::kContainerIndexDocKeyFieldName << docKey);
        global_index::insertKey(operationContext(), uuid, key, docKey);

        // Validate that the document key is stored in the index entry's _id field.
        StatusWith<BSONObj> status =
            storageInterface()->findById(operationContext(),
                                         NamespaceString::makeGlobalIndexNSS(uuid),
                                         entryId[global_index::kContainerIndexDocKeyFieldName]);
        ASSERT_OK(status.getStatus());
        const auto indexEntry = status.getValue();

        // Validate the index key.
        verifyStoredKeyMatchesIndexKey(key, indexEntry);
    }

    // Compound index with non-empty TypeBits (NumberLong).
    {
        const auto key = BSON(""
                              << "hola"
                              << "" << 2LL);
        const auto docKey = BSON("shk0" << 2 << "shk1" << 2 << "_id" << 2);
        const auto entryId = BSON(global_index::kContainerIndexDocKeyFieldName << docKey);
        global_index::insertKey(operationContext(), uuid, key, docKey);

        // Validate that the document key is stored in the index entry's _id field.
        StatusWith<BSONObj> status =
            storageInterface()->findById(operationContext(),
                                         NamespaceString::makeGlobalIndexNSS(uuid),
                                         entryId[global_index::kContainerIndexDocKeyFieldName]);
        ASSERT_OK(status.getStatus());
        const auto indexEntry = status.getValue();

        // Validate the index key.
        verifyStoredKeyMatchesIndexKey(key, indexEntry, true /* expectTypeBits */);
    }

    // Compound index with non-empty TypeBits (Decimal).
    {
        const auto key = BSON(""
                              << "hola"
                              << "" << 3.0);
        const auto docKey = BSON("shk0" << 2 << "shk1" << 3 << "_id" << 3);
        const auto entryId = BSON(global_index::kContainerIndexDocKeyFieldName << docKey);
        global_index::insertKey(operationContext(), uuid, key, docKey);

        // Validate that the document key is stored in the index entry's _id field.
        StatusWith<BSONObj> status =
            storageInterface()->findById(operationContext(),
                                         NamespaceString::makeGlobalIndexNSS(uuid),
                                         entryId[global_index::kContainerIndexDocKeyFieldName]);
        ASSERT_OK(status.getStatus());
        const auto indexEntry = status.getValue();

        // Validate the index key.
        verifyStoredKeyMatchesIndexKey(key, indexEntry, true /* expectTypeBits */);
    }
}

TEST_F(GlobalIndexTest, DuplicateKey) {
    const auto uuid = UUID::gen();
    global_index::createContainer(operationContext(), uuid);
    global_index::insertKey(
        operationContext(), uuid, BSON("" << 1), BSON("shk0" << 1 << "_id" << 1));

    // Duplicate index key.
    ASSERT_THROWS_CODE(
        global_index::insertKey(
            operationContext(), uuid, BSON("" << 1), BSON("shk0" << 123 << "_id" << 123)),
        DBException,
        ErrorCodes::DuplicateKey);
    // Duplicate index key - Decimal.
    ASSERT_THROWS_CODE(
        global_index::insertKey(
            operationContext(), uuid, BSON("" << 1.0), BSON("shk0" << 123 << "_id" << 123)),
        DBException,
        ErrorCodes::DuplicateKey);
    // Duplicate index key - NumberLong.
    ASSERT_THROWS_CODE(
        global_index::insertKey(
            operationContext(), uuid, BSON("" << 1LL), BSON("shk0" << 123 << "_id" << 123)),
        DBException,
        ErrorCodes::DuplicateKey);
}

TEST_F(GlobalIndexTest, DuplicateDocumentKey) {
    const auto uuid = UUID::gen();
    global_index::createContainer(operationContext(), uuid);
    global_index::insertKey(
        operationContext(), uuid, BSON("" << 1), BSON("shk0" << 1 << "_id" << 1));

    // Duplicate document key.
    ASSERT_THROWS_CODE(
        global_index::insertKey(
            operationContext(), uuid, BSON("" << 2), BSON("shk0" << 1 << "_id" << 1)),
        DBException,
        ErrorCodes::DuplicateKey);
    // Duplicate document key - NumberLong.
    ASSERT_THROWS_CODE(
        global_index::insertKey(
            operationContext(), uuid, BSON("" << 2), BSON("shk0" << 1LL << "_id" << 1)),
        DBException,
        ErrorCodes::DuplicateKey);
}

TEST_F(GlobalIndexTest, DeleteKey) {
    const auto uuid = UUID::gen();

    global_index::createContainer(operationContext(), uuid);

    const auto insertAndVerifyDelete =
        [this](const UUID& uuid, const BSONObj& key, const BSONObj& docKey) {
            const auto entryId = BSON(global_index::kContainerIndexDocKeyFieldName << docKey);
            const auto nss = NamespaceString::makeGlobalIndexNSS(uuid);

            // Inserts already tested in StorageFormat case.
            global_index::insertKey(operationContext(), uuid, key, docKey);

            // Delete and validate that the key is not found.
            global_index::deleteKey(operationContext(), uuid, key, docKey);
            ASSERT_NOT_OK(storageInterface()->findById(
                operationContext(), nss, entryId[global_index::kContainerIndexDocKeyFieldName]));
        };

    const auto docKey = BSON("shk0" << 0 << "shk1" << 0 << "_id" << 0);

    // Single field index.
    {
        const auto key = BSON(""
                              << "hola");
        insertAndVerifyDelete(uuid, key, docKey);
    }

    // Compound index.
    {
        const auto key = BSON(""
                              << "hola"
                              << "" << 1);
        insertAndVerifyDelete(uuid, key, docKey);
    }

    // Compound index with non-empty TypeBits (NumberLong).
    {
        const auto key = BSON(""
                              << "hola"
                              << "" << 2LL);
        insertAndVerifyDelete(uuid, key, docKey);
    }

    // Compound index with non-empty TypeBits (Decimal).
    {
        const auto key = BSON(""
                              << "hola"
                              << "" << 3.0);
        insertAndVerifyDelete(uuid, key, docKey);
    }
}

TEST_F(GlobalIndexTest, DeleteNonExistingKeyThrows) {
    const auto uuid = UUID::gen();
    global_index::createContainer(operationContext(), uuid);

    auto key = BSON(""
                    << "hola");
    auto docKey = BSON("shk0" << 0 << "shk1" << 0 << "_id" << 0);
    ASSERT_THROWS_CODE(global_index::deleteKey(operationContext(), uuid, key, docKey),
                       DBException,
                       ErrorCodes::KeyNotFound);
}

/**
 * Check collection contents.
 */
void _assertDocumentsInGlobalIndexById(OperationContext* opCtx,
                                       const UUID& uuid,
                                       const std::vector<BSONObj>& ids) {

    AutoGetCollectionForRead collToScan(opCtx, NamespaceString::makeGlobalIndexNSS(uuid));
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> exec =
        InternalPlanner::collectionScan(opCtx,
                                        &collToScan.getCollection(),
                                        PlanYieldPolicy::YieldPolicy::NO_YIELD,
                                        InternalPlanner::FORWARD);

    BSONObj obj;
    for (auto& id : ids) {
        ASSERT_EQUALS(exec->getNext(&obj, nullptr), PlanExecutor::ADVANCED);
        ASSERT_BSONOBJ_EQ(id, obj.getObjectField(global_index::kContainerIndexDocKeyFieldName));
    }
    ASSERT_EQUALS(exec->getNext(&obj, nullptr), PlanExecutor::IS_EOF);
}

TEST_F(GlobalIndexTest, DeleteIndexLookup) {
    const auto uuid = UUID::gen();

    global_index::createContainer(operationContext(), uuid);

    global_index::insertKey(
        operationContext(), uuid, BSON("" << 0), BSON("shk0" << 0 << "_id" << 0));
    global_index::insertKey(
        operationContext(), uuid, BSON("" << 1), BSON("shk0" << 0 << "_id" << 1));
    global_index::insertKey(
        operationContext(), uuid, BSON("" << 2), BSON("shk0" << 0 << "_id" << 2));
    global_index::insertKey(
        operationContext(), uuid, BSON("" << 3), BSON("shk0" << 0 << "_id" << 3));

    global_index::deleteKey(
        operationContext(), uuid, BSON("" << 3), BSON("shk0" << 0 << "_id" << 3));
    _assertDocumentsInGlobalIndexById(operationContext(),
                                      uuid,
                                      {BSON("shk0" << 0 << "_id" << 0),
                                       BSON("shk0" << 0 << "_id" << 1),
                                       BSON("shk0" << 0 << "_id" << 2)});

    global_index::deleteKey(
        operationContext(), uuid, BSON("" << 1), BSON("shk0" << 0 << "_id" << 1));
    _assertDocumentsInGlobalIndexById(
        operationContext(),
        uuid,
        {BSON("shk0" << 0 << "_id" << 0), BSON("shk0" << 0 << "_id" << 2)});

    global_index::deleteKey(
        operationContext(), uuid, BSON("" << 0), BSON("shk0" << 0 << "_id" << 0));
    _assertDocumentsInGlobalIndexById(operationContext(), uuid, {BSON("shk0" << 0 << "_id" << 2)});
}

}  // namespace
}  // namespace mongo
