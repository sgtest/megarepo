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

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/bsontypes_util.h"
#include "mongo/bson/json.h"
#include "mongo/bson/util/builder.h"
#include "mongo/crypto/encryption_fields_gen.h"
#include "mongo/db/catalog/catalog_test_fixture.h"
#include "mongo/db/catalog/clustered_collection_options_gen.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/index/index_build_interceptor.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/record_data.h"
#include "mongo/db/storage/storage_engine.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/timeseries/timeseries_gen.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace {
class IndexBuilderInterceptorTest : public CatalogTestFixture {
protected:
    const IndexCatalogEntry* createIndex(BSONObj spec) {
        WriteUnitOfWork wuow(operationContext());
        auto* indexCatalog = _coll->getWritableCollection(operationContext())->getIndexCatalog();
        uassertStatusOK(indexCatalog->createIndexOnEmptyCollection(
            operationContext(), _coll->getWritableCollection(operationContext()), spec));
        wuow.commit();

        return indexCatalog->getEntry(indexCatalog->findIndexByName(
            operationContext(), spec.getStringField(IndexDescriptor::kIndexNameFieldName)));
    }

    std::unique_ptr<IndexBuildInterceptor> createIndexBuildInterceptor(BSONObj spec) {
        return std::make_unique<IndexBuildInterceptor>(operationContext(),
                                                       createIndex(std::move(spec)));
    }

    std::unique_ptr<TemporaryRecordStore> getSideWritesTable(
        std::unique_ptr<IndexBuildInterceptor> interceptor) {
        // In order to get access to the interceptor's side writes table, we have to mark the table
        // as permanent and then destroy the interceptor.
        interceptor->keepTemporaryTables();
        auto sideWritesIdent = interceptor->getSideWritesTableIdent();
        interceptor.reset();

        return operationContext()
            ->getServiceContext()
            ->getStorageEngine()
            ->makeTemporaryRecordStoreFromExistingIdent(
                operationContext(), sideWritesIdent, KeyFormat::Long);
    }

    std::vector<BSONObj> getSideWritesTableContents(
        std::unique_ptr<IndexBuildInterceptor> interceptor) {
        auto table = getSideWritesTable(std::move(interceptor));

        std::vector<BSONObj> contents;
        auto cursor = table->rs()->getCursor(operationContext());
        while (auto record = cursor->next()) {
            contents.push_back(record->data.toBson().getOwned());
        }
        return contents;
    }

    const IndexDescriptor* getIndexDescriptor(const std::string& indexName) {
        return _coll->getCollection()->getIndexCatalog()->findIndexByName(operationContext(),
                                                                          indexName);
    }

    void setUp() override {
        CatalogTestFixture::setUp();
        ASSERT_OK(storageInterface()->createCollection(operationContext(), _nss, {}));
        _coll.emplace(operationContext(), _nss, MODE_X);
    }

    void tearDown() override {
        _coll.reset();
        CatalogTestFixture::tearDown();
    }

private:
    NamespaceString _nss = NamespaceString::createNamespaceString_forTest("testDB.interceptor");
    boost::optional<AutoGetCollection> _coll;
};

TEST_F(IndexBuilderInterceptorTest, SingleInsertIsSavedToSideWritesTable) {
    auto interceptor = createIndexBuildInterceptor(fromjson("{v: 2, name: 'a_1', key: {a: 1}}"));
    const IndexDescriptor* desc = getIndexDescriptor("a_1");

    key_string::HeapBuilder ksBuilder(key_string::Version::kLatestVersion);
    ksBuilder.appendNumberLong(10);
    key_string::Value keyString(ksBuilder.release());

    WriteUnitOfWork wuow(operationContext());
    int64_t numKeys = 0;
    ASSERT_OK(interceptor->sideWrite(operationContext(),
                                     desc->getEntry(),
                                     {keyString},
                                     {},
                                     {},
                                     IndexBuildInterceptor::Op::kInsert,
                                     &numKeys));
    ASSERT_EQ(1, numKeys);
    wuow.commit();

    BufBuilder bufBuilder;
    keyString.serialize(bufBuilder);
    BSONBinData serializedKeyString(bufBuilder.buf(), bufBuilder.len(), BinDataGeneral);

    auto sideWrites = getSideWritesTableContents(std::move(interceptor));
    ASSERT_EQ(1, sideWrites.size());
    ASSERT_BSONOBJ_EQ(BSON("op"
                           << "i"
                           << "key" << serializedKeyString),
                      sideWrites[0]);
}


