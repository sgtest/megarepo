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

#include "mongo/db/pipeline/sharded_agg_helpers.h"

#include <absl/container/flat_hash_map.h>
#include <absl/meta/type_traits.h>
#include <algorithm>
#include <boost/cstdint.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <boost/smart_ptr.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <cstdint>
#include <functional>
#include <iterator>
#include <list>
#include <map>
#include <ratio>
#include <set>
#include <string>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/timestamp.h"
#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/client/read_preference.h"
#include "mongo/crypto/sha256_block.h"
#include "mongo/db/basic_types_gen.h"
#include "mongo/db/commands.h"
#include "mongo/db/curop.h"
#include "mongo/db/exec/document_value/document_metadata_fields.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/field_ref.h"
#include "mongo/db/keypattern.h"
#include "mongo/db/logical_time.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/pipeline/aggregation_request_helper.h"
#include "mongo/db/pipeline/dependencies.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/document_source_change_stream.h"
#include "mongo/db/pipeline/document_source_change_stream_gen.h"
#include "mongo/db/pipeline/document_source_group.h"
#include "mongo/db/pipeline/document_source_limit.h"
#include "mongo/db/pipeline/document_source_merge.h"
#include "mongo/db/pipeline/document_source_project.h"
#include "mongo/db/pipeline/document_source_sequential_document_cache.h"
#include "mongo/db/pipeline/document_source_set_variable_from_subpipeline.h"
#include "mongo/db/pipeline/document_source_skip.h"
#include "mongo/db/pipeline/document_source_sort.h"
#include "mongo/db/pipeline/document_source_unwind.h"
#include "mongo/db/pipeline/lite_parsed_pipeline.h"
#include "mongo/db/pipeline/process_interface/mongo_process_interface.h"
#include "mongo/db/pipeline/search_helper.h"
#include "mongo/db/pipeline/semantic_analysis.h"
#include "mongo/db/pipeline/stage_constraints.h"
#include "mongo/db/pipeline/variables.h"
#include "mongo/db/query/collation/collation_spec.h"
#include "mongo/db/query/cursor_response.h"
#include "mongo/db/query/cursor_response_gen.h"
#include "mongo/db/query/query_request_helper.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/session/logical_session_id_gen.h"
#include "mongo/db/vector_clock.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/executor/remote_command_response.h"
#include "mongo/executor/task_executor.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/rpc/get_status_from_command_result.h"
#include "mongo/s/analyze_shard_key_common_gen.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/cluster_commands_helpers.h"
#include "mongo/s/collection_uuid_mismatch.h"
#include "mongo/s/database_version.h"
#include "mongo/s/grid.h"
#include "mongo/s/query/async_results_merger_params_gen.h"
#include "mongo/s/query/cluster_query_knobs_gen.h"
#include "mongo/s/query/document_source_merge_cursors.h"
#include "mongo/s/query/establish_cursors.h"
#include "mongo/s/query_analysis_sampler_util.h"
#include "mongo/s/router_role.h"
#include "mongo/s/shard_key_pattern.h"
#include "mongo/s/shard_version.h"
#include "mongo/s/stale_exception.h"
#include "mongo/s/transaction_router.h"
#include "mongo/stdx/variant.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/overloaded_visitor.h"  // IWYU pragma: keep
#include "mongo/util/str.h"
#include "mongo/util/string_map.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo {
namespace sharded_agg_helpers {
namespace {

MONGO_FAIL_POINT_DEFINE(shardedAggregateHangBeforeEstablishingShardCursors);

/**
 * Given a document representing an aggregation command such as
 * {aggregate: "myCollection", pipeline: [], ...},
 *
 * produces the corresponding explain command:
 * {explain: {aggregate: "myCollection", pipline: [], ...}, $queryOptions: {...}, verbosity: ...}
 */
Document wrapAggAsExplain(Document aggregateCommand, ExplainOptions::Verbosity verbosity) {
    MutableDocument explainCommandBuilder;
    explainCommandBuilder["explain"] = Value(aggregateCommand);
    // Downstream host targeting code expects queryOptions at the top level of the command object.
    explainCommandBuilder[query_request_helper::kUnwrappedReadPrefField] =
        Value(aggregateCommand[query_request_helper::kUnwrappedReadPrefField]);

    // readConcern needs to be promoted to the top-level of the request.
    explainCommandBuilder[repl::ReadConcernArgs::kReadConcernFieldName] =
        Value(aggregateCommand[repl::ReadConcernArgs::kReadConcernFieldName]);

    // Add explain command options.
    for (auto&& explainOption : ExplainOptions::toBSON(verbosity)) {
        explainCommandBuilder[explainOption.fieldNameStringData()] = Value(explainOption);
    }

    return explainCommandBuilder.freeze();
}

/**
 * Open a $changeStream cursor on the 'config.shards' collection to watch for new shards.
 */
RemoteCursor openChangeStreamNewShardMonitor(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                             Timestamp startMonitoringAtTime) {
    const auto& configShard = Grid::get(expCtx->opCtx)->shardRegistry()->getConfigShard();
    // Pipeline: {$changeStream: {startAtOperationTime: [now], allowToRunOnConfigDB: true}}
    AggregateCommandRequest aggReq(
        NamespaceString::kConfigsvrShardsNamespace,
        {BSON(DocumentSourceChangeStream::kStageName
              << BSON(DocumentSourceChangeStreamSpec::kStartAtOperationTimeFieldName
                      << startMonitoringAtTime
                      << DocumentSourceChangeStreamSpec::kAllowToRunOnConfigDBFieldName << true))});
    aggReq.setFromMongos(true);
    aggReq.setNeedsMerge(true);

    SimpleCursorOptions cursor;
    cursor.setBatchSize(0);
    aggReq.setCursor(cursor);
    auto cmdObjWithRWC =
        applyReadWriteConcern(expCtx->opCtx,
                              true,             /* appendRC */
                              !expCtx->explain, /* appendWC */
                              aggregation_request_helper::serializeToCommandObj(aggReq));
    auto configCursor = establishCursors(expCtx->opCtx,
                                         expCtx->mongoProcessInterface->taskExecutor,
                                         aggReq.getNamespace(),
                                         ReadPreferenceSetting{ReadPreference::SecondaryPreferred},
                                         {{configShard->getId(), cmdObjWithRWC}},
                                         false);
    invariant(configCursor.size() == 1);
    return std::move(*configCursor.begin());
}

BSONObj genericTransformForShards(MutableDocument&& cmdForShards,
                                  const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                  boost::optional<ExplainOptions::Verbosity> explainVerbosity,
                                  BSONObj collationObj,
                                  boost::optional<BSONObj> readConcern) {
    cmdForShards[AggregateCommandRequest::kLetFieldName] =
        Value(expCtx->variablesParseState.serialize(expCtx->variables));

    cmdForShards[AggregateCommandRequest::kFromMongosFieldName] = Value(expCtx->inMongos);

    if (!collationObj.isEmpty()) {
        cmdForShards[AggregateCommandRequest::kCollationFieldName] = Value(collationObj);
    }

    // If this is a request for an aggregation explain, then we must wrap the aggregate inside an
    // explain command.
    if (explainVerbosity) {
        cmdForShards.reset(wrapAggAsExplain(cmdForShards.freeze(), *explainVerbosity));
    }

    if (expCtx->opCtx->getTxnNumber()) {
        invariant(
            cmdForShards.peek()[OperationSessionInfoFromClient::kTxnNumberFieldName].missing(),
            str::stream() << "Command for shards unexpectedly had the "
                          << OperationSessionInfoFromClient::kTxnNumberFieldName
                          << " field set: " << cmdForShards.peek().toString());
        cmdForShards[OperationSessionInfoFromClient::kTxnNumberFieldName] =
            Value(static_cast<long long>(*expCtx->opCtx->getTxnNumber()));
    }

    if (readConcern) {
        cmdForShards["readConcern"] = Value(std::move(*readConcern));
    }

    return cmdForShards.freeze().toBson();
}

std::vector<RemoteCursor> establishShardCursors(
    OperationContext* opCtx,
    std::shared_ptr<executor::TaskExecutor> executor,
    const NamespaceString& nss,
    bool mustRunOnAllShards,
    const boost::optional<CollectionRoutingInfo>& cri,
    const std::set<ShardId>& shardIds,
    const BSONObj& cmdObj,
    const boost::optional<analyze_shard_key::TargetedSampleId>& sampleId,
    const ReadPreferenceSetting& readPref,
    AsyncRequestsSender::ShardHostMap designatedHostsMap,
    stdx::unordered_map<ShardId, BSONObj> resumeTokenMap,
    bool targetEveryShardServer) {
    LOGV2_DEBUG(20904,
                1,
                "Dispatching command {cmdObj} to establish cursors on shards",
                "cmdObj"_attr = redact(cmdObj));

    std::vector<std::pair<ShardId, BSONObj>> requests;

    // If we don't need to run on all shards, then we should always have a valid routing table.
    invariant(cri || mustRunOnAllShards);

    if (targetEveryShardServer) {
        // If we are running on all shard servers we should never designate a particular server.
        invariant(designatedHostsMap.empty());
        // Resume tokens are particular to a host, so it will never make sense to use them when
        // running on all shard servers.
        invariant(resumeTokenMap.empty());
        if (MONGO_unlikely(shardedAggregateHangBeforeEstablishingShardCursors.shouldFail())) {
            LOGV2(
                7355704,
                "shardedAggregateHangBeforeEstablishingShardCursors fail point enabled.  Blocking "
                "until fail point is disabled.");
            while (
                MONGO_unlikely(shardedAggregateHangBeforeEstablishingShardCursors.shouldFail())) {
                sleepsecs(1);
            }
        }
        return establishCursorsOnAllHosts(
            opCtx, std::move(executor), nss, shardIds, cmdObj, false, getDesiredRetryPolicy(opCtx));
    }

    if (mustRunOnAllShards) {
        // The pipeline contains a stage which must be run on all shards. Skip versioning and
        // enqueue the raw command objects.
        for (const auto& shardId : shardIds) {
            requests.emplace_back(shardId, cmdObj);
        }
    } else if (cri->cm.isSharded()) {
        // The collection is sharded. Use the routing table to decide which shards to target
        // based on the query and collation, and build versioned requests for them.
        for (const auto& shardId : shardIds) {
            auto versionedCmdObj = appendShardVersion(cmdObj, cri->getShardVersion(shardId));

            if (sampleId && sampleId->isFor(shardId)) {
                versionedCmdObj =
                    analyze_shard_key::appendSampleId(versionedCmdObj, sampleId->getId());
            }

            requests.emplace_back(shardId, std::move(versionedCmdObj));
        }
    } else {
        // The collection is unsharded. Target only the primary shard for the database.
        // Don't append shard version info when contacting a fixed db collection.
        auto versionedCmdObj = !cri->cm.dbVersion().isFixed()
            ? appendShardVersion(cmdObj, ShardVersion::UNSHARDED())
            : cmdObj;
        versionedCmdObj = appendDbVersionIfPresent(versionedCmdObj, cri->cm.dbVersion());

        if (sampleId) {
            invariant(sampleId->isFor(cri->cm.dbPrimary()));
            versionedCmdObj = analyze_shard_key::appendSampleId(versionedCmdObj, sampleId->getId());
        }

        requests.emplace_back(cri->cm.dbPrimary(), std::move(versionedCmdObj));
    }

    // If we have resume data, use it.
    if (!resumeTokenMap.empty()) {
        for (auto& requestPair : requests) {
            const auto& shardId = requestPair.first;
            auto& request = requestPair.second;
            auto resumeTokenIter = resumeTokenMap.find(shardId);
            if (resumeTokenIter != resumeTokenMap.end()) {
                request = request.addField(
                    BSON(AggregateCommandRequest::kResumeAfterFieldName << resumeTokenIter->second)
                        .firstElement());
            }
        }
    }

    if (MONGO_unlikely(shardedAggregateHangBeforeEstablishingShardCursors.shouldFail())) {
        LOGV2(20905,
              "shardedAggregateHangBeforeEstablishingShardCursors fail point enabled.  Blocking "
              "until fail point is disabled.");
        while (MONGO_unlikely(shardedAggregateHangBeforeEstablishingShardCursors.shouldFail())) {
            sleepsecs(1);
        }
    }

    return establishCursors(opCtx,
                            std::move(executor),
                            nss,
                            readPref,
                            requests,
                            false /* do not allow partial results */,
                            getDesiredRetryPolicy(opCtx),
                            {} /* providedOpKeys */,
                            designatedHostsMap);
}

std::set<ShardId> getTargetedShards(boost::intrusive_ptr<ExpressionContext> expCtx,
                                    bool mustRunOnAllShards,
                                    const boost::optional<CollectionRoutingInfo>& cri,
                                    const BSONObj shardQuery,
                                    const BSONObj collation) {
    if (mustRunOnAllShards) {
        // The pipeline begins with a stage which must be run on all shards.
        auto shardIds = Grid::get(expCtx->opCtx)->shardRegistry()->getAllShardIds(expCtx->opCtx);
        return {std::make_move_iterator(shardIds.begin()), std::make_move_iterator(shardIds.end())};
    }

    invariant(cri);
    return getTargetedShardsForQuery(expCtx, cri->cm, shardQuery, collation);
}

/**
 * Helpers to check and move stages from a DistributedPlanLogic.
 */
void addMaybeNullStageToFront(Pipeline* pipe, boost::intrusive_ptr<DocumentSource> ds) {
    if (ds) {
        pipe->addInitialSource(std::move(ds));
    }
}
void addMaybeNullStageToBack(Pipeline* pipe, boost::intrusive_ptr<DocumentSource> ds) {
    if (ds) {
        pipe->addFinalSource(std::move(ds));
    }
}

boost::optional<BSONObj> getOwnedOrNone(boost::optional<BSONObj> obj) {
    if (obj) {
        return obj->getOwned();
    }
    return boost::none;
}

void addSplitStages(const DocumentSource::DistributedPlanLogic& distributedPlanLogic,
                    Pipeline* mergePipe,
                    Pipeline* shardPipe) {
    // This stage must be split, split it normally.
    // Add in reverse order since we add each to the front and this would flip the order otherwise.
    for (auto reverseIt = distributedPlanLogic.mergingStages.rbegin();
         reverseIt != distributedPlanLogic.mergingStages.rend();
         ++reverseIt) {
        tassert(6448012,
                "A stage cannot simultaneously be present on both sides of a pipeline split",
                distributedPlanLogic.shardsStage != *reverseIt);
        mergePipe->addInitialSource(*reverseIt);
    }
    addMaybeNullStageToBack(shardPipe, distributedPlanLogic.shardsStage);
}

/**
 * Helper for find split point that handles the split after a stage that must be on
 * the merging half of the pipeline defers being added to the merging pipeline.
 */
std::pair<std::unique_ptr<Pipeline, PipelineDeleter>, boost::optional<BSONObj>>
finishFindSplitPointAfterDeferral(
    Pipeline* mergePipe,
    std::unique_ptr<Pipeline, PipelineDeleter> shardPipe,
    boost::intrusive_ptr<DocumentSource> deferredStage,
    boost::optional<BSONObj> mergeSort,
    DocumentSource::DistributedPlanLogic::movePastFunctionType moveCheckFunc) {
    tassert(6253723, "Expected shard pipeline", shardPipe);
    tassert(6253724, "Expected original pipeline", mergePipe);

    while (!mergePipe->getSources().empty()) {
        boost::intrusive_ptr<DocumentSource> current = mergePipe->popFront();
        if (!moveCheckFunc(*current)) {
            mergePipe->addInitialSource(std::move(current));
            break;
        }

        // If this stage also would like to split, split here. Don't defer multiple stages.
        if (auto distributedPlanLogic = current->distributedPlanLogic()) {
            addSplitStages(*distributedPlanLogic, mergePipe, shardPipe.get());

            // The sort that was earlier in the pipeline takes precedence.
            if (!mergeSort) {
                mergeSort = getOwnedOrNone(distributedPlanLogic->mergeSortPattern);
            }
            break;
        }

        // Move the source from the merger _sources to the shard _sources.
        shardPipe->addFinalSource(current);
    }

    // We got to the end of the pipeline or found a split point.
    addMaybeNullStageToFront(mergePipe, std::move(deferredStage));
    return {std::move(shardPipe), getOwnedOrNone(mergeSort)};
}

/**
 * Moves everything before a splittable stage to the shards. If there are no splittable stages,
 * moves everything to the shards.
 *
 * It is not safe to call this optimization multiple times.
 *
 * Returns {shardPipe, sortSpec}. The original passed in pipeline retains all stages after the split
 * point and becomes the merge pipeline.
 */
std::pair<std::unique_ptr<Pipeline, PipelineDeleter>, boost::optional<BSONObj>> findSplitPoint(
    Pipeline* mergePipe) {
    const auto& expCtx = mergePipe->getContext();
    auto shardPipe = Pipeline::create({}, expCtx);
    while (!mergePipe->getSources().empty()) {
        boost::intrusive_ptr<DocumentSource> current = mergePipe->popFront();
        auto distributedPlanLogic = current->distributedPlanLogic();

        // Check if this source is splittable.
        if (!distributedPlanLogic) {
            // Move the source from the merger _sources to the shard _sources.
            shardPipe->addFinalSource(current);
            continue;
        }

        // If we got a plan logic which doesn't require a split, save it and keep going.
        if (!distributedPlanLogic->needsSplit) {
            addMaybeNullStageToBack(shardPipe.get(), std::move(distributedPlanLogic->shardsStage));
            tassert(6253721,
                    "Must have deferral function if deferring pipeline split",
                    distributedPlanLogic->canMovePast);
            auto mergingStageList = distributedPlanLogic->mergingStages;
            tassert(6448007,
                    "Only support deferring at most one stage for now.",
                    mergingStageList.size() <= 1);
            // We know these are all currently null/none, as if we had deferred something and
            // 'current' did not need split we would have returned above.
            return finishFindSplitPointAfterDeferral(
                mergePipe,
                std::move(shardPipe),
                mergingStageList.empty() ? nullptr : std::move(*mergingStageList.begin()),
                getOwnedOrNone(distributedPlanLogic->mergeSortPattern),
                distributedPlanLogic->canMovePast);
        }

        addSplitStages(*distributedPlanLogic, mergePipe, shardPipe.get());
        return {std::move(shardPipe), getOwnedOrNone(distributedPlanLogic->mergeSortPattern)};
    }

    return {std::move(shardPipe), boost::none};
}

/**
 * If the final stage on shards is to unwind an array, move that stage to the merger. This cuts down
 * on network traffic and allows us to take advantage of reduced copying in unwind.
 */
void moveFinalUnwindFromShardsToMerger(Pipeline* shardPipe, Pipeline* mergePipe) {
    while (!shardPipe->getSources().empty() &&
           dynamic_cast<DocumentSourceUnwind*>(shardPipe->getSources().back().get())) {
        mergePipe->addInitialSource(shardPipe->popBack());
    }
}

/**
 * When the last stage of shard pipeline is $sort, move stages that can run on shards and don't
 * rename or modify the fields in $sort from merge pipeline. The function starts from the beginning
 * of the merge pipeline and finds the first consecutive eligible stages.
 */
void moveEligibleStreamingStagesBeforeSortOnShards(Pipeline* shardPipe,
                                                   Pipeline* mergePipe,
                                                   const BSONObj& sortPattern) {
    tassert(5363800,
            "Expected non-empty shardPipe consisting of at least a $sort stage",
            !shardPipe->getSources().empty());
    if (!dynamic_cast<DocumentSourceSort*>(shardPipe->getSources().back().get())) {
        // Expected last stage on the shards to be a $sort.
        return;
    }
    auto sortPaths = sortPattern.getFieldNames<OrderedPathSet>();
    auto firstMergeStage = mergePipe->getSources().cbegin();
    std::function<bool(DocumentSource*)> distributedPlanLogicCallback = [](DocumentSource* stage) {
        return !static_cast<bool>(stage->distributedPlanLogic());
    };
    auto [lastUnmodified, renameMap] = semantic_analysis::findLongestViablePrefixPreservingPaths(
        firstMergeStage, mergePipe->getSources().cend(), sortPaths, distributedPlanLogicCallback);
    for (const auto& sortPath : sortPaths) {
        auto pair = renameMap.find(sortPath);
        if (pair == renameMap.end() || pair->first != pair->second) {
            return;
        }
    }
    shardPipe->getSources().insert(shardPipe->getSources().end(), firstMergeStage, lastUnmodified);
    mergePipe->getSources().erase(firstMergeStage, lastUnmodified);
}

/**
 * Returns true if the final stage of the pipeline limits the number of documents it could output
 * (such as a $limit stage).
 *
 * This function is not meant to exhaustively catch every single case where a pipeline might have
 * some kind of limit. It's only here so that propagateDocLimitsToShards() can avoid adding an
 * obviously unnecessary $limit to a shard's pipeline.
 */
boost::optional<long long> getPipelineLimit(Pipeline* pipeline) {
    for (auto source_it = pipeline->getSources().rbegin();
         source_it != pipeline->getSources().rend();
         ++source_it) {
        const auto source = source_it->get();

        auto limitStage = dynamic_cast<DocumentSourceLimit*>(source);
        if (limitStage) {
            return limitStage->getLimit();
        }

        auto sortStage = dynamic_cast<DocumentSourceSort*>(source);
        if (sortStage) {
            return sortStage->getLimit();
        }

        auto cursorStage = dynamic_cast<DocumentSourceSort*>(source);
        if (cursorStage) {
            return cursorStage->getLimit();
        }

        // If this stage is one that can swap with a $limit stage, then we can look at the previous
        // stage to see if it includes a limit. Otherwise, we give up trying to find a limit on this
        // stage's output.
        if (!source->constraints().canSwapWithSkippingOrLimitingStage) {
            break;
        }
    }

    return boost::none;
}

/**
 * If the merging pipeline includes a $limit stage that creates an upper bound on how many input
 * documents it needs to compute the aggregation, we can use that as an upper bound on how many
 * documents each of the shards needs to produce. Propagating that upper bound to the shards (using
 * a $limit in the shard pipeline) can reduce the number of documents the shards need to process and
 * transfer over the network (see SERVER-36881).
 *
 * If there are $skip stages before the $limit, the skipped documents also contribute to the upper
 * bound.
 */
void propagateDocLimitToShards(Pipeline* shardPipe, Pipeline* mergePipe) {
    long long numDocumentsNeeded = 0;

    for (auto&& source : mergePipe->getSources()) {
        auto skipStage = dynamic_cast<DocumentSourceSkip*>(source.get());
        if (skipStage) {
            numDocumentsNeeded += skipStage->getSkip();
            continue;
        }

        auto limitStage = dynamic_cast<DocumentSourceLimit*>(source.get());
        if (limitStage) {
            numDocumentsNeeded += limitStage->getLimit();

            auto existingShardLimit = getPipelineLimit(shardPipe);
            if (existingShardLimit && *existingShardLimit <= numDocumentsNeeded) {
                // The sharding pipeline already has a limit that is no greater than the limit we
                // were going to add, so no changes are necessary.
                return;
            }

            auto shardLimit =
                DocumentSourceLimit::create(mergePipe->getContext(), numDocumentsNeeded);
            shardPipe->addFinalSource(shardLimit);

            // We have successfully applied a limit to the number of documents we need from each
            // shard.
            return;
        }

        // If there are any stages in the merge pipeline before the $skip and $limit stages, then we
        // cannot use the $limit to determine an upper bound, unless those stages could be swapped
        // with the $limit.
        if (!source->constraints().canSwapWithSkippingOrLimitingStage) {
            return;
        }
    }

    // We did not find any limit in the merge pipeline that would allow us to set an upper bound on
    // the number of documents we need from each shard.
    return;
}

/**
 * Adds a stage to the end of 'shardPipe' explicitly requesting all fields that 'mergePipe' needs.
 * This is only done if it heuristically determines that it is needed. This optimization can reduce
 * the amount of network traffic and can also enable the shards to convert less source BSON into
 * Documents.
 */
void limitFieldsSentFromShardsToMerger(Pipeline* shardPipe, Pipeline* mergePipe) {
    DepsTracker mergeDeps(mergePipe->getDependencies(DepsTracker::kNoMetadata));
    if (mergeDeps.needWholeDocument)
        return;  // the merge needs all fields, so nothing we can do.

    // Empty project is "special" so if no fields are needed, we just ask for _id instead.
    if (mergeDeps.fields.empty())
        mergeDeps.fields.insert("_id");

    // HEURISTIC: only apply optimization if none of the shard stages have an exhaustive list of
    // field dependencies. While this may not be 100% ideal in all cases, it is simple and
    // avoids the worst cases by ensuring that:
    // 1) Optimization IS applied when the shards wouldn't have known their exhaustive list of
    //    dependencies. This situation can happen when a $sort is before the first $project or
    //    $group. Without the optimization, the shards would have to reify and transmit full
    //    objects even though only a subset of fields are needed.
    // 2) Optimization IS NOT applied immediately following a $project or $group since it would
    //    add an unnecessary project (and therefore a deep-copy).
    for (auto&& source : shardPipe->getSources()) {
        DepsTracker dt(DepsTracker::kNoMetadata);
        if (source->getDependencies(&dt) & DepsTracker::State::EXHAUSTIVE_FIELDS)
            return;
    }
    // if we get here, add the project.
    boost::intrusive_ptr<DocumentSource> project = DocumentSourceProject::createFromBson(
        BSON("$project" << mergeDeps.toProjectionWithoutMetadata()).firstElement(),
        shardPipe->getContext());
    shardPipe->pushBack(project);
}

bool stageCanRunInParallel(const boost::intrusive_ptr<DocumentSource>& stage,
                           const OrderedPathSet& nameOfShardKeyFieldsUponEntryToStage) {
    if (stage->distributedPlanLogic()) {
        return stage->canRunInParallelBeforeWriteStage(nameOfShardKeyFieldsUponEntryToStage);
    } else {
        // This stage is fine to execute in parallel on each stream. For example, a $match can be
        // applied to each stream in parallel.
        return true;
    }
}

std::string mapToString(const StringMap<std::string>& map) {
    StringBuilder sb;
    sb << "{";
    for (auto&& entry : map) {
        if (sb.len() != 1) {
            sb << ", ";
        }
        sb << entry.first << ": " << entry.second;
    }
    sb << "}";
    return sb.str();
}

BSONObj buildNewKeyPattern(const ShardKeyPattern& shardKey, StringMap<std::string> renames) {
    BSONObjBuilder newPattern;
    for (auto&& elem : shardKey.getKeyPattern().toBSON()) {
        auto it = renames.find(elem.fieldNameStringData());
        invariant(it != renames.end(),
                  str::stream() << "Could not find new name of shard key field \""
                                << elem.fieldName() << "\": rename map was "
                                << mapToString(renames));
        newPattern.appendAs(elem, it->second);
    }
    return newPattern.obj();
}

StringMap<std::string> computeShardKeyRenameMap(const Pipeline* mergePipeline,
                                                OrderedPathSet&& pathsOfShardKey) {
    auto traversalStart = mergePipeline->getSources().crbegin();
    auto traversalEnd = mergePipeline->getSources().crend();
    const auto leadingGroup =
        dynamic_cast<DocumentSourceGroup*>(mergePipeline->getSources().front().get());
    if (leadingGroup && leadingGroup->doingMerge()) {
        // A leading $group stage will not report to preserve any fields, since it blows away the
        // _id and replaces it with something new. It possibly renames some fields, but when
        // computing the new shard key we are interested in the name of the shard key *in the middle
        // of the $group*. The $exchange will be inserted between the shard-local groups and the
        // global groups. Thus we want to exclude this stage from our rename tracking.
        traversalEnd = std::prev(traversalEnd);
    }
    auto renameMap = semantic_analysis::renamedPaths(traversalStart, traversalEnd, pathsOfShardKey);
    invariant(renameMap,
              str::stream()
                  << "Analyzed pipeline was thought to preserve the shard key fields, but did not: "
                  << Value(mergePipeline->serialize()).toString());
    return *renameMap;
}

/**
 * Returns true if any stage in the pipeline would modify any of the fields in 'shardKeyPaths', or
 * if there is any stage in the pipeline requires a unified stream to do its computation like a
 * $limit would.
 *
 * Purposefully takes 'shardKeyPaths' by value so that it can be modified throughout.
 */
bool anyStageModifiesShardKeyOrNeedsMerge(OrderedPathSet shardKeyPaths,
                                          const Pipeline* mergePipeline) {
    const auto& stages = mergePipeline->getSources();
    for (auto it = stages.crbegin(); it != stages.crend(); ++it) {
        const auto& stage = *it;
        auto renames = semantic_analysis::renamedPaths(
            std::move(shardKeyPaths), *stage, semantic_analysis::Direction::kBackward);
        if (!renames) {
            return true;
        }
        shardKeyPaths.clear();
        for (auto&& rename : *renames) {
            shardKeyPaths.insert(rename.second);
        }
        if (!stageCanRunInParallel(stage, shardKeyPaths)) {
            // In order for this stage to work it needs a single input stream which it wouldn't get
            // if we inserted an exchange before it.
            return true;
        }
    }
    return false;
}

boost::optional<ShardedExchangePolicy> walkPipelineBackwardsTrackingShardKey(
    OperationContext* opCtx, const Pipeline* mergePipeline, const ChunkManager& chunkManager) {

    const ShardKeyPattern& shardKey = chunkManager.getShardKeyPattern();
    OrderedPathSet shardKeyPaths;
    for (auto&& path : shardKey.getKeyPatternFields()) {
        shardKeyPaths.emplace(path->dottedField().toString());
    }
    if (anyStageModifiesShardKeyOrNeedsMerge(shardKeyPaths, mergePipeline)) {
        return boost::none;
    }

    // All the fields of the shard key are preserved by the pipeline, but they might be renamed. To
    // set up the $exchange, we need to build a fake shard key pattern which uses the names of the
    // shard key fields as they are at the split point of the pipeline.
    auto renames = computeShardKeyRenameMap(mergePipeline, std::move(shardKeyPaths));
    ShardKeyPattern newShardKey(buildNewKeyPattern(shardKey, renames));

    // Append the boundaries with the new names from the new shard key.
    auto translateBoundary = [&renames](const BSONObj& oldBoundary) {
        BSONObjBuilder bob;
        for (auto&& elem : oldBoundary) {
            bob.appendAs(elem, renames[elem.fieldNameStringData()]);
        }
        return bob.obj();
    };

    // Given the new shard key fields, build the distribution map.
    ExchangeSpec exchangeSpec;
    std::vector<BSONObj> boundaries;
    std::vector<int> consumerIds;
    std::map<ShardId, int> shardToConsumer;
    std::vector<ShardId> consumerShards;
    int numConsumers = 0;

    // The chunk manager enumerates the chunks in the ascending order from MinKey to MaxKey. Every
    // chunk has an associated range [from, to); i.e. inclusive lower bound and exclusive upper
    // bound. The chunk ranges must cover all domain without any holes. For the exchange we coalesce
    // ranges into a single vector of points. E.g. chunks [min,5], [5,10], [10,max] will produce
    // [min,5,10,max] vector. Number of points in the vector is always one greater than number of
    // chunks.
    // We also compute consumer indices for every chunk. From the example above (3 chunks) we may
    // get the vector [0,1,2]; i.e. the first chunk goes to the consumer 0 and so on. Note that
    // the consumer id may be repeated if the consumer hosts more than 1 chunk.
    chunkManager.forEachChunk([&](const auto& chunk) {
        if (boundaries.empty())
            boundaries.emplace_back(translateBoundary(chunk.getMin()));

        boundaries.emplace_back(translateBoundary(chunk.getMax()));
        if (shardToConsumer.find(chunk.getShardId()) == shardToConsumer.end()) {
            shardToConsumer.emplace(chunk.getShardId(), numConsumers++);
            consumerShards.emplace_back(chunk.getShardId());
        }
        consumerIds.emplace_back(shardToConsumer[chunk.getShardId()]);

        return true;
    });

    exchangeSpec.setPolicy(ExchangePolicyEnum::kKeyRange);
    exchangeSpec.setKey(newShardKey.toBSON());
    exchangeSpec.setBoundaries(std::move(boundaries));
    exchangeSpec.setConsumers(shardToConsumer.size());
    exchangeSpec.setConsumerIds(std::move(consumerIds));

    return ShardedExchangePolicy{std::move(exchangeSpec), std::move(consumerShards)};
}

/**
 * Non-correlated pipeline caching is only supported locally. When the
 * DocumentSourceSequentialDocumentCache stage has been moved to the shards pipeline, abandon the
 * associated local cache.
 */
void abandonCacheIfSentToShards(Pipeline* shardsPipeline) {
    for (auto&& stage : shardsPipeline->getSources()) {
        if (StringData(stage->getSourceName()) ==
            DocumentSourceSequentialDocumentCache::kStageName) {
            static_cast<DocumentSourceSequentialDocumentCache*>(stage.get())->abandonCache();
        }
    }
}

}  // namespace

std::unique_ptr<Pipeline, PipelineDeleter> targetShardsAndAddMergeCursors(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    stdx::variant<std::unique_ptr<Pipeline, PipelineDeleter>,
                  AggregateCommandRequest,
                  std::pair<AggregateCommandRequest, std::unique_ptr<Pipeline, PipelineDeleter>>>
        targetRequest,
    boost::optional<BSONObj> shardCursorsSortSpec,
    ShardTargetingPolicy shardTargetingPolicy,
    boost::optional<BSONObj> readConcern) {
    auto&& [aggRequest, pipeline] = [&] {
        return stdx::visit(
            OverloadedVisitor{
                [&](std::unique_ptr<Pipeline, PipelineDeleter>&& pipeline) {
                    return std::make_pair(
                        AggregateCommandRequest(expCtx->ns, pipeline->serializeToBson()),
                        std::move(pipeline));
                },
                [&](AggregateCommandRequest&& aggRequest) {
                    auto rawPipeline = aggRequest.getPipeline();
                    return std::make_pair(std::move(aggRequest),
                                          Pipeline::parse(std::move(rawPipeline), expCtx));
                },
                [&](std::pair<AggregateCommandRequest, std::unique_ptr<Pipeline, PipelineDeleter>>&&
                        aggRequestPipelinePair) {
                    return std::move(aggRequestPipelinePair);
                }},
            std::move(targetRequest));
    }();

    invariant(pipeline->getSources().empty() ||
              !dynamic_cast<DocumentSourceMergeCursors*>(pipeline->getSources().front().get()));

    // The default value for 'allowDiskUse' and 'maxTimeMS' in the AggregateCommandRequest may not
    // match what was set on the originating command, so copy it from the ExpressionContext.
    aggRequest.setAllowDiskUse(expCtx->allowDiskUse);

    if (auto maxTimeMS = expCtx->opCtx->getRemainingMaxTimeMillis();
        maxTimeMS < Microseconds::max()) {
        aggRequest.setMaxTimeMS(durationCount<Milliseconds>(maxTimeMS));
    }

    LiteParsedPipeline liteParsedPipeline(aggRequest);
    auto hasChangeStream = liteParsedPipeline.hasChangeStream();
    auto startsWithDocuments = liteParsedPipeline.startsWithDocuments();
    auto shardDispatchResults =
        dispatchShardPipeline(aggregation_request_helper::serializeToCommandDoc(aggRequest),
                              hasChangeStream,
                              startsWithDocuments,
                              expCtx->eligibleForSampling(),
                              std::move(pipeline),
                              // Even if the overall operation is an explain, callers of this
                              // function always intend to actually execute a regular agg command
                              // and merge the results with $mergeCursors.
                              boost::none /*explain*/,
                              shardTargetingPolicy,
                              std::move(readConcern));

    std::vector<ShardId> targetedShards;
    targetedShards.reserve(shardDispatchResults.remoteCursors.size());
    for (auto&& remoteCursor : shardDispatchResults.remoteCursors) {
        targetedShards.emplace_back(remoteCursor->getShardId().toString());
    }

    std::unique_ptr<Pipeline, PipelineDeleter> mergePipeline;
    if (shardDispatchResults.splitPipeline) {
        mergePipeline = std::move(shardDispatchResults.splitPipeline->mergePipeline);
        if (shardDispatchResults.splitPipeline->shardCursorsSortSpec) {
            uassert(4929304, "Split pipeline provides its own sort already", !shardCursorsSortSpec);
            shardCursorsSortSpec = shardDispatchResults.splitPipeline->shardCursorsSortSpec;
        }
    } else {
        // We have not split the pipeline, and will execute entirely on the remote shards. Set up an
        // empty local pipeline which we will attach the merge cursors stage to.
        mergePipeline = Pipeline::parse(std::vector<BSONObj>(), expCtx);
    }

    partitionAndAddMergeCursorsSource(
        mergePipeline.get(), std::move(shardDispatchResults.remoteCursors), shardCursorsSortSpec);
    return mergePipeline;
}

std::unique_ptr<Pipeline, PipelineDeleter> runPipelineDirectlyOnSingleShard(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    AggregateCommandRequest request,
    ShardId shardId) {
    invariant(!request.getExplain());

    auto readPreference = uassertStatusOK(ReadPreferenceSetting::fromContainingBSON(
        request.getUnwrappedReadPref().value_or(BSONObj())));

    auto* opCtx = expCtx->opCtx;
    auto* catalogCache = Grid::get(opCtx)->catalogCache();
    auto cri =
        uassertStatusOK(catalogCache->getCollectionRoutingInfo(opCtx, request.getNamespace()));

    auto versionedCmdObj = [&] {
        if (cri.cm.isSharded()) {
            return appendShardVersion(aggregation_request_helper::serializeToCommandObj(request),
                                      cri.getShardVersion(shardId));
        } else {
            // The collection is unsharded. Don't append shard version info when contacting a fixed
            // db collection.
            const auto cmdObjWithShardVersion = !cri.cm.dbVersion().isFixed()
                ? appendShardVersion(aggregation_request_helper::serializeToCommandObj(request),
                                     ShardVersion::UNSHARDED())
                : aggregation_request_helper::serializeToCommandObj(request);
            return appendDbVersionIfPresent(std::move(cmdObjWithShardVersion), cri.cm.dbVersion());
        }
    }();

    auto cursors = establishCursors(opCtx,
                                    expCtx->mongoProcessInterface->taskExecutor,
                                    request.getNamespace(),
                                    std::move(readPreference),
                                    {{shardId, versionedCmdObj}},
                                    false /* allowPartialResults */,
                                    Shard::RetryPolicy::kIdempotent);
    invariant(cursors.size() == 1);

    // Convert remote cursors into a vector of "owned" cursors.
    std::vector<OwnedRemoteCursor> ownedCursors;
    for (auto&& cursor : cursors) {
        auto cursorNss = cursor.getCursorResponse().getNSS();
        ownedCursors.emplace_back(opCtx, std::move(cursor), std::move(cursorNss));
    }

    // We have not split the pipeline, and will execute entirely on the remote shard. Set up an
    // empty local pipeline which we will attach the merge cursors stage to.
    auto mergePipeline = Pipeline::parse(std::vector<BSONObj>{}, expCtx);

    partitionAndAddMergeCursorsSource(mergePipeline.get(), std::move(ownedCursors), boost::none);
    return mergePipeline;
}

boost::optional<ShardedExchangePolicy> checkIfEligibleForExchange(OperationContext* opCtx,
                                                                  const Pipeline* mergePipeline) {
    if (internalQueryDisableExchange.load()) {
        return boost::none;
    }

    if (mergePipeline->getSources().empty()) {
        return boost::none;
    }

    auto mergeStage = dynamic_cast<DocumentSourceMerge*>(mergePipeline->getSources().back().get());
    if (!mergeStage) {
        // If there's no $merge stage we won't try to do an $exchange. For the $out stage there's no
        // point doing an $exchange because all the writes will go to a single node, so we should
        // just perform the merge on that host.
        return boost::none;
    }

    const auto [cm, _] =
        uassertStatusOK(getCollectionRoutingInfoForTxnCmd(opCtx, mergeStage->getOutputNs()));
    if (!cm.isSharded()) {
        return boost::none;
    }

    // The collection is sharded and we have a $merge stage! Here we assume the $merge stage has
    // already verified that the shard key pattern is compatible with the unique key being used.
    // Assuming this, we just have to make sure the shard key is preserved (though possibly renamed)
    // all the way to the front of the merge pipeline. If this is the case then for any document
    // entering the merging pipeline we can predict which shard it will need to end up being
    // inserted on. With this ability we can insert an exchange on the shards to partition the
    // documents based on which shard will end up owning them. Then each shard can perform a merge
    // of only those documents which belong to it (optimistically, barring chunk migrations).
    return walkPipelineBackwardsTrackingShardKey(opCtx, mergePipeline, cm);
}

SplitPipeline splitPipeline(std::unique_ptr<Pipeline, PipelineDeleter> pipeline) {
    // Re-brand 'pipeline' as the merging pipeline. We will move stages one by one from the merging
    // half to the shards, as possible.
    auto mergePipeline = std::move(pipeline);

    auto [shardsPipeline, inputsSort] = findSplitPoint(mergePipeline.get());

    // The order in which optimizations are applied can have significant impact on the efficiency of
    // the final pipeline. Be Careful!
    if (inputsSort) {
        moveEligibleStreamingStagesBeforeSortOnShards(
            shardsPipeline.get(), mergePipeline.get(), *inputsSort);
    }
    moveFinalUnwindFromShardsToMerger(shardsPipeline.get(), mergePipeline.get());
    propagateDocLimitToShards(shardsPipeline.get(), mergePipeline.get());
    limitFieldsSentFromShardsToMerger(shardsPipeline.get(), mergePipeline.get());

    abandonCacheIfSentToShards(shardsPipeline.get());
    shardsPipeline->setSplitState(Pipeline::SplitState::kSplitForShards);
    mergePipeline->setSplitState(Pipeline::SplitState::kSplitForMerge);

    return {std::move(shardsPipeline), std::move(mergePipeline), std::move(inputsSort)};
}

BSONObj createPassthroughCommandForShard(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    Document serializedCommand,
    boost::optional<ExplainOptions::Verbosity> explainVerbosity,
    Pipeline* pipeline,
    BSONObj collationObj,
    boost::optional<BSONObj> readConcern,
    boost::optional<int> overrideBatchSize) {
    // Create the command for the shards.
    MutableDocument targetedCmd(serializedCommand);
    if (pipeline) {
        targetedCmd[AggregateCommandRequest::kPipelineFieldName] = Value(pipeline->serialize());
    }

    if (overrideBatchSize.has_value()) {
        if (serializedCommand[AggregateCommandRequest::kCursorFieldName].missing()) {
            targetedCmd[AggregateCommandRequest::kCursorFieldName] =
                Value(DOC(SimpleCursorOptions::kBatchSizeFieldName << Value(*overrideBatchSize)));
        } else {
            targetedCmd[AggregateCommandRequest::kCursorFieldName]
                       [SimpleCursorOptions::kBatchSizeFieldName] = Value(*overrideBatchSize);
        }
    }

    auto shardCommand = genericTransformForShards(std::move(targetedCmd),
                                                  expCtx,
                                                  explainVerbosity,
                                                  std::move(collationObj),
                                                  std::move(readConcern));

    // Apply filter and RW concern to the final shard command.
    return CommandHelpers::filterCommandRequestForPassthrough(
        applyReadWriteConcern(expCtx->opCtx,
                              true,              /* appendRC */
                              !explainVerbosity, /* appendWC */
                              shardCommand));
}

BSONObj createCommandForTargetedShards(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                       Document serializedCommand,
                                       const SplitPipeline& splitPipeline,
                                       const boost::optional<ShardedExchangePolicy> exchangeSpec,
                                       bool needsMerge,
                                       boost::optional<ExplainOptions::Verbosity> explain,
                                       boost::optional<BSONObj> readConcern) {
    // Create the command for the shards.
    MutableDocument targetedCmd(serializedCommand);
    // If we've parsed a pipeline on mongos, always override the pipeline, in case parsing it
    // has defaulted any arguments or otherwise changed the spec. For example, $listSessions may
    // have detected a logged in user and appended that user name to the $listSessions spec to
    // send to the shards.
    targetedCmd[AggregateCommandRequest::kPipelineFieldName] =
        Value(splitPipeline.shardsPipeline->serialize());

    // When running on many shards with the exchange we may not need merging.
    if (needsMerge) {
        targetedCmd[AggregateCommandRequest::kNeedsMergeFieldName] = Value(true);

        // If there aren't any stages like $out in the pipeline being sent to the shards, remove the
        // write concern. The write concern should only be applied when there are writes performed
        // to avoid mistakenly waiting for writes which didn't happen.
        const auto& shardsPipe = splitPipeline.shardsPipeline->getSources();
        if (!std::any_of(shardsPipe.begin(), shardsPipe.end(), [](const auto& stage) {
                return stage->constraints().writesPersistentData();
            })) {
            targetedCmd[WriteConcernOptions::kWriteConcernField] = Value();
        }
    }

    targetedCmd[AggregateCommandRequest::kCursorFieldName] =
        Value(DOC(aggregation_request_helper::kBatchSizeField << 0));

    targetedCmd[AggregateCommandRequest::kExchangeFieldName] =
        exchangeSpec ? Value(exchangeSpec->exchangeSpec.toBSON()) : Value();

    auto shardCommand = genericTransformForShards(
        std::move(targetedCmd), expCtx, explain, expCtx->getCollatorBSON(), std::move(readConcern));

    // Apply RW concern to the final shard command.
    return applyReadWriteConcern(expCtx->opCtx,
                                 true,     /* appendRC */
                                 !explain, /* appendWC */
                                 shardCommand);
}

DispatchShardPipelineResults dispatchShardPipeline(
    Document serializedCommand,
    bool hasChangeStream,
    bool startsWithDocuments,
    bool eligibleForSampling,
    std::unique_ptr<Pipeline, PipelineDeleter> pipeline,
    boost::optional<ExplainOptions::Verbosity> explain,
    ShardTargetingPolicy shardTargetingPolicy,
    boost::optional<BSONObj> readConcern,
    AsyncRequestsSender::ShardHostMap designatedHostsMap,
    stdx::unordered_map<ShardId, BSONObj> resumeTokenMap) {
    auto expCtx = pipeline->getContext();

    // The process is as follows:
    // - First, determine whether we need to target more than one shard. If so, we split the
    // pipeline; if not, we retain the existing pipeline.
    // - Call establishShardCursors to dispatch the aggregation to the targeted shards.
    // - Stale shard version errors are thrown up to the top-level handler, causing a retry on the
    // entire aggregation command.
    auto cursors = std::vector<RemoteCursor>();
    auto shardResults = std::vector<AsyncRequestsSender::Response>();
    auto opCtx = expCtx->opCtx;

    const bool needsPrimaryShardMerge =
        (pipeline->needsPrimaryShardMerger() || internalQueryAlwaysMergeOnPrimaryShard.load());

    const bool needsMongosMerge = pipeline->needsMongosMerger();

    const auto shardQuery = pipeline->getInitialQuery();

    auto executionNsRoutingInfoStatus = getExecutionNsRoutingInfo(opCtx, expCtx->ns);

    // If this is a $changeStream, we swallow NamespaceNotFound exceptions and continue.
    // Otherwise, uassert on all exceptions here.
    if (!(hasChangeStream && executionNsRoutingInfoStatus == ErrorCodes::NamespaceNotFound)) {
        uassertStatusOK(executionNsRoutingInfoStatus);
    }

    auto executionNsRoutingInfo = executionNsRoutingInfoStatus.isOK()
        ? std::move(executionNsRoutingInfoStatus.getValue())
        : boost::optional<CollectionRoutingInfo>{};

    // A $changeStream update lookup attempts to retrieve a single document by documentKey. In this
    // case, we wish to target a single shard using the simple collation, but we also want to ensure
    // that we use the collection-default collation on the shard so that the lookup can use the _id
    // index. We therefore ignore the collation on the expCtx.
    const auto& shardTargetingCollation =
        shardTargetingPolicy == ShardTargetingPolicy::kForceTargetingWithSimpleCollation
        ? CollationSpec::kSimpleSpec
        : expCtx->getCollatorBSON();

    // Determine whether we can run the entire aggregation on a single shard.
    const bool mustRunOnAllShards =
        checkIfMustRunOnAllShards(expCtx->ns, hasChangeStream, startsWithDocuments);
    std::set<ShardId> shardIds = getTargetedShards(
        expCtx, mustRunOnAllShards, executionNsRoutingInfo, shardQuery, shardTargetingCollation);

    bool targetEveryShardServer = pipeline->needsAllShardServers();
    // Don't need to split the pipeline if we are only targeting a single shard, unless:
    // - There is a stage that needs to be run on the primary shard and the single target shard
    //   is not the primary.
    // - The pipeline contains one or more stages which must always merge on mongoS.
    const bool needsSplit = (shardIds.size() > 1u || needsMongosMerge || targetEveryShardServer ||
                             (needsPrimaryShardMerge && executionNsRoutingInfo &&
                              *(shardIds.begin()) != executionNsRoutingInfo->cm.dbPrimary()));

    boost::optional<ShardedExchangePolicy> exchangeSpec;
    boost::optional<SplitPipeline> splitPipelines;


    if (needsSplit) {
        LOGV2_DEBUG(20906,
                    5,
                    "Splitting pipeline: targeting = {shardIds_size} shards, needsMongosMerge = "
                    "{needsMongosMerge}, needsPrimaryShardMerge = {needsPrimaryShardMerge}",
                    "shardIds_size"_attr = shardIds.size(),
                    "needsMongosMerge"_attr = needsMongosMerge,
                    "needsPrimaryShardMerge"_attr = needsPrimaryShardMerge);
        splitPipelines = splitPipeline(std::move(pipeline));

        // If the first stage of the pipeline is a $search stage, exchange optimization isn't
        // possible.
        // TODO SERVER-65349 Investigate relaxing this restriction.
        if (!splitPipelines || !splitPipelines->shardsPipeline ||
            !splitPipelines->shardsPipeline->peekFront() ||
            !getSearchHelpers(expCtx->opCtx->getServiceContext())
                 ->isSearchPipeline(splitPipelines->shardsPipeline.get())) {
            exchangeSpec = checkIfEligibleForExchange(opCtx, splitPipelines->mergePipeline.get());
        }
    }

    // Generate the command object for the targeted shards.
    BSONObj targetedCommand =
        (splitPipelines ? createCommandForTargetedShards(expCtx,
                                                         serializedCommand,
                                                         *splitPipelines,
                                                         exchangeSpec,
                                                         true /* needsMerge */,
                                                         explain,
                                                         std::move(readConcern))
                        : createPassthroughCommandForShard(expCtx,
                                                           serializedCommand,
                                                           explain,
                                                           pipeline.get(),
                                                           expCtx->getCollatorBSON(),
                                                           std::move(readConcern),
                                                           boost::none));
    const auto targetedSampleId = eligibleForSampling
        ? analyze_shard_key::tryGenerateTargetedSampleId(
              opCtx, expCtx->ns, analyze_shard_key::SampledCommandNameEnum::kAggregate, shardIds)
        : boost::none;

    // A $changeStream pipeline must run on all shards, and will also open an extra cursor on the
    // config server in order to monitor for new shards. To guarantee that we do not miss any
    // shards, we must ensure that the list of shards to which we initially dispatch the pipeline is
    // at least as current as the logical time at which the stream begins scanning for new shards.
    // We therefore set 'shardRegistryReloadTime' to the current clusterTime and then hard-reload
    // the shard registry. We don't refresh for other pipelines that must run on all shards (e.g.
    // $currentOp) because, unlike $changeStream, those pipelines may not have been forced to split
    // if there was only one shard in the cluster when the command began execution. If a shard was
    // added since the earlier targeting logic ran, then refreshing here may cause us to illegally
    // target an unsplit pipeline to more than one shard.
    const auto currentTime = VectorClock::get(opCtx)->getTime();
    auto shardRegistryReloadTime = currentTime.clusterTime().asTimestamp();
    if (hasChangeStream) {
        Grid::get(opCtx)->shardRegistry()->reload(opCtx);
        // Rebuild the set of shards as the shard registry might have changed.
        shardIds = getTargetedShards(expCtx,
                                     mustRunOnAllShards,
                                     executionNsRoutingInfo,
                                     shardQuery,
                                     shardTargetingCollation);
    }

    // If there were no shards when we began execution, we wouldn't have run this aggregation in the
    // first place. Here, we double-check that the shards have not been removed mid-operation.
    uassert(ErrorCodes::ShardNotFound,
            "Unexpectedly found 0 shards while preparing to dispatch aggregation requests. Were "
            "the shards removed mid-operation?",
            shardIds.size() > 0);

    // Explain does not produce a cursor, so instead we scatter-gather commands to the shards.
    if (explain) {
        if (mustRunOnAllShards) {
            // Some stages (such as $currentOp) need to be broadcast to all shards, and
            // should not participate in the shard version protocol.
            shardResults =
                scatterGatherUnversionedTargetAllShards(opCtx,
                                                        expCtx->ns.db_deprecated(),
                                                        targetedCommand,
                                                        ReadPreferenceSetting::get(opCtx),
                                                        Shard::RetryPolicy::kIdempotent);
        } else {
            // Aggregations on a real namespace should use the routing table to target
            // shards, and should participate in the shard version protocol.
            invariant(executionNsRoutingInfo);
            shardResults =
                scatterGatherVersionedTargetByRoutingTable(expCtx,
                                                           expCtx->ns.db_deprecated(),
                                                           expCtx->ns,
                                                           *executionNsRoutingInfo,
                                                           targetedCommand,
                                                           ReadPreferenceSetting::get(opCtx),
                                                           Shard::RetryPolicy::kIdempotent,
                                                           shardQuery,
                                                           shardTargetingCollation);
        }
    } else {
        try {
            cursors = establishShardCursors(opCtx,
                                            expCtx->mongoProcessInterface->taskExecutor,
                                            expCtx->ns,
                                            mustRunOnAllShards,
                                            executionNsRoutingInfo,
                                            shardIds,
                                            targetedCommand,
                                            targetedSampleId,
                                            ReadPreferenceSetting::get(opCtx),
                                            designatedHostsMap,
                                            resumeTokenMap,
                                            targetEveryShardServer);

        } catch (const ExceptionFor<ErrorCodes::StaleConfig>& e) {
            // Check to see if the command failed because of a stale shard version or something
            // else.
            auto staleInfo = e.extraInfo<StaleConfigInfo>();
            tassert(6441003, "StaleConfigInfo was null during sharded aggregation", staleInfo);
            throw;
        } catch (const ExceptionFor<ErrorCodes::CollectionUUIDMismatch>& ex) {
            uassertStatusOK(populateCollectionUUIDMismatch(opCtx, ex.toStatus()));
            MONGO_UNREACHABLE_TASSERT(6487201);
        }

        tassert(7937200,
                str::stream() << "Number of cursors (" << cursors.size()
                              << ") is not a multiple of the number of targeted shards ("
                              << shardIds.size()
                              << ") and we were not targeting each mongod in each shard",
                targetEveryShardServer || cursors.size() % shardIds.size() == 0);

        // For $changeStream, we must open an extra cursor on the 'config.shards' collection, so
        // that we can monitor for the addition of new shards inline with real events.
        if (hasChangeStream && !expCtx->ns.isEqualDb(NamespaceString::kConfigsvrShardsNamespace)) {
            cursors.emplace_back(openChangeStreamNewShardMonitor(expCtx, shardRegistryReloadTime));
        }
    }

    // Convert remote cursors into a vector of "owned" cursors.
    std::vector<OwnedRemoteCursor> ownedCursors;
    for (auto&& cursor : cursors) {
        auto cursorNss = cursor.getCursorResponse().getNSS();
        ownedCursors.emplace_back(opCtx, std::move(cursor), std::move(cursorNss));
    }

    // Record the number of shards involved in the aggregation. If we are required to merge on
    // the primary shard, but the primary shard was not in the set of targeted shards, then we
    // must increment the number of involved shards.
    CurOp::get(opCtx)->debug().nShards = shardIds.size() +
        (needsPrimaryShardMerge && executionNsRoutingInfo &&
         !shardIds.count(executionNsRoutingInfo->cm.dbPrimary()));

    return DispatchShardPipelineResults{needsPrimaryShardMerge,
                                        std::move(ownedCursors),
                                        std::move(shardResults),
                                        std::move(splitPipelines),
                                        std::move(pipeline),
                                        targetedCommand,
                                        shardIds.size(),
                                        exchangeSpec};
}

/**
 * Build the AsyncResultsMergerParams from the cursor set and sort spec.
 */
AsyncResultsMergerParams buildArmParams(boost::intrusive_ptr<ExpressionContext> expCtx,
                                        std::vector<OwnedRemoteCursor> ownedCursors,
                                        boost::optional<BSONObj> shardCursorsSortSpec) {
    AsyncResultsMergerParams armParams;
    armParams.setSort(std::move(shardCursorsSortSpec));
    armParams.setTailableMode(expCtx->tailableMode);
    armParams.setNss(expCtx->ns);

    if (auto lsid = expCtx->opCtx->getLogicalSessionId()) {
        OperationSessionInfoFromClient sessionInfo(*lsid, expCtx->opCtx->getTxnNumber());

        if (TransactionRouter::get(expCtx->opCtx)) {
            sessionInfo.setAutocommit(false);
        }

        armParams.setOperationSessionInfo(sessionInfo);
    }

    // Convert owned cursors into a vector of remote cursors to be transferred to the merge
    // pipeline.
    std::vector<RemoteCursor> remoteCursors;
    for (auto&& cursor : ownedCursors) {
        // Transfer ownership of the remote cursor to the $mergeCursors stage.
        remoteCursors.emplace_back(cursor.releaseCursor());
    }
    armParams.setRemotes(std::move(remoteCursors));

    return armParams;
}

// Anonnymous namespace for helpers of partitionCursorsAndAddMergeCursors.
namespace {
/**
 * Given the owned cursors vector, partitions the cursors into either one or two vectors. If
 * untyped cursors are present, returned pair will be {results, boost::none}. If results or meta are
 * present, the returned pair will be {results, meta}.
 */
std::pair<std::vector<OwnedRemoteCursor>, boost::optional<std::vector<OwnedRemoteCursor>>>
partitionCursors(std::vector<OwnedRemoteCursor> ownedCursors) {

    // Partition cursor set based on type/label.
    std::vector<OwnedRemoteCursor> resultsCursors;
    std::vector<OwnedRemoteCursor> metaCursors;
    std::vector<OwnedRemoteCursor> untypedCursors;
    for (OwnedRemoteCursor& ownedCursor : ownedCursors) {
        auto cursor = *ownedCursor;
        auto maybeCursorType = cursor->getCursorResponse().getCursorType();
        if (!maybeCursorType) {
            untypedCursors.push_back(std::move(ownedCursor));
        } else {
            auto cursorType = CursorType_parse(IDLParserContext("ShardedAggHelperCursorType"),
                                               maybeCursorType.value());
            if (cursorType == CursorTypeEnum::DocumentResult) {
                resultsCursors.push_back(std::move(ownedCursor));
            } else if (cursorType == CursorTypeEnum::SearchMetaResult) {
                metaCursors.push_back(std::move(ownedCursor));
            } else {
                tasserted(625304, "Received unknown cursor type from mongot.");
            }
        }
    }

    // Verify we don't have illegal mix of types and untyped cursors from the shards.
    bool haveTypedCursors = !resultsCursors.empty() || !metaCursors.empty();
    if (haveTypedCursors) {
        tassert(625305,
                "Received unexpected mix of labelled and unlabelled cursors.",
                untypedCursors.empty());
    }

    if (haveTypedCursors) {
        return {std::move(resultsCursors), std::move(metaCursors)};
    }
    return {std::move(untypedCursors), boost::none};
}


/**
 * Adds a merge cursors stage to the pipeline for metadata cursors. Should not be called if
 * the query did not generate metadata cursors.
 */
void injectMetaCursor(Pipeline* mergePipeline, std::vector<OwnedRemoteCursor> metaCursors) {
    // Provide the "meta" cursors to the $setVariableFromSubPipeline stage.
    for (const auto& source : mergePipeline->getSources()) {
        if (auto* setVarStage =
                dynamic_cast<DocumentSourceSetVariableFromSubPipeline*>(source.get())) {

            // If $setVar is present, we must have a non-empty set of "meta" cursors.
            tassert(625307, "Missing meta cursor set.", !metaCursors.empty());

            auto armParams = sharded_agg_helpers::buildArmParams(
                mergePipeline->getContext(), std::move(metaCursors), {});

            setVarStage->addSubPipelineInitialSource(DocumentSourceMergeCursors::create(
                mergePipeline->getContext(), std::move(armParams)));
            break;
        }
    }
}

/**
 * Adds a mergeCursors stage to the front of the pipeline to handle merging cursors from each
 * shard.
 */
void addMergeCursorsSource(Pipeline* mergePipeline,
                           std::vector<OwnedRemoteCursor> cursorsToMerge,
                           boost::optional<BSONObj> shardCursorsSortSpec) {

    auto armParams = sharded_agg_helpers::buildArmParams(
        mergePipeline->getContext(), std::move(cursorsToMerge), std::move(shardCursorsSortSpec));

    mergePipeline->addInitialSource(
        DocumentSourceMergeCursors::create(mergePipeline->getContext(), std::move(armParams)));
}

}  // namespace
void partitionAndAddMergeCursorsSource(Pipeline* mergePipeline,
                                       std::vector<OwnedRemoteCursor> cursors,
                                       boost::optional<BSONObj> shardCursorsSortSpec) {
    auto [resultsCursors, metaCursors] = partitionCursors(std::move(cursors));
    // Whether or not cursors are typed/untyped, the first is always the results cursor.
    addMergeCursorsSource(mergePipeline, std::move(resultsCursors), shardCursorsSortSpec);
    if (metaCursors) {
        injectMetaCursor(mergePipeline, std::move(*metaCursors));
    }
}

Status appendExplainResults(DispatchShardPipelineResults&& dispatchResults,
                            const boost::intrusive_ptr<ExpressionContext>& mergeCtx,
                            BSONObjBuilder* result) {
    if (dispatchResults.splitPipeline) {
        auto* mergePipeline = dispatchResults.splitPipeline->mergePipeline.get();
        const char* mergeType = [&]() {
            if (mergePipeline->canRunOnMongos()) {
                if (mergeCtx->inMongos) {
                    return "mongos";
                }
                return "local";
            } else if (dispatchResults.exchangeSpec) {
                return "exchange";
            } else if (mergePipeline->needsPrimaryShardMerger()) {
                return "primaryShard";
            } else {
                return "anyShard";
            }
        }();

        *result << "mergeType" << mergeType;

        MutableDocument pipelinesDoc;
        // We specify "queryPlanner" verbosity when building the output for "shardsPart" because
        // execution stats are reported by each shard individually.
        auto opts = SerializationOptions{
            .verbosity = boost::make_optional(ExplainOptions::Verbosity::kQueryPlanner)};
        pipelinesDoc.addField(
            "shardsPart",
            Value(dispatchResults.splitPipeline->shardsPipeline->writeExplainOps(opts)));
        if (dispatchResults.exchangeSpec) {
            BSONObjBuilder bob;
            dispatchResults.exchangeSpec->exchangeSpec.serialize(&bob);
            bob.append("consumerShards", dispatchResults.exchangeSpec->consumerShards);
            pipelinesDoc.addField("exchange", Value(bob.obj()));
        }
        // We specify "queryPlanner" verbosity because execution stats are not currently
        // supported when building the output for "mergerPart".
        auto explainOps = mergePipeline->writeExplainOps(opts);

        // No cursors to remote shards are established for an explain, and the $mergeCursors
        // aggregation stage which is normally built in addMergeCursorsSource() requires vectors of
        // cursors and ShardIDs. For explain output, we construct the armParams that would normally
        // be used in the serialization of the $mergeCursors stage and add it to the serialization
        // of the pipeline.
        auto armParams =
            // Since no cursors are actually established for an explain, construct ARM params with
            // an empty vector and then remove it from the explain BSON.
            buildArmParams(dispatchResults.splitPipeline->mergePipeline->getContext(),
                           std::vector<OwnedRemoteCursor>(),
                           std::move(dispatchResults.splitPipeline->shardCursorsSortSpec))
                .toBSON()
                .removeField(AsyncResultsMergerParams::kRemotesFieldName);

        // See DocumentSourceMergeCursors::serialize().
        explainOps.insert(explainOps.begin(), Value(Document{{"$mergeCursors"_sd, armParams}}));

        pipelinesDoc.addField("mergerPart", Value(explainOps));

        *result << "splitPipeline" << pipelinesDoc.freeze();
    } else {
        *result << "splitPipeline" << BSONNULL;
    }

    BSONObjBuilder shardExplains(result->subobjStart("shards"));
    for (const auto& shardResult : dispatchResults.remoteExplainOutput) {
        invariant(shardResult.shardHostAndPort);

        uassertStatusOK(shardResult.swResponse.getStatus());
        uassertStatusOK(getStatusFromCommandResult(shardResult.swResponse.getValue().data));

        auto shardId = shardResult.shardId.toString();
        const auto& data = shardResult.swResponse.getValue().data;
        BSONObjBuilder explain(shardExplains.subobjStart(shardId));
        explain << "host" << shardResult.shardHostAndPort->toString();
        if (auto stagesElement = data["stages"]) {
            explain << "stages" << stagesElement;
        } else {
            auto queryPlannerElement = data["queryPlanner"];
            uassert(51157,
                    str::stream() << "Malformed explain response received from shard " << shardId
                                  << ": " << data.toString(),
                    queryPlannerElement);
            explain << "queryPlanner" << queryPlannerElement;
            if (auto executionStatsElement = data["executionStats"]) {
                explain << "executionStats" << executionStatsElement;
            }
        }
    }
    return Status::OK();
}

BSONObj targetShardsForExplain(Pipeline* ownedPipeline) {
    auto expCtx = ownedPipeline->getContext();
    std::unique_ptr<Pipeline, PipelineDeleter> pipeline(ownedPipeline,
                                                        PipelineDeleter(expCtx->opCtx));
    // The pipeline is going to be explained on the shards, and we don't want to send a
    // mergeCursors stage.
    invariant(pipeline->getSources().empty() ||
              !dynamic_cast<DocumentSourceMergeCursors*>(pipeline->getSources().front().get()));
    invariant(expCtx->explain);
    // Generate the command object for the targeted shards.
    auto rawStages = [&pipeline]() {
        auto serialization = pipeline->serialize();
        std::vector<BSONObj> stages;
        stages.reserve(serialization.size());

        for (const auto& stageObj : serialization) {
            invariant(stageObj.getType() == BSONType::Object);
            stages.push_back(stageObj.getDocument().toBson());
        }

        return stages;
    }();

    AggregateCommandRequest aggRequest(expCtx->ns, rawStages);
    LiteParsedPipeline liteParsedPipeline(aggRequest);
    auto hasChangeStream = liteParsedPipeline.hasChangeStream();
    auto startsWithDocuments = liteParsedPipeline.startsWithDocuments();
    auto shardDispatchResults =
        dispatchShardPipeline(aggregation_request_helper::serializeToCommandDoc(aggRequest),
                              hasChangeStream,
                              startsWithDocuments,
                              expCtx->eligibleForSampling(),
                              std::move(pipeline),
                              expCtx->explain);
    BSONObjBuilder explainBuilder;
    auto appendStatus =
        appendExplainResults(std::move(shardDispatchResults), expCtx, &explainBuilder);
    uassertStatusOK(appendStatus);
    return BSON("pipeline" << explainBuilder.done());
}

StatusWith<CollectionRoutingInfo> getExecutionNsRoutingInfo(OperationContext* opCtx,
                                                            const NamespaceString& execNss) {
    // First, verify that there are shards present in the cluster. If not, then we return the
    // stronger 'ShardNotFound' error rather than 'NamespaceNotFound'. We must do this because
    // $changeStream aggregations ignore NamespaceNotFound in order to allow streams to be opened on
    // a collection before its enclosing database is created. However, if there are no shards
    // present, then $changeStream should immediately return an empty cursor just as other
    // aggregations do when the database does not exist.
    const auto shardIds = Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx);
    if (shardIds.empty()) {
        return {ErrorCodes::ShardNotFound, "No shards are present in the cluster"};
    }

