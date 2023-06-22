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

#include <fmt/format.h>
#include <memory>
#include <string>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/crypto/encryption_fields_gen.h"
#include "mongo/db/catalog/clustered_collection_options_gen.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/database.h"
#include "mongo/db/catalog/index_build_block.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/unique_collection_name.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/op_observer/op_observer_impl.h"
#include "mongo/db/op_observer/op_observer_registry.h"
#include "mongo/db/op_observer/oplog_writer_mock.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/drop_pending_collection_reaper.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/storage_interface_mock.h"
#include "mongo/db/service_context.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/timeseries/timeseries_gen.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/pcre.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/time_support.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace {

class DatabaseTest : public ServiceContextMongoDTest {
private:
    void setUp() override;
    void tearDown() override;

protected:
    explicit DatabaseTest(Options options = {}) : ServiceContextMongoDTest(std::move(options)) {}

    ServiceContext::UniqueOperationContext _opCtx;
    NamespaceString _nss;
};

void DatabaseTest::setUp() {
    // Set up mongod.
    ServiceContextMongoDTest::setUp();

    auto service = getServiceContext();
    _opCtx = cc().makeOperationContext();

    repl::StorageInterface::set(service, std::make_unique<repl::StorageInterfaceMock>());
    repl::DropPendingCollectionReaper::set(
        service,
        std::make_unique<repl::DropPendingCollectionReaper>(repl::StorageInterface::get(service)));

    // Set up ReplicationCoordinator and create oplog.
    repl::ReplicationCoordinator::set(service,
                                      std::make_unique<repl::ReplicationCoordinatorMock>(service));
    repl::createOplog(_opCtx.get());

    // Ensure that we are primary.
    auto replCoord = repl::ReplicationCoordinator::get(_opCtx.get());
    ASSERT_OK(replCoord->setFollowerMode(repl::MemberState::RS_PRIMARY));

    // Set up OpObserver so that Database will append actual oplog entries to the oplog using
    // repl::logOp(). repl::logOp() will also store the oplog entry's optime in ReplClientInfo.
    OpObserverRegistry* opObserverRegistry =
        dynamic_cast<OpObserverRegistry*>(service->getOpObserver());
    opObserverRegistry->addObserver(
        std::make_unique<OpObserverImpl>(std::make_unique<OplogWriterMock>()));

    _nss = NamespaceString::createNamespaceString_forTest("test.foo");
}

void DatabaseTest::tearDown() {
    _nss = {};
    _opCtx = {};

    auto service = getServiceContext();
    repl::DropPendingCollectionReaper::set(service, {});
    repl::StorageInterface::set(service, {});

    ServiceContextMongoDTest::tearDown();
}

TEST_F(DatabaseTest, SetDropPendingThrowsExceptionIfDatabaseIsAlreadyInADropPendingState) {
    writeConflictRetry(_opCtx.get(), "testSetDropPending", _nss, [this] {
        AutoGetDb autoDb(_opCtx.get(), _nss.dbName(), MODE_X);
        auto db = autoDb.ensureDbExists(_opCtx.get());
        ASSERT_TRUE(db);

        ASSERT_FALSE(db->isDropPending(_opCtx.get()));
        db->setDropPending(_opCtx.get(), true);
        ASSERT_TRUE(db->isDropPending(_opCtx.get()));

        db->setDropPending(_opCtx.get(), true);
        ASSERT_TRUE(db->isDropPending(_opCtx.get()));

        db->setDropPending(_opCtx.get(), false);
        ASSERT_FALSE(db->isDropPending(_opCtx.get()));

        // It's fine to reset 'dropPending' multiple times.
        db->setDropPending(_opCtx.get(), false);
        ASSERT_FALSE(db->isDropPending(_opCtx.get()));
    });
}

TEST_F(DatabaseTest, CreateCollectionThrowsExceptionWhenDatabaseIsInADropPendingState) {
    writeConflictRetry(
        _opCtx.get(), "testÇreateCollectionWhenDatabaseIsInADropPendingState", _nss, [this] {
            AutoGetDb autoDb(_opCtx.get(), _nss.dbName(), MODE_X);
            auto db = autoDb.ensureDbExists(_opCtx.get());
            ASSERT_TRUE(db);

            db->setDropPending(_opCtx.get(), true);

            WriteUnitOfWork wuow(_opCtx.get());

            ASSERT_THROWS_CODE_AND_WHAT(
                db->createCollection(_opCtx.get(), _nss),
                AssertionException,
                ErrorCodes::DatabaseDropPending,
                (StringBuilder() << "Cannot create collection " << _nss.toStringForErrorMsg()
                                 << " - database is in the process of being dropped.")
                    .stringData());
        });
}