TEST_F(IndexBuilderInterceptorTest, SingleColumnInsertIsSavedToSideWritesTable) {
    RAIIServerParameterControllerForTest controller("featureFlagColumnstoreIndexes", true);
    auto interceptor = createIndexBuildInterceptor(
        fromjson("{v: 2, name: 'columnstore', key: {'$**': 'columnstore'}}"));
    const IndexDescriptor* desc = getIndexDescriptor("columnstore");

    std::vector<column_keygen::CellPatch> columnChanges;
    columnChanges.emplace_back(
        "changedPath", "cell", RecordId(1), column_keygen::ColumnKeyGenerator::DiffAction::kInsert);

    WriteUnitOfWork wuow(operationContext());
    int64_t numKeysInserted = 0;
    int64_t numKeysDeleted = 0;
    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(1, numKeysInserted);
    ASSERT_EQ(0, numKeysDeleted);
    wuow.commit();

    BSONObjBuilder builder;
    RecordId(1).serializeToken("rid", &builder);
    BSONObj obj = builder.obj();
    BSONElement elem = obj["rid"];

    auto sideWrites = getSideWritesTableContents(std::move(interceptor));
    ASSERT_EQ(1, sideWrites.size());
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem << "op"
                                 << "i"
                                 << "path"
                                 << "changedPath"
                                 << "cell"
                                 << "cell"),
                      sideWrites[0]);
}

TEST_F(IndexBuilderInterceptorTest, SingleColumnDeleteIsSavedToSideWritesTable) {
    RAIIServerParameterControllerForTest controller("featureFlagColumnstoreIndexes", true);
    auto interceptor = createIndexBuildInterceptor(
        fromjson("{v: 2, name: 'columnstore', key: {'$**': 'columnstore'}}"));
    const IndexDescriptor* desc = getIndexDescriptor("columnstore");

    std::vector<column_keygen::CellPatch> columnChanges;
    columnChanges.emplace_back(
        "changedPath", "", RecordId(1), column_keygen::ColumnKeyGenerator::DiffAction::kDelete);

    WriteUnitOfWork wuow(operationContext());
    int64_t numKeysInserted = 0;
    int64_t numKeysDeleted = 0;
    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(0, numKeysInserted);
    ASSERT_EQ(1, numKeysDeleted);
    wuow.commit();

    BSONObjBuilder builder;
    RecordId(1).serializeToken("rid", &builder);
    BSONObj obj = builder.obj();
    BSONElement elem = obj["rid"];

    auto sideWrites = getSideWritesTableContents(std::move(interceptor));
    ASSERT_EQ(1, sideWrites.size());
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem << "op"
                                 << "d"
                                 << "path"
                                 << "changedPath"
                                 << "cell"
                                 << ""),
                      sideWrites[0]);
}

TEST_F(IndexBuilderInterceptorTest, SingleColumnUpdateIsSavedToSideWritesTable) {
    RAIIServerParameterControllerForTest controller("featureFlagColumnstoreIndexes", true);
    auto interceptor = createIndexBuildInterceptor(
        fromjson("{v: 2, name: 'columnstore', key: {'$**': 'columnstore'}}"));
    const IndexDescriptor* desc = getIndexDescriptor("columnstore");

    // create path + cell + rid
    std::vector<column_keygen::CellPatch> columnChanges;
    columnChanges.emplace_back(
        "changedPath", "cell", RecordId(1), column_keygen::ColumnKeyGenerator::DiffAction::kUpdate);

    WriteUnitOfWork wuow(operationContext());
    int64_t numKeysInserted = 0;
    int64_t numKeysDeleted = 0;
    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(1, numKeysInserted);
    ASSERT_EQ(0, numKeysDeleted);
    wuow.commit();

    BSONObjBuilder builder;
    RecordId(1).serializeToken("rid", &builder);
    BSONObj obj = builder.obj();
    BSONElement elem = obj["rid"];

    auto sideWrites = getSideWritesTableContents(std::move(interceptor));
    ASSERT_EQ(1, sideWrites.size());
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem << "op"
                                 << "u"
                                 << "path"
                                 << "changedPath"
                                 << "cell"
                                 << "cell"),
                      sideWrites[0]);
}

TEST_F(IndexBuilderInterceptorTest, MultipleColumnInsertsAreSavedToSideWritesTable) {
    RAIIServerParameterControllerForTest controller("featureFlagColumnstoreIndexes", true);
    auto interceptor = createIndexBuildInterceptor(
        fromjson("{v: 2, name: 'columnstore', key: {'$**': 'columnstore'}}"));
    const IndexDescriptor* desc = getIndexDescriptor("columnstore");

    std::vector<column_keygen::CellPatch> columnChanges;
    columnChanges.emplace_back("changedPath1",
                               "cell",
                               RecordId(1),
                               column_keygen::ColumnKeyGenerator::DiffAction::kInsert);
    columnChanges.emplace_back("changedPath2",
                               "cell1",
                               RecordId(1),
                               column_keygen::ColumnKeyGenerator::DiffAction::kInsert);
    columnChanges.emplace_back("changedPath3",
                               "cell2",
                               RecordId(2),
                               column_keygen::ColumnKeyGenerator::DiffAction::kInsert);
    columnChanges.emplace_back("changedPath4",
                               "cell3",
                               RecordId(2),
                               column_keygen::ColumnKeyGenerator::DiffAction::kInsert);

    WriteUnitOfWork wuow(operationContext());
    int64_t numKeysInserted = 0;
    int64_t numKeysDeleted = 0;

    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(4, numKeysInserted);
    ASSERT_EQ(0, numKeysDeleted);
    wuow.commit();

    BSONObjBuilder builder;
    RecordId(1).serializeToken("rid", &builder);
    BSONObj obj = builder.obj();
    BSONElement elem1 = obj["rid"];

    BSONObjBuilder builder2;
    RecordId(2).serializeToken("rid", &builder2);
    BSONObj obj2 = builder2.obj();
    BSONElement elem2 = obj2["rid"];

    auto sideWrites = getSideWritesTableContents(std::move(interceptor));
    ASSERT_EQ(4, sideWrites.size());
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem1 << "op"
                                 << "i"
                                 << "path"
                                 << "changedPath1"
                                 << "cell"
                                 << "cell"),
                      sideWrites[0]);
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem1 << "op"
                                 << "i"
                                 << "path"
                                 << "changedPath2"
                                 << "cell"
                                 << "cell1"),
                      sideWrites[1]);
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem2 << "op"
                                 << "i"
                                 << "path"
                                 << "changedPath3"
                                 << "cell"
                                 << "cell2"),
                      sideWrites[2]);
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem2 << "op"
                                 << "i"
                                 << "path"
                                 << "changedPath4"
                                 << "cell"
                                 << "cell3"),
                      sideWrites[3]);
}

