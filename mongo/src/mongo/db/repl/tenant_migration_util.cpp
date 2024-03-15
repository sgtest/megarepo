/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include "mongo/db/repl/tenant_migration_util.h"

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/smart_ptr.hpp>
#include <tuple>

#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/json.h"
#include "mongo/bson/mutable/algorithm.h"
#include "mongo/bson/mutable/document.h"
#include "mongo/bson/mutable/element.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/keys_collection_document_gen.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/ops/update.h"
#include "mongo/db/ops/update_request.h"
#include "mongo/db/ops/write_ops_parsers.h"
#include "mongo/db/pipeline/document_source_add_fields.h"
#include "mongo/db/pipeline/document_source_find_and_modify_image_lookup.h"
#include "mongo/db/pipeline/document_source_graph_lookup.h"
#include "mongo/db/pipeline/document_source_lookup.h"
#include "mongo/db/pipeline/document_source_match.h"
#include "mongo/db/pipeline/document_source_project.h"
#include "mongo/db/pipeline/document_source_replace_root.h"
#include "mongo/db/pipeline/document_source_unwind.h"
#include "mongo/db/pipeline/field_path.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/repl/repl_server_parameters_gen.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/s/database_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/util/cancellation.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/duration.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/future_util.h"
#include "mongo/util/time_support.h"

