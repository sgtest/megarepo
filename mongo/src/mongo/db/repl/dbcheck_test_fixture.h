/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include <boost/optional/optional.hpp>

#include "mongo/db/catalog/catalog_test_fixture.h"
#include "mongo/db/commands/dbcheck_command.h"
#include "mongo/db/repl/dbcheck.h"
#include "mongo/db/repl/dbcheck_gen.h"
#include "mongo/db/write_concern.h"
#include "mongo/db/write_concern_options.h"


namespace mongo {

const NamespaceString kNss = NamespaceString::createNamespaceString_forTest("test.t");

const int64_t kDefaultMaxCount = std::numeric_limits<int64_t>::max();
const int64_t kDefaultMaxSize = std::numeric_limits<int64_t>::max();
const int64_t kDefaultMaxRate = std::numeric_limits<int64_t>::max();
const int64_t kDefaultMaxDocsPerBatch = 5000;
const int64_t kDefaultMaxBytesPerBatch = 20 * 1024 * 1024;
const int64_t kDefaultMaxDocsPerSec = 5000;
const int64_t kDefaultMaxBytesPerSec = 20 * 1024 * 1024;
const int64_t kDefaultMaxBatchTimeMillis = 1000;

class DbCheckTest : public CatalogTestFixture {
public:
    DbCheckTest(Options options = {}) : CatalogTestFixture(std::move(options)) {}

    void setUp() override;

    /**
     *  Inserts 'numDocs' docs with _id values starting at 'startIDNum' and incrementing for each
     * document. Callers must avoid duplicate key insertions. These keys always contain a value in
     * the 'a' field.
     */
    void insertDocs(OperationContext* opCtx,
                    int startIDNum,
                    int numDocs,
                    const std::vector<std::string>& fieldNames);

    /**
     * Deletes 'numDocs' docs from kNss with _id values starting at 'startIDNum' and incrementing
     * for each document.
     */
    void deleteDocs(OperationContext* opCtx, int startIDNum, int numDocs);

    /**
     * Inserts documents without updating corresponding index tables to generate missing index
     * entries for the inserted documents.
     */
    void insertDocsWithMissingIndexKeys(OperationContext* opCtx,
                                        int startIDNum,
                                        int numDocs,
                                        const std::vector<std::string>& fieldNames);

    /**
     * Inserts and deletes documents but skips cleaning up corresponding index tables to generate
     * extra index entries.
     */
    void insertExtraIndexKeys(OperationContext* opCtx,
                              int startIDNum,
                              int numDocs,
                              const std::vector<std::string>& fieldNames);

    /**
     * Builds an index on kNss. 'indexKey' specifies the index key, e.g. {'a': 1};
     */
    void createIndex(OperationContext* opCtx, const BSONObj& indexKey);

    /**
     *  Drops the index on kNss.
     */
    void dropIndex(OperationContext* opCtx, const std::string& indexName);

    /**
     * Runs hashing and the missing keys check for kNss.
     */
    void runHashForCollectionCheck(
        OperationContext* opCtx,
        const BSONObj& start,
        const BSONObj& end,
        boost::optional<SecondaryIndexCheckParameters> secondaryIndexCheckParams,
        int64_t maxCount = std::numeric_limits<int64_t>::max(),
        int64_t maxBytes = std::numeric_limits<int64_t>::max());

    /**
     *  Creates a secondary index check params struct to define the dbCheck operation.
     */
    SecondaryIndexCheckParameters createSecondaryIndexCheckParams(
        DbCheckValidationModeEnum validateMode,
        StringData secondaryIndex,
        bool skipLookupForExtraKeys = false);

    /**
     * Creates a DbCheckCollectionInfo struct.
     */
    DbCheckCollectionInfo createDbCheckCollectionInfo(OperationContext* opCtx,
                                                      const BSONObj& start,
                                                      const BSONObj& end,
                                                      const SecondaryIndexCheckParameters& params);

    /**
     * Fetches the number of entries in the health log that match the given query.
     */
    int getNumDocsFoundInHealthLog(OperationContext* opCtx, const BSONObj& query);
};


const auto docMinKey = BSON("_id" << MINKEY);
const auto docMaxKey = BSON("_id" << MAXKEY);
const auto aIndexMinKey = BSON("a" << MINKEY);
const auto aIndexMaxKey = BSON("a" << MAXKEY);

const auto errQuery = BSON(HealthLogEntry::kSeverityFieldName << "error");
const auto missingKeyQuery =
    BSON(HealthLogEntry::kSeverityFieldName << "error" << HealthLogEntry::kMsgFieldName
                                            << "Document has missing index keys");

}  // namespace mongo