TEST_F(IndexBuilderInterceptorTest, MultipleColumnSideWritesAreSavedToSideWritesTable) {
    RAIIServerParameterControllerForTest controller("featureFlagColumnstoreIndexes", true);
    auto interceptor = createIndexBuildInterceptor(
        fromjson("{v: 2, name: 'columnstore', key: {'$**': 'columnstore'}}"));
    const IndexDescriptor* desc = getIndexDescriptor("columnstore");

    WriteUnitOfWork wuow(operationContext());
    int64_t numKeysInserted = 0;
    int64_t numKeysDeleted = 0;

    std::vector<column_keygen::CellPatch> columnChanges;
    columnChanges.emplace_back("changedPath1",
                               "cell",
                               RecordId(1),
                               column_keygen::ColumnKeyGenerator::DiffAction::kInsert);
    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(1, numKeysInserted);
    ASSERT_EQ(0, numKeysDeleted);

    std::vector<column_keygen::CellPatch> columnChanges2;
    columnChanges2.emplace_back(
        "changedPath1", "", RecordId(1), column_keygen::ColumnKeyGenerator::DiffAction::kDelete);
    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges2, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(0, numKeysInserted);
    ASSERT_EQ(1, numKeysDeleted);

    std::vector<column_keygen::CellPatch> columnChanges3;
    columnChanges3.emplace_back("changedPath2",
                                "cell1",
                                RecordId(2),
                                column_keygen::ColumnKeyGenerator::DiffAction::kUpdate);
    columnChanges3.emplace_back(
        "changedPath3", "", RecordId(2), column_keygen::ColumnKeyGenerator::DiffAction::kDelete);
    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges3, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(1, numKeysInserted);
    ASSERT_EQ(1, numKeysDeleted);

    std::vector<column_keygen::CellPatch> columnChanges4;
    columnChanges4.emplace_back("changedPath3",
                                "cell2",
                                RecordId(2),
                                column_keygen::ColumnKeyGenerator::DiffAction::kInsert);
    ASSERT_OK(interceptor->sideWrite(
        operationContext(), desc->getEntry(), columnChanges4, &numKeysInserted, &numKeysDeleted));
    ASSERT_EQ(1, numKeysInserted);
    ASSERT_EQ(0, numKeysDeleted);
    wuow.commit();

    BSONObjBuilder builder;
    RecordId(1).serializeToken("rid", &builder);
    BSONObj obj = builder.obj();
    BSONElement elem1 = obj["rid"];

    BSONObjBuilder builder2;
    RecordId(2).serializeToken("rid", &builder2);
    BSONObj obj2 = builder2.obj();
    BSONElement elem2 = obj2["rid"];

    auto sideWrites = getSideWritesTableContents(std::move(interceptor));
    ASSERT_EQ(5, sideWrites.size());
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem1 << "op"
                                 << "i"
                                 << "path"
                                 << "changedPath1"
                                 << "cell"
                                 << "cell"),
                      sideWrites[0]);
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem1 << "op"
                                 << "d"
                                 << "path"
                                 << "changedPath1"
                                 << "cell"
                                 << ""),
                      sideWrites[1]);
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem2 << "op"
                                 << "u"
                                 << "path"
                                 << "changedPath2"
                                 << "cell"
                                 << "cell1"),
                      sideWrites[2]);
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem2 << "op"
                                 << "d"
                                 << "path"
                                 << "changedPath3"
                                 << "cell"
                                 << ""),
                      sideWrites[3]);
    ASSERT_BSONOBJ_EQ(BSON("rid" << elem2 << "op"
                                 << "i"
                                 << "path"
                                 << "changedPath3"
                                 << "cell"
                                 << "cell2"),
                      sideWrites[4]);
}

}  // namespace
}  // namespace mongo
