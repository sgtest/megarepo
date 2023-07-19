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

#include <exception>
#include <fmt/format.h>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_validation.h"
#include "mongo/db/catalog/database_holder.h"
#include "mongo/db/catalog/document_validation.h"
#include "mongo/db/catalog/validate_results.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/rebuild_indexes.h"
#include "mongo/db/repair.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl_set_member_in_standalone_mode.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/storage_engine.h"
#include "mongo/db/storage/storage_repair_observer.h"
#include "mongo/db/storage/storage_util.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/str.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage

namespace mongo {

using namespace fmt::literals;

Status rebuildIndexesForNamespace(OperationContext* opCtx,
                                  const NamespaceString& nss,
                                  StorageEngine* engine) {
    if (opCtx->recoveryUnit()->isActive()) {
        // This function is shared by multiple callers. Some of which have opened a transaction to
        // perform reads. This function may make mixed-mode writes. Mixed-mode assertions can only
        // be suppressed when beginning a fresh transaction.
        opCtx->recoveryUnit()->abandonSnapshot();
    }

    opCtx->checkForInterrupt();
    auto collection = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, nss);
    auto swIndexNameObjs = getIndexNameObjs(collection);
    if (!swIndexNameObjs.isOK())
        return swIndexNameObjs.getStatus();

    std::vector<BSONObj> indexSpecs = swIndexNameObjs.getValue().second;
    Status status = rebuildIndexesOnCollection(opCtx, collection, indexSpecs, RepairData::kYes);
    if (!status.isOK())
        return status;

    engine->flushAllFiles(opCtx, /*callerHoldsReadLock*/ false);
    return Status::OK();
}

namespace {

/**
 * Re-opening the database can throw an InvalidIndexSpecificationOption error. This can occur if the
 * index option was previously valid, but a node tries to upgrade to a version where the option is
 * invalid. We should remove all invalid options in all index specifications of the database and
 * retry so the database is successfully re-opened for the rest of the repair sequence.
 */
void openDbAndRepairIndexSpec(OperationContext* opCtx, const DatabaseName& dbName) {
    auto databaseHolder = DatabaseHolder::get(opCtx);

    try {
        databaseHolder->openDb(opCtx, dbName);
        return;
    } catch (const ExceptionFor<ErrorCodes::InvalidIndexSpecificationOption>&) {
        // Fix any invalid index options for this database.
        auto colls = CollectionCatalog::get(opCtx)->getAllCollectionNamesFromDb(opCtx, dbName);

        for (const auto& nss : colls) {
            auto coll = CollectionCatalog::get(opCtx)->lookupCollectionByNamespaceForMetadataWrite(
                opCtx, nss);

            writeConflictRetry(opCtx, "repairInvalidIndexOptions", nss, [&] {
                WriteUnitOfWork wuow(opCtx);

                std::vector<std::string> indexesWithInvalidOptions =
                    coll->repairInvalidIndexOptions(opCtx);

                for (const auto& indexWithInvalidOptions : indexesWithInvalidOptions) {
                    LOGV2_WARNING(7610902,
                                  "Removed invalid options from index",
                                  "indexWithInvalidOptions"_attr = redact(indexWithInvalidOptions));
                }
                wuow.commit();
            });
        }

        // The rest of the --repair sequence requires an open database.
        databaseHolder->openDb(opCtx, dbName);
    }
}

Status dropUnfinishedIndexes(OperationContext* opCtx, Collection* collection) {
    std::vector<std::string> indexNames;
    collection->getAllIndexes(&indexNames);
    for (const auto& indexName : indexNames) {
        if (!collection->isIndexReady(indexName)) {
            LOGV2(3871400,
                  "Dropping unfinished index '{name}' after collection was modified by "
                  "repair",
                  "Dropping unfinished index after collection was modified by repair",
                  "index"_attr = indexName);

            WriteUnitOfWork wuow(opCtx);
            // There are no concurrent users of the index while --repair is running, so it is OK to
            // pass in a nullptr for the index 'ident', promising that the index is not in use.
            catalog::removeIndex(
                opCtx,
                indexName,
                collection,
                nullptr /*ident */,
                // Unfinished indexes do not need two-phase drop because the incomplete index will
                // never be recovered. This is an optimization that will return disk space to the
                // user more quickly.
                catalog::DataRemoval::kImmediate);
            wuow.commit();

            StorageRepairObserver::get(opCtx->getServiceContext())
                ->invalidatingModification(str::stream()
                                           << "Dropped unfinished index '" << indexName << "' on "
                                           << collection->ns().toStringForErrorMsg());
        }
    }
    return Status::OK();
}

Status repairCollections(OperationContext* opCtx,
                         StorageEngine* engine,
                         const DatabaseName& dbName) {
    auto colls = CollectionCatalog::get(opCtx)->getAllCollectionNamesFromDb(opCtx, dbName);

    for (const auto& nss : colls) {
        auto status = repair::repairCollection(opCtx, engine, nss);
        if (!status.isOK()) {
            return status;
        }
    }
    return Status::OK();
}
}  // namespace

