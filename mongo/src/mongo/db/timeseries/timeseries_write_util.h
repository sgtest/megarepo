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

#include <algorithm>
#include <boost/optional/optional.hpp>
#include <functional>
#include <memory>
#include <variant>
#include <vector>

#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/oid.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/ops/write_ops.h"
#include "mongo/db/ops/write_ops_gen.h"
#include "mongo/db/ops/write_ops_parsers.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_catalog.h"
#include "mongo/db/timeseries/bucket_catalog/write_batch.h"
#include "mongo/db/timeseries/timeseries_gen.h"
#include "mongo/db/timeseries/timeseries_options.h"
#include "mongo/stdx/unordered_map.h"

namespace mongo::timeseries {

/**
 * Constructs an update request using a single update statement at position `opIndex`.
 */
write_ops::UpdateCommandRequest buildSingleUpdateOp(const write_ops::UpdateCommandRequest& wholeOp,
                                                    size_t opIndex);


/**
 * Asserts the buckets collection exists and has valid time-series options.
 *
 * Assumes already holding a lock on the collection.
 */
void assertTimeseriesBucketsCollection(const Collection* bucketsColl);

/**
 * Returns the document for writing a new bucket with a write batch.
 */
BSONObj makeNewDocumentForWrite(std::shared_ptr<timeseries::bucket_catalog::WriteBatch> batch,
                                const BSONObj& metadata);

/**
 * Returns a new document, compressed, with which to initialize a new bucket containing only the
 * given 'batch'. If compression fails for any reason, an uncompressed document will be returned.
 */
BSONObj makeNewCompressedDocumentForWrite(
    std::shared_ptr<timeseries::bucket_catalog::WriteBatch> batch,
    const BSONObj& metadata,
    const NamespaceString& nss,
    StringData timeField);

/**
 * Returns the document for writing a new bucket with 'measurements'. Calculates the min and max
 * fields while building the document.
 *
 * The measurements must already be known to fit in the same bucket. No checks will be done.
 */
BSONObj makeNewDocumentForWrite(
    const OID& bucketId,
    const std::vector<BSONObj>& measurements,
    const BSONObj& metadata,
    const boost::optional<TimeseriesOptions>& options,
    const boost::optional<const StringData::ComparatorInterface*>& comparator);

/**
 * Returns the document for writing a new bucket with 'measurements'. Generates the id and
 * calculates the min and max fields while building the document.
 *
 * The measurements must already be known to fit in the same bucket. No checks will be done.
 */
BSONObj makeBucketDocument(const std::vector<BSONObj>& measurements,
                           const NamespaceString& nss,
                           const TimeseriesOptions& options,
                           const StringData::ComparatorInterface* comparator);

/**
 * Returns an update request to the bucket when the 'measurements' is non-empty. Otherwise, returns
 * a delete request to the bucket.
 */
stdx::variant<write_ops::UpdateCommandRequest, write_ops::DeleteCommandRequest> makeModificationOp(
    const OID& bucketId, const CollectionPtr& coll, const std::vector<BSONObj>& measurements);

using TimeseriesBatches = std::vector<std::shared_ptr<bucket_catalog::WriteBatch>>;
using TimeseriesStmtIds = stdx::unordered_map<OID, std::vector<StmtId>, OID::Hasher>;

/**
 * Builds the transform update oplog entry with a transform function.
 */
write_ops::UpdateOpEntry makeTimeseriesTransformationOpEntry(
    OperationContext* opCtx,
    const OID& bucketId,
    write_ops::UpdateModification::TransformFunc transformationFunc);

/**
 * Retrieves the opTime and electionId according to the current replication mode.
 */
void getOpTimeAndElectionId(OperationContext* opCtx,
                            boost::optional<repl::OpTime>* opTime,
                            boost::optional<OID>* electionId);

/**
 * Builds the insert command request from a time-series insert write batch.
 */
write_ops::InsertCommandRequest makeTimeseriesInsertOp(
    std::shared_ptr<timeseries::bucket_catalog::WriteBatch> batch,
    const NamespaceString& bucketsNs,
    const BSONObj& metadata,
    std::vector<StmtId>&& stmtIds = {});

/**
 * Builds the update command request from a time-series insert write batch.
 */
write_ops::UpdateCommandRequest makeTimeseriesUpdateOp(
    OperationContext* opCtx,
    std::shared_ptr<timeseries::bucket_catalog::WriteBatch> batch,
    const NamespaceString& bucketsNs,
    const BSONObj& metadata,
    std::vector<StmtId>&& stmtIds = {});

/**
 * Builds the decompress and update command request from a time-series insert write batch.
 */
write_ops::UpdateCommandRequest makeTimeseriesDecompressAndUpdateOp(
    OperationContext* opCtx,
    std::shared_ptr<timeseries::bucket_catalog::WriteBatch> batch,
    const NamespaceString& bucketsNs,
    const BSONObj& metadata,
    std::vector<StmtId>&& stmtIds = {});

/**
 * Attempts to insert a measurement doc into a bucket in the bucket catalog and retries
 * automatically on certain errors. Only reopens existing buckets if the insert was initiated from a
 * user insert.
 *
 * Returns the write batch of the insert and other information if succeeded.
 */
StatusWith<timeseries::bucket_catalog::InsertResult> attemptInsertIntoBucket(
    OperationContext* opCtx,
    bucket_catalog::BucketCatalog& bucketCatalog,
    const NamespaceString& viewNs,
    const Collection* bucketsColl,
    TimeseriesOptions& timeSeriesOptions,
    const BSONObj& measurementDoc,
    bucket_catalog::CombineWithInsertsFromOtherClients combine,
    bool fromUpdates = false);

/**
 * Prepares the final write batches needed for performing the writes to storage.
 */
template <typename T, typename Fn>
std::vector<std::reference_wrapper<std::shared_ptr<timeseries::bucket_catalog::WriteBatch>>>
determineBatchesToCommit(T& batches, Fn&& extractElem) {
    std::vector<std::reference_wrapper<std::shared_ptr<timeseries::bucket_catalog::WriteBatch>>>
        batchesToCommit;
    for (auto& elem : batches) {
        std::shared_ptr<timeseries::bucket_catalog::WriteBatch>& batch = extractElem(elem);
        if (timeseries::bucket_catalog::claimWriteBatchCommitRights(*batch)) {
            batchesToCommit.push_back(batch);
        }
    }

    // Sort by bucket so that preparing the commit for each batch cannot deadlock.
    std::sort(batchesToCommit.begin(), batchesToCommit.end(), [](auto left, auto right) {
        return left.get()->bucketHandle.bucketId.oid < right.get()->bucketHandle.bucketId.oid;
    });

    return batchesToCommit;
}

/**
 * Builds the insert and update requests for performing the writes to storage from the write batches
 * provided.
 */
void makeWriteRequest(OperationContext* opCtx,
                      std::shared_ptr<bucket_catalog::WriteBatch> batch,
                      const BSONObj& metadata,
                      TimeseriesStmtIds& stmtIds,
                      const NamespaceString& bucketsNs,
                      std::vector<write_ops::InsertCommandRequest>* insertOps,
                      std::vector<write_ops::UpdateCommandRequest>* updateOps);

/**
 * Performs modifications atomically for a user command on a time-series collection.
 *
 * Replaces the bucket document for a partial bucket modification and removes the bucket for a full
 * bucket modification. Inserts new bucket documents if provided.
 *
 * All the modifications are written and replicated atomically.
 */
void performAtomicWrites(
    OperationContext* opCtx,
    const CollectionPtr& coll,
    const RecordId& recordId,
    const boost::optional<stdx::variant<write_ops::UpdateCommandRequest,
                                        write_ops::DeleteCommandRequest>>& modificationOp,
    const std::vector<write_ops::InsertCommandRequest>& insertOps,
    bool fromMigrate,
    StmtId stmtId);

/**
 * Constructs the write request with the provided measurements and performs the write atomically for
 * a time-series user delete on one bucket.
 */
void performAtomicWritesForDelete(OperationContext* opCtx,
                                  const CollectionPtr& coll,
                                  const RecordId& recordId,
                                  const std::vector<BSONObj>& unchangedMeasurements,
                                  bool fromMigrate,
                                  StmtId stmtId);

/**
 * Constructs the write requests with the provided measurements and performs the writes atomically
 * for a time-series user update on one bucket.
 */
void performAtomicWritesForUpdate(
    OperationContext* opCtx,
    const CollectionPtr& coll,
    const RecordId& recordId,
    const boost::optional<std::vector<BSONObj>>& unchangedMeasurements,
    const std::vector<BSONObj>& modifiedMeasurements,
    bool fromMigrate,
    StmtId stmtId);
}  // namespace mongo::timeseries
