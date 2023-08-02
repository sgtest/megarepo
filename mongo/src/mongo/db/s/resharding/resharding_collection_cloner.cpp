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


#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr.hpp>
#include <mutex>
#include <string>
#include <tuple>
#include <utility>

#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/json.h"
#include "mongo/client/read_preference.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/curop.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value_comparator.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/repl/read_concern_level.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/s/resharding/document_source_resharding_ownership_match.h"
#include "mongo/db/s/resharding/resharding_collection_cloner.h"
#include "mongo/db/s/resharding/resharding_data_copy_util.h"
#include "mongo/db/s/resharding/resharding_future_util.h"
#include "mongo/db/s/resharding/resharding_metrics.h"
#include "mongo/db/s/resharding/resharding_server_parameters_gen.h"
#include "mongo/db/s/resharding/resharding_util.h"
#include "mongo/db/service_context.h"
#include "mongo/db/session/logical_session_id_helpers.h"
#include "mongo/executor/task_executor.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/database_version.h"
#include "mongo/s/grid.h"
#include "mongo/s/index_version.h"
#include "mongo/s/resharding/resharding_feature_flag_gen.h"
#include "mongo/s/shard_version.h"
#include "mongo/s/shard_version_factory.h"
#include "mongo/s/sharding_index_catalog_cache.h"
#include "mongo/s/stale_shard_version_helpers.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/future_util.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"
#include "mongo/util/string_map.h"
#include "mongo/util/timer.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kResharding


