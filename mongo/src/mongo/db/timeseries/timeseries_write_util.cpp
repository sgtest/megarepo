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


#include "mongo/db/timeseries/timeseries_write_util.h"

#include <cstdint>
#include <string>
#include <type_traits>
#include <utility>

#include <absl/container/flat_hash_map.h>
#include <absl/meta/type_traits.h>
#include <boost/container/small_vector.hpp>
#include <boost/container/vector.hpp>
#include <boost/cstdint.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/client/dbclient_cursor.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_operation_source.h"
#include "mongo/db/catalog/collection_write_path.h"
#include "mongo/db/catalog/document_validation.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/curop.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/ops/delete_request_gen.h"
#include "mongo/db/ops/update_request.h"
#include "mongo/db/ops/write_ops_exec_util.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/db/record_id_helpers.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/tenant_migration_decoration.h"
#include "mongo/db/server_options.h"
#include "mongo/db/storage/snapshot.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_catalog.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_catalog_helpers.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_catalog_internal.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_identifiers.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_metadata.h"
#include "mongo/db/timeseries/bucket_catalog/execution_stats.h"
#include "mongo/db/timeseries/bucket_catalog/flat_bson.h"
#include "mongo/db/timeseries/bucket_catalog/reopening.h"
#include "mongo/db/timeseries/bucket_compression.h"
#include "mongo/db/timeseries/timeseries_constants.h"
#include "mongo/db/timeseries/timeseries_index_schema_conversion_functions.h"
#include "mongo/db/timeseries/timeseries_options.h"
#include "mongo/db/timeseries/timeseries_update_delete_util.h"
#include "mongo/db/update/document_diff_applier.h"
#include "mongo/db/update/document_diff_serialization.h"
#include "mongo/db/update/update_oplog_entry_serialization.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decimal_counter.h"
#include "mongo/util/decorable.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"
#include "mongo/util/string_map.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage


namespace mongo::timeseries {
namespace {

// Builds the data field of a bucket document. Computes the min and max fields if necessary.
boost::optional<bucket_catalog::MinMax> processTimeseriesMeasurements(
    const std::vector<BSONObj>& measurements,
    const BSONObj& metadata,
    StringDataMap<BSONObjBuilder>& dataBuilders,
    const boost::optional<TimeseriesOptions>& options = boost::none,
    const boost::optional<const StringDataComparator*>& comparator = boost::none) {
    bucket_catalog::MinMax minmax;
    bool computeMinmax = options && comparator;

    auto metadataElem = metadata.firstElement();
    boost::optional<StringData> metaFieldName;
    if (metadataElem) {
        metaFieldName = metadataElem.fieldNameStringData();
    }

    DecimalCounter<uint32_t> count;
    for (const auto& doc : measurements) {
        if (computeMinmax) {
            minmax.update(doc, metaFieldName, *comparator);
        }
        for (const auto& elem : doc) {
            auto key = elem.fieldNameStringData();
            if (key == metaFieldName) {
                continue;
            }
            dataBuilders[key].appendAs(elem, count);
        }
        ++count;
    }

    // Rounds the minimum timestamp and updates the min time field.
    if (computeMinmax) {
        auto minTime = roundTimestampToGranularity(
            minmax.min().getField(options->getTimeField()).Date(), *options);
        auto controlDoc =
            bucket_catalog::buildControlMinTimestampDoc(options->getTimeField(), minTime);
        minmax.update(controlDoc, /*metaField=*/boost::none, *comparator);
        return minmax;
    }

    return boost::none;
}

// Builds a complete and new bucket document.
BucketDocument makeNewDocument(const OID& bucketId,
                               const BSONObj& metadata,
                               const BSONObj& min,
                               const BSONObj& max,
                               StringDataMap<BSONObjBuilder>& dataBuilders,
                               StringData timeField,
                               const NamespaceString& nss) {
    auto metadataElem = metadata.firstElement();
    BSONObjBuilder builder;
    builder.append("_id", bucketId);
    {
        BSONObjBuilder bucketControlBuilder(builder.subobjStart("control"));
        bucketControlBuilder.append(kBucketControlVersionFieldName,
                                    kTimeseriesControlUncompressedVersion);
        bucketControlBuilder.append(kBucketControlMinFieldName, min);
        bucketControlBuilder.append(kBucketControlMaxFieldName, max);
    }
    if (metadataElem) {
        builder.appendAs(metadataElem, kBucketMetaFieldName);
    }
    {
        BSONObjBuilder bucketDataBuilder(builder.subobjStart(kBucketDataFieldName));
        for (auto& dataBuilder : dataBuilders) {
            bucketDataBuilder.append(dataBuilder.first, dataBuilder.second.obj());
        }
    }

    BucketDocument bucketDoc{builder.obj()};
    if (!feature_flags::gTimeseriesAlwaysUseCompressedBuckets.isEnabled(
            serverGlobalParams.featureCompatibility.acquireFCVSnapshot())) {
        return bucketDoc;
    }

    const bool validateCompression = gValidateTimeseriesCompression.load();
    auto compressed = timeseries::compressBucket(
        bucketDoc.uncompressedBucket, timeField, nss, validateCompression);
    if (compressed.compressedBucket) {
        bucketDoc.compressedBucket = std::move(*compressed.compressedBucket);
    } else {
        bucketDoc.compressionFailed = true;
    }

    return bucketDoc;
}

// Makes a write command request base and sets the statement Ids if provided a non-empty vector.
write_ops::WriteCommandRequestBase makeTimeseriesWriteOpBase(std::vector<StmtId>&& stmtIds) {
    write_ops::WriteCommandRequestBase base;

    // The schema validation configured in the bucket collection is intended for direct
    // operations by end users and is not applicable here.
    base.setBypassDocumentValidation(true);

    if (!stmtIds.empty()) {
        base.setStmtIds(std::move(stmtIds));
    }

    return base;
}

// Generates a delta update oplog entry using the before and after compressed bucket documents.
write_ops::UpdateOpEntry makeTimeseriesCompressedDiffEntry(
    OperationContext* opCtx,
    std::shared_ptr<bucket_catalog::WriteBatch> batch,
    const BSONObj& before,
    const BSONObj& after,
    const BSONObj& metadata) {
    BSONObjBuilder updateBuilder;
    {
        // Control builder.
        BSONObjBuilder controlBuilder(updateBuilder.subobjStart(
            str::stream() << doc_diff::kSubDiffSectionFieldPrefix << kBucketControlFieldName));
        BSONObj countObj =
            BSON(kBucketControlCountFieldName << after.getObjectField(kBucketControlFieldName)
                                                     .getField(kBucketControlCountFieldName)
                                                     .Int());
        controlBuilder.append(doc_diff::kUpdateSectionFieldName, countObj);

        if (!batch->min.isEmpty() || !batch->max.isEmpty()) {
            if (!batch->min.isEmpty()) {
                controlBuilder.append(str::stream() << doc_diff::kSubDiffSectionFieldPrefix
                                                    << kBucketControlMinFieldName,
                                      batch->min);
            }
            if (!batch->max.isEmpty()) {
                controlBuilder.append(str::stream() << doc_diff::kSubDiffSectionFieldPrefix
                                                    << kBucketControlMaxFieldName,
                                      batch->max);
            }
        }
    }

    {
        // Data builder.
        const BSONObj& beforeData = before.getObjectField(kBucketDataFieldName);
        const BSONObj& afterData = after.getObjectField(kBucketDataFieldName);

        BSONObjBuilder dataBuilder(updateBuilder.subobjStart("sdata"));
        BSONObjBuilder newDataFieldsBuilder;
        BSONObjBuilder updatedDataFieldsBuilder;
        auto beforeIt = beforeData.begin();
        auto afterIt = afterData.begin();

        while (beforeIt != beforeData.end()) {
            invariant(afterIt != afterData.end());
            invariant(beforeIt->fieldNameStringData() == afterIt->fieldNameStringData());

            if (beforeIt->binaryEqual(*afterIt)) {
                // Contents are the same, nothing to diff.
                beforeIt++;
                afterIt++;
                continue;
            }

            // Generate the binary diff.
            int beforeLen = 0;
            const char* beforeData = beforeIt->binData(beforeLen);

            int afterLen = 0;
            const char* afterData = afterIt->binData(afterLen);

            int offset = 0;
            while (beforeData[offset] == afterData[offset]) {
                if (offset == beforeLen - 1 || offset == afterLen - 1) {
                    break;
                }
                offset++;
            }

            BSONObj binaryObj = BSON("o" << offset << "d"
                                         << BSONBinData(afterData + offset,
                                                        afterLen - offset,
                                                        BinDataType::BinDataGeneral));
            updatedDataFieldsBuilder.append(beforeIt->fieldNameStringData(), binaryObj);
            beforeIt++;
            afterIt++;
        }

        // Finish consuming the after iterator, which should only contain new fields at this point
        // as we've finished consuming the before iterator.
        while (afterIt != afterData.end()) {
            // Newly inserted fields are added as DocDiff inserts using the BSONColumn format.
            invariant(batch->newFieldNamesToBeInserted.count(afterIt->fieldNameStringData()) == 1);
            newDataFieldsBuilder.append(*afterIt);
            afterIt++;
        }

        auto newDataFields = newDataFieldsBuilder.obj();
        if (!newDataFields.isEmpty()) {
            dataBuilder.append(doc_diff::kInsertSectionFieldName, newDataFields);
        }

        auto updatedDataFields = updatedDataFieldsBuilder.obj();
        if (!updatedDataFields.isEmpty()) {
            dataBuilder.append(doc_diff::kBinarySectionFieldName, updatedDataFields);
        }
    }

    write_ops::UpdateModification::DiffOptions options;
    options.mustCheckExistenceForInsertOperations =
        static_cast<bool>(repl::tenantMigrationInfo(opCtx));
    write_ops::UpdateModification u(
        updateBuilder.obj(), write_ops::UpdateModification::DeltaTag{}, options);
    auto oid = batch->bucketHandle.bucketId.oid;
    write_ops::UpdateOpEntry update(BSON("_id" << oid), std::move(u));
    invariant(!update.getMulti(), oid.toString());
    invariant(!update.getUpsert(), oid.toString());
    return update;
}

// Builds the delta update oplog entry from a time-series insert write batch.
write_ops::UpdateOpEntry makeTimeseriesUpdateOpEntry(
    OperationContext* opCtx,
    std::shared_ptr<bucket_catalog::WriteBatch> batch,
    const BSONObj& metadata) {
    BSONObjBuilder updateBuilder;
    {
        if (!batch->min.isEmpty() || !batch->max.isEmpty()) {
            BSONObjBuilder controlBuilder(updateBuilder.subobjStart(
                str::stream() << doc_diff::kSubDiffSectionFieldPrefix << "control"));
            if (!batch->min.isEmpty()) {
                controlBuilder.append(
                    str::stream() << doc_diff::kSubDiffSectionFieldPrefix << "min", batch->min);
            }
            if (!batch->max.isEmpty()) {
                controlBuilder.append(
                    str::stream() << doc_diff::kSubDiffSectionFieldPrefix << "max", batch->max);
            }
        }
    }
    {  // doc_diff::kSubDiffSectionFieldPrefix + <field name> => {<index_0>: ..., <index_1>:}
        StringDataMap<BSONObjBuilder> dataFieldBuilders;
        auto metadataElem = metadata.firstElement();
        DecimalCounter<uint32_t> count(batch->numPreviouslyCommittedMeasurements);
        for (const auto& doc : batch->measurements) {
            for (const auto& elem : doc) {
                auto key = elem.fieldNameStringData();
                if (metadataElem && key == metadataElem.fieldNameStringData()) {
                    continue;
                }
                auto& builder = dataFieldBuilders[key];
                builder.appendAs(elem, count);
            }
            ++count;
        }

        // doc_diff::kSubDiffSectionFieldPrefix + <field name>
        BSONObjBuilder dataBuilder(updateBuilder.subobjStart("sdata"));
        BSONObjBuilder newDataFieldsBuilder;
        for (auto& pair : dataFieldBuilders) {
            // Existing 'data' fields with measurements require different treatment from fields
            // not observed before (missing from control.min and control.max).
            if (batch->newFieldNamesToBeInserted.count(pair.first)) {
                newDataFieldsBuilder.append(pair.first, pair.second.obj());
            }
        }
        auto newDataFields = newDataFieldsBuilder.obj();
        if (!newDataFields.isEmpty()) {
            dataBuilder.append(doc_diff::kInsertSectionFieldName, newDataFields);
        }
        for (auto& pair : dataFieldBuilders) {
            // Existing 'data' fields with measurements require different treatment from fields
            // not observed before (missing from control.min and control.max).
            if (!batch->newFieldNamesToBeInserted.count(pair.first)) {
                dataBuilder.append(doc_diff::kSubDiffSectionFieldPrefix + pair.first.toString(),
                                   BSON(doc_diff::kInsertSectionFieldName << pair.second.obj()));
            }
        }
    }
    write_ops::UpdateModification::DiffOptions options;
    options.mustCheckExistenceForInsertOperations =
        static_cast<bool>(repl::tenantMigrationInfo(opCtx));
    write_ops::UpdateModification u(
        updateBuilder.obj(), write_ops::UpdateModification::DeltaTag{}, options);
    auto oid = batch->bucketHandle.bucketId.oid;
    write_ops::UpdateOpEntry update(BSON("_id" << oid), std::move(u));
    invariant(!update.getMulti(), oid.toString());
    invariant(!update.getUpsert(), oid.toString());
    return update;
}

// Performs the storage write of an update to a time-series bucket document.
void updateTimeseriesDocument(OperationContext* opCtx,
                              const CollectionPtr& coll,
                              const write_ops::UpdateCommandRequest& op,
                              OpDebug* opDebug,
                              bool fromMigrate,
                              StmtId stmtId) {
    invariant(op.getUpdates().size() == 1);
    auto& update = op.getUpdates().front();

    invariant(coll->isClustered());
    auto recordId = record_id_helpers::keyForOID(update.getQ()["_id"].OID());

    auto original = coll->docFor(opCtx, recordId);

    CollectionUpdateArgs args{original.value()};
    args.criteria = update.getQ();
    args.stmtIds = {stmtId};
    if (fromMigrate) {
        args.source = OperationSource::kFromMigrate;
    }

    BSONObj updated;
    BSONObj diffFromUpdate;
    const BSONObj* diffOnIndexes =
        collection_internal::kUpdateAllIndexes;  // Assume all indexes are affected.
    if (update.getU().type() == write_ops::UpdateModification::Type::kDelta) {
        diffFromUpdate = update.getU().getDiff();
        updated = doc_diff::applyDiff(
            original.value(), diffFromUpdate, static_cast<bool>(repl::tenantMigrationInfo(opCtx)));
        diffOnIndexes = &diffFromUpdate;
        args.update = update_oplog_entry::makeDeltaOplogEntry(diffFromUpdate);
    } else if (update.getU().type() == write_ops::UpdateModification::Type::kTransform) {
        const auto& transform = update.getU().getTransform();
        auto transformed = transform(original.value());
        tassert(7667900,
                "Could not apply transformation to time series bucket document",
                transformed.has_value());
        updated = std::move(transformed.value());
        args.update = update_oplog_entry::makeReplacementOplogEntry(updated);
    } else if (update.getU().type() == write_ops::UpdateModification::Type::kReplacement) {
        updated = update.getU().getUpdateReplacement();
        args.update = update_oplog_entry::makeReplacementOplogEntry(updated);
    } else {
        invariant(false, "Unexpected update type");
    }

    collection_internal::updateDocument(opCtx,
                                        coll,
                                        recordId,
                                        original,
                                        updated,
                                        diffOnIndexes,
                                        nullptr /*indexesAffected*/,
                                        opDebug,
                                        &args);
}

std::shared_ptr<bucket_catalog::WriteBatch>& extractFromSelf(
    std::shared_ptr<bucket_catalog::WriteBatch>& batch) {
    return batch;
}

}  // namespace

write_ops::UpdateCommandRequest buildSingleUpdateOp(const write_ops::UpdateCommandRequest& wholeOp,
                                                    size_t opIndex) {
    write_ops::UpdateCommandRequest singleUpdateOp(wholeOp.getNamespace(),
                                                   {wholeOp.getUpdates()[opIndex]});
    auto commandBase = singleUpdateOp.getWriteCommandRequestBase();
    commandBase.setOrdered(wholeOp.getOrdered());
    commandBase.setBypassDocumentValidation(wholeOp.getBypassDocumentValidation());

    return singleUpdateOp;
}

void assertTimeseriesBucketsCollection(const Collection* bucketsColl) {
    uassert(
        8555700,
        "Catalog changed during operation, could not find time series buckets collection for write",
        bucketsColl);
    uassert(8555701,
            "Catalog changed during operation, missing time-series options",
            bucketsColl->getTimeseriesOptions());
}

BucketDocument makeNewDocumentForWrite(std::shared_ptr<bucket_catalog::WriteBatch> batch,
                                       const BSONObj& metadata) {
    StringDataMap<BSONObjBuilder> dataBuilders;
    processTimeseriesMeasurements(
        {batch->measurements.begin(), batch->measurements.end()}, metadata, dataBuilders);

    return makeNewDocument(batch->bucketHandle.bucketId.oid,
                           metadata,
                           batch->min,
                           batch->max,
                           dataBuilders,
                           batch->timeField,
                           batch->bucketHandle.bucketId.ns);
}

BucketDocument makeNewDocumentForWrite(
    const NamespaceString& nss,
    const OID& bucketId,
    const std::vector<BSONObj>& measurements,
    const BSONObj& metadata,
    const TimeseriesOptions& options,
    const boost::optional<const StringDataComparator*>& comparator) {
    StringDataMap<BSONObjBuilder> dataBuilders;
    auto minmax =
        processTimeseriesMeasurements(measurements, metadata, dataBuilders, options, comparator);

    invariant(minmax);

    return makeNewDocument(bucketId,
                           metadata,
                           minmax->min(),
                           minmax->max(),
                           dataBuilders,
                           options.getTimeField(),
                           nss);
}

BSONObj makeBucketDocument(const std::vector<BSONObj>& measurements,
                           const NamespaceString& nss,
                           const TimeseriesOptions& options,
                           const StringDataComparator* comparator) {
    std::vector<write_ops::InsertCommandRequest> insertOps;
    auto res = uassertStatusOK(bucket_catalog::internal::extractBucketingParameters(
        nss, comparator, options, measurements[0]));
    auto time = res.second;
    auto [oid, _] = bucket_catalog::internal::generateBucketOID(time, options);
    BucketDocument bucketDoc = makeNewDocumentForWrite(
        nss, oid, measurements, res.first.metadata.toBSON(), options, comparator);

    // TODO SERVER-80653: Handle bucket compression failure.
    if (bucketDoc.compressedBucket) {
        return *bucketDoc.compressedBucket;
    }
    return bucketDoc.uncompressedBucket;
}

std::variant<write_ops::UpdateCommandRequest, write_ops::DeleteCommandRequest> makeModificationOp(
    const OID& bucketId, const CollectionPtr& coll, const std::vector<BSONObj>& measurements) {
    // A bucket will be fully deleted if no measurements are passed in.
    if (measurements.empty()) {
        write_ops::DeleteOpEntry deleteEntry(BSON("_id" << bucketId), false);
        write_ops::DeleteCommandRequest op(coll->ns(), {deleteEntry});
        return op;
    }
    auto timeseriesOptions = coll->getTimeseriesOptions();
    invariant(timeseriesOptions);

    auto metaFieldName = timeseriesOptions->getMetaField();
    auto metadata = [&] {
        if (!metaFieldName) {  // Collection has no metadata field.
            return BSONObj();
        }
        // Look for the metadata field on this bucket and return it if present.
        auto metaField = measurements[0].getField(*metaFieldName);
        return metaField ? metaField.wrap() : BSONObj();
    }();

    // TODO SERVER-80653: Handle bucket compression failure.
    BucketDocument bucketDoc = makeNewDocumentForWrite(coll->ns(),
                                                       bucketId,
                                                       measurements,
                                                       metadata,
                                                       *timeseriesOptions,
                                                       coll->getDefaultCollator());
    BSONObj bucketToReplace = bucketDoc.uncompressedBucket;
    if (bucketDoc.compressedBucket) {
        bucketToReplace = *bucketDoc.compressedBucket;
    }

    write_ops::UpdateModification u(bucketToReplace);
    write_ops::UpdateOpEntry updateEntry(BSON("_id" << bucketId), std::move(u));
    write_ops::UpdateCommandRequest op(coll->ns(), {updateEntry});
    return op;
}

write_ops::UpdateOpEntry makeTimeseriesTransformationOpEntry(
    OperationContext* opCtx,
    const OID& bucketId,
    write_ops::UpdateModification::TransformFunc transformationFunc) {
    write_ops::UpdateModification u(std::move(transformationFunc));
    write_ops::UpdateOpEntry update(BSON("_id" << bucketId), std::move(u));
    invariant(!update.getMulti(), bucketId.toString());
    invariant(!update.getUpsert(), bucketId.toString());
    return update;
}

void getOpTimeAndElectionId(OperationContext* opCtx,
                            boost::optional<repl::OpTime>* opTime,
                            boost::optional<OID>* electionId) {
    auto* replCoord = repl::ReplicationCoordinator::get(opCtx->getServiceContext());
    const auto isReplSet = replCoord->getSettings().isReplSet();

    *opTime = isReplSet
        ? boost::make_optional(repl::ReplClientInfo::forClient(opCtx->getClient()).getLastOp())
        : boost::none;
    *electionId = isReplSet ? boost::make_optional(replCoord->getElectionId()) : boost::none;
}

write_ops::InsertCommandRequest makeTimeseriesInsertOp(
    std::shared_ptr<bucket_catalog::WriteBatch> batch,
    const NamespaceString& bucketsNs,
    const BSONObj& metadata,
    std::vector<StmtId>&& stmtIds) {
    // TODO SERVER-80653: Handle bucket compression failure.
    BucketDocument bucketDoc = makeNewDocumentForWrite(batch, metadata);
    BSONObj bucketToInsert = bucketDoc.uncompressedBucket;

    if (feature_flags::gTimeseriesAlwaysUseCompressedBuckets.isEnabled(
            serverGlobalParams.featureCompatibility.acquireFCVSnapshot())) {
        invariant(bucketDoc.compressedBucket);
        batch->uncompressed = bucketDoc.uncompressedBucket.getOwned();
    }
    if (bucketDoc.compressedBucket) {
        batch->compressed = bucketDoc.compressedBucket->getOwned();
        bucketToInsert = *bucketDoc.compressedBucket;
    }

    write_ops::InsertCommandRequest op{bucketsNs, {bucketToInsert}};
    op.setWriteCommandRequestBase(makeTimeseriesWriteOpBase(std::move(stmtIds)));
    return op;
}

write_ops::UpdateCommandRequest makeTimeseriesUpdateOp(
    OperationContext* opCtx,
    std::shared_ptr<bucket_catalog::WriteBatch> batch,
    const NamespaceString& bucketsNs,
    const BSONObj& metadata,
    std::vector<StmtId>&& stmtIds) {
    if (!feature_flags::gTimeseriesAlwaysUseCompressedBuckets.isEnabled(
            serverGlobalParams.featureCompatibility.acquireFCVSnapshot())) {
        write_ops::UpdateCommandRequest op(bucketsNs,
                                           {makeTimeseriesUpdateOpEntry(opCtx, batch, metadata)});
        op.setWriteCommandRequestBase(makeTimeseriesWriteOpBase(std::move(stmtIds)));
        return op;
    }

    auto updateMod = makeTimeseriesUpdateOpEntry(opCtx, batch, metadata).getU();
    auto updated = doc_diff::applyDiff(batch->uncompressed,
                                       updateMod.getDiff(),
                                       updateMod.mustCheckExistenceForInsertOperations());

    // Hold the uncompressed bucket document that's currently on-disk prior to this write batch
    // running.
    auto before = std::move(batch->uncompressed);

    // TODO SERVER-80653: Handle bucket compression failure.
    auto compressionResult = timeseries::compressBucket(
        updated, batch->timeField, bucketsNs, gValidateTimeseriesCompression.load());
    batch->uncompressed = updated;
    if (compressionResult.compressedBucket) {
        batch->compressed = *compressionResult.compressedBucket;
    } else {
        // Clear the previous compression state if we're inserting an uncompressed bucket due to
        // compression failure.
        batch->compressed = boost::none;
    }

    auto after = compressionResult.compressedBucket ? *compressionResult.compressedBucket : updated;

    auto bucketTransformationFunc = [before = std::move(before), after = std::move(after)](
                                        const BSONObj& bucketDoc) -> boost::optional<BSONObj> {
        // Make sure the document hasn't changed since we read it into the BucketCatalog.
        // This should not happen, but since we can double-check it here, we can guard
        // against the missed update that would result from simply replacing with 'after'.
        if (!timeseries::decompressBucket(bucketDoc).value_or(bucketDoc).binaryEqual(before)) {
            throwWriteConflictException("Bucket document changed between initial read and update");
        }
        return after;
    };

    auto updates = makeTimeseriesTransformationOpEntry(
        opCtx,
        /*bucketId=*/batch->bucketHandle.bucketId.oid,
        /*transformationFunc=*/std::move(bucketTransformationFunc));

    write_ops::UpdateCommandRequest op(bucketsNs, {updates});
    op.setWriteCommandRequestBase(makeTimeseriesWriteOpBase(std::move(stmtIds)));
    return op;
}

write_ops::UpdateCommandRequest makeTimeseriesDecompressAndUpdateOp(
    OperationContext* opCtx,
    std::shared_ptr<bucket_catalog::WriteBatch> batch,
    const NamespaceString& bucketsNs,
    const BSONObj& metadata,
    std::vector<StmtId>&& stmtIds) {
    invariant(feature_flags::gTimeseriesAlwaysUseCompressedBuckets.isEnabled(
        serverGlobalParams.featureCompatibility.acquireFCVSnapshot()));

    // Generate the diff and apply it against the previously decompressed bucket document.
    auto updateMod = makeTimeseriesUpdateOpEntry(opCtx, batch, metadata).getU();
    auto diff = updateMod.getDiff();
    auto updated = doc_diff::applyDiff(
        batch->uncompressed, diff, updateMod.mustCheckExistenceForInsertOperations());

    // Holds the compressed bucket document that's currently on-disk prior to this write batch
    // running.
    auto before = std::move(*batch->compressed);

    CompressionResult compressionResult = timeseries::compressBucket(
        updated, batch->timeField, bucketsNs, gValidateTimeseriesCompression.load());
    // TODO SERVER-80653: convert to uassert.
    invariant(compressionResult.compressedBucket);
    batch->uncompressed = updated;
    batch->compressed = *compressionResult.compressedBucket;

    // Generates a delta update request using the before and after compressed bucket documents.
    auto after = *compressionResult.compressedBucket;
    const auto updateEntry =
        makeTimeseriesCompressedDiffEntry(opCtx, batch, before, after, metadata);
    write_ops::UpdateCommandRequest op(bucketsNs, {updateEntry});
    op.setWriteCommandRequestBase(makeTimeseriesWriteOpBase(std::move(stmtIds)));
    return op;
}

StatusWith<bucket_catalog::InsertResult> attemptInsertIntoBucket(
    OperationContext* opCtx,
    bucket_catalog::BucketCatalog& bucketCatalog,
    const NamespaceString& viewNs,
    const Collection* bucketsColl,
    TimeseriesOptions& timeSeriesOptions,
    const BSONObj& measurementDoc,
    bucket_catalog::CombineWithInsertsFromOtherClients combine,
    bool fromUpdates) {
    StatusWith<bucket_catalog::InsertResult> swResult =
        Status{ErrorCodes::BadValue, "Uninitialized InsertResult"};
    do {
        // Avoids reopening existing buckets for the inserts of the updated measurements from
        // time-series user updates.
        if (!fromUpdates) {
            swResult = bucket_catalog::tryInsert(opCtx,
                                                 bucketCatalog,
                                                 viewNs,
                                                 bucketsColl->getDefaultCollator(),
                                                 timeSeriesOptions,
                                                 measurementDoc,
                                                 combine);

            if (swResult.isOK()) {
                auto& insertResult = swResult.getValue();

                if (auto* reopeningContext =
                        get_if<bucket_catalog::ReopeningContext>(&insertResult)) {
                    BSONObj suitableBucket;

                    if (auto* bucketId = get_if<OID>(&reopeningContext->candidate)) {
                        DBDirectClient client{opCtx};
                        suitableBucket =
                            client.findOne(bucketsColl->ns(), BSON("_id" << *bucketId));
                        reopeningContext->fetchedBucket = true;
                    } else if (auto* pipeline =
                                   get_if<std::vector<BSONObj>>(&reopeningContext->candidate)) {
                        // Resort to Query-Based reopening approach.
                        DBDirectClient client{opCtx};

                        // Ensure we have a index on meta and time for the time-series
                        // collection before performing the query. Without the index we
                        // will perform a full collection scan which could cause us to
                        // take a performance hit.
                        if (auto index = getIndexSupportingReopeningQuery(
                                opCtx, bucketsColl->getIndexCatalog(), timeSeriesOptions)) {

                            // Run an aggregation to find a suitable bucket to reopen.
                            AggregateCommandRequest aggRequest(bucketsColl->ns(), *pipeline);
                            aggRequest.setHint(index);

                            auto swCursor = 
                                    DBClientCursor::fromAggregationRequest(&client,
                                                                           aggRequest,
                                                                           false /* secondaryOk
                                                                           */, false /*
                                                                           useExhaust*/);
                            if (swCursor.isOK()) {
                                auto& cursor = swCursor.getValue();
                                if (cursor->more()) {
                                    suitableBucket = cursor->next();
                                }
                            }

                            reopeningContext->queriedBucket = true;
                        }
                    }

                    if (!suitableBucket.isEmpty()) {
                        auto validator = [&](OperationContext * opCtx,
                                             const BSONObj& bucketDoc) -> auto {
                            return bucketsColl->checkValidation(opCtx, bucketDoc);
                        };
                        reopeningContext->bucketToReopen =
                            bucket_catalog::BucketToReopen{suitableBucket, validator};
                    }

                    swResult = bucket_catalog::insertWithReopeningContext(
                        opCtx,
                        bucketCatalog,
                        viewNs,
                        bucketsColl->getDefaultCollator(),
                        timeSeriesOptions,
                        measurementDoc,
                        combine,
                        *reopeningContext);
                } else if (auto* waiter = get_if<bucket_catalog::InsertWaiter>(&insertResult)) {
                    // Need to wait for another operation to finish, then retry. This could be
                    // another reopening request or a previously prepared write batch for the same
                    // series (metaField value). The easiest way to retry here is to reset swResult
                    // to a WriteConflict, which will automatically re-run the while loop.
                    bucket_catalog::waitToInsert(waiter);
                    swResult = Status{ErrorCodes::WriteConflict, "waited to retry"};
                }
            }
        } else {
            swResult = bucket_catalog::insert(opCtx,
                                              bucketCatalog,
                                              viewNs,
                                              bucketsColl->getDefaultCollator(),
                                              timeSeriesOptions,
                                              measurementDoc,
                                              combine);
        }

        // If there is an era offset (between the bucket we want to reopen and the
        // catalog's current era), we could hit a WriteConflict error indicating we will
        // need to refetch a bucket document as it is potentially stale.
    } while (!swResult.isOK() && (swResult.getStatus().code() == ErrorCodes::WriteConflict));
    return swResult;
}

void makeWriteRequest(OperationContext* opCtx,
                      std::shared_ptr<bucket_catalog::WriteBatch> batch,
                      const BSONObj& metadata,
                      TimeseriesStmtIds& stmtIds,
                      const NamespaceString& bucketsNs,
                      std::vector<write_ops::InsertCommandRequest>* insertOps,
                      std::vector<write_ops::UpdateCommandRequest>* updateOps) {
    if (batch->numPreviouslyCommittedMeasurements == 0) {
        insertOps->push_back(makeTimeseriesInsertOp(
            batch, bucketsNs, metadata, std::move(stmtIds[batch->bucketHandle.bucketId.oid])));
        return;
    }
    if (batch->compressed) {
        updateOps->push_back(makeTimeseriesDecompressAndUpdateOp(
            opCtx,
            batch,
            bucketsNs,
            metadata,
            std::move(stmtIds[batch->bucketHandle.bucketId.oid])));
    } else {
        updateOps->push_back(
            makeTimeseriesUpdateOp(opCtx,
                                   batch,
                                   bucketsNs,
                                   metadata,
                                   std::move(stmtIds[batch->bucketHandle.bucketId.oid])));
    }
}

TimeseriesBatches insertIntoBucketCatalogForUpdate(OperationContext* opCtx,
                                                   bucket_catalog::BucketCatalog& bucketCatalog,
                                                   const CollectionPtr& bucketsColl,
                                                   const std::vector<BSONObj>& measurements,
                                                   const NamespaceString& bucketsNs,
                                                   TimeseriesOptions& timeSeriesOptions) {
    TimeseriesBatches batches;
    auto viewNs = bucketsNs.getTimeseriesViewNamespace();

    for (const auto& measurement : measurements) {
        auto result = uassertStatusOK(
            attemptInsertIntoBucket(opCtx,
                                    bucketCatalog,
                                    viewNs,
                                    bucketsColl.get(),
                                    timeSeriesOptions,
                                    measurement,
                                    bucket_catalog::CombineWithInsertsFromOtherClients::kDisallow,
                                    /*fromUpdates=*/true));
        auto* insertResult = get_if<bucket_catalog::SuccessfulInsertion>(&result);
        invariant(insertResult);
        batches.emplace_back(std::move(insertResult->batch));
    }

    return batches;
}

void performAtomicWrites(
    OperationContext* opCtx,
    const CollectionPtr& coll,
    const RecordId& recordId,
    const boost::optional<std::variant<write_ops::UpdateCommandRequest,
                                       write_ops::DeleteCommandRequest>>& modificationOp,
    const std::vector<write_ops::InsertCommandRequest>& insertOps,
    const std::vector<write_ops::UpdateCommandRequest>& updateOps,
    bool fromMigrate,
    StmtId stmtId) {
    tassert(
        7655102, "must specify at least one type of write", modificationOp || !insertOps.empty());
    NamespaceString ns = coll->ns();

    DisableDocumentValidation disableDocumentValidation{opCtx};

    write_ops_exec::LastOpFixer lastOpFixer{opCtx};
    lastOpFixer.startingOp(ns);

    auto curOp = CurOp::get(opCtx);
    curOp->raiseDbProfileLevel(CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(ns.dbName()));

    write_ops_exec::assertCanWrite_inlock(opCtx, ns);

    // Groups all operations in one or several chained oplog entries to ensure the writes are
    // replicated atomically.
    auto groupOplogEntries =
        !opCtx->getTxnNumber() && (!insertOps.empty() || !updateOps.empty()) && modificationOp;
    WriteUnitOfWork wuow{opCtx, groupOplogEntries};

    if (modificationOp) {
        visit(
            OverloadedVisitor{[&](const write_ops::UpdateCommandRequest& updateOp) {
                                  updateTimeseriesDocument(
                                      opCtx, coll, updateOp, &curOp->debug(), fromMigrate, stmtId);
                              },
                              [&](const write_ops::DeleteCommandRequest& deleteOp) {
                                  invariant(deleteOp.getDeletes().size() == 1);
                                  auto deleteId = record_id_helpers::keyForOID(
                                      deleteOp.getDeletes().front().getQ()["_id"].OID());
                                  invariant(recordId == deleteId);
                                  collection_internal::deleteDocument(
                                      opCtx, coll, stmtId, recordId, &curOp->debug(), fromMigrate);
                              }},
            *modificationOp);
    }

    if (!insertOps.empty()) {
        std::vector<InsertStatement> insertStatements;
        for (auto& op : insertOps) {
            invariant(op.getDocuments().size() == 1);
            if (modificationOp) {
                insertStatements.emplace_back(op.getDocuments().front());
            } else {
                // Appends the stmtId for upsert.
                insertStatements.emplace_back(stmtId, op.getDocuments().front());
            }
        }
        uassertStatusOK(collection_internal::insertDocuments(
            opCtx, coll, insertStatements.begin(), insertStatements.end(), &curOp->debug()));
    }

    for (auto& updateOp : updateOps) {
        updateTimeseriesDocument(opCtx, coll, updateOp, &curOp->debug(), fromMigrate, stmtId);
    }

    wuow.commit();

    lastOpFixer.finishedOpSuccessfully();
}

void commitTimeseriesBucketsAtomically(
    OperationContext* opCtx,
    bucket_catalog::BucketCatalog& sideBucketCatalog,
    const CollectionPtr& coll,
    const RecordId& recordId,
    const boost::optional<std::variant<write_ops::UpdateCommandRequest,
                                       write_ops::DeleteCommandRequest>>& modificationOp,
    TimeseriesBatches* batches,
    const NamespaceString& bucketsNs,
    bool fromMigrate,
    StmtId stmtId,
    std::set<OID>* bucketIds) {
    auto batchesToCommit = determineBatchesToCommit(*batches, extractFromSelf);
    if (batchesToCommit.empty()) {
        return;
    }

    Status abortStatus = Status::OK();
    ScopeGuard batchGuard{[&] {
        for (auto batch : batchesToCommit) {
            if (batch.get()) {
                abort(sideBucketCatalog, batch, abortStatus);
            }
        }
    }};

    try {
        std::vector<write_ops::InsertCommandRequest> insertOps;
        std::vector<write_ops::UpdateCommandRequest> updateOps;

        auto& mainBucketCatalog = bucket_catalog::BucketCatalog::get(opCtx);
        for (auto batch : batchesToCommit) {
            auto metadata = getMetadata(sideBucketCatalog, batch.get()->bucketHandle);
            auto prepareCommitStatus = prepareCommit(sideBucketCatalog, batch);
            if (!prepareCommitStatus.isOK()) {
                abortStatus = prepareCommitStatus;
                return;
            }

            TimeseriesStmtIds emptyStmtIds = {};
            makeWriteRequest(
                opCtx, batch, metadata, emptyStmtIds, bucketsNs, &insertOps, &updateOps);

            // Starts tracking the newly inserted bucket in the main bucket catalog as a direct
            // write to prevent other writers from modifying it.
            if (batch.get()->numPreviouslyCommittedMeasurements == 0) {
                auto bucketId = batch.get()->bucketHandle.bucketId.oid;
                directWriteStart(mainBucketCatalog.bucketStateRegistry,
                                 bucketsNs.getTimeseriesViewNamespace(),
                                 bucketId);
                bucketIds->insert(bucketId);
            }
        }

        performAtomicWrites(
            opCtx, coll, recordId, modificationOp, insertOps, updateOps, fromMigrate, stmtId);

        boost::optional<repl::OpTime> opTime;
        boost::optional<OID> electionId;
        getOpTimeAndElectionId(opCtx, &opTime, &electionId);

        for (auto batch : batchesToCommit) {
            finish(opCtx, sideBucketCatalog, batch, bucket_catalog::CommitInfo{opTime, electionId});
            batch.get().reset();
        }
    } catch (const DBException& ex) {
        abortStatus = ex.toStatus();
        throw;
    }

    batchGuard.dismiss();
}

void performAtomicWritesForDelete(OperationContext* opCtx,
                                  const CollectionPtr& coll,
                                  const RecordId& recordId,
                                  const std::vector<BSONObj>& unchangedMeasurements,
                                  bool fromMigrate,
                                  StmtId stmtId) {
    OID bucketId = record_id_helpers::toBSONAs(recordId, "_id")["_id"].OID();
    auto modificationOp = makeModificationOp(bucketId, coll, unchangedMeasurements);
    performAtomicWrites(opCtx, coll, recordId, modificationOp, {}, {}, fromMigrate, stmtId);
}

void performAtomicWritesForUpdate(
    OperationContext* opCtx,
    const CollectionPtr& coll,
    const RecordId& recordId,
    const boost::optional<std::vector<BSONObj>>& unchangedMeasurements,
    const std::vector<BSONObj>& modifiedMeasurements,
    bucket_catalog::BucketCatalog& sideBucketCatalog,
    bool fromMigrate,
    StmtId stmtId,
    std::set<OID>* bucketIds) {
    auto timeSeriesOptions = *coll->getTimeseriesOptions();
    auto batches = insertIntoBucketCatalogForUpdate(
        opCtx, sideBucketCatalog, coll, modifiedMeasurements, coll->ns(), timeSeriesOptions);

    auto modificationRequest = unchangedMeasurements
        ? boost::make_optional(
              makeModificationOp(record_id_helpers::toBSONAs(recordId, "_id")["_id"].OID(),
                                 coll,
                                 *unchangedMeasurements))
        : boost::none;
    commitTimeseriesBucketsAtomically(opCtx,
                                      sideBucketCatalog,
                                      coll,
                                      recordId,
                                      modificationRequest,
                                      &batches,
                                      coll->ns(),
                                      fromMigrate,
                                      stmtId,
                                      bucketIds);
}

BSONObj timeseriesViewCommand(const BSONObj& cmd, std::string cmdName, StringData viewNss) {
    BSONObjBuilder b;
    for (auto&& e : cmd) {
        if (e.fieldNameStringData() == cmdName) {
            b.append(cmdName, viewNss);
        } else {
            b.append(e);
        }
    }
    return b.obj();
}

void deleteRequestCheckFunction(DeleteRequest* request, const TimeseriesOptions& options) {
    if (!feature_flags::gTimeseriesDeletesSupport.isEnabled(
            serverGlobalParams.featureCompatibility.acquireFCVSnapshot())) {
        uassert(ErrorCodes::InvalidOptions,
                "Cannot perform a delete with a non-empty query on a time-series "
                "collection that "
                "does not have a metaField ",
                options.getMetaField() || request->getQuery().isEmpty());

        uassert(ErrorCodes::IllegalOperation,
                "Cannot perform a non-multi delete on a time-series collection",
                request->getMulti());
        if (auto metaField = options.getMetaField()) {
            request->setQuery(timeseries::translateQuery(request->getQuery(), *metaField));
        }
    }
}

void updateRequestCheckFunction(UpdateRequest* request, const TimeseriesOptions& options) {
    if (!feature_flags::gTimeseriesUpdatesSupport.isEnabled(
            serverGlobalParams.featureCompatibility.acquireFCVSnapshot())) {
        uassert(ErrorCodes::InvalidOptions,
                "Cannot perform a non-multi update on a time-series collection",
                request->isMulti());

        uassert(ErrorCodes::InvalidOptions,
                "Cannot perform an upsert on a time-series collection",
                !request->isUpsert());

        auto metaField = options.getMetaField();
        uassert(ErrorCodes::InvalidOptions,
                "Cannot perform an update on a time-series collection that does not have a "
                "metaField",
                options.getMetaField());

        request->setQuery(timeseries::translateQuery(request->getQuery(), *metaField));
        auto modification = uassertStatusOK(
            timeseries::translateUpdate(request->getUpdateModification(), *metaField));
        request->setUpdateModification(modification);
    }
}
}  // namespace mongo::timeseries
