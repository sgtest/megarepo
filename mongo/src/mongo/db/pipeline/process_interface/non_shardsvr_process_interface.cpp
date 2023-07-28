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

#include "mongo/db/pipeline/process_interface/non_shardsvr_process_interface.h"

#include <boost/preprocessor/control/iif.hpp>
#include <typeinfo>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/create_collection.h"
#include "mongo/db/catalog/database.h"
#include "mongo/db/catalog/drop_collection.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/list_indexes.h"
#include "mongo/db/catalog/rename_collection.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/index_builds_coordinator.h"
#include "mongo/db/ops/single_write_result_gen.h"
#include "mongo/db/ops/write_ops.h"
#include "mongo/db/ops/write_ops_exec.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/pipeline/document_source_cursor.h"
#include "mongo/db/repl/speculative_majority_read_info.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/util/str.h"

namespace mongo {

std::unique_ptr<Pipeline, PipelineDeleter>
NonShardServerProcessInterface::attachCursorSourceToPipeline(
    Pipeline* ownedPipeline,
    ShardTargetingPolicy shardTargetingPolicy,
    boost::optional<BSONObj> readConcern) {
    return attachCursorSourceToPipelineForLocalRead(ownedPipeline);
}

std::unique_ptr<Pipeline, PipelineDeleter>
NonShardServerProcessInterface::attachCursorSourceToPipeline(
    const AggregateCommandRequest& aggRequest,
    Pipeline* pipeline,
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    boost::optional<BSONObj> shardCursorsSortSpec,
    ShardTargetingPolicy shardTargetingPolicy,
    boost::optional<BSONObj> readConcern) {
    return attachCursorSourceToPipelineForLocalRead(pipeline, aggRequest);
}

std::list<BSONObj> NonShardServerProcessInterface::getIndexSpecs(OperationContext* opCtx,
                                                                 const NamespaceString& ns,
                                                                 bool includeBuildUUIDs) {
    return listIndexesEmptyListIfMissing(
        opCtx, ns, includeBuildUUIDs ? ListIndexesInclude::BuildUUID : ListIndexesInclude::Nothing);
}

std::vector<FieldPath> NonShardServerProcessInterface::collectDocumentKeyFieldsActingAsRouter(
    OperationContext* opCtx, const NamespaceString& nss) const {
    return {"_id"};  // Nothing is sharded.
}

boost::optional<Document> NonShardServerProcessInterface::lookupSingleDocument(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const NamespaceString& nss,
    UUID collectionUUID,
    const Document& documentKey,
    boost::optional<BSONObj> readConcern) {
    MakePipelineOptions opts;
    opts.shardTargetingPolicy = ShardTargetingPolicy::kNotAllowed;
    opts.readConcern = std::move(readConcern);

    auto lookedUpDocument =
        doLookupSingleDocument(expCtx, nss, collectionUUID, documentKey, std::move(opts));

    // Set the speculative read timestamp appropriately after we do a document lookup locally. We
    // set the speculative read timestamp based on the timestamp used by the transaction.
    repl::SpeculativeMajorityReadInfo& speculativeMajorityReadInfo =
        repl::SpeculativeMajorityReadInfo::get(expCtx->opCtx);
    if (speculativeMajorityReadInfo.isSpeculativeRead()) {
        // Speculative majority reads are required to use the 'kNoOverlap' read source.
        // Storage engine operations require at least Global IS.
        Lock::GlobalLock lk(expCtx->opCtx, MODE_IS);
        invariant(expCtx->opCtx->recoveryUnit()->getTimestampReadSource() ==
                  RecoveryUnit::ReadSource::kNoOverlap);
        boost::optional<Timestamp> readTs =
            expCtx->opCtx->recoveryUnit()->getPointInTimeReadTimestamp(expCtx->opCtx);
        invariant(readTs);
        speculativeMajorityReadInfo.setSpeculativeReadTimestampForward(*readTs);
    }

    return lookedUpDocument;
}

Status NonShardServerProcessInterface::insert(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const NamespaceString& ns,
    std::unique_ptr<write_ops::InsertCommandRequest> insertCommand,
    const WriteConcernOptions& wc,
    boost::optional<OID> targetEpoch) {
    auto writeResults = write_ops_exec::performInserts(expCtx->opCtx, *insertCommand);

    // Need to check each result in the batch since the writes are unordered.
    for (const auto& result : writeResults.results) {
        if (result.getStatus() != Status::OK()) {
            return result.getStatus();
        }
    }
    return Status::OK();
}

Status NonShardServerProcessInterface::insertTimeseries(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const NamespaceString& ns,
    std::unique_ptr<write_ops::InsertCommandRequest> insertCommand,
    const WriteConcernOptions& wc,
    boost::optional<OID> targetEpoch) {
    try {
        auto insertReply = write_ops_exec::performTimeseriesWrites(expCtx->opCtx, *insertCommand);

        checkWriteErrors(insertReply.getWriteCommandReplyBase());
    } catch (DBException& ex) {
        ex.addContext(str::stream() << "time-series insert failed: " << ns.toStringForErrorMsg());
        throw;
    }
    return Status::OK();
}

StatusWith<MongoProcessInterface::UpdateResult> NonShardServerProcessInterface::update(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const NamespaceString& ns,
    std::unique_ptr<write_ops::UpdateCommandRequest> updateCommand,
    const WriteConcernOptions& wc,
    UpsertType upsert,
    bool multi,
    boost::optional<OID> targetEpoch) {
    auto writeResults = write_ops_exec::performUpdates(expCtx->opCtx, *updateCommand);

    // Need to check each result in the batch since the writes are unordered.
    UpdateResult updateResult;
    for (const auto& result : writeResults.results) {
        if (result.getStatus() != Status::OK()) {
            return result.getStatus();
        }

        updateResult.nMatched += result.getValue().getN();
        updateResult.nModified += result.getValue().getNModified();
    }
    return updateResult;
}

void NonShardServerProcessInterface::createIndexesOnEmptyCollection(
    OperationContext* opCtx, const NamespaceString& ns, const std::vector<BSONObj>& indexSpecs) {
    AutoGetCollection autoColl(opCtx, ns, MODE_X);
    CollectionWriter collection(opCtx, autoColl);
    writeConflictRetry(
        opCtx, "CommonMongodProcessInterface::createIndexesOnEmptyCollection", ns, [&] {
            uassert(ErrorCodes::DatabaseDropPending,
                    str::stream() << "The database is in the process of being dropped "
                                  << ns.dbName().toStringForErrorMsg(),
                    autoColl.getDb() && !autoColl.getDb()->isDropPending(opCtx));

            uassert(ErrorCodes::NamespaceNotFound,
                    str::stream() << "Failed to create indexes for aggregation because collection "
                                     "does not exist: "
                                  << ns.toStringForErrorMsg() << ": "
                                  << BSON("indexes" << indexSpecs),
                    collection.get());

            invariant(collection->isEmpty(opCtx),
                      str::stream() << "Expected empty collection for index creation: "
                                    << ns.toStringForErrorMsg()
                                    << ": numRecords: " << collection->numRecords(opCtx) << ": "
                                    << BSON("indexes" << indexSpecs));

            // Secondary index builds do not filter existing indexes so we have to do this on the
            // primary.
            auto removeIndexBuildsToo = false;
            auto filteredIndexes = collection->getIndexCatalog()->removeExistingIndexes(
                opCtx, collection.get(), indexSpecs, removeIndexBuildsToo);
            if (filteredIndexes.empty()) {
                return;
            }

            WriteUnitOfWork wuow(opCtx);
            IndexBuildsCoordinator::get(opCtx)->createIndexesOnEmptyCollection(
                opCtx, collection, filteredIndexes, false  // fromMigrate
            );
            wuow.commit();
        });
}
void NonShardServerProcessInterface::renameIfOptionsAndIndexesHaveNotChanged(
    OperationContext* opCtx,
    const NamespaceString& sourceNs,
    const NamespaceString& targetNs,
    bool dropTarget,
    bool stayTemp,
    const BSONObj& originalCollectionOptions,
    const std::list<BSONObj>& originalIndexes) {
    RenameCollectionOptions options;
    options.dropTarget = dropTarget;
    options.stayTemp = stayTemp;
    // skip sharding validation on non sharded servers
    doLocalRenameIfOptionsAndIndexesHaveNotChanged(
        opCtx, sourceNs, targetNs, options, originalIndexes, originalCollectionOptions);
}

void NonShardServerProcessInterface::createTimeseriesView(OperationContext* opCtx,
                                                          const NamespaceString& ns,
                                                          const BSONObj& cmdObj,
                                                          const TimeseriesOptions& userOpts) {
    try {
        uassertStatusOK(mongo::createTimeseries(opCtx, ns, cmdObj));
    } catch (DBException& ex) {
        _handleTimeseriesCreateError(ex, opCtx, ns, userOpts);
    }
}

void NonShardServerProcessInterface::createCollection(OperationContext* opCtx,
                                                      const DatabaseName& dbName,
                                                      const BSONObj& cmdObj) {
    uassertStatusOK(mongo::createCollection(opCtx, dbName, cmdObj));
}

void NonShardServerProcessInterface::dropCollection(OperationContext* opCtx,
                                                    const NamespaceString& ns) {
    uassertStatusOK(mongo::dropCollectionForApplyOps(
        opCtx, ns, {}, DropCollectionSystemCollectionMode::kDisallowSystemCollectionDrops));
}

BSONObj NonShardServerProcessInterface::preparePipelineAndExplain(
    Pipeline* ownedPipeline, ExplainOptions::Verbosity verbosity) {
    std::vector<Value> pipelineVec;
    auto firstStage = ownedPipeline->peekFront();
    auto opts = SerializationOptions{.verbosity = verbosity};
    // If the pipeline already has a cursor explain with that one, otherwise attach a new one like
    // we would for a normal execution and explain that.
    if (firstStage && typeid(*firstStage) == typeid(DocumentSourceCursor)) {
        // Managed pipeline goes out of scope at the end of this else block, but we've already
        // extracted the necessary information and won't need it again.
        std::unique_ptr<Pipeline, PipelineDeleter> managedPipeline(
            ownedPipeline, PipelineDeleter(ownedPipeline->getContext()->opCtx));
        pipelineVec = managedPipeline->writeExplainOps(opts);
        ownedPipeline = nullptr;
    } else {
        auto pipelineWithCursor = attachCursorSourceToPipelineForLocalRead(ownedPipeline);
        // If we need execution stats, this runs the plan in order to gather the stats.
        if (verbosity >= ExplainOptions::Verbosity::kExecStats) {
            while (pipelineWithCursor->getNext()) {
            }
        }
        pipelineVec = pipelineWithCursor->writeExplainOps(opts);
    }
    BSONArrayBuilder bab;
    for (auto&& stage : pipelineVec) {
        bab << stage;
    }

    return BSON("pipeline" << bab.arr());
}

}  // namespace mongo