namespace repair {
Status repairDatabase(OperationContext* opCtx, StorageEngine* engine, const DatabaseName& dbName) {
    DisableDocumentValidation validationDisabler(opCtx);

    // We must hold some form of lock here
    invariant(opCtx->lockState()->isW());

    LOGV2(21029, "repairDatabase", logAttrs(dbName));

    opCtx->checkForInterrupt();

    // Close the db and invalidate all current users and caches.
    auto databaseHolder = DatabaseHolder::get(opCtx);
    databaseHolder->close(opCtx, dbName);

    // Sucessfully re-opening the db is necessary for repairCollections.
    openDbAndRepairIndexSpec(opCtx, dbName);

    auto status = repairCollections(opCtx, engine, dbName);
    if (!status.isOK()) {
        LOGV2_FATAL_CONTINUE(21030,
                             "Failed to repair database {dbName}: {status_reason}",
                             "Failed to repair database",
                             logAttrs(dbName),
                             "error"_attr = status);
    }

    try {
        // Restore oplog Collection pointer cache.
        repl::acquireOplogCollectionForLogging(opCtx);
    } catch (...) {
        // The only expected exception is an interrupt.
        opCtx->checkForInterrupt();
        LOGV2_FATAL_CONTINUE(
            21031,
            "Unexpected exception encountered while reacquiring oplog collection after repair.");
        std::terminate();  // Logs additional info about the specific error.
    }

    return status;
}

Status repairCollection(OperationContext* opCtx,
                        StorageEngine* engine,
                        const NamespaceString& nss) {
    opCtx->checkForInterrupt();

    LOGV2(21027, "Repairing collection", logAttrs(nss));

    Status status = Status::OK();
    {
        auto collection = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, nss);
        status = engine->repairRecordStore(opCtx, collection->getCatalogId(), nss);
    }


    // Need to lookup from catalog again because the old collection object was invalidated by
    // repairRecordStore.
    auto collection =
        CollectionCatalog::get(opCtx)->lookupCollectionByNamespaceForMetadataWrite(opCtx, nss);

    // If data was modified during repairRecordStore, we know to rebuild indexes without needing
    // to run an expensive collection validation.
    if (status.code() == ErrorCodes::DataModifiedByRepair) {
        invariant(StorageRepairObserver::get(opCtx->getServiceContext())->isDataInvalidated(),
                  "Collection '{}' ({})"_format(toStringForLogging(collection->ns()),
                                                collection->uuid().toString()));

        // If we are a replica set member in standalone mode and we have unfinished indexes,
        // drop them before rebuilding any completed indexes. Since we have already made
        // invalidating modifications to our data, it is safe to just drop the indexes entirely
        // to avoid the risk of the index rebuild failing.
        if (getReplSetMemberInStandaloneMode(opCtx->getServiceContext())) {
            if (auto status = dropUnfinishedIndexes(opCtx, collection); !status.isOK()) {
                return status;
            }
        }

        return rebuildIndexesForNamespace(opCtx, nss, engine);
    } else if (!status.isOK()) {
        return status;
    }

    // Run collection validation to avoid unnecessarily rebuilding indexes on valid collections
    // with consistent indexes. Initialize the collection prior to validation.
    collection->init(opCtx);

    ValidateResults validateResults;
    BSONObjBuilder output;

    // Exclude full record store validation because we have already validated the underlying
    // record store in the call to repairRecordStore above.
    status =
        CollectionValidation::validate(opCtx,
                                       nss,
                                       CollectionValidation::ValidateMode::kForegroundFullIndexOnly,
                                       CollectionValidation::RepairMode::kFixErrors,
                                       &validateResults,
                                       &output,
                                       /*logDiagnostics=*/false);
    if (!status.isOK()) {
        return status;
    }

    BSONObjBuilder detailedResults;
    const bool debug = false;
    validateResults.appendToResultObj(&detailedResults, debug);

    LOGV2(21028,
          "Collection validation",
          "results"_attr = output.done(),
          "detailedResults"_attr = detailedResults.done());

    if (validateResults.repaired) {
        if (validateResults.valid) {
            LOGV2(4934000, "Validate successfully repaired all data", "collection"_attr = nss);
        } else {
            LOGV2(4934001, "Validate was unable to repair all data", "collection"_attr = nss);
        }
    } else {
        LOGV2(4934002, "Validate did not make any repairs", "collection"_attr = nss);
    }

    // If not valid, whether repair ran or not, indexes will need to be rebuilt.
    if (!validateResults.valid) {
        return rebuildIndexesForNamespace(opCtx, nss, engine);
    }
    return Status::OK();
}
}  // namespace repair

}  // namespace mongo