namespace mongo {

namespace repl {

MONGO_FAIL_POINT_DEFINE(pauseBeforeRunTenantMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(pauseAfterRunTenantMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(skipTenantMigrationRecipientAuth);
MONGO_FAIL_POINT_DEFINE(skipComparingRecipientAndDonorFCV);
MONGO_FAIL_POINT_DEFINE(autoRecipientForgetMigration);
MONGO_FAIL_POINT_DEFINE(skipFetchingCommittedTransactions);
MONGO_FAIL_POINT_DEFINE(skipFetchingRetryableWritesEntriesBeforeStartOpTime);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationRecipientBeforeDeletingStateDoc);
MONGO_FAIL_POINT_DEFINE(failWhilePersistingTenantMigrationRecipientInstanceStateDoc);
MONGO_FAIL_POINT_DEFINE(fpAfterPersistingTenantMigrationRecipientInstanceStateDoc);
MONGO_FAIL_POINT_DEFINE(fpBeforeFetchingDonorClusterTimeKeys);
MONGO_FAIL_POINT_DEFINE(fpAfterConnectingTenantMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(fpAfterRecordingRecipientPrimaryStartingFCV);
MONGO_FAIL_POINT_DEFINE(fpAfterComparingRecipientAndDonorFCV);
MONGO_FAIL_POINT_DEFINE(fpAfterRetrievingStartOpTimesMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(fpSetSmallAggregationBatchSize);
MONGO_FAIL_POINT_DEFINE(fpBeforeWaitingForRetryableWritePreFetchMajorityCommitted);
MONGO_FAIL_POINT_DEFINE(pauseAfterRetrievingRetryableWritesBatch);
MONGO_FAIL_POINT_DEFINE(fpAfterFetchingRetryableWritesEntriesBeforeStartOpTime);
MONGO_FAIL_POINT_DEFINE(fpAfterStartingOplogFetcherMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(setTenantMigrationRecipientInstanceHostTimeout);
MONGO_FAIL_POINT_DEFINE(pauseAfterRetrievingLastTxnMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(fpBeforeMarkingCloneSuccess);
MONGO_FAIL_POINT_DEFINE(fpBeforeFetchingCommittedTransactions);
MONGO_FAIL_POINT_DEFINE(fpAfterFetchingCommittedTransactions);
MONGO_FAIL_POINT_DEFINE(fpAfterStartingOplogApplierMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(fpBeforeFulfillingDataConsistentPromise);
MONGO_FAIL_POINT_DEFINE(fpAfterDataConsistentMigrationRecipientInstance);
MONGO_FAIL_POINT_DEFINE(fpBeforePersistingRejectReadsBeforeTimestamp);
MONGO_FAIL_POINT_DEFINE(fpAfterWaitForRejectReadsBeforeTimestamp);
MONGO_FAIL_POINT_DEFINE(hangBeforeTaskCompletion);
MONGO_FAIL_POINT_DEFINE(fpAfterReceivingRecipientForgetMigration);
MONGO_FAIL_POINT_DEFINE(hangAfterCreatingRSM);
MONGO_FAIL_POINT_DEFINE(skipRetriesWhenConnectingToDonorHost);
MONGO_FAIL_POINT_DEFINE(fpBeforeDroppingTempCollections);
MONGO_FAIL_POINT_DEFINE(fpWaitUntilTimestampMajorityCommitted);
MONGO_FAIL_POINT_DEFINE(hangAfterUpdatingTransactionEntry);
MONGO_FAIL_POINT_DEFINE(fpBeforeAdvancingStableTimestamp);

}  // namespace repl

namespace tenant_migration_util {

namespace {

const std::set<std::string> kSensitiveFieldNames{"donorCertificateForRecipient",
                                                 "recipientCertificateForDonor"};

MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeMarkingExternalKeysGarbageCollectable);

}  // namespace

const Backoff kExponentialBackoff(Seconds(1), Milliseconds::max());

void createOplogViewForTenantMigrations(OperationContext* opCtx, Database* db) {
    writeConflictRetry(
        opCtx, "createDonorOplogView", NamespaceString::kTenantMigrationOplogView, [&] {
            {
                // Create 'system.views' in a separate WUOW if it does not exist.
                WriteUnitOfWork wuow(opCtx);
                const Collection* coll = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(
                    opCtx, NamespaceString(db->getSystemViewsName()));
                if (!coll) {
                    coll = db->createCollection(opCtx, NamespaceString(db->getSystemViewsName()));
                }
                invariant(coll);
                wuow.commit();
            }

            // Project the fields that a tenant migration recipient needs to refetch retryable
            // writes oplog entries: `ts`, `prevOpTime`, `preImageOpTime`, and `postImageOpTime`.
            // Also projects the first 'ns' field of 'applyOps' for transactions.
            //
            // We use two stages in this pipeline because 'o.applyOps' is an array but '$project'
            // does not recognize numeric paths as array indices. As a result, we use one '$project'
            // stage to get the first element in 'o.applyOps', then a second stage to store the 'ns'
            // field of the element into 'applyOpsNs'.
            BSONArrayBuilder pipeline;
            pipeline.append(BSON("$project" << BSON("_id"
                                                    << "$ts"
                                                    << "ns" << 1 << "ts" << 1 << "prevOpTime" << 1
                                                    << "preImageOpTime" << 1 << "postImageOpTime"
                                                    << 1 << "applyOpsNs"
                                                    << BSON("$first"
                                                            << "$o.applyOps"))));
            pipeline.append(BSON("$project" << BSON("_id"
                                                    << "$ts"
                                                    << "ns" << 1 << "ts" << 1 << "prevOpTime" << 1
                                                    << "preImageOpTime" << 1 << "postImageOpTime"
                                                    << 1 << "applyOpsNs"
                                                    << "$applyOpsNs.ns")));

            CollectionOptions options;
            options.viewOn = NamespaceString::kRsOplogNamespace.coll().toString();
            options.pipeline = pipeline.arr();

            WriteUnitOfWork wuow(opCtx);
            auto status =
                db->createView(opCtx, NamespaceString::kTenantMigrationOplogView, options);
            if (status == ErrorCodes::NamespaceExists) {
                return;
            }
            uassertStatusOK(status);
            wuow.commit();
        });
}

std::unique_ptr<Pipeline, PipelineDeleter> createCommittedTransactionsPipelineForTenantMigrations(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const Timestamp& startApplyingTimestamp,
    const std::string& tenantId) {
    Pipeline::SourceContainer stages;
    using Doc = Document;

    // 1. Match config.transactions entries that have a 'lastWriteOpTime.ts' before or at
    //    'startApplyingTimestamp' and 'state: committed', which indicates that it is a committed
    //    transaction. Retryable writes should not have the 'state' field.
    stages.emplace_back(DocumentSourceMatch::createFromBson(
        Doc{{"$match",
             Doc{{"state", Value{"committed"_sd}},
                 {"lastWriteOpTime.ts", Doc{{"$lte", startApplyingTimestamp}}}}}}
            .toBson()
            .firstElement(),
        expCtx));

    // 2. Get all oplog entries that have a timestamp equal to 'lastWriteOpTime.ts'. Store these
    //    oplog entries in the 'oplogEntry' field.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(fromjson("{\
        $lookup: {\
            from: {db: 'local', coll: 'system.tenantMigration.oplogView'},\
            localField: 'lastWriteOpTime.ts',\
            foreignField: 'ts',\
            as: 'oplogEntry'\
        }}")
                                                                 .firstElement(),
                                                             expCtx));

    // 3. Filter out the entries that do not belong to the tenant.
    stages.emplace_back(DocumentSourceMatch::createFromBson(fromjson("{\
        $match: {\
            'oplogEntry.applyOpsNs': {$regex: '^" + tenantId + "_'}\
        }}")
                                                                .firstElement(),
                                                            expCtx));

    // 4. Unset the 'oplogEntry' field and return the committed transaction entries.
    stages.emplace_back(DocumentSourceProject::createUnset(FieldPath("oplogEntry"), expCtx));

    return Pipeline::create(std::move(stages), expCtx);
}

std::unique_ptr<Pipeline, PipelineDeleter> createRetryableWritesOplogFetchingPipeline(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const Timestamp& startFetchingTimestamp,
    const std::string& tenantId) {

    using Doc = Document;
    const Value DNE = Value{Doc{{"$exists", false}}};

    Pipeline::SourceContainer stages;

    // 1. Match config.transactions entries that do not have a `state` field, which indicates that
    //    the last write on the session was a retryable write and not a transaction.
    stages.emplace_back(DocumentSourceMatch::create(Doc{{"state", DNE}}.toBson(), expCtx));

    // 2. Fetch latest oplog entry for each config.transactions entry from the oplog view. `lastOps`
    //    is expected to contain exactly one element, unless `ns` does not contain the correct
    //    `tenantId`. In that case, it will be empty.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(fromjson("{\
                    $lookup: {\
                        from: {db: 'local', coll: 'system.tenantMigration.oplogView'},\
                        localField: 'lastWriteOpTime.ts',\
                        foreignField: 'ts',\
                        pipeline: [{\
                            $match: {\
                                $or: [\
                                    {ns: {$regex: '^" + tenantId + "_'}}, \
                                    {applyOpsNs: {$regex: '^" + tenantId +
                                                                      "_'}}\
                                ]\
                            }\
                        }],\
                        as: 'lastOps'\
                    }}")
                                                                 .firstElement(),
                                                             expCtx));

    // 3. Filter out entries with an empty `lastOps` array since they do not correspond to the
    //    correct tenant.
    stages.emplace_back(DocumentSourceMatch::create(fromjson("{'lastOps': {$ne: []}}"), expCtx));

    // 4. Replace the single-element 'lastOps' array field with a single 'lastOp' field.
    stages.emplace_back(
        DocumentSourceAddFields::create(fromjson("{lastOp: {$first: '$lastOps'}}"), expCtx));

    // 5. Remove `lastOps` in favor of `lastOp`.
    stages.emplace_back(DocumentSourceProject::createUnset(FieldPath("lastOps"), expCtx));

    // 6. If `lastOp` does not have `preImageOpTime` or `postImageOpTime` field, assign a dummy
    //    timestamp so that the next two $lookup stages do not need to do collection scan on the
    //    the oplog collection, because otherwise $lookup treats the field as having a value of
    //    of null, preventing it from seeking directly to the entry with the matching timestamp.
    stages.emplace_back(DocumentSourceAddFields::create(fromjson("{\
            'lastOp.preImageOpTime': {\
                $ifNull: ['$lastOp.preImageOpTime', {ts: Timestamp(0, 0), t: -1}]\
            },\
            'lastOp.postImageOpTime': {\
                $ifNull: ['$lastOp.postImageOpTime', {ts: Timestamp(0, 0), t: -1}]\
            }\
        }"),
                                                        expCtx));

    // 7. Fetch preImage oplog entry for `findAndModify` from the oplog view. `preImageOps` is
    //    expected to contain exactly one element if the `preImageOpTime` field is not null and
    //    is earlier than `startFetchingTimestamp`.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(fromjson("{\
                    $lookup: {\
                        from: {db: 'local', coll: 'system.tenantMigration.oplogView'},\
                        localField: 'lastOp.preImageOpTime.ts',\
                        foreignField: 'ts',\
                        pipeline: [{\
                            $match: {\
                                'ts': {$lt: " + startFetchingTimestamp.toString() +
                                                                      "}\
                            }\
                        }],\
                        as: 'preImageOps'\
                    }}")
                                                                 .firstElement(),
                                                             expCtx));

    // 8. Fetch postImage oplog entry for `findAndModify` from the oplog view. `postImageOps` is
    //    expected to contain exactly one element if the `postImageOpTime` field is not null and is
    //    earlier than `startFetchingTimestamp`.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(fromjson("{\
                    $lookup: {\
                        from: {db: 'local', coll: 'system.tenantMigration.oplogView'},\
                        localField: 'lastOp.postImageOpTime.ts',\
                        foreignField: 'ts',\
                        pipeline: [{\
                            $match: {\
                                'ts': {$lt: " + startFetchingTimestamp.toString() +
                                                                      "}\
                            }\
                        }],\
                        as: 'postImageOps'\
                    }}")
                                                                 .firstElement(),
                                                             expCtx));

    // 9. Fetch oplog entries in each chain from the oplog view.
    stages.emplace_back(DocumentSourceGraphLookUp::createFromBson(
        Doc{{"$graphLookup",
             Doc{{"from", Doc{{"db", "local"_sd}, {"coll", "system.tenantMigration.oplogView"_sd}}},
                 {"startWith", "$lastOp.ts"_sd},
                 {"connectFromField", "prevOpTime.ts"_sd},
                 {"connectToField", "ts"_sd},
                 {"as", "history"_sd},
                 {"depthField", "depthForTenantMigration"_sd}}}}
            .toBson()
            .firstElement(),
        expCtx));

    // 10. Filter out all oplog entries from the `history` array that occur after
    //    `startFetchingTimestamp`. We keep the entry at the `startFetchingTimestamp` here so that
    //    we can capture any synthetic oplog entries that need to be created in the
    //    `FindAndModifyImageLookup` stage later. We do not need to sort the history after this
    //    since we will put the fetched entries into the oplog buffer collection, where entries are
    //    read in timestamp order.
    stages.emplace_back(DocumentSourceAddFields::create(fromjson("{\
                    history: {$filter: {\
                        input: '$history',\
                        cond: {$lte: ['$$this.ts', " + startFetchingTimestamp.toString() +
                                                                 "]}}}}"),
                                                        expCtx));

    // 11. Combine the oplog entries.
    stages.emplace_back(DocumentSourceAddFields::create(fromjson("{\
                        'history': {$concatArrays: [\
                            '$preImageOps', '$postImageOps', '$history']}}"),
                                                        expCtx));

    // 12. Keep only the `history` field to minimize the unwind result in the next stage.
    stages.emplace_back(DocumentSourceProject::createFromBson(
        BSON("$project" << BSON("_id" << 0 << "history" << 1)).firstElement(), expCtx));

    // 13. Unwind oplog entries in each `history` chain. This serves as an optimization for the
    //     next $lookup stage. Without unwinding, `history` is an array and the next $lookup will
    //     do a collection scan on the oplog collection to find all entries that match any element
    //     in the array, which is not efficient. After unwinding, the $lookup can utilize the fact
    //     that oplog collection is order by timestamp to seek directly to an entry that matches
    //     a timestamp without scanning the entire oplog collection.
    stages.emplace_back(DocumentSourceUnwind::create(expCtx, "history", false, boost::none));

    // 14. Fetch the complete oplog entries. `completeOplogEntry` is expected to contain exactly one
    //     element.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(
        Doc{{"$lookup",
             Doc{{"from", Doc{{"db", "local"_sd}, {"coll", "oplog.rs"_sd}}},
                 {"localField", "history.ts"_sd},
                 {"foreignField", "ts"_sd},
                 {"as", "completeOplogEntry"_sd}}}}
            .toBson()
            .firstElement(),
        expCtx));

    // 15. Unwind oplog entries in each chain to the top-level array.
    stages.emplace_back(
        DocumentSourceUnwind::create(expCtx, "completeOplogEntry", false, boost::none));

    // 16. Replace root.
    stages.emplace_back(DocumentSourceReplaceRoot::createFromBson(
        fromjson("{$replaceRoot: {newRoot: '$completeOplogEntry'}}").firstElement(), expCtx));

    // 17. Downconvert any 'findAndModify' oplog entries to store pre- and post-images in the
    //     oplog rather than in a side collection.
    stages.emplace_back(DocumentSourceFindAndModifyImageLookup::create(expCtx));

    // 18. Since the oplog fetching and application stages will already capture entries after
    //    `startFetchingTimestamp`, we only need the earlier part of the oplog chain.
    stages.emplace_back(DocumentSourceMatch::createFromBson(
        BSON("$match" << BSON("ts" << BSON("$lt" << startFetchingTimestamp))).firstElement(),
        expCtx));

    return Pipeline::create(std::move(stages), expCtx);
}

std::unique_ptr<Pipeline, PipelineDeleter> createRetryableWritesOplogFetchingPipelineForAllTenants(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const Timestamp& startFetchingTimestamp) {

    using Doc = Document;
    const Value DNE = Value{Doc{{"$exists", false}}};

    Pipeline::SourceContainer stages;

    // 1. Match config.transactions entries that do not have a `state` field, which indicates that
    //    the last write on the session was a retryable write and not a transaction.
    stages.emplace_back(DocumentSourceMatch::create(Doc{{"state", DNE}}.toBson(), expCtx));

    // 2. Fetch latest oplog entry for each config.transactions entry from the oplog view. `lastOps`
    //    is expected to contain every elements from `oplogView` for all the tenants.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(fromjson("{\
                    $lookup: {\
                        from: {db: 'local', coll: 'system.tenantMigration.oplogView'},\
                        localField: 'lastWriteOpTime.ts',\
                        foreignField: 'ts',\
                        as: 'lastOps'\
                    }}")
                                                                 .firstElement(),
                                                             expCtx));

    // 3. Replace the single-element 'lastOps' array field with a single 'lastOp' field.
    stages.emplace_back(
        DocumentSourceAddFields::create(fromjson("{lastOp: {$first: '$lastOps'}}"), expCtx));

    // 4. Remove `lastOps` in favor of `lastOp`.
    stages.emplace_back(DocumentSourceProject::createUnset(FieldPath("lastOps"), expCtx));

    // 5. If `lastOp` does not have `preImageOpTime` or `postImageOpTime` field, assign a dummy
    //    timestamp so that the next two $lookup stages do not need to do collection scan on the
    //    the oplog collection, because otherwise $lookup treats the field as having a value of
    //    of null, preventing it from seeking directly to the entry with the matching timestamp.
    stages.emplace_back(DocumentSourceAddFields::create(fromjson("{\
            'lastOp.preImageOpTime': {\
                $ifNull: ['$lastOp.preImageOpTime', {ts: Timestamp(0, 0), t: -1}]\
            },\
            'lastOp.postImageOpTime': {\
                $ifNull: ['$lastOp.postImageOpTime', {ts: Timestamp(0, 0), t: -1}]\
            }\
        }"),
                                                        expCtx));

    // 6. Fetch preImage oplog entry for `findAndModify` from the oplog view. `preImageOps` is
    //    expected to contain exactly one element if the `preImageOpTime` field is not null and
    //    is earlier than `startFetchingTimestamp`.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(fromjson("{\
                    $lookup: {\
                        from: {db: 'local', coll: 'system.tenantMigration.oplogView'},\
                        localField: 'lastOp.preImageOpTime.ts',\
                        foreignField: 'ts',\
                        pipeline: [{\
                            $match: {\
                                'ts': {$lt: " + startFetchingTimestamp.toString() +
                                                                      "}\
                            }\
                        }],\
                        as: 'preImageOps'\
                    }}")
                                                                 .firstElement(),
                                                             expCtx));

    // 7. Fetch postImage oplog entry for `findAndModify` from the oplog view. `postImageOps` is
    //    expected to contain exactly one element if the `postImageOpTime` field is not null and is
    //    earlier than `startFetchingTimestamp`.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(fromjson("{\
                    $lookup: {\
                        from: {db: 'local', coll: 'system.tenantMigration.oplogView'},\
                        localField: 'lastOp.postImageOpTime.ts',\
                        foreignField: 'ts',\
                        pipeline: [{\
                            $match: {\
                                'ts': {$lt: " + startFetchingTimestamp.toString() +
                                                                      "}\
                            }\
                        }],\
                        as: 'postImageOps'\
                    }}")
                                                                 .firstElement(),
                                                             expCtx));

    // 8. Fetch oplog entries in each chain from the oplog view.
    stages.emplace_back(DocumentSourceGraphLookUp::createFromBson(
        Doc{{"$graphLookup",
             Doc{{"from", Doc{{"db", "local"_sd}, {"coll", "system.tenantMigration.oplogView"_sd}}},
                 {"startWith", "$lastOp.ts"_sd},
                 {"connectFromField", "prevOpTime.ts"_sd},
                 {"connectToField", "ts"_sd},
                 {"as", "history"_sd},
                 {"depthField", "depthForTenantMigration"_sd}}}}
            .toBson()
            .firstElement(),
        expCtx));

    // 9. Filter out all oplog entries from the `history` array that occur after
    //    `startFetchingTimestamp`. We keep the entry at the `startFetchingTimestamp` here so that
    //    we can capture any synthetic oplog entries that need to be created in the
    //    `FindAndModifyImageLookup` stage later. We do not need to sort the history after this
    //    since we will put the fetched entries into the oplog buffer collection, where entries are
    //    read in timestamp order.
    stages.emplace_back(DocumentSourceAddFields::create(fromjson("{\
                    history: {$filter: {\
                        input: '$history',\
                        cond: {$lte: ['$$this.ts', " + startFetchingTimestamp.toString() +
                                                                 "]}}}}"),
                                                        expCtx));

    // 10. Combine the oplog entries.
    stages.emplace_back(DocumentSourceAddFields::create(fromjson("{\
                        'history': {$concatArrays: [\
                            '$preImageOps', '$postImageOps', '$history']}}"),
                                                        expCtx));

    // 11. Keep only the `history` field to minimize the unwind result in the next stage.
    stages.emplace_back(DocumentSourceProject::createFromBson(
        BSON("$project" << BSON("_id" << 0 << "history" << 1)).firstElement(), expCtx));

    // 12. Unwind oplog entries in each `history` chain. This serves as an optimization for the
    //     next $lookup stage. Without unwinding, `history` is an array and the next $lookup will
    //     do a collection scan on the oplog collection to find all entries that match any element
    //     in the array, which is not efficient. After unwinding, the $lookup can utilize the fact
    //     that oplog collection is order by timestamp to seek directly to an entry that matches
    //     a timestamp without scanning the entire oplog collection.
    stages.emplace_back(DocumentSourceUnwind::create(expCtx, "history", false, boost::none));

    // 13. Fetch the complete oplog entries. `completeOplogEntry` is expected to contain exactly one
    //     element.
    stages.emplace_back(DocumentSourceLookUp::createFromBson(
        Doc{{"$lookup",
             Doc{{"from", Doc{{"db", "local"_sd}, {"coll", "oplog.rs"_sd}}},
                 {"localField", "history.ts"_sd},
                 {"foreignField", "ts"_sd},
                 {"as", "completeOplogEntry"_sd}}}}
            .toBson()
            .firstElement(),
        expCtx));

    // 14. Unwind oplog entries in each chain to the top-level array.
    stages.emplace_back(
        DocumentSourceUnwind::create(expCtx, "completeOplogEntry", false, boost::none));

    // 15. Replace root.
    stages.emplace_back(DocumentSourceReplaceRoot::createFromBson(
        fromjson("{$replaceRoot: {newRoot: '$completeOplogEntry'}}").firstElement(), expCtx));

    // 16. Downconvert any 'findAndModify' oplog entries to store pre- and post-images in the
    //     oplog rather than in a side collection.
    stages.emplace_back(DocumentSourceFindAndModifyImageLookup::create(expCtx));

    // 17. Since the oplog fetching and application stages will already capture entries after
    //    `startFetchingTimestamp`, we only need the earlier part of the oplog chain.
    stages.emplace_back(DocumentSourceMatch::createFromBson(
        BSON("$match" << BSON("ts" << BSON("$lt" << startFetchingTimestamp))).firstElement(),
        expCtx));

    return Pipeline::create(std::move(stages), expCtx);
}


bool shouldStopUpdatingExternalKeys(Status status, const CancellationToken& token) {
    return status.isOK() || token.isCanceled();
}

ExecutorFuture<void> markExternalKeysAsGarbageCollectable(
    ServiceContext* serviceContext,
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    std::shared_ptr<executor::TaskExecutor> parentExecutor,
    UUID migrationId,
    const CancellationToken& token) {
    auto ttlExpiresAt = serviceContext->getFastClockSource()->now() +
        Milliseconds{repl::tenantMigrationGarbageCollectionDelayMS.load()} +
        Seconds{repl::tenantMigrationExternalKeysRemovalBufferSecs.load()};
    return AsyncTry([executor, migrationId, ttlExpiresAt] {
               return ExecutorFuture(**executor).then([migrationId, ttlExpiresAt] {
                   auto opCtxHolder = cc().makeOperationContext();
                   auto opCtx = opCtxHolder.get();

                   pauseTenantMigrationBeforeMarkingExternalKeysGarbageCollectable.pauseWhileSet(
                       opCtx);

                   const auto& nss = NamespaceString::kExternalKeysCollectionNamespace;
                   auto collection = acquireCollection(
                       opCtx,
                       CollectionAcquisitionRequest(
                           nss,
                           PlacementConcern{boost::none, ShardVersion::UNSHARDED()},
                           repl::ReadConcernArgs::get(opCtx),
                           AcquisitionPrerequisites::kWrite),
                       MODE_IX);

                   writeConflictRetry(
                       opCtx, "TenantMigrationMarkExternalKeysAsGarbageCollectable", nss, [&] {
                           auto request = UpdateRequest();
                           request.setNamespaceString(nss);
                           request.setQuery(
                               BSON(ExternalKeysCollectionDocument::kMigrationIdFieldName
                                    << migrationId));
                           request.setUpdateModification(
                               write_ops::UpdateModification::parseFromClassicUpdate(BSON(
                                   "$set"
                                   << BSON(ExternalKeysCollectionDocument::kTTLExpiresAtFieldName
                                           << ttlExpiresAt))));
                           request.setMulti(true);

                           // Note marking keys garbage collectable is not atomic with marking the
                           // state document garbage collectable, so after a failover this update
                           // may fail to match any keys if they were previously marked garbage
                           // collectable and deleted by the TTL monitor. Because of this we can't
                           // assert on the update result's numMatched or numDocsModified.
                           update(opCtx, collection, request);
                       });
               });
           })
        .until([token](Status status) { return shouldStopUpdatingExternalKeys(status, token); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, CancellationToken::uncancelable());
}

BSONObj redactStateDoc(BSONObj stateDoc) {
    mutablebson::Document stateDocToLog(stateDoc, mutablebson::Document::kInPlaceDisabled);
    for (auto& sensitiveField : kSensitiveFieldNames) {
        for (mutablebson::Element element =
                 mutablebson::findFirstChildNamed(stateDocToLog.root(), sensitiveField);
             element.ok();
             element = mutablebson::findElementNamed(element.rightSibling(), sensitiveField)) {
            uassertStatusOK(element.setValueString("xxx"));
        }
    }
    return stateDocToLog.getObject();
}

}  // namespace tenant_migration_util

}  // namespace mongo
