/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#pragma once

#include <boost/iterator/transform_iterator.hpp>

#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_impl.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/multitenancy.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/storage_interface_impl.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/db/storage/durable_catalog.h"
#include "mongo/db/storage/kv/kv_engine.h"
#include "mongo/db/storage/storage_engine_impl.h"
#include "mongo/db/storage/storage_repair_observer.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/logv2/log.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kDefault

namespace mongo {

class StorageEngineTest : public ServiceContextMongoDTest {
public:
    explicit StorageEngineTest(Options options = {})
        : ServiceContextMongoDTest(std::move(options)),
          _storageEngine(getServiceContext()->getStorageEngine()) {
        repl::ReplicationCoordinator::set(
            getServiceContext(),
            std::make_unique<repl::ReplicationCoordinatorMock>(getServiceContext()));
    }

    StatusWith<DurableCatalog::EntryIdentifier> createCollection(OperationContext* opCtx,
                                                                 NamespaceString ns) {
        Lock::GlobalWrite lk(opCtx);
        AutoGetDb db(opCtx, ns.dbName(), LockMode::MODE_X);
        CollectionOptions options;
        options.uuid = UUID::gen();
        RecordId catalogId;
        std::unique_ptr<RecordStore> rs;
        {
            WriteUnitOfWork wuow(opCtx);
            std::tie(catalogId, rs) = unittest::assertGet(
                _storageEngine->getCatalog()->createCollection(opCtx, ns, options, true));
            wuow.commit();
        }
        std::shared_ptr<Collection> coll = std::make_shared<CollectionImpl>(
            opCtx,
            ns,
            catalogId,
            _storageEngine->getCatalog()->getParsedCatalogEntry(opCtx, catalogId)->metadata,
            std::move(rs));

        CollectionCatalog::write(opCtx, [&](CollectionCatalog& catalog) {
            catalog.registerCollection(opCtx, std::move(coll), /*ts=*/boost::none);
        });

        return {{_storageEngine->getCatalog()->getEntry(catalogId)}};
    }

    std::unique_ptr<TemporaryRecordStore> makeTemporary(OperationContext* opCtx) {
        return _storageEngine->makeTemporaryRecordStore(opCtx, KeyFormat::Long);
    }

    std::unique_ptr<TemporaryRecordStore> makeTemporaryClustered(OperationContext* opCtx) {
        return _storageEngine->makeTemporaryRecordStore(opCtx, KeyFormat::String);
    }

    /**
     * Create a collection table in the KVEngine not reflected in the DurableCatalog.
     */
    Status createCollTable(OperationContext* opCtx, NamespaceString collName) {
        const std::string identName = "collection-" + collName.ns_forTest();
        return _storageEngine->getEngine()->createRecordStore(
            opCtx, collName, identName, CollectionOptions());
    }

    Status dropIndexTable(OperationContext* opCtx, NamespaceString nss, std::string indexName) {
        RecordId catalogId =
            CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, nss)->getCatalogId();
        std::string indexIdent =
            _storageEngine->getCatalog()->getIndexIdent(opCtx, catalogId, indexName);
        return dropIdent(shard_role_details::getRecoveryUnit(opCtx), indexIdent);
    }

    Status dropIdent(RecoveryUnit* ru, StringData ident) {
        return _storageEngine->getEngine()->dropIdent(ru, ident);
    }

    StatusWith<StorageEngine::ReconcileResult> reconcile(OperationContext* opCtx) {
        Lock::GlobalLock globalLock{opCtx, MODE_IX};
        return _storageEngine->reconcileCatalogAndIdents(
            opCtx, Timestamp::min(), StorageEngine::LastShutdownState::kClean);
    }

    StatusWith<StorageEngine::ReconcileResult> reconcileAfterUncleanShutdown(
        OperationContext* opCtx) {
        return _storageEngine->reconcileCatalogAndIdents(
            opCtx, Timestamp::min(), StorageEngine::LastShutdownState::kUnclean);
    }

    std::vector<std::string> getAllKVEngineIdents(OperationContext* opCtx) {
        return _storageEngine->getEngine()->getAllIdents(opCtx);
    }

