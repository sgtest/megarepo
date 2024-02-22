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

#include "mongo/db/catalog/collection_write_path.h"

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <cstddef>
#include <cstdint>
#include <iterator>
#include <memory>
#include <string>
#include <type_traits>
#include <utility>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/simple_bsonelement_comparator.h"
#include "mongo/bson/timestamp.h"
#include "mongo/crypto/fle_crypto_types.h"
#include "mongo/db/catalog/capped_collection_maintenance.h"
#include "mongo/db/catalog/clustered_collection_options_gen.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/collection_options_gen.h"
#include "mongo/db/catalog/document_validation.h"
#include "mongo/db/catalog/local_oplog_info.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/write_stage_common.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/record_id_helpers.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/server_options.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/duplicate_key_error_info.h"
#include "mongo/db/storage/index_entry_comparison.h"
#include "mongo/db/storage/key_format.h"
#include "mongo/db/storage/record_data.h"
#include "mongo/db/storage/record_store.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage

namespace mongo {
namespace collection_internal {
namespace {

// This failpoint throws a WriteConflictException after a successful call to
// insertDocumentForBulkLoader
MONGO_FAIL_POINT_DEFINE(failAfterBulkLoadDocInsert);

//  This fail point injects insertion failures for all collections unless a collection name is
//  provided in the optional data object during configuration:
//  data: {
//      collectionNS: <fully-qualified collection namespace>,
//  }
MONGO_FAIL_POINT_DEFINE(failCollectionInserts);

// Used to pause after inserting collection data and calling the opObservers.  Inserts to
// replicated collections that are not part of a multi-statement transaction will have generated
// their OpTime and oplog entry. Supports parameters to limit pause by namespace and by _id
// of first data item in an insert (must be of type string):
//  data: {
//      collectionNS: <fully-qualified collection namespace>,
//      first_id: <string>
//  }
MONGO_FAIL_POINT_DEFINE(hangAfterCollectionInserts);

// This fail point introduces corruption to documents during insert.
MONGO_FAIL_POINT_DEFINE(corruptDocumentOnInsert);

// This fail point manually forces the RecordId to be of a given value during insert.
MONGO_FAIL_POINT_DEFINE(explicitlySetRecordIdOnInsert);

// This fail point skips deletion of the record, so that the deletion call would only delete the
// index keys.
MONGO_FAIL_POINT_DEFINE(skipDeleteRecord);

bool compareSafeContentElem(const BSONObj& oldDoc, const BSONObj& newDoc) {
    if (newDoc.hasField(kSafeContent) != oldDoc.hasField(kSafeContent)) {
        return false;
    }
    if (!newDoc.hasField(kSafeContent)) {
        return true;
    }

    return newDoc.getField(kSafeContent).binaryEqual(oldDoc.getField(kSafeContent));
}

std::vector<OplogSlot> reserveOplogSlotsForRetryableFindAndModify(OperationContext* opCtx) {
    // For retryable findAndModify running in a multi-document transaction, we will reserve the
    // oplog entries when the transaction prepares or commits without prepare.
    if (opCtx->inMultiDocumentTransaction()) {
        return {};
    }

    // We reserve oplog slots here, expecting the slot with the greatest timestmap (say TS) to be
    // used as the oplog timestamp. Tenant migrations and resharding will forge no-op image oplog
    // entries and set the timestamp for these synthetic entries to be TS - 1.
    auto oplogInfo = LocalOplogInfo::get(opCtx);
    auto slots = oplogInfo->getNextOpTimes(opCtx, 2);
    uassertStatusOK(
        shard_role_details::getRecoveryUnit(opCtx)->setTimestamp(slots.back().getTimestamp()));
    return slots;
}

/**
 * Returns an array of 'fromMigrate' values for a range of insert operations.
 * The 'fromMigrate' oplog entry field is used to identify operations that are a result
 * of chunk migration and should not generate change stream events.
 * Accepts a default 'fromMigrate' value that determines if there is a need to check
 * each insert operation individually.
 * See SERVER-62581 and SERVER-65858.
 */
std::vector<bool> makeFromMigrateForInserts(
    OperationContext* opCtx,
    const NamespaceString& nss,
    const std::vector<InsertStatement>::const_iterator begin,
    const std::vector<InsertStatement>::const_iterator end,
    bool defaultFromMigrate) {
    auto count = std::distance(begin, end);
    std::vector fromMigrate(count, defaultFromMigrate);
    if (defaultFromMigrate) {
        return fromMigrate;
    }

    // 'fromMigrate' is an oplog entry field. If we do not need to write this operation to
    // the oplog, there is no reason to proceed with the orphan document check.
    if (repl::ReplicationCoordinator::get(opCtx)->isOplogDisabledFor(opCtx, nss)) {
        return fromMigrate;
    }

    // Overriding the 'fromMigrate' flag makes sense only for requests coming from clients
    // directly connected to shards.
    if (OperationShardingState::isComingFromRouter(opCtx)) {
        return fromMigrate;
    }

    // This is used to check whether the write should be performed, and if so, any other
    // behavior that should be done as part of the write (e.g. skipping it because it affects an
    // orphan document).
    write_stage_common::PreWriteFilter preWriteFilter(opCtx, nss);

    for (decltype(count) i = 0; i < count; i++) {
        auto& insertStmt = begin[i];
        if (preWriteFilter.computeAction(Document(insertStmt.doc)) ==
            write_stage_common::PreWriteFilter::Action::kWriteAsFromMigrate) {
            LOGV2_DEBUG(7458900,
                        3,
                        "Marking insert operation of orphan document with the 'fromMigrate' flag "
                        "to prevent a wrong change stream event",
                        "namespace"_attr = nss,
                        "document"_attr = insertStmt.doc);

            fromMigrate[i] = true;
        }
    }

    return fromMigrate;
}

Status insertDocumentsImpl(OperationContext* opCtx,
                           const CollectionPtr& collection,
                           const std::vector<InsertStatement>::const_iterator begin,
                           const std::vector<InsertStatement>::const_iterator end,
                           OpDebug* opDebug,
                           bool fromMigrate) {
    const auto& nss = collection->ns();

    dassert(shard_role_details::getLocker(opCtx)->isCollectionLockedForMode(nss, MODE_IX) ||
            (nss.isOplog() && shard_role_details::getLocker(opCtx)->isWriteLocked()) ||
            (nss.isChangeCollection() && nss.tenantId() &&
             shard_role_details::getLocker(opCtx)->isLockHeldForMode(
                 {ResourceType::RESOURCE_TENANT, *nss.tenantId()}, MODE_IX)));

    const size_t count = std::distance(begin, end);

    if (collection->isCapped() && collection->getIndexCatalog()->haveAnyIndexes() && count > 1) {
        // We require that inserts to indexed capped collections be done one-at-a-time to avoid the
        // possibility that a later document causes an earlier document to be deleted before it can
        // be indexed.
        // TODO SERVER-21512 It would be better to handle this here by just doing single inserts.
        return {ErrorCodes::OperationCannotBeBatched,
                "Can't batch inserts into indexed capped collections"};
    }

    if (collection->needsCappedLock()) {
        // X-lock the metadata resource for this replicated, non-clustered capped collection until
        // the end of the WUOW. Non-clustered capped collections require writes to be serialized on
        // the secondary in order to guarantee insertion order (SERVER-21483); this exclusive access
        // to the metadata resource prevents the primary from executing with more concurrency than
        // secondaries - thus helping secondaries keep up - and protects '_cappedFirstRecord'. See
        // SERVER-21646. On the other hand, capped clustered collections with a monotonically
        // increasing cluster key natively guarantee preservation of the insertion order, and don't
        // need serialisation. We allow concurrent inserts for clustered capped collections.
        Lock::ResourceLock heldUntilEndOfWUOW{opCtx, ResourceId(RESOURCE_METADATA, nss), MODE_X};
    }

    std::vector<Record> records;
    records.reserve(count);
    std::vector<Timestamp> timestamps;
    timestamps.reserve(count);

    std::vector<RecordId> cappedRecordIds;
    // For capped collections requiring capped snapshots, usually RecordIds are reserved and
    // registered here to handle visibility. If the RecordId is provided by the caller, it is
    // assumed the caller already reserved and properly registered the inserts in the
    // CappedVisibilityObserver.
    if (collection->usesCappedSnapshots() && begin->recordId.isNull()) {
        cappedRecordIds = collection->reserveCappedRecordIds(opCtx, count);
    }

    size_t i = 0;
    for (auto it = begin; it != end; it++, i++) {
        const auto& doc = it->doc;

        RecordId recordId;
        if (collection->isClustered()) {
            invariant(collection->getRecordStore()->keyFormat() == KeyFormat::String);
            recordId = uassertStatusOK(
                record_id_helpers::keyForDoc(doc,
                                             collection->getClusteredInfo()->getIndexSpec(),
                                             collection->getDefaultCollator()));
        } else if (!it->replRid.isNull()) {
            // The 'replRid' being set indicates that this insert belongs to a replicated
            // recordId collection, and we need to use the given recordId while inserting.
            recordId = it->replRid;
        } else if (!it->recordId.isNull()) {
            // This case would only normally be called in a testing circumstance to avoid
            // automatically generating record ids for capped collections.
            recordId = it->recordId;
        } else if (cappedRecordIds.size()) {
            recordId = std::move(cappedRecordIds[i]);
        }

        if (MONGO_unlikely(corruptDocumentOnInsert.shouldFail())) {
            // Insert a truncated record that is half the expected size of the source document.
            records.emplace_back(
                Record{std::move(recordId), RecordData(doc.objdata(), doc.objsize() / 2)});
            timestamps.emplace_back(it->oplogSlot.getTimestamp());
            continue;
        }

        explicitlySetRecordIdOnInsert.execute([&](const BSONObj& data) {
            const auto docToMatch = data["doc"].Obj();
            if (doc.woCompare(docToMatch) == 0) {
                {
                    auto ridValue = data["rid"].safeNumberInt();
                    recordId = RecordId(ridValue);
                }
            }
        });

        records.emplace_back(Record{std::move(recordId), RecordData(doc.objdata(), doc.objsize())});
        timestamps.emplace_back(it->oplogSlot.getTimestamp());
    }

    Status status = collection->getRecordStore()->insertRecords(opCtx, &records, timestamps);

    if (!status.isOK()) {
        if (auto extraInfo = status.extraInfo<DuplicateKeyErrorInfo>();
            extraInfo && collection->isClustered()) {
            // Generate a useful error message that is consistent with duplicate key error messages
            // on indexes. This transforms the error from a duplicate clustered key error into a
            // duplicate key error. We have to perform this in order to maintain compatibility with
            // already existing user code.
            const auto& rId = extraInfo->getDuplicateRid();
            const auto& foundValue = extraInfo->getFoundValue();
            invariant(rId,
                      "Clustered Collections must return the RecordId when returning a duplicate "
                      "key error");
            BSONObj obj = record_id_helpers::toBSONAs(*rId, "");
            status = buildDupKeyErrorStatus(obj,
                                            NamespaceString(collection->ns()),
                                            "" /* indexName */,
                                            BSON("_id" << 1),
                                            collection->getCollectionOptions().collation,
                                            DuplicateKeyErrorInfo::FoundValue{foundValue});
        }
        return status;
    }

    std::vector<BsonRecord> bsonRecords;
    bsonRecords.reserve(count);
    int recordIndex = 0;
    for (auto it = begin; it != end; it++) {
        RecordId loc = records[recordIndex++].id;
        if (collection->getRecordStore()->keyFormat() == KeyFormat::Long) {
            invariant(RecordId::minLong() < loc);
            invariant(loc < RecordId::maxLong());
        }

        BsonRecord bsonRecord = {
            std::move(loc), Timestamp(it->oplogSlot.getTimestamp()), &(it->doc)};
        bsonRecords.emplace_back(std::move(bsonRecord));
    }

    // An empty vector of recordIds is ignored by the OpObserver. When non-empty,
    // the OpObserver will add recordIds to the generated oplog entries.
    std::vector<RecordId> recordIds;
    if (collection->areRecordIdsReplicated()) {
        recordIds.reserve(count);
        for (const auto& r : records) {
            recordIds.push_back(r.id);
        }
    }

    int64_t keysInserted = 0;
    status =
        collection->getIndexCatalog()->indexRecords(opCtx, collection, bsonRecords, &keysInserted);
    if (!status.isOK()) {
        return status;
    }

    if (opDebug) {
        opDebug->additiveMetrics.incrementKeysInserted(keysInserted);
        // 'opDebug' may be deleted at rollback time in case of multi-document transaction.
        if (!opCtx->inMultiDocumentTransaction()) {
            shard_role_details::getRecoveryUnit(opCtx)->onRollback(
                [opDebug, keysInserted](OperationContext*) {
                    opDebug->additiveMetrics.incrementKeysInserted(-keysInserted);
                });
        }
    }

    if (!nss.isImplicitlyReplicated()) {
        opCtx->getServiceContext()->getOpObserver()->onInserts(
            opCtx,
            collection,
            begin,
            end,
            recordIds,
            /*fromMigrate=*/makeFromMigrateForInserts(opCtx, nss, begin, end, fromMigrate),
            /*defaultFromMigrate=*/fromMigrate);
    }

    cappedDeleteUntilBelowConfiguredMaximum(opCtx, collection, records.begin()->id);

    return Status::OK();
}

}  // namespace

Status insertDocumentForBulkLoader(OperationContext* opCtx,
                                   const CollectionPtr& collection,
                                   const BSONObj& doc,
                                   RecordId replRid,
                                   const OnRecordInsertedFn& onRecordInserted) {
    const auto& nss = collection->ns();

    auto status = checkFailCollectionInsertsFailPoint(nss, doc);
    if (!status.isOK()) {
        return status;
    }

    status = collection->checkValidationAndParseResult(opCtx, doc);
    if (!status.isOK()) {
        return status;
    }

    dassert(shard_role_details::getLocker(opCtx)->isCollectionLockedForMode(nss, MODE_IX) ||
            (nss.isOplog() && shard_role_details::getLocker(opCtx)->isWriteLocked()));

    // The replRid must be provided if the collection has recordIdsReplicated:true and it must
    // not be provided if the collection has recordIdsReplicated:false
    invariant(collection->areRecordIdsReplicated() != replRid.isNull(),
              str::stream() << "Unexpected recordId value for collection with ns: '"
                            << collection->ns().toStringForErrorMsg() << "', uuid: '"
                            << collection->uuid());

    RecordId recordId = replRid;
    if (collection->isClustered()) {
        invariant(collection->getRecordStore()->keyFormat() == KeyFormat::String);
        recordId = uassertStatusOK(record_id_helpers::keyForDoc(
            doc, collection->getClusteredInfo()->getIndexSpec(), collection->getDefaultCollator()));
    }

    // Using timestamp 0 for these inserts, which are non-oplog so we don't have an appropriate
    // timestamp to use.
    StatusWith<RecordId> loc = collection->getRecordStore()->insertRecord(
        opCtx, recordId, doc.objdata(), doc.objsize(), Timestamp());

    if (!loc.isOK())
        return loc.getStatus();

    status = onRecordInserted(loc.getValue());

    if (MONGO_unlikely(failAfterBulkLoadDocInsert.shouldFail())) {
        LOGV2(20290,
              "Failpoint failAfterBulkLoadDocInsert enabled. Throwing "
              "WriteConflictException",
              logAttrs(nss));
        throwWriteConflictException(str::stream() << "Hit failpoint '"
                                                  << failAfterBulkLoadDocInsert.getName() << "'.");
    }

    std::vector<InsertStatement> inserts;
    OplogSlot slot;
    // Fetch a new optime now, if necessary.
    auto replCoord = repl::ReplicationCoordinator::get(opCtx);
    if (!replCoord->isOplogDisabledFor(opCtx, nss)) {
        auto slots = LocalOplogInfo::get(opCtx)->getNextOpTimes(opCtx, 1);
        slot = slots.back();
    }
    inserts.emplace_back(kUninitializedStmtId, doc, slot);

    // During initial sync, there are no recordIds to be passed to the OpObserver to
    // include in oplog entries, as we don't generate oplog entries.
    opCtx->getServiceContext()->getOpObserver()->onInserts(
        opCtx,
        collection,
        inserts.begin(),
        inserts.end(),
        /*recordIds=*/{},
        /*fromMigrate=*/std::vector<bool>(inserts.size(), false),
        /*defaultFromMigrate=*/false);

    cappedDeleteUntilBelowConfiguredMaximum(opCtx, collection, loc.getValue());

    // Capture the recordStore here instead of the CollectionPtr object itself, because the record
    // store's lifetime is controlled by the collection IX lock held on the write paths, whereas the
    // CollectionPtr is just a front to the collection and its lifetime is shorter
    shard_role_details::getRecoveryUnit(opCtx)->onCommit(
        [recordStore = collection->getRecordStore()](OperationContext*,
                                                     boost::optional<Timestamp>) {
            recordStore->notifyCappedWaitersIfNeeded();
        });

    return loc.getStatus();
}

Status insertDocuments(OperationContext* opCtx,
                       const CollectionPtr& collection,
                       std::vector<InsertStatement>::const_iterator begin,
                       std::vector<InsertStatement>::const_iterator end,
                       OpDebug* opDebug,
                       bool fromMigrate) {
    const auto& nss = collection->ns();

    auto status = checkFailCollectionInsertsFailPoint(nss, (begin != end ? begin->doc : BSONObj()));
    if (!status.isOK()) {
        return status;
    }

    // Should really be done in the collection object at creation and updated on index create.
    const bool hasIdIndex = collection->getIndexCatalog()->findIdIndex(opCtx);

    for (auto it = begin; it != end; it++) {
        if (hasIdIndex && it->doc["_id"].eoo()) {
            return Status(ErrorCodes::InternalError,
                          str::stream()
                              << "Collection::insertDocument got document without _id for ns:"
                              << nss.toStringForErrorMsg());
        }

        auto status = collection->checkValidationAndParseResult(opCtx, it->doc);
        if (!status.isOK()) {
            return status;
        }

        auto& validationSettings = DocumentValidationSettings::get(opCtx);

        if (collection->getCollectionOptions().encryptedFieldConfig &&
            !collection->ns().isTemporaryReshardingCollection() &&
            !validationSettings.isSchemaValidationDisabled() &&
            !validationSettings.isSafeContentValidationDisabled() &&
            it->doc.hasField(kSafeContent)) {
            return Status(ErrorCodes::BadValue,
                          str::stream()
                              << "Cannot insert a document with field name " << kSafeContent);
        }
    }

    const SnapshotId sid = shard_role_details::getRecoveryUnit(opCtx)->getSnapshotId();

    status = insertDocumentsImpl(opCtx, collection, begin, end, opDebug, fromMigrate);
    if (!status.isOK()) {
        return status;
    }
    invariant(sid == shard_role_details::getRecoveryUnit(opCtx)->getSnapshotId());

    // Capture the recordStore here instead of the CollectionPtr object itself, because the record
    // store's lifetime is controlled by the collection IX lock held on the write paths, whereas the
    // CollectionPtr is just a front to the collection and its lifetime is shorter
    shard_role_details::getRecoveryUnit(opCtx)->onCommit(
        [recordStore = collection->getRecordStore()](OperationContext*,
                                                     boost::optional<Timestamp>) {
            recordStore->notifyCappedWaitersIfNeeded();
        });

    hangAfterCollectionInserts.executeIf(
        [&](const BSONObj& data) {
            const auto& firstIdElem = data["first_id"];
            std::string whenFirst;
            if (firstIdElem) {
                whenFirst += " when first _id is ";
                whenFirst += firstIdElem.str();
            }
            LOGV2(20289,
                  "hangAfterCollectionInserts fail point enabled. Blocking "
                  "until fail point is disabled.",
                  "ns"_attr = nss,
                  "whenFirst"_attr = whenFirst);
            hangAfterCollectionInserts.pauseWhileSet(opCtx);
        },
        [&](const BSONObj& data) {
            const auto fpNss = NamespaceStringUtil::parseFailPointData(data, "collectionNS");
            const auto& firstIdElem = data["first_id"];
            // If the failpoint specifies no collection or matches the existing one, hang.
            return (fpNss.isEmpty() || nss == fpNss) &&
                (!firstIdElem ||
                 (begin != end && firstIdElem.type() == mongo::String &&
                  begin->doc["_id"].str() == firstIdElem.str()));
        });

    return Status::OK();
}

Status insertDocument(OperationContext* opCtx,
                      const CollectionPtr& collection,
                      const InsertStatement& doc,
                      OpDebug* opDebug,
                      bool fromMigrate) {
    std::vector<InsertStatement> docs;
    docs.push_back(doc);
    return insertDocuments(opCtx, collection, docs.begin(), docs.end(), opDebug, fromMigrate);
}

Status checkFailCollectionInsertsFailPoint(const NamespaceString& ns, const BSONObj& firstDoc) {
    Status s = Status::OK();
    failCollectionInserts.executeIf(
        [&](const BSONObj& data) {
            const std::string msg = str::stream()
                << "Failpoint (failCollectionInserts) has been enabled (" << data
                << "), so rejecting insert (first doc): " << firstDoc;
            LOGV2(20287,
                  "Failpoint (failCollectionInserts) has been enabled, so rejecting insert",
                  "data"_attr = data,
                  "document"_attr = firstDoc);
            s = {ErrorCodes::FailPointEnabled, msg};
        },
        [&](const BSONObj& data) {
            // If the failpoint specifies no collection or matches the existing one, fail.
            const auto fpNss = NamespaceStringUtil::parseFailPointData(data, "collectionNS");
            return fpNss.isEmpty() || ns == fpNss;
        });
    return s;
}

void updateDocument(OperationContext* opCtx,
                    const CollectionPtr& collection,
                    const RecordId& oldLocation,
                    const Snapshotted<BSONObj>& oldDoc,
                    const BSONObj& newDoc,
                    const BSONObj* opDiff,
                    bool* indexesAffected,
                    OpDebug* opDebug,
                    CollectionUpdateArgs* args) {
    {
        auto status = collection->checkValidationAndParseResult(opCtx, newDoc);
        if (!status.isOK()) {
            if (validationLevelOrDefault(collection->getCollectionOptions().validationLevel) ==
                ValidationLevelEnum::strict) {
                uassertStatusOK(status);
            }
            // moderate means we have to check the old doc
            auto oldDocStatus = collection->checkValidationAndParseResult(opCtx, oldDoc.value());
            if (oldDocStatus.isOK()) {
                // transitioning from good -> bad is not ok
                uassertStatusOK(status);
            }
            // bad -> bad is ok in moderate mode
        }
    }

    auto& validationSettings = DocumentValidationSettings::get(opCtx);
    if (collection->getCollectionOptions().encryptedFieldConfig &&
        !collection->ns().isTemporaryReshardingCollection() &&
        !validationSettings.isSchemaValidationDisabled() &&
        !validationSettings.isSafeContentValidationDisabled()) {

        uassert(ErrorCodes::BadValue,
                str::stream() << "New document and old document both need to have " << kSafeContent
                              << " field.",
                compareSafeContentElem(oldDoc.value(), newDoc));
    }

    dassert(
        shard_role_details::getLocker(opCtx)->isCollectionLockedForMode(collection->ns(), MODE_IX));
    invariant(oldDoc.snapshotId() == shard_role_details::getRecoveryUnit(opCtx)->getSnapshotId());
    invariant(newDoc.isOwned());

    if (collection->needsCappedLock()) {
        Lock::ResourceLock heldUntilEndOfWUOW{
            opCtx, ResourceId(RESOURCE_METADATA, collection->ns()), MODE_X};
    }

    SnapshotId sid = shard_role_details::getRecoveryUnit(opCtx)->getSnapshotId();

    BSONElement oldId = oldDoc.value()["_id"];
    // We accept equivalent _id according to the collation defined in the collection. 'foo' and
    // 'Foo' could be equivalent but not byte-identical according to the collation of the
    // collection.
    BSONElementComparator eltCmp{BSONElementComparator::FieldNamesMode::kConsider,
                                 collection->getDefaultCollator()};
    if (!oldId.eoo() && eltCmp.evaluate(oldId != newDoc["_id"]))
        uasserted(13596, "in Collection::updateDocument _id mismatch");

    args->changeStreamPreAndPostImagesEnabledForCollection =
        collection->isChangeStreamPreAndPostImagesEnabled();

    OplogUpdateEntryArgs onUpdateArgs(args, collection);
    const bool setNeedsRetryImageOplogField =
        args->storeDocOption != CollectionUpdateArgs::StoreDocOption::None;
    if (args->oplogSlots.empty() && setNeedsRetryImageOplogField && args->retryableWrite) {
        onUpdateArgs.retryableFindAndModifyLocation =
            RetryableFindAndModifyLocation::kSideCollection;
        // If the update is part of a retryable write and we expect to be storing the pre- or
        // post-image in a side collection, then we must reserve oplog slots in advance. We
        // expect to use the reserved oplog slots as follows, where TS is the greatest
        // timestamp of 'oplogSlots':
        // TS - 1: Tenant migrations and resharding will forge no-op image oplog entries and set
        //         the entry timestamps to TS - 1.
        // TS:     The timestamp given to the update oplog entry.
        args->oplogSlots = reserveOplogSlotsForRetryableFindAndModify(opCtx);
    } else {
        // Retryable findAndModify commands should not reserve oplog slots before entering this
        // function since tenant migrations and resharding rely on always being able to set
        // timestamps of forged pre- and post- image entries to timestamp of findAndModify - 1.
        invariant(!(args->retryableWrite && setNeedsRetryImageOplogField));
    }

    uassertStatusOK(collection->getRecordStore()->updateRecord(
        opCtx, oldLocation, newDoc.objdata(), newDoc.objsize()));

    // don't update the indexes if kUpdateNoIndexes has been specified.
    if (opDiff != kUpdateNoIndexes) {
        int64_t keysInserted = 0;
        int64_t keysDeleted = 0;

        uassertStatusOK(collection->getIndexCatalog()->updateRecord(opCtx,
                                                                    collection,
                                                                    args->preImageDoc,
                                                                    newDoc,
                                                                    opDiff,
                                                                    oldLocation,
                                                                    &keysInserted,
                                                                    &keysDeleted));
        if (indexesAffected) {
            *indexesAffected = (keysInserted > 0 || keysDeleted > 0);
        }

        if (opDebug) {
            opDebug->additiveMetrics.incrementKeysInserted(keysInserted);
            opDebug->additiveMetrics.incrementKeysDeleted(keysDeleted);
            // 'opDebug' may be deleted at rollback time in case of multi-document transaction.
            if (!opCtx->inMultiDocumentTransaction()) {
                shard_role_details::getRecoveryUnit(opCtx)->onRollback(
                    [opDebug, keysInserted, keysDeleted](OperationContext*) {
                        opDebug->additiveMetrics.incrementKeysInserted(-keysInserted);
                        opDebug->additiveMetrics.incrementKeysDeleted(-keysDeleted);
                    });
            }
        }
    }

    invariant(sid == shard_role_details::getRecoveryUnit(opCtx)->getSnapshotId());
    args->updatedDoc = newDoc;

    opCtx->getServiceContext()->getOpObserver()->onUpdate(opCtx, onUpdateArgs);
}

StatusWith<BSONObj> updateDocumentWithDamages(OperationContext* opCtx,
                                              const CollectionPtr& collection,
                                              const RecordId& loc,
                                              const Snapshotted<BSONObj>& oldDoc,
                                              const char* damageSource,
                                              const mutablebson::DamageVector& damages,
                                              const BSONObj* opDiff,
                                              bool* indexesAffected,
                                              OpDebug* opDebug,
                                              CollectionUpdateArgs* args) {
    dassert(
        shard_role_details::getLocker(opCtx)->isCollectionLockedForMode(collection->ns(), MODE_IX));
    invariant(oldDoc.snapshotId() == shard_role_details::getRecoveryUnit(opCtx)->getSnapshotId());
    invariant(collection->updateWithDamagesSupported());

    OplogUpdateEntryArgs onUpdateArgs(args, collection);
    const bool setNeedsRetryImageOplogField =
        args->storeDocOption != CollectionUpdateArgs::StoreDocOption::None;
    if (args->oplogSlots.empty() && setNeedsRetryImageOplogField && args->retryableWrite) {
        onUpdateArgs.retryableFindAndModifyLocation =
            RetryableFindAndModifyLocation::kSideCollection;
        // If the update is part of a retryable write and we expect to be storing the pre- or
        // post-image in a side collection, then we must reserve oplog slots in advance. We
        // expect to use the reserved oplog slots as follows, where TS is the greatest
        // timestamp of 'oplogSlots':
        // TS - 1: Tenant migrations and resharding will forge no-op image oplog entries and set
        //         the entry timestamps to TS - 1.
        // TS:     The timestamp given to the update oplog entry.
        args->oplogSlots = reserveOplogSlotsForRetryableFindAndModify(opCtx);
    } else {
        // Retryable findAndModify commands should not reserve oplog slots before entering this
        // function since tenant migrations and resharding rely on always being able to set
        // timestamps of forged pre- and post- image entries to timestamp of findAndModify - 1.
        invariant(!(args->retryableWrite && setNeedsRetryImageOplogField));
    }

    RecordData oldRecordData(oldDoc.value().objdata(), oldDoc.value().objsize());
    StatusWith<RecordData> recordData = collection->getRecordStore()->updateWithDamages(
        opCtx, loc, oldRecordData, damageSource, damages);
    if (!recordData.isOK())
        return recordData.getStatus();
    BSONObj newDoc = std::move(recordData.getValue()).releaseToBson().getOwned();

    args->updatedDoc = newDoc;
    args->changeStreamPreAndPostImagesEnabledForCollection =
        collection->isChangeStreamPreAndPostImagesEnabled();

    // don't update the indexes if kUpdateNoIndexes has been specified.
    if (opDiff != kUpdateNoIndexes) {
        int64_t keysInserted = 0;
        int64_t keysDeleted = 0;

        uassertStatusOK(collection->getIndexCatalog()->updateRecord(opCtx,
                                                                    collection,
                                                                    oldDoc.value(),
                                                                    args->updatedDoc,
                                                                    opDiff,
                                                                    loc,
                                                                    &keysInserted,
                                                                    &keysDeleted));
        if (indexesAffected) {
            *indexesAffected = (keysInserted > 0 || keysDeleted > 0);
        }

        if (opDebug) {
            opDebug->additiveMetrics.incrementKeysInserted(keysInserted);
            opDebug->additiveMetrics.incrementKeysDeleted(keysDeleted);
            // 'opDebug' may be deleted at rollback time in case of multi-document transaction.
            if (!opCtx->inMultiDocumentTransaction()) {
                shard_role_details::getRecoveryUnit(opCtx)->onRollback(
                    [opDebug, keysInserted, keysDeleted](OperationContext*) {
                        opDebug->additiveMetrics.incrementKeysInserted(-keysInserted);
                        opDebug->additiveMetrics.incrementKeysDeleted(-keysDeleted);
                    });
            }
        }
    }

    opCtx->getServiceContext()->getOpObserver()->onUpdate(opCtx, onUpdateArgs);
    return newDoc;
}

void deleteDocument(OperationContext* opCtx,
                    const CollectionPtr& collection,
                    StmtId stmtId,
                    const RecordId& loc,
                    OpDebug* opDebug,
                    bool fromMigrate,
                    bool noWarn,
                    StoreDeletedDoc storeDeletedDoc,
                    CheckRecordId checkRecordId,
                    RetryableWrite retryableWrite) {
    Snapshotted<BSONObj> doc = collection->docFor(opCtx, loc);
    deleteDocument(opCtx,
                   collection,
                   doc,
                   stmtId,
                   loc,
                   opDebug,
                   fromMigrate,
                   noWarn,
                   storeDeletedDoc,
                   checkRecordId);
}

void deleteDocument(OperationContext* opCtx,
                    const CollectionPtr& collection,
                    Snapshotted<BSONObj> doc,
                    StmtId stmtId,
                    const RecordId& loc,
                    OpDebug* opDebug,
                    bool fromMigrate,
                    bool noWarn,
                    StoreDeletedDoc storeDeletedDoc,
                    CheckRecordId checkRecordId,
                    RetryableWrite retryableWrite) {
    const auto& nss = collection->ns();

    if (collection->isCapped() && opCtx->inMultiDocumentTransaction()) {
        uasserted(ErrorCodes::IllegalOperation,
                  "Cannot remove from a capped collection in a multi-document transaction");
    }

    if (collection->needsCappedLock()) {
        Lock::ResourceLock heldUntilEndOfWUOW{opCtx, ResourceId(RESOURCE_METADATA, nss), MODE_X};
    }

    OplogDeleteEntryArgs deleteArgs;

    // TODO(SERVER-80956): remove this call.
    opCtx->getServiceContext()->getOpObserver()->aboutToDelete(
        opCtx, collection, doc.value(), &deleteArgs);

    invariant(doc.value().isOwned(),
              str::stream() << "Document to delete is not owned: snapshot id: " << doc.snapshotId()
                            << " document: " << doc.value());

    deleteArgs.fromMigrate = fromMigrate;
    deleteArgs.changeStreamPreAndPostImagesEnabledForCollection =
        collection->isChangeStreamPreAndPostImagesEnabled();

    const bool shouldRecordPreImageForRetryableWrite =
        storeDeletedDoc == StoreDeletedDoc::On && retryableWrite == RetryableWrite::kYes;
    if (shouldRecordPreImageForRetryableWrite) {
        deleteArgs.retryableFindAndModifyLocation = RetryableFindAndModifyLocation::kSideCollection;
        deleteArgs.retryableFindAndModifyOplogSlots =
            reserveOplogSlotsForRetryableFindAndModify(opCtx);
    }

    int64_t keysDeleted = 0;
    collection->getIndexCatalog()->unindexRecord(
        opCtx, collection, doc.value(), loc, noWarn, &keysDeleted, checkRecordId);

    if (MONGO_unlikely(skipDeleteRecord.shouldFail())) {
        LOGV2_DEBUG(8096000,
                    3,
                    "Skipping deleting record in deleteDocument",
                    "recordId"_attr = loc,
                    "doc"_attr = doc.value().toString());
    } else {
        collection->getRecordStore()->deleteRecord(opCtx, loc);
    }

    opCtx->getServiceContext()->getOpObserver()->onDelete(
        opCtx, collection, stmtId, doc.value(), deleteArgs);

    if (opDebug) {
        opDebug->additiveMetrics.incrementKeysDeleted(keysDeleted);
        // 'opDebug' may be deleted at rollback time in case of multi-document transaction.
        if (!opCtx->inMultiDocumentTransaction()) {
            shard_role_details::getRecoveryUnit(opCtx)->onRollback(
                [opDebug, keysDeleted](OperationContext*) {
                    opDebug->additiveMetrics.incrementKeysDeleted(-keysDeleted);
                });
        }
    }
}

}  // namespace collection_internal
}  // namespace mongo