void _testDropCollection(OperationContext* opCtx,
                         const NamespaceString& nss,
                         bool createCollectionBeforeDrop,
                         const repl::OpTime& dropOpTime = {},
                         const CollectionOptions& collOpts = {}) {
    if (createCollectionBeforeDrop) {
        writeConflictRetry(opCtx, "testDropCollection", nss, [=] {
            WriteUnitOfWork wuow(opCtx);
            AutoGetDb autoDb(opCtx, nss.dbName(), MODE_X);
            auto db = autoDb.ensureDbExists(opCtx);
            ASSERT_TRUE(db);
            ASSERT_TRUE(db->createCollection(opCtx, nss, collOpts));
            wuow.commit();
        });
    }

    writeConflictRetry(opCtx, "testDropCollection", nss, [=] {
        AutoGetDb autoDb(opCtx, nss.dbName(), MODE_X);
        auto db = autoDb.ensureDbExists(opCtx);
        ASSERT_TRUE(db);

        WriteUnitOfWork wuow(opCtx);
        if (!createCollectionBeforeDrop) {
            ASSERT_FALSE(CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, nss));
        }

        ASSERT_OK(db->dropCollection(opCtx, nss, dropOpTime));
        ASSERT_FALSE(CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, nss));
        wuow.commit();
    });
}

TEST_F(DatabaseTest, DropCollectionReturnsOKIfCollectionDoesNotExist) {
    _testDropCollection(_opCtx.get(), _nss, false);
    // Check last optime for this client to ensure no entries were appended to the oplog.
    ASSERT_EQUALS(repl::OpTime(), repl::ReplClientInfo::forClient(&cc()).getLastOp());
}

TEST_F(DatabaseTest, DropCollectionDropsCollectionButDoesNotLogOperationIfWritesAreNotReplicated) {
    repl::UnreplicatedWritesBlock uwb(_opCtx.get());
    ASSERT_FALSE(_opCtx->writesAreReplicated());
    ASSERT_TRUE(
        repl::ReplicationCoordinator::get(_opCtx.get())->isOplogDisabledFor(_opCtx.get(), _nss));

    _testDropCollection(_opCtx.get(), _nss, true);

    // Drop optime is null because no op was written to the oplog.
    auto dropOpTime = repl::ReplClientInfo::forClient(&cc()).getLastOp();
    ASSERT_EQUALS(repl::OpTime(), dropOpTime);
}

TEST_F(DatabaseTest, DropCollectionRejectsProvidedDropOpTimeIfWritesAreReplicated) {
    ASSERT_TRUE(_opCtx->writesAreReplicated());
    ASSERT_FALSE(
        repl::ReplicationCoordinator::get(_opCtx.get())->isOplogDisabledFor(_opCtx.get(), _nss));

    auto opCtx = _opCtx.get();
    auto nss = _nss;
    AutoGetDb autoDb(opCtx, nss.dbName(), MODE_X);
    auto db = autoDb.ensureDbExists(opCtx);
    writeConflictRetry(opCtx, "testDropOpTimeWithReplicated", nss, [&] {
        ASSERT_TRUE(db);

        WriteUnitOfWork wuow(opCtx);
        ASSERT_TRUE(db->createCollection(opCtx, nss));
        wuow.commit();
    });

    WriteUnitOfWork wuow(opCtx);
    repl::OpTime dropOpTime(Timestamp(Seconds(100), 0), 1LL);
    ASSERT_EQUALS(ErrorCodes::BadValue, db->dropCollection(opCtx, nss, dropOpTime));
}

