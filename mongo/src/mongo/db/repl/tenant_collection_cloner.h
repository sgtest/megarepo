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

#pragma once

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <cstddef>
#include <memory>
#include <string>
#include <vector>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/dbclient_connection.h"
#include "mongo/client/dbclient_cursor.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/repl/base_cloner.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/task_runner.h"
#include "mongo/db/repl/tenant_base_cloner.h"
#include "mongo/db/repl/tenant_migration_shared_data.h"
#include "mongo/util/concurrency/thread_pool.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/progress_meter.h"
#include "mongo/util/time_support.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace repl {

class TenantCollectionCloner : public TenantBaseCloner {
public:
    struct Stats {
        static constexpr StringData kDocumentsToCopyFieldName = "documentsToCopyAtStartOfClone"_sd;
        static constexpr StringData kDocumentsCopiedFieldName = "documentsCopied"_sd;

        std::string ns;
        Date_t start;
        Date_t end;
        size_t documentsToCopyAtStartOfClone{0};
        size_t documentsCopied{0};
        size_t indexes{0};
        size_t insertedBatches{0};
        size_t receivedBatches{0};
        long long avgObjSize{0};
        long long approxTotalDataSize{0};
        long long approxTotalBytesCopied{0};


        std::string toString() const;
        BSONObj toBSON() const;
        void append(BSONObjBuilder* builder) const;
    };

    TenantCollectionCloner(const NamespaceString& ns,
                           const CollectionOptions& collectionOptions,
                           TenantMigrationSharedData* sharedData,
                           const HostAndPort& source,
                           DBClientConnection* client,
                           StorageInterface* storageInterface,
                           ThreadPool* dbPool,
                           StringData tenantId);

    ~TenantCollectionCloner() override = default;

    Stats getStats() const;

    std::string toString() const;

    NamespaceString getSourceNss() const {
        return _sourceNss;
    }
    UUID getSourceUuid() const {
        return _sourceDbAndUuid.uuid();
    }
    const std::string& getTenantId() const {
        return _tenantId;
    }

    /**
     * Set the cloner batch size.
     *
     * Used for testing only.  Set by server parameter 'collectionClonerBatchSize' in normal
     * operation.
     */
    void setBatchSize_forTest(int batchSize) {
        _collectionClonerBatchSize = batchSize;
    }

    Timestamp getOperationTime_forTest();

protected:
    ClonerStages getStages() final;

    bool isMyFailPoint(const BSONObj& data) const final;

private:
    friend class TenantCollectionClonerTest;
    friend class TenantCollectionClonerStage;

    class TenantCollectionClonerStage : public ClonerStage<TenantCollectionCloner> {
    public:
        TenantCollectionClonerStage(std::string name,
                                    TenantCollectionCloner* cloner,
                                    ClonerRunFn stageFunc)
            : ClonerStage<TenantCollectionCloner>(name, cloner, stageFunc) {}
        AfterStageBehavior run() override;

        bool isTransientError(const Status& status) override {
            // Always abort on error.
            return false;
        }
    };

    class TenantCollectionClonerQueryStage : public TenantCollectionClonerStage {
    public:
        TenantCollectionClonerQueryStage(std::string name,
                                         TenantCollectionCloner* cloner,
                                         ClonerRunFn stageFunc)
            : TenantCollectionClonerStage(name, cloner, stageFunc) {}

        bool isTransientError(const Status& status) override {
            if (isCursorError(status)) {
                return true;
            }
            return TenantCollectionClonerStage::isTransientError(status);
        }

        static bool isCursorError(const Status& status) {
            // Our cursor was killed due to changes on the remote collection.
            // We do not expect the connection to have been closed so we try to resume the stage.
            if ((status == ErrorCodes::CursorNotFound) || (status == ErrorCodes::OperationFailed) ||
                (status == ErrorCodes::QueryPlanKilled)) {
                return true;
            }
            return false;
        }
    };

    /**
     * The preStage sets the start time in _stats.
     */
    void preStage() final;

    /**
     * The postStage sets the end time in _stats.
     */
    void postStage() final;

    /**
     * Stage function that counts the number of documents in the collection on the source in order
     * to generate progress information.
     */
    AfterStageBehavior countStage();

    /**
     * Stage function that checks to see if the donor collection is empty (and therefore we may
     * race with createIndexes on empty collections) before running listIndexes.
     */
    AfterStageBehavior checkIfDonorCollectionIsEmptyStage();

    /**
     * Stage function that gets the index information of the collection on the source to re-create
     * it.
     */
    AfterStageBehavior listIndexesStage();

    /**
     * Stage function that creates the collection using the storageInterface.  This stage does not
     * actually contact the sync source.
     */
    AfterStageBehavior createCollectionStage();

    /**
     * Stage function that executes a query to retrieve all documents in the collection.  For each
     * batch returned by the upstream node, handleNextBatch will be called with the data.  This
     * stage will finish when the entire query is finished or failed.
     */
    AfterStageBehavior queryStage();

    /**
     * Put all results from a query batch into a buffer to be inserted, and schedule it to be
     * inserted.
     */
    void handleNextBatch(DBClientCursor& cursor);

    /**
     * Called whenever there is a new batch of documents ready from the DBClientConnection.
     */
    void insertDocuments(std::vector<BSONObj> docsToInsert);

    /**
     * Sends a query command to the source.
     */
    void runQuery();

    // All member variables are labeled with one of the following codes indicating the
    // synchronization rules for accessing them.
    //
    // (R)  Read-only in concurrent operation; no synchronization required.
    // (S)  Self-synchronizing; access according to class's own rules.
    // (M)  Reads and writes guarded by _mutex (defined in base class).
    // (X)  Access only allowed from the main flow of control called from run() or constructor.
    const NamespaceString _sourceNss;            // (R)
    const CollectionOptions _collectionOptions;  // (R)
    // Despite the type name, this member must always contain a UUID.
    NamespaceStringOrUUID _sourceDbAndUuid;  // (R)
    // Namespace of the existing collection (with the same UUID as _sourceDbAndUuid) after resuming
    // the collection cloner. This existing collection normally has the same namespace as _sourceNss
    // except when the collection has been renamed on the donor.
    boost::optional<NamespaceString> _existingNss;  // (R)
    // The size of the batches of documents returned in collection cloning.
    int _collectionClonerBatchSize;  // (R)

    TenantCollectionClonerStage _countStage;                               // (R)
    TenantCollectionClonerQueryStage _checkIfDonorCollectionIsEmptyStage;  // (R)
    TenantCollectionClonerStage _listIndexesStage;                         // (R)
    TenantCollectionClonerStage _createCollectionStage;                    // (R)
    TenantCollectionClonerQueryStage _queryStage;                          // (R)

    ProgressMeter _progressMeter;           // (X) progress meter for this instance.
    std::vector<BSONObj> _readyIndexSpecs;  // (X) Except for _id_
    BSONObj _idIndexSpec;                   // (X)

    BSONObj _lastDocId;  // (X)
    // Documents read from source to insert.
    std::vector<BSONObj> _documentsToInsert;  // (M)
    Stats _stats;                             // (M)

    // The database name prefix of the tenant associated with this migration.
    std::string _tenantId;  // (R)

    // The operationTime returned with the listIndexes result.
    Timestamp _operationTime;  // (X)

    // Was the collection empty the first time we checked?
    bool _donorCollectionWasEmptyBeforeListIndexes = false;  // (X)
};

}  // namespace repl
}  // namespace mongo