    // This call to getCollectionRoutingInfoForTxnCmd will return !OK if the database does not exist
    return getCollectionRoutingInfoForTxnCmd(opCtx, execNss);
}

Shard::RetryPolicy getDesiredRetryPolicy(OperationContext* opCtx) {
    // The idempotent retry policy will retry even for writeConcern failures, so only set it if the
    // pipeline does not support writeConcern.
    if (!opCtx->getWriteConcern().usedDefaultConstructedWC) {
        return Shard::RetryPolicy::kNotIdempotent;
    }
    return Shard::RetryPolicy::kIdempotent;
}

bool checkIfMustRunOnAllShards(const NamespaceString& nss,
                               bool hasChangeStream,
                               bool startsWithDocuments) {
    // The following aggregations must be routed to all shards:
    // - Any collectionless aggregation, such as non-localOps $currentOp.
    // - Any aggregation which begins with a $changeStream stage.
    return !startsWithDocuments && (nss.isCollectionlessAggregateNS() || hasChangeStream);
}

std::unique_ptr<Pipeline, PipelineDeleter> attachCursorToPipeline(
    Pipeline* ownedPipeline,
    ShardTargetingPolicy shardTargetingPolicy,
    boost::optional<BSONObj> readConcern) {
    auto expCtx = ownedPipeline->getContext();
    std::unique_ptr<Pipeline, PipelineDeleter> pipeline(ownedPipeline,
                                                        PipelineDeleter(expCtx->opCtx));
    boost::optional<DocumentSource*> hasFirstStage = pipeline->getSources().empty()
        ? boost::optional<DocumentSource*>{}
        : pipeline->getSources().front().get();

    if (hasFirstStage) {
        // Make sure the first stage isn't already a $mergeCursors, and also check if it is a stage
        // which needs to actually get a cursor attached or not.
        const auto* firstStage = *hasFirstStage;
        invariant(!dynamic_cast<const DocumentSourceMergeCursors*>(firstStage));
        // Here we check the hostRequirment because there is at least one stage ($indexStats) which
        // does not require input data, but is still expected to fan out and contact remote shards
        // nonetheless.
        if (auto constraints = firstStage->constraints(); !constraints.requiresInputDocSource &&
            (constraints.hostRequirement == StageConstraints::HostTypeRequirement::kLocalOnly)) {
            // There's no need to attach a cursor here - the first stage provides its own data and
            // is meant to be run locally (e.g. $documents).
            return pipeline;
        }
    }

    // Helper to decide whether we should ignore the given shardTargetingPolicy for this namespace.
    // Certain namespaces are shard-local; that is, they exist independently on every shard. For
    // these namespaces, a local cursor should always be used.
    // TODO SERVER-59957: use NamespaceString::isPerShardNamespace instead.
    auto shouldAlwaysAttachLocalCursorForNamespace = [](const NamespaceString& ns) {
        return (ns.isLocalDB() || ns.isConfigDotCacheDotChunks() ||
                ns.isReshardingLocalOplogBufferCollection() ||
                ns == NamespaceString::kConfigImagesNamespace ||
                ns.isChangeStreamPreImagesCollection());
    };

    if (shardTargetingPolicy == ShardTargetingPolicy::kNotAllowed ||
        shouldAlwaysAttachLocalCursorForNamespace(expCtx->ns)) {
        auto pipelineToTarget = pipeline->clone();

        return expCtx->mongoProcessInterface->attachCursorSourceToPipelineForLocalRead(
            pipelineToTarget.release());
    }

    sharding::router::CollectionRouter router(expCtx->opCtx->getServiceContext(), expCtx->ns);
    return router.route(
        expCtx->opCtx,
        "targeting pipeline to attach cursors"_sd,
        [&](OperationContext* opCtx, const CollectionRoutingInfo& cri) {
            const auto& cm = cri.cm;
            auto pipelineToTarget = pipeline->clone();

            const bool useLocalRead = !cm.isSharded() &&
                (!expCtx->mongoProcessInterface->inShardedEnvironment(opCtx) ||
                 cm.dbPrimary() ==
                     (serverGlobalParams.clusterRole.has(ClusterRole::ConfigServer)
                          ? ShardId::kConfigServerId
                          : ShardingState::get(opCtx)->shardId()));

            if (useLocalRead) {
                // If the collection is unsharded and we are on the primary, we should be able to
                // do a local read. The primary may be moved right after the primary shard check,
                // but the local read path will do a db version check before it establishes a cursor
                // to catch this case and ensure we fail to read locally.
                try {
                    auto expectUnshardedCollection(
                        expCtx->mongoProcessInterface->expectUnshardedCollectionInScope(
                            expCtx->opCtx, expCtx->ns, cm.dbVersion()));

                    // TODO SERVER-77402 Wrap this in a shardRoleRetry loop instead of
                    // catching gexceptions. attachCursorSourceToPipelineForLocalRead enters the
                    // shard role but does not refresh the shard if the shard has stale metadata.
                    // Proceeding to do normal shard targeting, which will go through the
                    // service_entry_point and refresh the shard if needed.
                    auto pipelineWithCursor =
                        expCtx->mongoProcessInterface->attachCursorSourceToPipelineForLocalRead(
                            pipelineToTarget.release());

                    LOGV2_DEBUG(5837600,
                                3,
                                "Performing local read",
                                logAttrs(expCtx->ns),
                                "pipeline"_attr = pipelineWithCursor->serializeToBson(),
                                "comment"_attr = expCtx->opCtx->getComment());

                    return pipelineWithCursor;
                } catch (ExceptionFor<ErrorCodes::StaleDbVersion>&) {
                    // The current node has stale information about this collection, proceed with
                    // shard targeting, which has logic to handle refreshing that may be needed.
                } catch (ExceptionForCat<ErrorCategory::StaleShardVersionError>&) {
                    // The current node has stale information about this collection, proceed with
                    // shard targeting, which has logic to handle refreshing that may be needed.
                } catch (ExceptionFor<ErrorCodes::CommandNotSupportedOnView>&) {
                    // The current node may be trying to run a pipeline on a namespace which is an
                    // unresolved view, proceed with shard targeting,
                }

                // The local read failed. Recreate 'pipelineToTarget' if it was released above.
                if (!pipelineToTarget) {
                    pipelineToTarget = pipeline->clone();
                }
            }

            return targetShardsAndAddMergeCursors(expCtx,
                                                  std::move(pipelineToTarget),
                                                  boost::none,
                                                  shardTargetingPolicy,
                                                  std::move(readConcern));
        });
}

}  // namespace sharded_agg_helpers
}  // namespace mongo