void _testDropCollectionThrowsExceptionIfThereAreIndexesInProgress(OperationContext* opCtx,
                                                                   const NamespaceString& nss) {
    writeConflictRetry(opCtx, "testDropCollectionWithIndexesInProgress", nss, [opCtx, nss] {
        AutoGetDb autoDb(opCtx, nss.dbName(), MODE_X);
        auto db = autoDb.ensureDbExists(opCtx);
        ASSERT_TRUE(db);

        Collection* collection = nullptr;
        {
            WriteUnitOfWork wuow(opCtx);
            ASSERT_TRUE((collection = db->createCollection(opCtx, nss)));
            wuow.commit();
        }

        auto indexCatalog = collection->getIndexCatalog();
        ASSERT_EQUALS(indexCatalog->numIndexesInProgress(), 0);
        auto indexInfoObj = BSON("v" << int(IndexDescriptor::kLatestIndexVersion) << "key"
                                     << BSON("a" << 1) << "name"
                                     << "a_1");

        auto indexBuildBlock = std::make_unique<IndexBuildBlock>(
            collection->ns(), indexInfoObj, IndexBuildMethod::kHybrid, UUID::gen());
        {
            WriteUnitOfWork wuow(opCtx);
            ASSERT_OK(indexBuildBlock->init(opCtx, collection, /*forRecovery=*/false));
            wuow.commit();
        }
        ON_BLOCK_EXIT([&indexBuildBlock, opCtx, collection] {
            WriteUnitOfWork wuow(opCtx);
            indexBuildBlock->success(opCtx, collection);
            wuow.commit();
        });

        ASSERT_GREATER_THAN(indexCatalog->numIndexesInProgress(), 0);

        WriteUnitOfWork wuow(opCtx);
        ASSERT_THROWS_CODE(db->dropCollection(opCtx, nss),
                           AssertionException,
                           ErrorCodes::BackgroundOperationInProgressForNamespace);
    });
}

TEST_F(DatabaseTest,
       DropCollectionThrowsExceptionIfThereAreIndexesInProgressAndWritesAreNotReplicated) {
    repl::UnreplicatedWritesBlock uwb(_opCtx.get());
    ASSERT_FALSE(_opCtx->writesAreReplicated());
    _testDropCollectionThrowsExceptionIfThereAreIndexesInProgress(_opCtx.get(), _nss);
}

TEST_F(DatabaseTest,
       DropCollectionThrowsExceptionIfThereAreIndexesInProgressAndWritesAreReplicated) {
    ASSERT_TRUE(_opCtx->writesAreReplicated());
    _testDropCollectionThrowsExceptionIfThereAreIndexesInProgress(_opCtx.get(), _nss);
}

TEST_F(DatabaseTest, RenameCollectionPreservesUuidOfSourceCollectionAndUpdatesUuidCatalog) {
    auto opCtx = _opCtx.get();
    auto fromNss = _nss;
    auto toNss = NamespaceString::createNamespaceString_forTest(fromNss.getSisterNS("bar"));
    ASSERT_NOT_EQUALS(fromNss, toNss);

    AutoGetDb autoDb(opCtx, fromNss.dbName(), MODE_X);
    auto db = autoDb.ensureDbExists(opCtx);
    ASSERT_TRUE(db);

    auto fromUuid = UUID::gen();
    writeConflictRetry(opCtx, "create", fromNss, [&] {
        auto catalog = CollectionCatalog::get(opCtx);
        ASSERT_EQUALS(boost::none, catalog->lookupNSSByUUID(opCtx, fromUuid));

        WriteUnitOfWork wuow(opCtx);
        CollectionOptions fromCollectionOptions;
        fromCollectionOptions.uuid = fromUuid;
        ASSERT_TRUE(db->createCollection(opCtx, fromNss, fromCollectionOptions));
        ASSERT_EQUALS(fromNss, *catalog->lookupNSSByUUID(opCtx, fromUuid));
        wuow.commit();
    });

    writeConflictRetry(opCtx, "rename", fromNss, [&] {
        WriteUnitOfWork wuow(opCtx);
        auto stayTemp = false;
        ASSERT_OK(db->renameCollection(opCtx, fromNss, toNss, stayTemp));

        auto catalog = CollectionCatalog::get(opCtx);
        ASSERT_FALSE(catalog->lookupCollectionByNamespace(opCtx, fromNss));
        auto toCollection = catalog->lookupCollectionByNamespace(opCtx, toNss);
        ASSERT_TRUE(toCollection);

        const auto& toCollectionOptions = toCollection->getCollectionOptions();

        auto toUuid = toCollectionOptions.uuid;
        ASSERT_TRUE(toUuid);
        ASSERT_EQUALS(fromUuid, *toUuid);

        ASSERT_EQUALS(toNss, *catalog->lookupNSSByUUID(opCtx, *toUuid));

        wuow.commit();
    });
}

