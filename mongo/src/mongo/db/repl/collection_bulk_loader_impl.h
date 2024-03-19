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


#pragma once

#include <memory>
#include <string>
#include <vector>

#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/catalog/multi_index_block.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/collection_bulk_loader.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/service_context.h"
#include "mongo/db/shard_role.h"
#include "mongo/util/time_support.h"

namespace mongo {
namespace repl {

/**
 * Class in charge of building a collection during data loading (like initial sync).
 *
 * Note: Call commit when done inserting documents.
 */
class CollectionBulkLoaderImpl : public CollectionBulkLoader {
    CollectionBulkLoaderImpl(const CollectionBulkLoaderImpl&) = delete;
    CollectionBulkLoaderImpl& operator=(const CollectionBulkLoaderImpl&) = delete;

public:
    struct Stats {
        Date_t startBuildingIndexes;
        Date_t endBuildingIndexes;

        std::string toString() const;
        BSONObj toBSON() const;
    };

    CollectionBulkLoaderImpl(ServiceContext::UniqueClient client,
                             ServiceContext::UniqueOperationContext opCtx,
                             const NamespaceString& nss,
                             const BSONObj& idIndexSpec);
    ~CollectionBulkLoaderImpl() override;

    Status init(const std::vector<BSONObj>& secondaryIndexSpecs) override;

    Status insertDocuments(std::vector<BSONObj>::const_iterator begin,
                           std::vector<BSONObj>::const_iterator end,
                           ParseRecordIdAndDocFunc fn) override;
    Status commit() override;

    CollectionBulkLoaderImpl::Stats getStats() const;

private:
    void _releaseResources();

    template <typename F>
    Status _runTaskReleaseResourcesOnFailure(const F& task) noexcept;

    /**
     * For capped collections, each document will be inserted in its own WriteUnitOfWork.
     */
    Status _insertDocumentsForCappedCollection(std::vector<BSONObj>::const_iterator begin,
                                               std::vector<BSONObj>::const_iterator end,
                                               ParseRecordIdAndDocFunc fn);

    /**
     * For uncapped collections, we will insert documents in batches of size
     * collectionBulkLoaderBatchSizeInBytes or up to one document size greater. All insertions in a
     * given batch will be inserted in one WriteUnitOfWork.
     */
    Status _insertDocumentsForUncappedCollection(std::vector<BSONObj>::const_iterator begin,
                                                 std::vector<BSONObj>::const_iterator end,
                                                 ParseRecordIdAndDocFunc fn);

    /**
     * Adds document and associated RecordId to index blocks after inserting into RecordStore.
     */
    Status _addDocumentToIndexBlocks(const BSONObj& doc, const RecordId& loc);

    ServiceContext::UniqueClient _client;
    ServiceContext::UniqueOperationContext _opCtx;
    CollectionAcquisition _acquisition;
    NamespaceString _nss;
    std::unique_ptr<MultiIndexBlock> _idIndexBlock;
    std::unique_ptr<MultiIndexBlock> _secondaryIndexesBlock;
    BSONObj _idIndexSpec;
    Stats _stats;
};

}  // namespace repl
}  // namespace mongo