    bool collectionExists(OperationContext* opCtx, const NamespaceString& nss) {
        std::vector<DurableCatalog::EntryIdentifier> allCollections =
            _storageEngine->getCatalog()->getAllCatalogEntries(opCtx);
        return std::count_if(allCollections.begin(), allCollections.end(), [&](auto& entry) {
            return nss == entry.nss;
        });
    }

    bool identExists(OperationContext* opCtx, const std::string& ident) {
        auto idents = getAllKVEngineIdents(opCtx);
        return std::find(idents.begin(), idents.end(), ident) != idents.end();
    }

    /**
     * Create an index with a key of `{<key>: 1}` and a `name` of <key>.
     */
    Status createIndex(OperationContext* opCtx,
                       NamespaceString collNs,
                       std::string key,
                       bool isBackgroundSecondaryBuild) {
        auto buildUUID = UUID::gen();
        auto ret = startIndexBuild(opCtx, collNs, key, isBackgroundSecondaryBuild, buildUUID);
        if (!ret.isOK()) {
            return ret;
        }

        indexBuildSuccess(opCtx, collNs, key);
        return Status::OK();
    }

    Status startIndexBuild(OperationContext* opCtx,
                           NamespaceString collNs,
                           std::string key,
                           bool isBackgroundSecondaryBuild,
                           boost::optional<UUID> buildUUID) {
        BSONObjBuilder builder;
        {
            BSONObjBuilder keyObj;
            builder.append("key", keyObj.append(key, 1).done());
        }
        BSONObj spec = builder.append("name", key).append("v", 2).done();

        Collection* collection =
            CollectionCatalog::get(opCtx)->lookupCollectionByNamespaceForMetadataWrite(opCtx,
                                                                                       collNs);
        auto descriptor = std::make_unique<IndexDescriptor>(IndexNames::findPluginName(spec), spec);

        auto ret = collection->prepareForIndexBuild(
            opCtx, descriptor.get(), buildUUID, isBackgroundSecondaryBuild);
        return ret;
    }

    void indexBuildSuccess(OperationContext* opCtx, NamespaceString collNs, std::string key) {
        Collection* collection =
            CollectionCatalog::get(opCtx)->lookupCollectionByNamespaceForMetadataWrite(opCtx,
                                                                                       collNs);
        auto writableEntry = collection->getIndexCatalog()->getWritableEntryByName(
            opCtx,
            key,
            IndexCatalog::InclusionPolicy::kReady | IndexCatalog::InclusionPolicy::kUnfinished);
        collection->indexBuildSuccess(opCtx, writableEntry);
    }

    Status removeEntry(OperationContext* opCtx, StringData collNs, DurableCatalog* catalog) {
        const Collection* collection = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(
            opCtx, NamespaceString::createNamespaceString_forTest(collNs));
        return catalog->_removeEntry(opCtx, collection->getCatalogId());
    }

    StorageEngine* _storageEngine;
};

class StorageEngineRepairTest : public StorageEngineTest {
public:
    StorageEngineRepairTest()
        : StorageEngineTest(Options{}.repair(RepairAction::kRepair).ephemeral(false)) {
        repl::StorageInterface::set(getServiceContext(),
                                    std::make_unique<repl::StorageInterfaceImpl>());
    }

    void tearDown() {
        auto repairObserver = StorageRepairObserver::get(getGlobalServiceContext());
        ASSERT(repairObserver->isDone());

        auto asString = [](const StorageRepairObserver::Modification& mod) {
            return mod.getDescription();
        };
        auto modifications = repairObserver->getModifications();
        LOGV2(24150,
              "Modifications",
              "modifications"_attr =
                  logv2::seqLog(boost::make_transform_iterator(modifications.begin(), asString),
                                boost::make_transform_iterator(modifications.end(), asString)));
    }
};

class StorageEngineTestNotEphemeral : public StorageEngineTest {
public:
    StorageEngineTestNotEphemeral() : StorageEngineTest(Options{}.ephemeral(false)){};
};

}  // namespace mongo

#undef MONGO_LOGV2_DEFAULT_COMPONENT