TEST_F(DatabaseTest,
       MakeUniqueCollectionNamespaceReturnsFailedToParseIfModelDoesNotContainPercentSign) {
    writeConflictRetry(_opCtx.get(), "testMakeUniqueCollectionNamespace", _nss, [this] {
        AutoGetDb autoDb(_opCtx.get(), _nss.dbName(), MODE_X);
        auto db = autoDb.ensureDbExists(_opCtx.get());
        ASSERT_TRUE(db);
        ASSERT_EQUALS(ErrorCodes::FailedToParse,
                      makeUniqueCollectionName(
                          _opCtx.get(), db->name(), "CollectionModelWithoutPercentSign"));
    });
}

TEST_F(DatabaseTest, MakeUniqueCollectionNamespaceReplacesPercentSignsWithRandomCharacters) {
    writeConflictRetry(_opCtx.get(), "testMakeUniqueCollectionNamespace", _nss, [this] {
        AutoGetDb autoDb(_opCtx.get(), _nss.dbName(), MODE_X);
        auto db = autoDb.ensureDbExists(_opCtx.get());
        ASSERT_TRUE(db);

        auto model = "tmp%%%%"_sd;
        pcre::Regex re(_nss.db() + "\\.tmp[0-9A-Za-z][0-9A-Za-z][0-9A-Za-z][0-9A-Za-z]",
                       pcre::ANCHORED | pcre::ENDANCHORED);

        auto nss1 = unittest::assertGet(makeUniqueCollectionName(_opCtx.get(), db->name(), model));
        if (!re.matchView(nss1.ns_forTest())) {
            FAIL((StringBuilder() << "First generated namespace \"" << nss1.ns_forTest()
                                  << "\" does not match regular expression \"" << re.pattern()
                                  << "\"")
                     .str());
        }

        // Create collection using generated namespace so that makeUniqueCollectionNamespace()
        // will not return the same namespace the next time. This is because we check the
        // existing collections in the database for collisions while generating the namespace.
        {
            WriteUnitOfWork wuow(_opCtx.get());
            ASSERT_TRUE(db->createCollection(_opCtx.get(), nss1));
            wuow.commit();
        }

        auto nss2 = unittest::assertGet(makeUniqueCollectionName(_opCtx.get(), db->name(), model));
        if (!re.matchView(nss2.ns_forTest())) {
            FAIL((StringBuilder() << "Second generated namespace \"" << nss2.ns_forTest()
                                  << "\" does not match regular expression \"" << re.pattern()
                                  << "\"")
                     .str());
        }

        // Second generated namespace should not collide with the first because a collection
        // with name matching nss1 now exists.
        ASSERT_NOT_EQUALS(nss1, nss2);
    });
}

TEST_F(
    DatabaseTest,
    MakeUniqueCollectionNamespaceReturnsNamespaceExistsIfGeneratedNamesMatchExistingCollections) {
    writeConflictRetry(_opCtx.get(), "testMakeUniqueCollectionNamespace", _nss, [this] {
        AutoGetDb autoDb(_opCtx.get(), _nss.dbName(), MODE_X);
        auto db = autoDb.ensureDbExists(_opCtx.get());
        ASSERT_TRUE(db);

        auto model = "tmp%"_sd;

        // Create all possible collections matching model with single percent sign.
        const auto charsToChooseFrom =
            "0123456789"
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
            "abcdefghijklmnopqrstuvwxyz"_sd;
        for (const auto c : charsToChooseFrom) {
            NamespaceString nss = NamespaceString::createNamespaceString_forTest(
                _nss.dbName(), model.substr(0, model.find('%')) + std::string(1U, c));
            WriteUnitOfWork wuow(_opCtx.get());
            ASSERT_TRUE(db->createCollection(_opCtx.get(), nss));
            wuow.commit();
        }

        // makeUniqueCollectionName() returns NamespaceExists because it will not be able to
        // generate a namespace that will not collide with an existings collection.
        ASSERT_EQUALS(ErrorCodes::NamespaceExists,
                      makeUniqueCollectionName(_opCtx.get(), db->name(), model));
    });
}