namespace mongo {
namespace {

bool collectionHasSimpleCollation(OperationContext* opCtx, const NamespaceString& nss) {
    auto catalogCache = Grid::get(opCtx)->catalogCache();
    auto [sourceChunkMgr, _] = uassertStatusOK(catalogCache->getCollectionRoutingInfo(opCtx, nss));

    uassert(ErrorCodes::NamespaceNotSharded,
            str::stream() << "Expected collection " << nss.toStringForErrorMsg()
                          << " to be sharded",
            sourceChunkMgr.isSharded());

    return !sourceChunkMgr.getDefaultCollator();
}

}  // namespace

ReshardingCollectionCloner::ReshardingCollectionCloner(ReshardingMetrics* metrics,
                                                       const UUID& reshardingUUID,
                                                       ShardKeyPattern newShardKeyPattern,
                                                       NamespaceString sourceNss,
                                                       const UUID& sourceUUID,
                                                       ShardId recipientShard,
                                                       Timestamp atClusterTime,
                                                       NamespaceString outputNss)
    : _metrics(metrics),
      _reshardingUUID(std::move(reshardingUUID)),
      _newShardKeyPattern(std::move(newShardKeyPattern)),
      _sourceNss(std::move(sourceNss)),
      _sourceUUID(std::move(sourceUUID)),
      _recipientShard(std::move(recipientShard)),
      _atClusterTime(atClusterTime),
      _outputNss(std::move(outputNss)) {}

std::pair<std::vector<BSONObj>, boost::intrusive_ptr<ExpressionContext>>
ReshardingCollectionCloner::makeRawPipeline(
    OperationContext* opCtx,
    std::shared_ptr<MongoProcessInterface> mongoProcessInterface,
    Value resumeId) {
    // Assume that the input collection isn't a view. The collectionUUID parameter to
    // the aggregate would enforce this anyway.
    StringMap<ExpressionContext::ResolvedNamespace> resolvedNamespaces;
    resolvedNamespaces[_sourceNss.coll()] = {_sourceNss, std::vector<BSONObj>{}};

    // Assume that the config.cache.chunks collection isn't a view either.
    auto tempNss =
        resharding::constructTemporaryReshardingNss(_sourceNss.db_forSharding(), _sourceUUID);
    auto tempCacheChunksNss = NamespaceString::makeGlobalConfigCollection(
        "cache.chunks." + NamespaceStringUtil::serialize(tempNss));
    resolvedNamespaces[tempCacheChunksNss.coll()] = {tempCacheChunksNss, std::vector<BSONObj>{}};

    // Pipeline::makePipeline() ignores the collation set on the AggregationRequest (or lack
    // thereof) and instead only considers the collator set on the ExpressionContext. Setting
    // nullptr as the collator on the ExpressionContext means that the aggregation pipeline is
    // always using the "simple" collation, even when the collection default collation for
    // _sourceNss is non-simple. The chunk ranges in the $lookup stage must be compared using the
    // simple collation because collections are always sharded using the simple collation. However,
    // resuming by _id is only efficient (i.e. non-blocking seek/sort) when the aggregation pipeline
    // would be using the collection's default collation. We cannot do both so we choose to disallow
    // automatic resuming for collections with non-simple default collations.
    uassert(4929303,
            "Cannot resume cloning when sharded collection has non-simple default collation",
            resumeId.missing() || collectionHasSimpleCollation(opCtx, _sourceNss));

    auto expCtx = make_intrusive<ExpressionContext>(opCtx,
                                                    boost::none, /* explain */
                                                    false,       /* fromMongos */
                                                    false,       /* needsMerge */
                                                    false,       /* allowDiskUse */
                                                    false,       /* bypassDocumentValidation */
                                                    false,       /* isMapReduceCommand */
                                                    _sourceNss,
                                                    boost::none, /* runtimeConstants */
                                                    nullptr,     /* collator */
                                                    std::move(mongoProcessInterface),
                                                    std::move(resolvedNamespaces),
                                                    _sourceUUID);

    std::vector<BSONObj> rawPipeline;

    if (!resumeId.missing()) {
        rawPipeline.emplace_back(BSON(
            "$match" << BSON(
                "$expr" << BSON("$gte" << BSON_ARRAY("$_id" << BSON("$literal" << resumeId))))));
    }

    auto keyPattern = ShardKeyPattern(_newShardKeyPattern.getKeyPattern()).toBSON();
    rawPipeline.emplace_back(
        BSON(DocumentSourceReshardingOwnershipMatch::kStageName
             << BSON("recipientShardId" << _recipientShard << "reshardingKey" << keyPattern)));

    // We use $arrayToObject to synthesize the $sortKeys needed by the AsyncResultsMerger to
    // merge the results from all of the donor shards by {_id: 1}. This expression wouldn't
    // be correct if the aggregation pipeline was using a non-"simple" collation.
    rawPipeline.emplace_back(
        fromjson("{$replaceWith: {$mergeObjects: [\
            '$$ROOT',\
            {$arrayToObject: {$concatArrays: [[{\
                k: {$literal: '$sortKey'},\
                v: ['$$ROOT._id']\
            }]]}}\
        ]}}"));

    return std::make_pair(std::move(rawPipeline), std::move(expCtx));
}

std::unique_ptr<Pipeline, PipelineDeleter> ReshardingCollectionCloner::_targetAggregationRequest(
    const std::vector<BSONObj>& rawPipeline,
    const boost::intrusive_ptr<ExpressionContext>& expCtx) {
    auto opCtx = expCtx->opCtx;
    // We associate the aggregation cursors established on each donor shard with a logical
    // session to prevent them from killing the cursor when it is idle locally. Due to the
    // cursor's merging behavior across all donor shards, it is possible for the cursor to be
    // active on one donor shard while idle for a long period on another donor shard.
    {
        auto lk = stdx::lock_guard(*opCtx->getClient());
        opCtx->setLogicalSessionId(makeLogicalSessionId(opCtx));
    }

    AggregateCommandRequest request(_sourceNss, rawPipeline);
    request.setCollectionUUID(_sourceUUID);

    auto hint = collectionHasSimpleCollation(opCtx, _sourceNss)
        ? boost::optional<BSONObj>{BSON("_id" << 1)}
        : boost::none;

    if (hint) {
        request.setHint(*hint);
    }

    request.setReadConcern(BSON(repl::ReadConcernArgs::kLevelFieldName
                                << repl::readConcernLevels::kSnapshotName
                                << repl::ReadConcernArgs::kAtClusterTimeFieldName
                                << _atClusterTime));
    // The read preference on the request is merely informational (e.g. for profiler entries) -- the
    // pipeline's opCtx setting is actually used when sending the request.
    auto readPref = ReadPreferenceSetting{ReadPreference::Nearest};
    request.setUnwrappedReadPref(readPref.toContainingBSON());
    ReadPreferenceSetting::get(opCtx) = readPref;

    return shardVersionRetry(opCtx,
                             Grid::get(opCtx)->catalogCache(),
                             _sourceNss,
                             "targeting donor shards for resharding collection cloning"_sd,
                             [&] {
                                 // We use the hint as an implied sort for $mergeCursors because
                                 // the aggregation pipeline synthesizes the necessary $sortKeys
                                 // fields in the result set.
                                 return Pipeline::makePipeline(request, std::move(expCtx), hint);
                             });
}

std::unique_ptr<Pipeline, PipelineDeleter> ReshardingCollectionCloner::_restartPipeline(
    OperationContext* opCtx) {
    auto idToResumeFrom = [&] {
        AutoGetCollection outputColl(opCtx, _outputNss, MODE_IS);
        uassert(ErrorCodes::NamespaceNotFound,
                str::stream() << "Resharding collection cloner's output collection '"
                              << _outputNss.toStringForErrorMsg() << "' did not already exist",
                outputColl);
        return resharding::data_copy::findHighestInsertedId(opCtx, *outputColl);
    }();

    // The BlockingResultsMerger underlying by the $mergeCursors stage records how long the
    // recipient spent waiting for documents from the donor shards. It doing so requires the CurOp
    // to be marked as having started.
    auto* curOp = CurOp::get(opCtx);
    curOp->ensureStarted();
    ON_BLOCK_EXIT([curOp] { curOp->done(); });

    auto [rawPipeline, expCtx] =
        makeRawPipeline(opCtx, MongoProcessInterface::create(opCtx), idToResumeFrom);
    auto pipeline = _targetAggregationRequest(rawPipeline, expCtx);

    if (!idToResumeFrom.missing()) {
        // Skip inserting the first document retrieved after resuming because $gte was used in the
        // aggregation pipeline.
        auto firstDoc = pipeline->getNext();
        uassert(4929301,
                str::stream() << "Expected pipeline to retrieve document with _id: "
                              << redact(idToResumeFrom.toString()),
                firstDoc);

        // Note that the following uassert() could throw because we're using the simple string
        // comparator and the collection could have a non-simple collation. However, it would still
        // be correct to throw an exception because it would mean the collection being resharded
        // contains multiple documents with the same _id value as far as global uniqueness is
        // concerned.
        const auto& firstId = (*firstDoc)["_id"];
        uassert(4929302,
                str::stream() << "Expected pipeline to retrieve document with _id: "
                              << redact(idToResumeFrom.toString())
                              << ", but got _id: " << redact(firstId.toString()),
                ValueComparator::kInstance.evaluate(firstId == idToResumeFrom));
    }

    pipeline->detachFromOperationContext();
    pipeline.get_deleter().dismissDisposal();
    return pipeline;
}

bool ReshardingCollectionCloner::doOneBatch(OperationContext* opCtx,
                                            Pipeline& pipeline,
                                            TxnNumber& txnNum) {
    pipeline.reattachToOperationContext(opCtx);
    ON_BLOCK_EXIT([&pipeline] { pipeline.detachFromOperationContext(); });

    Timer latencyTimer;
    auto batch = resharding::data_copy::fillBatchForInsert(
        pipeline, resharding::gReshardingCollectionClonerBatchSizeInBytes.load());

    _metrics->onCloningRemoteBatchRetrieval(duration_cast<Milliseconds>(latencyTimer.elapsed()));

    if (batch.empty()) {
        return false;
    }

    Timer batchInsertTimer;
    int bytesInserted = resharding::data_copy::withOneStaleConfigRetry(opCtx, [&] {
        // ReshardingOpObserver depends on the collection metadata being known when processing
        // writes to the temporary resharding collection. We attach shard version IGNORED to the
        // insert operations and retry once on a StaleConfig error to allow the collection metadata
        // information to be recovered.
        auto [_, sii] = uassertStatusOK(
            Grid::get(opCtx)->catalogCache()->getCollectionRoutingInfo(opCtx, _outputNss));
        if (resharding::gFeatureFlagReshardingImprovements.isEnabled(
                serverGlobalParams.featureCompatibility)) {
            // TODO(SERVER-77636) -- This is temporary code, passing a dummy shard ID and the last
            // "_id" instead of the real source shard and the resume token.
            return resharding::data_copy::insertBatchTransactionally(
                opCtx,
                _outputNss,
                sii,
                txnNum,
                batch,
                _reshardingUUID,
                {"dummy"},
                HostAndPort("dummyHost", 27017),
                batch.back().doc["_id"].wrap());
        } else {
            ScopedSetShardRole scopedSetShardRole(
                opCtx,
                _outputNss,
                ShardVersionFactory::make(ChunkVersion::IGNORED(),
                                          sii ? boost::make_optional(sii->getCollectionIndexes())
                                              : boost::none) /* shardVersion */,
                boost::none /* databaseVersion */);
            return resharding::data_copy::insertBatch(opCtx, _outputNss, batch);
        }
    });

    _metrics->onDocumentsProcessed(
        batch.size(), bytesInserted, Milliseconds(batchInsertTimer.millis()));

    return true;
}

SemiFuture<void> ReshardingCollectionCloner::run(
    std::shared_ptr<executor::TaskExecutor> executor,
    std::shared_ptr<executor::TaskExecutor> cleanupExecutor,
    CancellationToken cancelToken,
    CancelableOperationContextFactory factory) {
    struct ChainContext {
        std::unique_ptr<Pipeline, PipelineDeleter> pipeline;
        bool moreToCome = true;
        boost::optional<LogicalSessionId> batchLogicalSessionId;
        TxnNumber batchTxnNumber = TxnNumber(0);
    };

    auto chainCtx = std::make_shared<ChainContext>();

    return resharding::WithAutomaticRetry([this, chainCtx, factory] {
               if (!chainCtx->pipeline) {
                   auto opCtx = factory.makeOperationContext(&cc());
                   chainCtx->pipeline = _restartPipeline(opCtx.get());
               }

               auto opCtx = factory.makeOperationContext(&cc());
               ScopeGuard guard([&] {
                   chainCtx->pipeline->dispose(opCtx.get());
                   chainCtx->pipeline.reset();
               });
               if (resharding::gFeatureFlagReshardingImprovements.isEnabled(
                       serverGlobalParams.featureCompatibility)) {
                   if (!chainCtx->batchLogicalSessionId) {
                       chainCtx->batchLogicalSessionId = makeLogicalSessionId(opCtx.get());
                   }
                   opCtx->setLogicalSessionId(*chainCtx->batchLogicalSessionId);
               }
               chainCtx->moreToCome =
                   doOneBatch(opCtx.get(), *chainCtx->pipeline, chainCtx->batchTxnNumber);
               guard.dismiss();
           })
        .onTransientError([this](const Status& status) {
            LOGV2(5269300,
                  "Transient error while cloning sharded collection",
                  "sourceNamespace"_attr = _sourceNss,
                  "outputNamespace"_attr = _outputNss,
                  "readTimestamp"_attr = _atClusterTime,
                  "error"_attr = redact(status));
        })
        .onUnrecoverableError([this](const Status& status) {
            LOGV2_ERROR(5352400,
                        "Operation-fatal error for resharding while cloning sharded collection",
                        "sourceNamespace"_attr = _sourceNss,
                        "outputNamespace"_attr = _outputNss,
                        "readTimestamp"_attr = _atClusterTime,
                        "error"_attr = redact(status));
        })
        .until<Status>([chainCtx, factory](const Status& status) {
            if (!status.isOK() && chainCtx->pipeline) {
                auto opCtx = factory.makeOperationContext(&cc());
                chainCtx->pipeline->dispose(opCtx.get());
                chainCtx->pipeline.reset();
            }

            return status.isOK() && !chainCtx->moreToCome;
        })
        .on(std::move(executor), std::move(cancelToken))
        .thenRunOn(std::move(cleanupExecutor))
        // It is unsafe to capture `this` once the task is running on the cleanupExecutor because
        // RecipientStateMachine, along with its ReshardingCollectionCloner member, may have already
        // been destructed.
        .onCompletion([chainCtx](Status status) {
            if (chainCtx->pipeline) {
                auto client =
                    cc().getServiceContext()->makeClient("ReshardingCollectionClonerCleanupClient");

                // TODO(SERVER-74658): Please revisit if this thread could be made killable.
                {
                    stdx::lock_guard<Client> lk(*client.get());
                    client.get()->setSystemOperationUnkillableByStepdown(lk);
                }

                AlternativeClientRegion acr(client);
                auto opCtx = cc().makeOperationContext();

                // Guarantee the pipeline is always cleaned up - even upon cancellation.
                chainCtx->pipeline->dispose(opCtx.get());
                chainCtx->pipeline.reset();
            }

            // Propagate the result of the AsyncTry.
            return status;
        })
        .semi();
}

}  // namespace mongo