TEST_F(DatabaseTest, AutoGetDBSucceedsWithDeadlineNow) {
    NamespaceString nss = NamespaceString::createNamespaceString_forTest("test", "coll");
    Lock::DBLock lock(_opCtx.get(), nss.dbName(), MODE_X);
    ASSERT(_opCtx.get()->lockState()->isDbLockedForMode(nss.dbName(), MODE_X));
    try {
        AutoGetDb db(_opCtx.get(), nss.dbName(), MODE_X, Date_t::now());
        ASSERT(_opCtx.get()->lockState()->isDbLockedForMode(nss.dbName(), MODE_X));
    } catch (const ExceptionFor<ErrorCodes::LockTimeout>&) {
        FAIL("Should get the db within the timeout");
    }
}

TEST_F(DatabaseTest, AutoGetDBSucceedsWithDeadlineMin) {
    NamespaceString nss = NamespaceString::createNamespaceString_forTest("test", "coll");
    Lock::DBLock lock(_opCtx.get(), nss.dbName(), MODE_X);
    ASSERT(_opCtx.get()->lockState()->isDbLockedForMode(nss.dbName(), MODE_X));
    try {
        AutoGetDb db(_opCtx.get(), nss.dbName(), MODE_X, Date_t());
        ASSERT(_opCtx.get()->lockState()->isDbLockedForMode(nss.dbName(), MODE_X));
    } catch (const ExceptionFor<ErrorCodes::LockTimeout>&) {
        FAIL("Should get the db within the timeout");
    }
}

TEST_F(DatabaseTest, AutoGetCollectionForReadCommandSucceedsWithDeadlineNow) {
    NamespaceString nss = NamespaceString::createNamespaceString_forTest("test", "coll");
    Lock::DBLock dbLock(_opCtx.get(), nss.dbName(), MODE_X);
    ASSERT(_opCtx.get()->lockState()->isDbLockedForMode(nss.dbName(), MODE_X));
    Lock::CollectionLock collLock(_opCtx.get(), nss, MODE_X);
    ASSERT(_opCtx.get()->lockState()->isCollectionLockedForMode(nss, MODE_X));
    try {
        AutoGetCollectionForReadCommand db(
            _opCtx.get(), nss, AutoGetCollection::Options{}.deadline(Date_t::now()));
    } catch (const ExceptionFor<ErrorCodes::LockTimeout>&) {
        FAIL("Should get the db within the timeout");
    }
}

TEST_F(DatabaseTest, AutoGetCollectionForReadCommandSucceedsWithDeadlineMin) {
    NamespaceString nss = NamespaceString::createNamespaceString_forTest("test", "coll");
    Lock::DBLock dbLock(_opCtx.get(), nss.dbName(), MODE_X);
    ASSERT(_opCtx.get()->lockState()->isDbLockedForMode(nss.dbName(), MODE_X));
    Lock::CollectionLock collLock(_opCtx.get(), nss, MODE_X);
    ASSERT(_opCtx.get()->lockState()->isCollectionLockedForMode(nss, MODE_X));
    try {
        AutoGetCollectionForReadCommand db(
            _opCtx.get(), nss, AutoGetCollection::Options{}.deadline(Date_t()));
    } catch (const ExceptionFor<ErrorCodes::LockTimeout>&) {
        FAIL("Should get the db within the timeout");
    }
}

TEST_F(DatabaseTest, CreateCollectionProhibitsReplicatedCollectionsWithoutIdIndex) {
    writeConflictRetry(_opCtx.get(),
                       "testÇreateCollectionProhibitsReplicatedCollectionsWithoutIdIndex",
                       _nss,
                       [this] {
                           AutoGetDb autoDb(_opCtx.get(), _nss.dbName(), MODE_X);
                           auto db = autoDb.ensureDbExists(_opCtx.get());
                           ASSERT_TRUE(db);

                           WriteUnitOfWork wuow(_opCtx.get());

                           CollectionOptions options;
                           options.setNoIdIndex();

                           ASSERT_THROWS_CODE_AND_WHAT(
                               db->createCollection(_opCtx.get(), _nss, options),
                               AssertionException,
                               50001,
                               (StringBuilder()
                                << "autoIndexId:false is not allowed for collection "
                                << _nss.toStringForErrorMsg() << " because it can be replicated")
                                   .stringData());
                       });
}

}  // namespace
}  // namespace mongo
