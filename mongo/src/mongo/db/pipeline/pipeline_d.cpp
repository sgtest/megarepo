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

#include "mongo/db/pipeline/pipeline_d.h"

#include <algorithm>
#include <bitset>
#include <boost/cstdint.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iterator>
#include <list>
#include <string>
#include <tuple>
#include <type_traits>
#include <vector>

#include "mongo/base/error_codes.h"
#include "mongo/base/exact_cast.h"
#include "mongo/base/status.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/basic_types.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/exec/cached_plan.h"
#include "mongo/db/exec/collection_scan.h"
#include "mongo/db/exec/collection_scan_common.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/exec/index_scan.h"
#include "mongo/db/exec/multi_iterator.h"
#include "mongo/db/exec/multi_plan.h"
#include "mongo/db/exec/plan_stats.h"
#include "mongo/db/exec/sample_from_timeseries_bucket.h"
#include "mongo/db/exec/shard_filter.h"
#include "mongo/db/exec/shard_filterer.h"
#include "mongo/db/exec/shard_filterer_impl.h"
#include "mongo/db/exec/trial_stage.h"
#include "mongo/db/exec/unpack_timeseries_bucket.h"
#include "mongo/db/exec/working_set.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/index_names.h"
#include "mongo/db/locker_api.h"
#include "mongo/db/matcher/expression_algo.h"
#include "mongo/db/matcher/expression_expr.h"
#include "mongo/db/matcher/extensions_callback_real.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/ops/write_ops_gen.h"
#include "mongo/db/pipeline/dependencies.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/document_source_cursor.h"
#include "mongo/db/pipeline/document_source_geo_near.h"
#include "mongo/db/pipeline/document_source_geo_near_cursor.h"
#include "mongo/db/pipeline/document_source_group.h"
#include "mongo/db/pipeline/document_source_group_base.h"
#include "mongo/db/pipeline/document_source_internal_projection.h"
#include "mongo/db/pipeline/document_source_internal_replace_root.h"
#include "mongo/db/pipeline/document_source_internal_unpack_bucket.h"
#include "mongo/db/pipeline/document_source_lookup.h"
#include "mongo/db/pipeline/document_source_match.h"
#include "mongo/db/pipeline/document_source_replace_root.h"
#include "mongo/db/pipeline/document_source_sample.h"
#include "mongo/db/pipeline/document_source_sample_from_random_cursor.h"
#include "mongo/db/pipeline/document_source_set_window_fields.h"
#include "mongo/db/pipeline/document_source_single_document_transformation.h"
#include "mongo/db/pipeline/document_source_skip.h"
#include "mongo/db/pipeline/document_source_sort.h"
#include "mongo/db/pipeline/expression.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/inner_pipeline_stage_impl.h"
#include "mongo/db/pipeline/inner_pipeline_stage_interface.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/pipeline/search_helper.h"
#include "mongo/db/pipeline/skip_and_limit.h"
#include "mongo/db/pipeline/stage_constraints.h"
#include "mongo/db/pipeline/transformer_interface.h"
#include "mongo/db/query/canonical_distinct.h"
#include "mongo/db/query/canonical_query.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/get_executor.h"
#include "mongo/db/query/index_bounds.h"
#include "mongo/db/query/plan_executor_factory.h"
#include "mongo/db/query/plan_executor_impl.h"
#include "mongo/db/query/plan_yield_policy.h"
#include "mongo/db/query/plan_yield_policy_remote_cursor.h"
#include "mongo/db/query/projection.h"
#include "mongo/db/query/projection_parser.h"
#include "mongo/db/query/projection_policies.h"
#include "mongo/db/query/query_decorations.h"
#include "mongo/db/query/query_feature_flags_gen.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/query/query_planner_params.h"
#include "mongo/db/query/query_request_helper.h"
#include "mongo/db/query/query_settings.h"
#include "mongo/db/query/query_utils.h"
#include "mongo/db/query/record_id_bound.h"
#include "mongo/db/query/sort_pattern.h"
#include "mongo/db/query/stage_types.h"
#include "mongo/db/query/tailable_mode_gen.h"
#include "mongo/db/query/timeseries/bucket_spec.h"
#include "mongo/db/query/util/make_data_structure.h"
#include "mongo/db/record_id.h"
#include "mongo/db/s/collection_sharding_state.h"
#include "mongo/db/s/scoped_collection_metadata.h"
#include "mongo/db/server_options.h"
#include "mongo/db/server_parameter.h"
#include "mongo/db/server_parameter_with_storage.h"
#include "mongo/db/storage/record_store.h"
#include "mongo/db/storage/sorted_data_interface.h"
#include "mongo/db/storage/test_harness_helper.h"
#include "mongo/db/timeseries/timeseries_constants.h"
#include "mongo/db/timeseries/timeseries_gen.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/rpc/metadata/client_metadata.h"
#include "mongo/s/query/document_source_merge_cursors.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery

namespace mongo {

using boost::intrusive_ptr;
using std::shared_ptr;
using std::string;
using std::unique_ptr;
using write_ops::InsertCommandRequest;

namespace {
/**
 * Helper for findSbeCompatibleStagesForPushdown() that checks whether 'stage' is a $project or
 * $addFields that can be pushed down to SBE as a 'DocumentSourceInternalProjection' stage. If so,
 * this returns a pointer to a constructed object of the latter type, else it returns nullptr.
 */
boost::intrusive_ptr<DocumentSource> sbeCompatibleProjectionFromSingleDocumentTransformation(
    const DocumentSourceSingleDocumentTransformation& transformStage,
    SbeCompatibility minRequiredCompatibility) {
    InternalProjectionPolicyEnum policies;
    switch (transformStage.getType()) {
        case TransformerInterface::TransformerType::kExclusionProjection:
        case TransformerInterface::TransformerType::kInclusionProjection:
            policies = InternalProjectionPolicyEnum::kAggregate;
            break;
        case TransformerInterface::TransformerType::kComputedProjection:
            policies = InternalProjectionPolicyEnum::kAddFields;
            break;
        default:
            return nullptr;
    }

    const boost::intrusive_ptr<ExpressionContext>& expCtx = transformStage.getContext();
    SbeCompatibility originalSbeCompatibility =
        std::exchange(expCtx->sbeCompatibility, SbeCompatibility::fullyCompatible);
    ON_BLOCK_EXIT([&] { expCtx->sbeCompatibility = originalSbeCompatibility; });

    boost::intrusive_ptr<DocumentSource> projectionStage =
        make_intrusive<DocumentSourceInternalProjection>(
            expCtx,
            transformStage.getTransformer().serializeTransformation(boost::none).toBson(),
            policies);

    if (expCtx->sbeCompatibility < minRequiredCompatibility) {
        return nullptr;
    }

    return projectionStage;
}

/**
 * Helper for findSbeCompatibleStagesForPushdown() that creates a
 * 'DocumentSourceInternalReplaceRoot' from 'stage' if 'stage' is a '$replaceRoot' that can be
 * pushed down to SBE or returns nullptr otherwise.
 */
boost::intrusive_ptr<DocumentSource> sbeCompatibleReplaceRootStage(
    DocumentSourceSingleDocumentTransformation* replaceRootStage,
    SbeCompatibility minRequiredCompatibility) {
    if (replaceRootStage->getType() != TransformerInterface::TransformerType::kReplaceRoot) {
        return nullptr;
    }

    const auto& replaceRootTransformation =
        dynamic_cast<const ReplaceRootTransformation&>(replaceRootStage->getTransformer());
    if (replaceRootTransformation.sbeCompatibility() < minRequiredCompatibility) {
        return nullptr;
    }

    return make_intrusive<DocumentSourceInternalReplaceRoot>(
        replaceRootStage->getContext(), replaceRootTransformation.getExpression());
}

// A bit field with a bool flag for each aggregation pipeline stage that can be translated to SBE.
// The flags can be used to indicate which translations are enabled and/or supported in a particular
// context.
struct CompatiblePipelineStages {
    bool group : 1;
    bool lookup : 1;

    // The $project and $addField stages are considered the same for the purposes of SBE
    // translation.
    bool transform : 1;

    bool match : 1;
    bool unwind : 1;
    bool sort : 1;
    bool limitSkip : 1;
    bool search : 1;
    bool window : 1;
    bool unpackBucket : 1;
};

// Determine if 'stage' is eligible for SBE, and if it is add it to the 'stagesForPushdown' list as
// a 'InnerPipelineStageInterface' and return true. Return false if 'stage' is ineligible, either
// because it is disallowed by 'allowedStages' or because it requires functionality that cannot be
// translated to SBE.
bool pushDownPipelineStageIfCompatible(
    const OperationContext* opCtx,
    const boost::intrusive_ptr<DocumentSource>& stage,
    SbeCompatibility minRequiredCompatibility,
    const CompatiblePipelineStages& allowedStages,
    bool isLastSource,
    std::vector<std::unique_ptr<InnerPipelineStageInterface>>& stagesForPushdown) {
    if (auto matchStage = dynamic_cast<DocumentSourceMatch*>(stage.get())) {
        if (!allowedStages.match || matchStage->sbeCompatibility() < minRequiredCompatibility) {
            return false;
        }

        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(matchStage, isLastSource));
        return true;
    } else if (auto groupStage = dynamic_cast<DocumentSourceGroup*>(stage.get())) {
        if (!allowedStages.group || groupStage->doingMerge() ||
            groupStage->sbeCompatibility() < minRequiredCompatibility) {
            return false;
        }

        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(groupStage, isLastSource));
        return true;
    } else if (auto lookupStage = dynamic_cast<DocumentSourceLookUp*>(stage.get())) {
        if (!allowedStages.lookup || lookupStage->sbeCompatibility() < minRequiredCompatibility) {
            return false;
        }

        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(lookupStage, isLastSource));
        return true;
    } else if (auto unwindStage = dynamic_cast<DocumentSourceUnwind*>(stage.get())) {
        if (!allowedStages.unwind || unwindStage->sbeCompatibility() < minRequiredCompatibility) {
            return false;
        }

        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(unwindStage, isLastSource));
        return true;
    } else if (auto transformStage =
                   dynamic_cast<DocumentSourceSingleDocumentTransformation*>(stage.get())) {
        // We do not push to SBE an addFields that has been created as part of a setWindowFields
        // stage because it causes a performance regression. TODO (SERVER-75103) : Once
        // setWindowFields has been pushed to SBE, this should be removed.
        if (transformStage->isCreatedBySetWindowFields() &&
            SbeCompatibility::flagGuarded < minRequiredCompatibility) {
            return false;
        }

        if (!allowedStages.transform) {
            return false;
        }
        if (auto replaceRoot =
                sbeCompatibleReplaceRootStage(transformStage, minRequiredCompatibility)) {
            stagesForPushdown.emplace_back(
                std::make_unique<InnerPipelineStageImpl>(replaceRoot, isLastSource));
            return true;
        } else if (auto projectionStage = sbeCompatibleProjectionFromSingleDocumentTransformation(
                       *transformStage, minRequiredCompatibility)) {
            stagesForPushdown.emplace_back(
                std::make_unique<InnerPipelineStageImpl>(projectionStage, isLastSource));
            return true;
        }
        return false;
    } else if (auto sortStage = dynamic_cast<DocumentSourceSort*>(stage.get())) {
        if (!allowedStages.sort || !isSortSbeCompatible(sortStage->getSortKeyPattern())) {
            return false;
        }

        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(sortStage, isLastSource));
        return true;
    } else if (dynamic_cast<DocumentSourceLimit*>(stage.get()) ||
               dynamic_cast<DocumentSourceSkip*>(stage.get())) {
        if (!allowedStages.limitSkip) {
            return false;
        }

        stagesForPushdown.push_back(
            std::make_unique<InnerPipelineStageImpl>(stage.get(), isLastSource));
        return true;
    } else if (const auto& searchHelpers = getSearchHelpers(opCtx->getServiceContext());
               searchHelpers->isSearchStage(stage.get()) ||
               searchHelpers->isSearchMetaStage(stage.get())) {
        if (!allowedStages.search) {
            return false;
        }

        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(stage, isLastSource));
        return true;
    } else if (auto windowStage =
                   dynamic_cast<DocumentSourceInternalSetWindowFields*>(stage.get())) {
        if (!allowedStages.window || windowStage->sbeCompatibility() < minRequiredCompatibility) {
            return false;
        }
        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(windowStage, isLastSource));
        return true;
    } else if (auto unpackBucketStage =
                   dynamic_cast<DocumentSourceInternalUnpackBucket*>(stage.get())) {
        if (!allowedStages.unpackBucket) {
            return false;
        }

        stagesForPushdown.emplace_back(
            std::make_unique<InnerPipelineStageImpl>(unpackBucketStage, isLastSource));
        return true;
    }

    return false;
}

/**
 * Prunes $addFields from 'stagesForPushdown' if it is the last stage, subject to additional
 * conditions. (Must be called repeatedly until it returns false.) When splitting a pipeline between
 * SBE and Classic DocumentSource stages, there is often a performance penalty for executing an
 * $addFields in SBE only to immediately translate its output to MutableDocument form for the
 * Classic DocumentSource execution phase. Instead, we keep the $addFields as a DocumentSource.
 *
 * 'alreadyPruned' tells whether the pipeline has had stages pruned away already.
 *
 * Returns true iff it pruned a stage.
 */
bool pruneTrailingAddFields(
    std::vector<std::unique_ptr<InnerPipelineStageInterface>>& stagesForPushdown,
    bool alreadyPruned) {
    // Push down the entire pipeline when possible. (It's not possible if 'alreadyPruned' is true.)
    if (stagesForPushdown.empty() || (!alreadyPruned && stagesForPushdown.back()->isLastSource())) {
        return false;
    }

    auto projectionStage =
        dynamic_cast<DocumentSourceInternalProjection*>(stagesForPushdown.back()->documentSource());
    if (projectionStage &&
        projectionStage->projection().type() == projection_ast::ProjectType::kAddition) {
        stagesForPushdown.pop_back();
        return true;
    }
    return false;
}

/**
 * Prunes $unwind from 'stagesForPushdown' if it is the last stage. (Must be called repeatedly until
 * it returns false.) This pruning is done because $unwind performance is bottlenecked by processing
 * of EExpressions for sbe::ProjectStages in the SBE VM, which is slower than Classic's native C++
 * projection implementation. Pushing $unwind down only has a performance benefit when doing so
 * allows additional non-$unwind stages to be pushed down after it.
 *
 * Returns true iff it pruned a stage.
 */
bool pruneTrailingUnwind(
    std::vector<std::unique_ptr<InnerPipelineStageInterface>>& stagesForPushdown) {
    if (!stagesForPushdown.empty() &&
        dynamic_cast<DocumentSourceUnwind*>(stagesForPushdown.back()->documentSource())) {
        stagesForPushdown.pop_back();
        return true;
    }
    return false;
}

/**
 * After copying as many pipeline stages as possible into the 'stagesForPushdown' pipeline, this
 * second pass takes off any stages that may not benefit from execution in SBE.
 */
void prunePushdownStages(
    std::vector<std::unique_ptr<InnerPipelineStageInterface>>& stagesForPushdown,
    SbeCompatibility minRequiredCompatibility) {
    bool pruned = false;       // have any stages been pruned?
    bool prunedThisIteration;  // were any stages pruned in the current loop iteration?
    do {
        prunedThisIteration = false;
        if (SbeCompatibility::flagGuarded >= minRequiredCompatibility) {
            // When 'minRequiredCompatibility' is permissive enough (because featureFlagSbeFull is
            // enabled), do not remove trailing $addFields stages.
        } else {
            // Otherwise, remove trailing $addFields stages that we don't expect to improve
            // performance when they execute in SBE.
            if (pruneTrailingAddFields(stagesForPushdown, pruned)) {
                prunedThisIteration = true;
                pruned = true;
            }
        }

        // $unwind should not be the last stage pushed down as it is more expensive in SBE.
        if (pruneTrailingUnwind(stagesForPushdown)) {
            prunedThisIteration = true;
            pruned = true;
        };
    } while (prunedThisIteration);
}

// Limit the number of aggregation pipeline stages that can be "pushed down" to the SBE stage
// builders. Compiling too many pipeline stages during stage building would overflow the call stack.
// The limit is higher for optimized builds, because optimization reduces the size of stack frames.
#ifdef MONGO_CONFIG_OPTIMIZED_BUILD
constexpr size_t kSbeMaxPipelineStages = 400;
#else
constexpr size_t kSbeMaxPipelineStages = 100;
#endif

/**
 * Finds a prefix of stages from the given pipeline to prepare for pushdown into the inner query
 * layer so that it can be executed using SBE. Unless pushdown is completely disabled by
 * {'internalQueryFrameworkControl': 'forceClassicEngine'}, a stage can be extracted from the
 * pipeline if and only if all the stages before it are extracted and it meets the criteria for its
 * stage type:
 * $group via 'DocumentSourceGroup':
 *   - The 'internalQuerySlotBasedExecutionDisableGroupPushdown' knob is false and
 *   - the $group is not a mering operation that aggregates partial groups
 *     (DocumentSourceGroupBase::doingMerge()).
 *
 * $lookup via 'DocumentSourceLookUp':
 *   - The 'internalQuerySlotBasedExecutionDisableLookupPushdown' query knob is false,
 *   - the $lookup uses only the 'localField'/'foreignField' syntax (no pipelines), and
 *   - the foreign collection is neither sharded nor a view.
 *
 * $project via 'DocumentSourceInternalProjection':
 *   - No additional criteria.
 *
 * $addFields via 'DocumentSourceInternalProjection':
 *   - The stage that _follows_ the $addFields is also pushed down _or_
 *   - the 'featureFlagSbeFull' flag is enabled.
 *
 * $replaceRoot/$replaceWith via 'DocumentSourceSingleDocumentTransformation':
 *   - No additional criteria.
 *
 * $sort via 'DocumentSourceSort':
 *   - The sort operation does not produce sort key "meta" fields need by a later merging operation
 *     (i.e., 'needsMerge' is false).
 *
 * $match via 'DocumentSourceMatch':
 *   - No additional criteria.
 *
 * $limit via 'DocumentSourceLimit':
 *   - No additional criteria.
 *
 * $skip via 'DocumentSourceSkip':
 *   - No additional criteria.
 *
 * 'DocumentSourceUnpackBucket':
 *   - The 'featureFlagSbeFull' flag is enabled.
 *
 * 'DocumentSourceSearch':
 *   - The 'featureFlagSearchInSbe' flag is enabled.
 *
 * $_internalUnpackBucket via 'DocumentSourceInternalUnpackBucket':
 *   - The 'featureFlagTimeSeriesInSbe' flag is enabled and
 *   - the 'internalQuerySlotBasedExecutionDisableTimeSeriesPushdown', is _not_ enabled,
 */
std::vector<std::unique_ptr<InnerPipelineStageInterface>> findSbeCompatibleStagesForPushdown(
    const MultipleCollectionAccessor& collections,
    const CanonicalQuery* cq,
    bool needsMerge,
    const Pipeline* pipeline) {
    // We will eventually use the extracted group stages to populate 'CanonicalQuery::pipeline'
    // which requires stages to be wrapped in an interface.
    std::vector<std::unique_ptr<InnerPipelineStageInterface>> stagesForPushdown;

    // No pushdown if we're using the classic engine.
    if (cq->getForceClassicEngine()) {
        return {};
    }

    const auto& sources = pipeline->getSources();

    bool isMainCollectionSharded = false;
    if (const auto& mainColl = collections.getMainCollection()) {
        isMainCollectionSharded = mainColl.isSharded_DEPRECATED();
    }

    // SERVER-78998: Refactor these checks so that they do not load their values multiple times
    // during the same query.
    // (Ignore FCV check): featureFlagSbeFull does not change the semantics of queries, so it can
    // safely be enabled on some nodes and disabled on other nodes during upgrade/downgrade.
    SbeCompatibility minRequiredCompatibility =
        feature_flags::gFeatureFlagSbeFull.isEnabledAndIgnoreFCVUnsafe()
        ? SbeCompatibility::flagGuarded
        : SbeCompatibility::fullyCompatible;

    auto& queryKnob = QueryKnobConfiguration::decoration(cq->getExpCtxRaw()->opCtx);
    CompatiblePipelineStages allowedStages = {
        .group = !queryKnob.getSbeDisableGroupPushdownForOp(),

        // If lookup pushdown isn't enabled or the main collection is sharded or any of the
        // secondary namespaces are sharded or are a view, then no $lookup stage will be eligible
        // for pushdown.
        //
        // When acquiring locks for multiple collections, it is the case that we can only determine
        // whether any secondary collection is a view or is sharded, not which ones are a view or
        // are sharded and which ones aren't. As such, if any secondary collection is a view or is
        // sharded, no $lookup will be eligible for pushdown.
        .lookup = !queryKnob.getSbeDisableLookupPushdownForOp() && !isMainCollectionSharded &&
            !collections.isAnySecondaryNamespaceAViewOrSharded(),

        .transform = SbeCompatibility::fullyCompatible >= minRequiredCompatibility,
        .match = SbeCompatibility::fullyCompatible >= minRequiredCompatibility,

        // TODO (SERVER-80226): SBE execution of 'unwind' stages requires 'featureFlagSbeFull' to be
        // enabled.
        .unwind = SbeCompatibility::flagGuarded >= minRequiredCompatibility,

        // Note: even if its sort pattern is SBE compatible, we cannot push down a $sort stage when
        // the pipeline is the shard part of a sorted-merge query on a sharded collection. It is
        // possible that the merge operation will need a $sortKey field from the sort, and SBE plans
        // do not yet support metadata fields.
        .sort = (SbeCompatibility::fullyCompatible >= minRequiredCompatibility) && !needsMerge,

        .limitSkip = SbeCompatibility::fullyCompatible >= minRequiredCompatibility,

        // TODO (SERVER-77229): SBE execution of $search requires 'featureFlagSearchInSbe' to be
        // enabled.
        // (Ignore FCV check): As with 'featureFlagSbeFull' (above), the effects of
        // 'featureFlagSearchInSbe' are local to this node, making it safe to ignore the FCV.
        .search = feature_flags::gFeatureFlagSearchInSbe.isEnabledAndIgnoreFCVUnsafe(),

        .window = !(SbeCompatibility::fullyCompatible < minRequiredCompatibility),

        // TODO (SERVER-80243): Remove 'featureFlagTimeSeriesInSbe' check.
        .unpackBucket = feature_flags::gFeatureFlagTimeSeriesInSbe.isEnabled(
                            serverGlobalParams.featureCompatibility.acquireFCVSnapshot()) &&

            !queryKnob.getSbeDisableTimeSeriesForOp() &&
            cq->getExpCtx()->sbePipelineCompatibility == SbeCompatibility::fullyCompatible,
    };

    for (auto itr = sources.begin(); itr != sources.end(); ++itr) {
        // Push down at most kMaxPipelineStages stages for execution in SBE.
        if (stagesForPushdown.size() >= kSbeMaxPipelineStages) {
            break;
        }

        const bool isLastSource = itr->get() == sources.back().get();
        if (!pushDownPipelineStageIfCompatible(pipeline->getContext()->opCtx,
                                               *itr,
                                               minRequiredCompatibility,
                                               allowedStages,
                                               isLastSource,
                                               stagesForPushdown)) {
            // Stop pushing stages down once we hit an incompatible stage.
            break;
        }
    }

    // Remove stage patterns where pushing down may degrade performance.
    prunePushdownStages(stagesForPushdown, minRequiredCompatibility);

    return stagesForPushdown;
}  // findSbeCompatibleStagesForPushdown

/**
 * Removes the first 'stagesToRemove' stages from the pipeline. This function is meant to be paired
 * with a call to findSbeCompatibleStagesForPushdown() - the caller must first get the stages to
 * push down, then remove them.
 */
void trimPipelineStages(Pipeline* pipeline, size_t stagesToRemove) {
    auto& sources = pipeline->getSources();
    tassert(7087104,
            "stagesToRemove must be <= number of pipeline sources",
            stagesToRemove <= sources.size());
    for (size_t i = 0; i < stagesToRemove; ++i) {
        sources.erase(sources.begin());
    }
}

std::unique_ptr<FindCommandRequest> createFindCommand(
    const intrusive_ptr<ExpressionContext>& expCtx,
    const NamespaceString& nss,
    BSONObj queryObj,
    BSONObj projectionObj,
    BSONObj sortObj,
    SkipThenLimit skipThenLimit,
    const AggregateCommandRequest* aggRequest) {
    auto findCommand = std::make_unique<FindCommandRequest>(nss);

    query_request_helper::setTailableMode(expCtx->tailableMode, findCommand.get());
    findCommand->setFilter(queryObj.getOwned());
    findCommand->setProjection(projectionObj.getOwned());
    findCommand->setSort(sortObj.getOwned());
    if (auto skip = skipThenLimit.getSkip()) {
        findCommand->setSkip(static_cast<std::int64_t>(*skip));
    }
    if (auto limit = skipThenLimit.getLimit()) {
        findCommand->setLimit(static_cast<std::int64_t>(*limit));
    }

    if (aggRequest) {
        findCommand->setAllowDiskUse(aggRequest->getAllowDiskUse());
        findCommand->setHint(aggRequest->getHint().value_or(BSONObj()).getOwned());
        findCommand->setRequestResumeToken(aggRequest->getRequestResumeToken());
        if (aggRequest->getResumeAfter()) {
            findCommand->setResumeAfter(*aggRequest->getResumeAfter());
        }
    }

    // The collation on the ExpressionContext has been resolved to either the user-specified
    // collation or the collection default. This BSON should never be empty even if the resolved
    // collator is simple.
    findCommand->setCollation(expCtx->getCollatorBSON().getOwned());

    return findCommand;
}

StatusWith<std::unique_ptr<PlanExecutor, PlanExecutor::Deleter>> attemptToGetExecutor(
    const intrusive_ptr<ExpressionContext>& expCtx,
    const MultipleCollectionAccessor& collections,
    const NamespaceString& nss,
    std::unique_ptr<FindCommandRequest> findCommand,
    const QueryMetadataBitSet& metadataRequested,
    const GroupFromFirstDocumentTransformation* groupForDistinctScan,
    const QueryPlannerParams& plannerOpts,
    const MatchExpressionParser::AllowedFeatureSet& matcherFeatures,
    Pipeline* pipeline,
    bool isCountLike) {
    // Reset the 'sbeCompatible' flag before canonicalizing the 'findCommand' to potentially
    // allow SBE to execute the portion of the query that's pushed down, even if the portion of
    // the query that is not pushed down contains expressions not supported by SBE.
    expCtx->sbeCompatibility = SbeCompatibility::fullyCompatible;

    auto cq = CanonicalQuery::make(
        {.expCtx = expCtx,
         .parsedFind =
             ParsedFindCommandParams{
                 .findCommand = std::move(findCommand),
                 .extensionsCallback = ExtensionsCallbackReal(expCtx->opCtx, &nss),
                 .allowedFeatures = matcherFeatures,
                 .projectionPolicies = ProjectionPolicies::aggregateProjectionPolicies()},
         .explain = static_cast<bool>(expCtx->explain),
         .isCountLike = isCountLike,
         .isSearchQuery = PipelineD::isSearchPresentAndEligibleForSbe(pipeline)});

    if (!cq.isOK()) {
        // Return an error instead of uasserting, since there are cases where the combination of
        // sort and projection will result in a bad query, but when we try with a different
        // combination it will be ok. e.g. a sort by {$meta: 'textScore'}, without any projection
        // will fail, but will succeed when the corresponding '$meta' projection is passed in
        // another attempt.
        return {cq.getStatus()};
    }

    // Mark the metadata that's requested by the pipeline on the CQ.
    cq.getValue()->requestAdditionalMetadata(metadataRequested);

    if (groupForDistinctScan) {
        // When the pipeline includes a $group that groups by a single field
        // (groupIdForDistinctScan), we use getExecutorDistinct() to attempt to get an executor that
        // uses a DISTINCT_SCAN to scan exactly one document for each group. When that's not
        // possible, we return nullptr, and the caller is responsible for trying again without
        // passing a 'groupIdForDistinctScan' value.
        CanonicalDistinct canonicalDistinct(std::move(cq.getValue()),
                                            groupForDistinctScan->groupId());

        // If the GroupFromFirst transformation was generated for the $last case, we will need to
        // flip the direction of any generated DISTINCT_SCAN to preserve the semantics of the query.
        const bool flipDistinctScanDirection = groupForDistinctScan->expectedInput() ==
            GroupFromFirstDocumentTransformation::ExpectedInput::kLastDocument;

        // Note that we request a "strict" distinct plan because:
        // 1) We do not want to have to de-duplicate the results of the plan.
        //
        // 2) We not want a plan that will return separate values for each array element. For
        // example, if we have a document {a: [1,2]} and group by "a" a DISTINCT_SCAN on an "a"
        // index would produce one result for '1' and another for '2', which would be incorrect.
        auto distinctExecutor =
            getExecutorDistinct(&collections.getMainCollection(),
                                plannerOpts.options | QueryPlannerParams::STRICT_DISTINCT_ONLY,
                                &canonicalDistinct,
                                flipDistinctScanDirection);
        if (!distinctExecutor.isOK()) {
            return distinctExecutor.getStatus().withContext(
                "Unable to use distinct scan to optimize $group stage");
        } else if (!distinctExecutor.getValue()) {
            return {ErrorCodes::NoQueryExecutionPlans,
                    "Unable to use distinct scan to optimize $group stage"};
        } else {
            return distinctExecutor;
        }
    }

    // Queries that can use SBE may push down compatible pipeline stages. 'getExecutorFind' will
    // call this lambda in two phases: 1) determine compatible stages and attach them to the
    // canonical query, and 2) finalize the push down and trim the pushed-down stages from the
    // original pipeline.
    auto extractAndAttachPipelineStages = [&collections, &pipeline, needsMerge{expCtx->needsMerge}](
                                              auto* canonicalQuery, bool attachOnly) {
        if (attachOnly) {
            canonicalQuery->setCqPipeline(findSbeCompatibleStagesForPushdown(
                collections, canonicalQuery, needsMerge, pipeline));
        } else {
            trimPipelineStages(pipeline, canonicalQuery->cqPipeline().size());
        }
    };

    // For performance, we pass a null callback instead of 'extractAndAttachPipelineStages' when
    // 'pipeline' is empty. The 'extractAndAttachPipelineStages' is a no-op when there are no
    // pipeline stages, so we can save some work by skipping it. The 'getExecutorFind()' function is
    // responsible for checking that the callback is non-null before calling it.
    return getExecutorFind(expCtx->opCtx,
                           collections,
                           std::move(cq.getValue()),
                           !pipeline->getSources().empty()
                               ? std::move(extractAndAttachPipelineStages)
                               : std::function<void(CanonicalQuery*, bool)>{},
                           true /* permitYield */,
                           plannerOpts);
}

/**
 * Examines the indexes in 'collection' and returns the field name of a geo-indexed field suitable
 * for use in $geoNear. 2d indexes are given priority over 2dsphere indexes.
 *
 * The 'collection' is required to exist. Throws if no usable 2d or 2dsphere index could be found.
 */
StringData extractGeoNearFieldFromIndexes(OperationContext* opCtx,
                                          const CollectionPtr& collection) {
    invariant(collection);

    std::vector<const IndexDescriptor*> idxs;
    collection->getIndexCatalog()->findIndexByType(opCtx, IndexNames::GEO_2D, idxs);
    uassert(ErrorCodes::IndexNotFound,
            str::stream() << "There is more than one 2d index on "
                          << collection->ns().toStringForErrorMsg()
                          << "; unsure which to use for $geoNear",
            idxs.size() <= 1U);
    if (idxs.size() == 1U) {
        for (auto&& elem : idxs.front()->keyPattern()) {
            if (elem.type() == BSONType::String && elem.valueStringData() == IndexNames::GEO_2D) {
                return elem.fieldNameStringData();
            }
        }
        MONGO_UNREACHABLE;
    }

    // If there are no 2d indexes, look for a 2dsphere index.
    idxs.clear();
    collection->getIndexCatalog()->findIndexByType(opCtx, IndexNames::GEO_2DSPHERE, idxs);
    uassert(ErrorCodes::IndexNotFound,
            "$geoNear requires a 2d or 2dsphere index, but none were found",
            !idxs.empty());
    uassert(ErrorCodes::IndexNotFound,
            str::stream() << "There is more than one 2dsphere index on "
                          << collection->ns().toStringForErrorMsg()
                          << "; unsure which to use for $geoNear",
            idxs.size() <= 1U);

    invariant(idxs.size() == 1U);
    for (auto&& elem : idxs.front()->keyPattern()) {
        if (elem.type() == BSONType::String && elem.valueStringData() == IndexNames::GEO_2DSPHERE) {
            return elem.fieldNameStringData();
        }
    }
    MONGO_UNREACHABLE;
}

/**
 * This attempts to either extract a $sample stage at the front of the pipeline or a
 * $_internalUnpackBucket stage at the front of the pipeline immediately followed by a $sample
 * stage. In the former case a 'nullptr' is returned for the second element of the pair <$sample,
 * $_internalUnpackBucket>, and if the latter case is encountered both elements of the pair will be
 * a populated. If the pipeline doesn't contain a $_internalUnpackBucket at the front of the
 * pipeline immediately followed by a $sample stage, then the first element in the pair will be a
 * 'nullptr'.
 */
std::pair<DocumentSourceSample*, DocumentSourceInternalUnpackBucket*> extractSampleUnpackBucket(
    const Pipeline::SourceContainer& sources) {
    DocumentSourceSample* sampleStage = nullptr;
    DocumentSourceInternalUnpackBucket* unpackStage = nullptr;

    auto sourcesIt = sources.begin();
    if (sourcesIt != sources.end()) {
        sampleStage = dynamic_cast<DocumentSourceSample*>(sourcesIt->get());
        if (sampleStage) {
            return std::pair{sampleStage, unpackStage};
        }

        unpackStage = dynamic_cast<DocumentSourceInternalUnpackBucket*>(sourcesIt->get());
        ++sourcesIt;

        if (unpackStage && sourcesIt != sources.end()) {
            sampleStage = dynamic_cast<DocumentSourceSample*>(sourcesIt->get());
            return std::pair{sampleStage, unpackStage};
        }
    }

    return std::pair{sampleStage, unpackStage};
}

bool areSortFieldsModifiedByEventProjection(const SortPattern& sortPattern,
                                            const DocumentSource::GetModPathsReturn& modPaths) {
    return std::any_of(sortPattern.begin(), sortPattern.end(), [&](const auto& sortPatternPart) {
        const auto& fieldPath = sortPatternPart.fieldPath;
        return !fieldPath || modPaths.canModify(*fieldPath);
    });
}

bool areSortFieldsModifiedByBucketProjection(const SortPattern& sortPattern,
                                             const DocumentSource::GetModPathsReturn& modPaths) {
    // The time field maps to control.min.[time], control.max.[time], or
    // _id, and $_internalUnpackBucket assumes that all of those fields are
    // preserved. (We never push down a stage that would overwrite them.)

    // Each field [meta].a.b.c maps to 'meta.a.b.c'.
    auto rename = [&](const FieldPath& eventField) -> FieldPath {
        if (eventField.getPathLength() == 1)
            return timeseries::kBucketMetaFieldName;
        return FieldPath{timeseries::kBucketMetaFieldName}.concat(eventField.tail());
    };

    return std::any_of(sortPattern.begin(),
                       // Skip the last field, which is time: only check the meta fields
                       std::prev(sortPattern.end()),
                       [&](const auto& sortPatternPart) {
                           auto bucketFieldPath = rename(*sortPatternPart.fieldPath);
                           return modPaths.canModify(bucketFieldPath);
                       });
}

bool areSortFieldsModifiedByProjection(bool seenUnpack,
                                       const SortPattern& sortPattern,
                                       const DocumentSource::GetModPathsReturn& modPaths) {
    if (seenUnpack) {
        // This stage operates on events: check the event-level field names.
        return areSortFieldsModifiedByEventProjection(sortPattern, modPaths);
    } else {
        // This stage operates on buckets: check the bucket-level field names.
        return areSortFieldsModifiedByBucketProjection(sortPattern, modPaths);
    }
}

// There can be exactly one unpack stage in a pipeline but multiple sort stages. We'll find the
// _first_ sort.
struct SortAndUnpackInPipeline {
    DocumentSourceInternalUnpackBucket* unpack = nullptr;
    DocumentSourceSort* sort = nullptr;
    int unpackIdx = -1;
    int sortIdx = -1;
};
SortAndUnpackInPipeline findUnpackAndSort(const Pipeline::SourceContainer& sources) {
    SortAndUnpackInPipeline su;

    int idx = 0;
    auto itr = sources.begin();
    while (itr != sources.end()) {
        if (!su.unpack) {
            su.unpack = dynamic_cast<DocumentSourceInternalUnpackBucket*>(itr->get());
            su.unpackIdx = idx;
        }
        if (!su.sort) {
            su.sort = dynamic_cast<DocumentSourceSort*>(itr->get());
            su.sortIdx = idx;
        }
        if (su.unpack && su.sort) {
            break;
        }

        ++itr;
        ++idx;
    }
    return su;
}
}  // namespace

StatusWith<unique_ptr<PlanExecutor, PlanExecutor::Deleter>> PipelineD::createRandomCursorExecutor(
    const CollectionPtr& coll,
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    Pipeline* pipeline,
    long long sampleSize,
    long long numRecords,
    boost::optional<timeseries::BucketUnpacker> bucketUnpacker) {
    OperationContext* opCtx = expCtx->opCtx;

    // Verify that we are already under a collection lock or in a lock-free read. We avoid taking
    // locks ourselves in this function because double-locking forces any PlanExecutor we create to
    // adopt an INTERRUPT_ONLY policy.
    invariant(opCtx->isLockFreeReadsOp() ||
              shard_role_details::getLocker(opCtx)->isCollectionLockedForMode(coll->ns(), MODE_IS));

    auto* clusterParameters = ServerParameterSet::getClusterParameterSet();
    auto* randomCursorSampleRatioParam =
        clusterParameters
            ->get<ClusterParameterWithStorage<InternalQueryCutoffForSampleFromRandomCursorStorage>>(
                "internalQueryCutoffForSampleFromRandomCursor");

    auto maxSampleRatioClusterParameter =
        randomCursorSampleRatioParam->getValue(expCtx->ns.tenantId());

    const double kMaxSampleRatioForRandCursor = maxSampleRatioClusterParameter.getSampleCutoff();

    if (!expCtx->ns.isTimeseriesBucketsCollection()) {
        if (sampleSize > numRecords * kMaxSampleRatioForRandCursor || numRecords <= 100) {
            return nullptr;
        }
    } else {
        // Suppose that a time-series bucket collection is observed to contain 200 buckets, and the
        // 'gTimeseriesBucketMaxCount' parameter is set to 1000. If all buckets are full, then the
        // maximum possible measurment count would be 200 * 1000 = 200,000. While the
        // 'SampleFromTimeseriesBucket' plan is more efficient when the sample size is small
        // relative to the total number of measurements in the time-series collection, for larger
        // sample sizes the top-k sort based sample is faster. Experiments have approximated that
        // the tipping point is roughly when the requested sample size is greater than 1% of the
        // maximum possible number of measurements in the collection (i.e. numBuckets *
        // maxMeasurementsPerBucket).
        static const double kCoefficient = 0.01;
        if (sampleSize > kCoefficient * numRecords * gTimeseriesBucketMaxCount) {
            return nullptr;
        }
    }

    // Attempt to get a random cursor from the RecordStore.
    auto rsRandCursor = coll->getRecordStore()->getRandomCursor(opCtx);
    if (!rsRandCursor) {
        // The storage engine has no random cursor support.
        return nullptr;
    }

    // Build a MultiIteratorStage and pass it the random-sampling RecordCursor.
    auto ws = std::make_unique<WorkingSet>();
    std::unique_ptr<PlanStage> root =
        std::make_unique<MultiIteratorStage>(expCtx.get(), ws.get(), &coll);
    static_cast<MultiIteratorStage*>(root.get())->addIterator(std::move(rsRandCursor));

    TrialStage* trialStage = nullptr;

    const auto [isSharded, optOwnershipFilter] = [&]() {
        auto scopedCss =
            CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, coll->ns());
        const bool isSharded = scopedCss->getCollectionDescription(opCtx).isSharded();
        boost::optional<ScopedCollectionFilter> optFilter = isSharded
            ? boost::optional<ScopedCollectionFilter>(scopedCss->getOwnershipFilter(
                  opCtx, CollectionShardingState::OrphanCleanupPolicy::kDisallowOrphanCleanup))
            : boost::none;
        return std::pair(isSharded, std::move(optFilter));
    }();

    // Because 'numRecords' includes orphan documents, our initial decision to optimize the $sample
    // cursor may have been mistaken. For sharded collections, build a TRIAL plan that will switch
    // to a collection scan if the ratio of orphaned to owned documents encountered over the first
    // 100 works() is such that we would have chosen not to optimize.
    static const size_t kMaxPresampleSize = 100;
    if (expCtx->ns.isTimeseriesBucketsCollection()) {
        // We can't take ARHASH optimization path for a direct $sample on the system.buckets
        // collection because data is in compressed form. If we did have a direct $sample on the
        // system.buckets collection, then the 'bucketUnpacker' would not be set up properly. We
        // also should bail out early if a $sample is made against a time series collection that is
        // empty. If we don't the 'minAdvancedToWorkRatio' can be nan/-nan depending on the
        // architecture.
        if (!(bucketUnpacker && numRecords)) {
            return nullptr;
        }

        // Use a 'TrialStage' to run a trial between 'SampleFromTimeseriesBucket' and
        // 'UnpackTimeseriesBucket' with $sample left in the pipeline in-place. If the buckets are
        // not sufficiently full, or the 'SampleFromTimeseriesBucket' plan draws too many
        // duplicates, then we will fall back to the 'TrialStage' backup plan. This backup plan uses
        // the top-k sort sampling approach.
        //
        // Suppose the 'gTimeseriesBucketMaxCount' is 1000, but each bucket only contains 500
        // documents on average. The observed trial advanced/work ratio approximates the average
        // bucket fullness, noted here as "abf". In this example, abf = 500 / 1000 = 0.5.
        // Experiments have shown that the optimized 'SampleFromTimeseriesBucket' algorithm performs
        // better than backup plan when
        //
        //     sampleSize < 0.02 * abf * numRecords * gTimeseriesBucketMaxCount
        //
        //  This inequality can be rewritten as
        //
        //     abf > sampleSize / (0.02 * numRecords * gTimeseriesBucketMaxCount)
        //
        // Therefore, if the advanced/work ratio exceeds this threshold, we will use the
        // 'SampleFromTimeseriesBucket' plan. Note that as the sample size requested by the user
        // becomes larger with respect to the number of buckets, we require a higher advanced/work
        // ratio in order to justify using 'SampleFromTimeseriesBucket'.
        //
        // Additionally, we require the 'TrialStage' to approximate the abf as at least 0.25. When
        // buckets are mostly empty, the 'SampleFromTimeseriesBucket' will be inefficient due to a
        // lot of sampling "misses".
        static const auto kCoefficient = 0.02;
        static const auto kMinBucketFullness = 0.25;
        const auto minAdvancedToWorkRatio = std::max(
            std::min(sampleSize / (kCoefficient * numRecords * gTimeseriesBucketMaxCount), 1.0),
            kMinBucketFullness);

        boost::optional<std::unique_ptr<ShardFilterer>> maybeShardFilter;
        if (isSharded) {
            // In the sharded case, we need to use a ShardFilterer within the ARHASH plan to
            // eliminate orphans from the working set, since the stage owns the cursor.
            invariant(optOwnershipFilter);
            maybeShardFilter = std::make_unique<ShardFiltererImpl>(*optOwnershipFilter);
        }

        auto arhashPlan = std::make_unique<SampleFromTimeseriesBucket>(
            expCtx.get(),
            ws.get(),
            std::move(root),
            bucketUnpacker->copy(),
            std::move(maybeShardFilter),
            // By using a quantity slightly higher than 'kMaxPresampleSize', we ensure that the
            // 'SampleFromTimeseriesBucket' stage won't fail due to too many consecutive sampling
            // attempts during the 'TrialStage's trial period.
            kMaxPresampleSize + 5,
            sampleSize,
            gTimeseriesBucketMaxCount);

        std::unique_ptr<PlanStage> collScanPlan = std::make_unique<CollectionScan>(
            expCtx.get(), &coll, CollectionScanParams{}, ws.get(), nullptr);

        if (isSharded) {
            // In the sharded case, we need to add a shard-filterer stage to the backup plan to
            // eliminate orphans. The trial plan is thus SHARDING_FILTER-COLLSCAN.
            invariant(optOwnershipFilter);
            collScanPlan = std::make_unique<ShardFilterStage>(
                expCtx.get(), *optOwnershipFilter, ws.get(), std::move(collScanPlan));
        }

        auto topkSortPlan = std::make_unique<UnpackTimeseriesBucket>(
            expCtx.get(), ws.get(), std::move(collScanPlan), bucketUnpacker->copy());

        // In a sharded collection we need to preserve the $sample source in order to provide the
        // AsyncResultsMerger with $sortKeys it can use to merge samples from multiple shards.
        // However, this means we need to perform a sort on the results of ARHASH. This work is not
        // counted by the TrialStage, so we impose an arbitrary upper limit on the sample size
        // before defaulting to a Top-K sort, in order to bound the cost of sorting the sample
        // returned by ARHASH.
        static const auto kMaxSortSizeForShardedARHASHSample = 1000;
        if (isSharded && (sampleSize > kMaxSortSizeForShardedARHASHSample)) {
            root = std::move(topkSortPlan);
        } else {
            // We need to use a TrialStage approach to handle a problem where ARHASH sampling can
            // fail due to small measurement counts. We can push sampling and bucket unpacking down
            // to the PlanStage layer and erase $_internalUnpackBucket and $sample.
            root = std::make_unique<TrialStage>(expCtx.get(),
                                                ws.get(),
                                                std::move(arhashPlan),
                                                std::move(topkSortPlan),
                                                kMaxPresampleSize,
                                                minAdvancedToWorkRatio);
            trialStage = static_cast<TrialStage*>(root.get());
        }

    } else if (isSharded) {
        // The ratio of owned to orphaned documents must be at least equal to the ratio between the
        // requested sampleSize and the maximum permitted sampleSize for the original constraints to
        // be satisfied. For instance, if there are 200 documents and the sampleSize is 5, then at
        // least (5 / (200*0.05)) = (5/10) = 50% of those documents must be owned. If less than 5%
        // of the documents in the collection are owned, we default to the backup plan.
        const auto minAdvancedToWorkRatio = std::max(
            sampleSize / (numRecords * kMaxSampleRatioForRandCursor), kMaxSampleRatioForRandCursor);
        // Since the incoming operation is sharded, use the CSS to infer the filtering metadata for
        // the collection. We get the shard ownership filter after checking to see if the collection
        // is sharded to avoid an invariant from being fired in this call.
        invariant(optOwnershipFilter);
        // The trial plan is SHARDING_FILTER-MULTI_ITERATOR.
        auto randomCursorPlan = std::make_unique<ShardFilterStage>(
            expCtx.get(), *optOwnershipFilter, ws.get(), std::move(root));
        // The backup plan is SHARDING_FILTER-COLLSCAN.
        std::unique_ptr<PlanStage> collScanPlan = std::make_unique<CollectionScan>(
            expCtx.get(), &coll, CollectionScanParams{}, ws.get(), nullptr);
        collScanPlan = std::make_unique<ShardFilterStage>(
            expCtx.get(), *optOwnershipFilter, ws.get(), std::move(collScanPlan));
        // Place a TRIAL stage at the root of the plan tree, and pass it the trial and backup plans.
        root = std::make_unique<TrialStage>(expCtx.get(),
                                            ws.get(),
                                            std::move(randomCursorPlan),
                                            std::move(collScanPlan),
                                            kMaxPresampleSize,
                                            minAdvancedToWorkRatio);
        trialStage = static_cast<TrialStage*>(root.get());
    }

    auto execStatus = plan_executor_factory::make(expCtx,
                                                  std::move(ws),
                                                  std::move(root),
                                                  &coll,
                                                  PlanYieldPolicy::YieldPolicy::YIELD_AUTO,
                                                  QueryPlannerParams::RETURN_OWNED_DATA);
    if (!execStatus.isOK()) {
        return execStatus.getStatus();
    }

    // For sharded collections, the root of the plan tree is a TrialStage that may have chosen
    // either a random-sampling cursor trial plan or a COLLSCAN backup plan. We can only optimize
    // the $sample aggregation stage if the trial plan was chosen.
    const auto isStorageOptimizedSample = !trialStage || !trialStage->pickedBackupPlan();
    if (!bucketUnpacker) {
        if (isStorageOptimizedSample) {
            // Replace $sample stage with $sampleFromRandomCursor stage.
            pipeline->popFront();
            std::string idString = coll->ns().isOplog() ? "ts" : "_id";
            pipeline->addInitialSource(DocumentSourceSampleFromRandomCursor::create(
                expCtx, sampleSize, idString, numRecords));
        }
    } else {
        // For timeseries collections, we should remove the $_internalUnpackBucket stage which is at
        // the front of the pipeline, regardless of which plan the TrialStage has chosen. The
        // unpacking will be done by the 'UnpackTimeseriesBucket' PlanStage if the backup plan
        // (Top-K sort plan) was chosen, and by the 'SampleFromTimeseriesBucket' PlanStage if the
        // ARHASH plan was chosen.
        Pipeline::SourceContainer& sources = pipeline->_sources;
        sources.erase(sources.begin());
        // We can push down the $sample source into the PlanStage layer if the chosen strategy uses
        // ARHASH sampling on unsharded collections. For sharded collections, we cannot erase
        // $sample because we need to preserve the sort metadata (the $sortKey field) for the merge
        // cursor on mongos.
        if (isStorageOptimizedSample && !isSharded) {
            sources.erase(sources.begin());
        }
    }

    return std::move(execStatus.getValue());
}

PipelineD::BuildQueryExecutorResult PipelineD::buildInnerQueryExecutorSample(
    DocumentSourceSample* sampleStage,
    DocumentSourceInternalUnpackBucket* unpackBucketStage,
    const CollectionPtr& collection,
    Pipeline* pipeline) {
    tassert(5422105, "sampleStage cannot be a nullptr", sampleStage);

    auto expCtx = pipeline->getContext();

    const long long sampleSize = sampleStage->getSampleSize();
    const long long numRecords = collection->getRecordStore()->numRecords(expCtx->opCtx);

    boost::optional<timeseries::BucketUnpacker> bucketUnpacker;
    if (unpackBucketStage) {
        bucketUnpacker = unpackBucketStage->bucketUnpacker().copy();
    }
    auto exec = uassertStatusOK(createRandomCursorExecutor(
        collection, expCtx, pipeline, sampleSize, numRecords, std::move(bucketUnpacker)));

    AttachExecutorCallback attachExecutorCallback;
    if (exec) {
        // The order in which we evaluate these arguments is significant. We'd like to be
        // sure that the DocumentSourceCursor is created _last_, because if we run into a
        // case where a DocumentSourceCursor has been created (yet hasn't been put into a
        // Pipeline) and an exception is thrown, an invariant will trigger in the
        // DocumentSourceCursor. This is a design flaw in DocumentSourceCursor.
        auto deps = pipeline->getDependencies(DepsTracker::kAllMetadata);
        const auto cursorType = deps.hasNoRequirements()
            ? DocumentSourceCursor::CursorType::kEmptyDocuments
            : DocumentSourceCursor::CursorType::kRegular;
        attachExecutorCallback =
            [cursorType](const MultipleCollectionAccessor& collections,
                         std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> exec,
                         Pipeline* pipeline) {
                auto cursor = DocumentSourceCursor::create(
                    collections, std::move(exec), pipeline->getContext(), cursorType);
                pipeline->addInitialSource(std::move(cursor));
            };
        return {std::move(exec), std::move(attachExecutorCallback), {}};
    }
    return {nullptr, std::move(attachExecutorCallback), {}};
}

PipelineD::BuildQueryExecutorResult PipelineD::buildInnerQueryExecutor(
    const MultipleCollectionAccessor& collections,
    const NamespaceString& nss,
    const AggregateCommandRequest* aggRequest,
    Pipeline* pipeline) {
    auto expCtx = pipeline->getContext();

    // We will be modifying the source vector as we go.
    Pipeline::SourceContainer& sources = pipeline->_sources;

    // We skip the 'requiresInputDocSource' check in the case of pushing $search down into SBE,
    // as $search has 'requiresInputDocSource' as false.
    bool skipRequiresInputDocSourceCheck = isSearchPresentAndEligibleForSbe(pipeline);

    if (!skipRequiresInputDocSourceCheck && !sources.empty() &&
        !sources.front()->constraints().requiresInputDocSource) {
        return {};
    }

    if (!sources.empty()) {
        // Try to inspect if the DocumentSourceSample or a DocumentSourceInternalUnpackBucket stage
        // can be optimized for sampling backed by a storage engine supplied random cursor.
        auto&& [sampleStage, unpackBucketStage] = extractSampleUnpackBucket(sources);
        const auto& collection = collections.getMainCollection();

        // Optimize an initial $sample stage if possible.
        if (collection && sampleStage) {
            auto queryExecutors =
                buildInnerQueryExecutorSample(sampleStage, unpackBucketStage, collection, pipeline);
            if (queryExecutors.mainExecutor) {
                return queryExecutors;
            }
        }
    }

    // If the first stage is $geoNear, prepare a special DocumentSourceGeoNearCursor stage;
    // otherwise, create a generic DocumentSourceCursor.
    const auto geoNearStage =
        sources.empty() ? nullptr : dynamic_cast<DocumentSourceGeoNear*>(sources.front().get());
    if (geoNearStage) {
        return buildInnerQueryExecutorGeoNear(collections, nss, aggRequest, pipeline);
    } else if (auto& searchHelper = getSearchHelpers(expCtx->opCtx->getServiceContext());
               searchHelper->isSearchPipeline(pipeline) ||
               searchHelper->isSearchMetaPipeline(pipeline)) {
        return buildInnerQueryExecutorSearch(collections, nss, aggRequest, pipeline);
    } else {
        return buildInnerQueryExecutorGeneric(collections, nss, aggRequest, pipeline);
    }
}

void PipelineD::attachInnerQueryExecutorToPipeline(
    const MultipleCollectionAccessor& collections,
    PipelineD::AttachExecutorCallback attachExecutorCallback,
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> exec,
    Pipeline* pipeline) {
    // If the pipeline doesn't need a $cursor stage, there will be no callback function and
    // PlanExecutor provided in the 'attachExecutorCallback' object, so we don't need to do
    // anything.
    if (attachExecutorCallback && exec) {
        attachExecutorCallback(collections, std::move(exec), pipeline);
    }
}

void PipelineD::buildAndAttachInnerQueryExecutorToPipeline(
    const MultipleCollectionAccessor& collections,
    const NamespaceString& nss,
    const AggregateCommandRequest* aggRequest,
    Pipeline* pipeline) {

    auto [executor, callback, additionalExec] =
        buildInnerQueryExecutor(collections, nss, aggRequest, pipeline);
    tassert(7856010, "Unexpected additional executors", additionalExec.empty());
    attachInnerQueryExecutorToPipeline(collections, callback, std::move(executor), pipeline);
}

namespace {

/**
 * Look for $sort, $group at the beginning of the pipeline, potentially returning either or both.
 * Returns nullptr for any of the stages that are not found. Note that we are not looking for the
 * opposite pattern ($group, $sort). In that case, this function will return only the $group stage.
 *
 * This function will not return the $group in the case that there is an initial $sort with
 * intermediate stages that separate it from the $group (e.g.: $sort, $limit, $group). That includes
 * the case of a $sort with a non-null value for getLimitSrc(), indicating that there was previously
 * a $limit stage that was optimized away.
 */
std::pair<boost::intrusive_ptr<DocumentSourceSort>, boost::intrusive_ptr<DocumentSourceGroupBase>>
getSortAndGroupStagesFromPipeline(const Pipeline::SourceContainer& sources) {
    boost::intrusive_ptr<DocumentSourceSort> sortStage = nullptr;
    boost::intrusive_ptr<DocumentSourceGroupBase> groupStage = nullptr;

    auto sourcesIt = sources.begin();
    if (sourcesIt != sources.end()) {
        sortStage = dynamic_cast<DocumentSourceSort*>(sourcesIt->get());
        if (sortStage) {
            if (!sortStage->hasLimit()) {
                ++sourcesIt;
            } else {
                // This $sort stage was previously followed by a $limit stage.
                sourcesIt = sources.end();
            }
        }
    }

    if (sourcesIt != sources.end()) {
        groupStage = dynamic_cast<DocumentSourceGroupBase*>(sourcesIt->get());
    }

    return std::make_pair(sortStage, groupStage);
}

boost::optional<long long> extractSkipForPushdown(Pipeline* pipeline) {
    // If the disablePipelineOptimization failpoint is enabled, then do not attempt the skip
    // pushdown optimization.
    if (MONGO_unlikely(disablePipelineOptimization.shouldFail())) {
        return boost::none;
    }
    auto&& sources = pipeline->getSources();

    auto skip = extractSkipForPushdown(sources.begin(), &sources);
    if (skip) {
        // Removing stages may have produced the opportunity for additional optimizations.
        pipeline->optimizePipeline();
    }
    return skip;
}

SkipThenLimit extractSkipAndLimitForPushdown(Pipeline* pipeline) {
    // If the disablePipelineOptimization failpoint is enabled, then do not attempt the limit and
    // skip pushdown optimization.
    if (MONGO_unlikely(disablePipelineOptimization.shouldFail())) {
        return {boost::none, boost::none};
    }
    auto&& sources = pipeline->getSources();

    // It is important to call 'extractLimitForPushdown' before 'extractSkipForPushdown'. Otherwise
    // there could be a situation when $limit stages in pipeline would prevent
    // 'extractSkipForPushdown' from extracting all $skip stages.
    auto limit = extractLimitForPushdown(sources.begin(), &sources);
    auto skip = extractSkipForPushdown(sources.begin(), &sources);
    auto skipThenLimit = LimitThenSkip(limit, skip).flip();
    if (skipThenLimit.getSkip() || skipThenLimit.getLimit()) {
        // Removing stages may have produced the opportunity for additional optimizations.
        pipeline->optimizePipeline();
    }
    return skipThenLimit;
}

/**
 * Given a dependency set and a pipeline, builds a projection BSON object to push down into the
 * PlanStage layer. The rules to push down the projection are as follows:
 *    1. If there is an inclusion projection at the front of the pipeline, it will be pushed down
 *       as is.
 *    2. If there is no inclusion projection at the front of the pipeline, but there is a finite
 *       dependency set, a projection representing this dependency set will be pushed down.
 *    3. If there is an exclusion projection at the front of the pipeline, it will be pushed down.
 *    4. Otherwise, an empty projection is returned and no projection push down will happen.
 *
 * If 'allowExpressions' is true, the returned projection may include expressions (which can only
 * happen in case 1). If 'allowExpressions' is false and the projection we find has expressions,
 * then we fall through to case 2 and attempt to push down a pure-inclusion projection based on its
 * dependencies.
 *
 * If 'timeseriesBoundedSortOptimization' is true, an exclusion projection won't be pushed down,
 * because it breaks PlanExecutorImpl analysis required to enable this optimization.
 */
auto buildProjectionForPushdown(const DepsTracker& deps,
                                Pipeline* pipeline,
                                bool allowExpressions,
                                bool timeseriesBoundedSortOptimization) {
    auto&& sources = pipeline->getSources();

    // Short-circuit if the pipeline is empty: there is no projection and nothing to push down.
    if (sources.empty()) {
        return BSONObj();
    }

    const auto projStage =
        exact_pointer_cast<DocumentSourceSingleDocumentTransformation*>(sources.front().get());
    const auto getProjectionObj = [&]() {
        return projStage->getTransformer().serializeTransformation(boost::none).toBson();
    };
    const auto parseProjection = [&](const BSONObj& projObj) {
        return projection_ast::parseAndAnalyze(
            projStage->getContext(), projObj, ProjectionPolicies::aggregateProjectionPolicies());
    };

    // If there is an inclusion projection at the front of the pipeline, we have case 1.
    if (projStage &&
        projStage->getType() == TransformerInterface::TransformerType::kInclusionProjection) {
        auto projObj = getProjectionObj();
        if (allowExpressions || !parseProjection(projObj).hasExpressions()) {
            sources.pop_front();
            return projObj;
        }
    }

    // If there is a finite dependency set, return a projection representing this dependency set.
    // This is case 2.
    if (!deps.getNeedsAnyMetadata()) {
        BSONObj depsProjObj = deps.toProjectionWithoutMetadata();
        if (!depsProjObj.isEmpty()) {
            return depsProjObj;
        }
    }

    // If there is an exclusion projection at the front of the pipeline, we have case 3.
    if (projStage &&
        projStage->getType() == TransformerInterface::TransformerType::kExclusionProjection &&
        // TODO SERVER-70655: Remove this check and argument when it is no longer needed.
        !timeseriesBoundedSortOptimization) {
        auto projObj = getProjectionObj();
        if (allowExpressions || !parseProjection(projObj).hasExpressions()) {
            sources.pop_front();
            return projObj;
        }
    }

    // Case 4: no projection to push down
    return BSONObj();
}
}  // namespace

boost::optional<std::pair<PipelineD::IndexSortOrderAgree, PipelineD::IndexOrderedByMinTime>>
PipelineD::supportsSort(const timeseries::BucketUnpacker& bucketUnpacker,
                        PlanStage* root,
                        const SortPattern& sort) {
    using SortPatternPart = SortPattern::SortPatternPart;

    if (!root)
        return boost::none;

    switch (root->stageType()) {
        case STAGE_COLLSCAN: {
            const CollectionScan* scan = static_cast<CollectionScan*>(root);
            if (sort.size() == 1) {
                auto part = sort[0];
                // Check the sort we're asking for is on time, and that the buckets are actually
                // ordered on time.
                if (part.fieldPath && *part.fieldPath == bucketUnpacker.getTimeField() &&
                    !bucketUnpacker.bucketSpec().usesExtendedRange()) {
                    // Check that the directions agree.
                    if ((scan->getDirection() == CollectionScanParams::Direction::FORWARD) ==
                        part.isAscending)
                        return std::pair{part.isAscending, true};
                }
            }
            return boost::none;
        }
        case STAGE_IXSCAN: {
            const IndexScan* scan = static_cast<IndexScan*>(root);

            // Scanning only part of an index means we don't see all the index keys for a
            // document, which means the representative (first key we encounter, for a
            // given document) will be different. For simplicity, just check whether the
            // index is multikey. Mabye we could do better by looking at whether each field
            // separately is multikey, or by allowing a full index scan.
            if (scan->getSpecificStats()->isMultiKey)
                return boost::none;

            const auto& keyPattern = scan->getKeyPattern();

            const auto& time = bucketUnpacker.getTimeField();
            const auto& controlMinTime = bucketUnpacker.getMinField(time);
            const auto& controlMaxTime = bucketUnpacker.getMaxField(time);

            auto directionCompatible = [&](const BSONElement& keyPatternComponent,
                                           const SortPatternPart& sortComponent) -> bool {
                // The index component must not be special.
                if (!keyPatternComponent.isNumber() || abs(keyPatternComponent.numberInt()) != 1)
                    return false;
                // Is the index (as it is stored) ascending or descending on this field?
                const bool indexIsAscending = keyPatternComponent.numberInt() == 1;
                // Does the index scan produce this field in ascending or descending order?
                // For example: a backwards scan of a descending index produces ascending data.
                const bool scanIsAscending = scan->isForward() == indexIsAscending;
                return scanIsAscending == sortComponent.isAscending;
            };

            // Return none if the keyPattern cannot support the sort.

            // Compare the requested 'sort' against the index 'keyPattern' one field at a time.
            // - If the leading fields are compatible, keep comparing.
            // - If the leading field of the index has a point predicate, ignore it.
            // - If we reach the end of the sort first, success!
            // - if we find a field of the sort that the index can't satisfy, fail.

            auto keyPatternIter = scan->getKeyPattern().begin();
            auto sortIter = sort.begin();
            for (;;) {
                if (sortIter == sort.end()) {
                    // We never found a 'time' field in the sort.
                    return boost::none;
                }
                if (keyPatternIter == keyPattern.end()) {
                    // There are still components of the sort, that the index key didn't satisfy.
                    return boost::none;
                }
                if (!sortIter->fieldPath) {
                    // We don't handle special $meta sort.
                    return boost::none;
                }

                // Does the leading sort field match the index?

                if (sortAndKeyPatternPartAgreeAndOnMeta(bucketUnpacker,
                                                        keyPatternIter->fieldNameStringData(),
                                                        *sortIter->fieldPath)) {
                    if (!directionCompatible(*keyPatternIter, *sortIter))
                        return boost::none;

                    // No conflict. Continue comparing the index vs the sort.
                    ++keyPatternIter;
                    ++sortIter;
                    continue;
                }

                // Does this index field have a point predicate?
                auto hasPointPredicate = [&](StringData fieldName) -> bool {
                    for (auto&& field : scan->getBounds().fields) {
                        if (field.name == fieldName)
                            return field.isPoint();
                    }
                    return false;
                };
                if (hasPointPredicate(keyPatternIter->fieldNameStringData())) {
                    ++keyPatternIter;
                    continue;
                }

                if ((*sortIter->fieldPath) == time) {
                    // We require the 'time' field to be the last component of the sort.
                    // (It's fine if the index has additional fields; we just ignore those.)
                    if (std::next(sortIter) != sort.end())
                        return boost::none;

                    // Now any of the following index fields can satisfy a sort on time:
                    // - control.min.time
                    // - control.max.time
                    // - _id  (like control.min.time but may break ties)
                    // as long as the direction matches.
                    // However, it's not possible for users to index the bucket _id (unless they
                    // bypass the view), so don't bother optimizing that case.
                    auto&& ixField = keyPatternIter->fieldNameStringData();
                    if (ixField != controlMinTime && ixField != controlMaxTime)
                        return boost::none;

                    // If we've inserted a date before 1-1-1970, we round the min up towards 1970,
                    // rather then down, which has the effect of increasing the control.min.t.
                    // This means the minimum time in the bucket is likely to be lower than
                    // indicated and thus, actual dates may be out of order relative to what's
                    // indicated by the bucket bounds.
                    if (ixField == controlMinTime &&
                        bucketUnpacker.bucketSpec().usesExtendedRange())
                        return boost::none;

                    if (!directionCompatible(*keyPatternIter, *sortIter))
                        return boost::none;

                    // Success! Every field of the sort can be satisfied by a field of the index.

                    // Now the caller wants to know:
                    // 1. Does the field in the index agree with the scan direction?
                    //    An index on 'control.min.time' or '_id' is better for ascending.
                    //    An index on 'control.max.time' is better for descending.
                    // 2. Which field was first? min or max (treating _id the same as min).
                    const bool isMinFirst = keyPatternIter->fieldNameStringData() != controlMaxTime;
                    const bool indexOrderAgree = isMinFirst == sortIter->isAscending;
                    return {{indexOrderAgree, isMinFirst}};
                }

                // This index field can't satisfy this sort field.
                return boost::none;
            }
        }
        default:
            return boost::none;
    }
}  // namespace mongo

boost::optional<std::pair<PipelineD::IndexSortOrderAgree, PipelineD::IndexOrderedByMinTime>>
PipelineD::checkTimeHelper(const timeseries::BucketUnpacker& bucketUnpacker,
                           BSONObj::iterator& keyPatternIter,
                           bool scanIsForward,
                           const FieldPath& timeSortFieldPath,
                           bool sortIsAscending) {
    bool wasMin = false;
    bool wasMax = false;

    // Check that the index isn't special.
    if ((*keyPatternIter).isNumber() && abs((*keyPatternIter).numberInt()) == 1) {
        bool direction = ((*keyPatternIter).numberInt() == 1);
        direction = (scanIsForward) ? direction : !direction;

        // Verify the direction and fieldNames match.
        wasMin = ((*keyPatternIter).fieldName() ==
                  bucketUnpacker.getMinField(timeSortFieldPath.fullPath()));
        wasMax = ((*keyPatternIter).fieldName() ==
                  bucketUnpacker.getMaxField(timeSortFieldPath.fullPath()));
        // Terminate early if it wasn't max or min or if the directions don't match.
        if ((wasMin || wasMax) && (sortIsAscending == direction))
            return std::pair{wasMin ? sortIsAscending : !sortIsAscending, wasMin};
    }

    return boost::none;
}

bool PipelineD::sortAndKeyPatternPartAgreeAndOnMeta(
    const timeseries::BucketUnpacker& bucketUnpacker,
    StringData keyPatternFieldName,
    const FieldPath& sortFieldPath) {
    FieldPath keyPatternFieldPath = FieldPath(keyPatternFieldName);

    // If they don't have the same path length they cannot agree.
    if (keyPatternFieldPath.getPathLength() != sortFieldPath.getPathLength())
        return false;

    // Check these paths are on the meta field.
    if (keyPatternFieldPath.getSubpath(0) != mongo::timeseries::kBucketMetaFieldName)
        return false;
    if (!bucketUnpacker.getMetaField() ||
        sortFieldPath.getSubpath(0) != *bucketUnpacker.getMetaField()) {
        return false;
    }

    // If meta was the only path component then return true.
    // Note: We already checked that the path lengths are equal.
    if (keyPatternFieldPath.getPathLength() == 1)
        return true;

    // Otherwise return if the remaining path components are equal.
    return (keyPatternFieldPath.tail() == sortFieldPath.tail());
}

boost::optional<TraversalPreference> createTimeSeriesTraversalPreference(
    DocumentSourceInternalUnpackBucket* unpack, DocumentSourceSort* sort) {
    const auto metaField = unpack->bucketUnpacker().getMetaField();
    BSONObjBuilder builder;
    // Reverse the sort pattern so we can look for indexes that match.
    for (const auto& sortPart : sort->getSortKeyPattern()) {
        if (!sortPart.fieldPath) {
            return boost::none;
        }
        const int reversedDirection = sortPart.isAscending ? -1 : 1;
        const auto& path = sortPart.fieldPath->fullPath();
        if (metaField.has_value() &&
            (expression::isPathPrefixOf(*metaField, path) || *metaField == path)) {
            std::string rewrittenField =
                std::string{timeseries::kBucketMetaFieldName} + path.substr(metaField->size());
            builder.append(rewrittenField, reversedDirection);
        } else if (path == unpack->bucketUnpacker().getTimeField()) {
            if (reversedDirection == 1) {
                builder.append(unpack->bucketUnpacker().getMinField(path), reversedDirection);
            } else {
                builder.append(unpack->bucketUnpacker().getMaxField(path), reversedDirection);
            }
        } else {
            // The field wasn't meta or time, so no direction preference should be made.
            return boost::none;
        }
    }

    TraversalPreference traversalPreference;
    traversalPreference.sortPattern = builder.obj();
    traversalPreference.clusterField = unpack->getMinTimeField();
    traversalPreference.direction = -1;
    return traversalPreference;
}

PipelineD::BuildQueryExecutorResult PipelineD::buildInnerQueryExecutorSearch(
    const MultipleCollectionAccessor& collections,
    const NamespaceString& nss,
    const AggregateCommandRequest* aggRequest,
    Pipeline* pipeline) {
    uassert(7856009,
            "Cannot have exchange specified in a $search pipeline",
            !aggRequest || !aggRequest->getExchange());

    auto expCtx = pipeline->getContext();
    auto& searchHelper = getSearchHelpers(expCtx->opCtx->getServiceContext());

    DocumentSource* searchStage = pipeline->peekFront();
    auto yieldPolicy = PlanYieldPolicyRemoteCursor::make(
        expCtx->opCtx, PlanYieldPolicy::YieldPolicy::YIELD_AUTO, collections, nss);

    if (!expCtx->explain) {
        if (searchHelper->isSearchPipeline(pipeline)) {
            searchHelper->establishSearchQueryCursors(expCtx, searchStage, std::move(yieldPolicy));
        } else if (searchHelper->isSearchMetaPipeline(pipeline)) {
            searchHelper->establishSearchMetaCursor(expCtx, searchStage, std::move(yieldPolicy));
        } else {
            tasserted(7856008, "Not search pipeline in buildInnerQueryExecutorSearch");
        }
    }

    auto [executor, callback, additionalExecutors] =
        buildInnerQueryExecutorGeneric(collections, nss, aggRequest, pipeline);

    const CanonicalQuery* cq = executor->getCanonicalQuery();

    if (!cq->cqPipeline().empty() &&
        searchHelper->isSearchStage(cq->cqPipeline().front()->documentSource())) {
        // The $search is pushed down into SBE executor.
        if (auto cursor = searchHelper->getSearchMetadataCursor(searchStage)) {
            // Create a yield policy for metadata cursor.
            auto metadataYieldPolicy = PlanYieldPolicyRemoteCursor::make(
                expCtx->opCtx, PlanYieldPolicy::YieldPolicy::YIELD_AUTO, collections, nss);
            cursor->updateYieldPolicy(std::move(metadataYieldPolicy));

            additionalExecutors.push_back(uassertStatusOK(getSearchMetadataExecutorSBE(
                expCtx->opCtx, collections, nss, *cq, std::move(*cursor))));
        }
    }
    return {std::move(executor), callback, std::move(additionalExecutors)};
}

PipelineD::BuildQueryExecutorResult PipelineD::buildInnerQueryExecutorGeneric(
    const MultipleCollectionAccessor& collections,
    const NamespaceString& nss,
    const AggregateCommandRequest* aggRequest,
    Pipeline* pipeline) {
    // Make a last effort to optimize pipeline stages before potentially detaching them to be
    // pushed down into the query executor.
    pipeline->optimizePipeline();

    Pipeline::SourceContainer& sources = pipeline->_sources;
    auto expCtx = pipeline->getContext();

    // Look for an initial match. This works whether we got an initial query or not. If not, it
    // results in a "{}" query, which will be what we want in that case.
    const BSONObj queryObj = pipeline->getInitialQuery();
    if (!queryObj.isEmpty()) {
        auto matchStage = dynamic_cast<DocumentSourceMatch*>(sources.front().get());
        if (matchStage) {
            // If a $match query is pulled into the cursor, the $match is redundant, and can be
            // removed from the pipeline.
            sources.pop_front();
        } else {
            // A $geoNear stage, the only other stage that can produce an initial query, is also
            // a valid initial stage. However, we should be in prepareGeoNearCursorSource() instead.
            MONGO_UNREACHABLE;
        }
    }

    auto&& [sortStage, groupStage] = getSortAndGroupStagesFromPipeline(pipeline->_sources);
    std::unique_ptr<GroupFromFirstDocumentTransformation> rewrittenGroupStage;
    if (groupStage) {
        rewrittenGroupStage = groupStage->rewriteGroupAsTransformOnFirstDocument();
    }

    // If there is a $limit or $skip stage (or multiple of them) that could be pushed down into the
    // PlanStage layer, obtain the value of the limit and skip and remove the $limit and $skip
    // stages from the pipeline.
    //
    // This analysis is done here rather than in 'optimizePipeline()' because swapping $limit before
    // stages such as $project is not always useful, and can sometimes defeat other optimizations.
    // In particular, in a sharded scenario a pipeline such as [$project, $limit] is preferable to
    // [$limit, $project]. The former permits the execution of the projection operation to be
    // parallelized across all targeted shards, whereas the latter would bring all of the data to a
    // merging shard first, and then apply the projection serially. See SERVER-24981 for a more
    // detailed discussion.
    //
    // This only handles the case in which the the $limit or $skip can logically be swapped to the
    // front of the pipeline. We can also push down a $limit which comes after a $sort into the
    // PlanStage layer, but that is handled elsewhere.
    const auto skipThenLimit = extractSkipAndLimitForPushdown(pipeline);

    auto unavailableMetadata = DocumentSourceMatch::isTextQuery(queryObj)
        ? DepsTracker::kDefaultUnavailableMetadata & ~DepsTracker::kOnlyTextScore
        : DepsTracker::kDefaultUnavailableMetadata;

    // If this is a query on a time-series collection we might need to keep it fully classic to
    // ensure no perf regressions until we implement the corresponding scenarios fully in SBE.
    SortAndUnpackInPipeline su = findUnpackAndSort(pipeline->_sources);
    // Do not double-optimize the sort.
    auto sort = (su.sort && su.sort->isBoundedSortStage()) ? nullptr : su.sort;
    auto unpack = su.unpack;
    if (unpack && !unpack->isSbeCompatible()) {
        expCtx->sbePipelineCompatibility = SbeCompatibility::notCompatible;
    }

    // But in classic it may be eligible for a post-planning sort optimization. We check eligibility
    // and perform the rewrite here.
    const bool timeseriesBoundedSortOptimization = unpack && sort && (su.unpackIdx < su.sortIdx);
    QueryPlannerParams plannerOpts;
    if (timeseriesBoundedSortOptimization) {
        plannerOpts.traversalPreference = createTimeSeriesTraversalPreference(unpack, sort);

        // Whether to use bounded sort or not is determined _after_ the executor is created, based
        // on whether the chosen collection access stage would support it. Because bounded sort and
        // streaming group aren't implemented in SBE yet we have to block the whole pipeline from
        // lowering to SBE so that it has the chance of doing the optimization. To allow as many
        // sort + group pipelines over time-series to lower to SBE we'll only block those that sort
        // on time as these are the only ones that _might_ end up using bounded sort.
        // Note: This check (sort on time after unpacking) also disables the streaming group
        // optimization, that might happen w/o bounded sort.
        for (const auto& sortKey : sort->getSortKeyPattern()) {
            if (sortKey.fieldPath &&
                *(sortKey.fieldPath) == unpack->bucketUnpacker().getTimeField()) {
                expCtx->sbePipelineCompatibility = SbeCompatibility::notCompatible;
                break;
            }
        }
    }

    // Create the PlanExecutor.
    bool shouldProduceEmptyDocs = false;
    auto exec = uassertStatusOK(prepareExecutor(expCtx,
                                                collections,
                                                nss,
                                                pipeline,
                                                sortStage,
                                                std::move(rewrittenGroupStage),
                                                unavailableMetadata,
                                                queryObj,
                                                skipThenLimit,
                                                aggRequest,
                                                Pipeline::kAllowedMatcherFeatures,
                                                &shouldProduceEmptyDocs,
                                                timeseriesBoundedSortOptimization,
                                                std::move(plannerOpts)));

    // If this is a query on a time-series collection then it may be eligible for a post-planning
    // sort optimization. We check eligibility and perform the rewrite here.
    if (timeseriesBoundedSortOptimization) {
        auto execImpl = dynamic_cast<PlanExecutorImpl*>(exec.get());
        if (execImpl) {
            // Get source stage
            PlanStage* rootStage = execImpl->getRootStage();
            while (rootStage &&
                   (rootStage->getChildren().size() == 1 ||
                    rootStage->stageType() == STAGE_MULTI_PLAN)) {
                switch (rootStage->stageType()) {
                    case STAGE_FETCH:
                        rootStage = rootStage->child().get();
                        break;
                    case STAGE_SHARDING_FILTER:
                        rootStage = rootStage->child().get();
                        break;
                    case STAGE_MULTI_PLAN: {
                        auto mps = static_cast<MultiPlanStage*>(rootStage);
                        if (mps->bestPlanChosen() && mps->bestPlanIdx()) {
                            rootStage = (mps->getChildren())[*(mps->bestPlanIdx())].get();
                        } else {
                            rootStage = nullptr;
                            tasserted(6655801,
                                      "Expected multiplanner to have selected a bestPlan.");
                        }
                        break;
                    }
                    case STAGE_CACHED_PLAN: {
                        auto cp = static_cast<CachedPlanStage*>(rootStage);
                        if (cp->bestPlanChosen()) {
                            rootStage = rootStage->child().get();
                        } else {
                            rootStage = nullptr;
                            tasserted(6655802, "Expected cached plan to have selected a bestPlan.");
                        }
                        break;
                    }
                    default:
                        rootStage = nullptr;
                }
            }

            if (rootStage && rootStage->getChildren().size() != 0) {
                rootStage = nullptr;
            }

            const auto& sortPattern = sort->getSortKeyPattern();
            if (auto agree = supportsSort(unpack->bucketUnpacker(), rootStage, sortPattern)) {
                // Scan the pipeline to check if it's compatible with the  optimization.
                bool badStage = false;
                bool seenSort = false;
                bool seenUnpack = false;
                std::list<boost::intrusive_ptr<DocumentSource>>::iterator iter =
                    pipeline->_sources.begin();
                std::list<boost::intrusive_ptr<DocumentSource>>::iterator unpackIter =
                    pipeline->_sources.end();
                for (; !badStage && iter != pipeline->_sources.end() && !seenSort; ++iter) {
                    if (dynamic_cast<const DocumentSourceSort*>(iter->get())) {
                        seenSort = true;
                    } else if (dynamic_cast<const DocumentSourceMatch*>(iter->get())) {
                        // do nothing
                    } else if (const auto* unpack =
                                   dynamic_cast<const DocumentSourceInternalUnpackBucket*>(
                                       iter->get())) {
                        unpackIter = iter;
                        tassert(6505001,
                                str::stream()
                                    << "Expected at most one "
                                    << DocumentSourceInternalUnpackBucket::kStageNameInternal
                                    << " stage in the pipeline",
                                !seenUnpack);
                        seenUnpack = true;

                        // Check that the time field is preserved.
                        if (!unpack->includeTimeField())
                            badStage = true;

                        // If the sort is compound, check that the entire meta field is preserved.
                        if (sortPattern.size() > 1) {
                            // - Is there a meta field?
                            // - Will it be unpacked?
                            // - Will it be overwritten by 'computedMetaProjFields'?
                            auto&& unpacker = unpack->bucketUnpacker();
                            const boost::optional<std::string>& metaField = unpacker.getMetaField();
                            if (!metaField || !unpack->includeMetaField() ||
                                unpacker.bucketSpec().fieldIsComputed(*metaField)) {
                                badStage = true;
                            }
                        }
                    } else if (auto projection =
                                   dynamic_cast<const DocumentSourceSingleDocumentTransformation*>(
                                       iter->get())) {
                        auto modPaths = projection->getModifiedPaths();
                        if (areSortFieldsModifiedByProjection(seenUnpack, sortPattern, modPaths)) {
                            badStage = true;
                        }
                    } else {
                        badStage = true;
                    }
                }
                if (!badStage && seenSort) {
                    auto [indexSortOrderAgree, indexOrderedByMinTime] = *agree;
                    // This is safe because we have seen a sort so we must have at least one stage
                    // to the left of the current iterator position.
                    --iter;

                    if (indexOrderedByMinTime) {
                        unpack->setIncludeMinTimeAsMetadata();
                    } else {
                        unpack->setIncludeMaxTimeAsMetadata();
                    }

                    if (indexSortOrderAgree) {
                        pipeline->_sources.insert(
                            iter,
                            DocumentSourceSort::createBoundedSort(sort->getSortKeyPattern(),
                                                                  (indexOrderedByMinTime
                                                                       ? DocumentSourceSort::kMin
                                                                       : DocumentSourceSort::kMax),
                                                                  0,
                                                                  sort->getLimit(),
                                                                  expCtx));
                    } else {
                        // Since the sortPattern and the direction of the index don't agree we must
                        // use the offset to get an estimate on the bounds of the bucket.
                        pipeline->_sources.insert(
                            iter,
                            DocumentSourceSort::createBoundedSort(
                                sort->getSortKeyPattern(),
                                (indexOrderedByMinTime ? DocumentSourceSort::kMin
                                                       : DocumentSourceSort::kMax),
                                static_cast<long long>((indexOrderedByMinTime)
                                                           ? unpack->getBucketMaxSpanSeconds()
                                                           : -unpack->getBucketMaxSpanSeconds()) *
                                    1000,
                                sort->getLimit(),
                                expCtx));

                        /**
                         * We wish to create the following predicate to avoid returning incorrect
                         * results in the unlikely event bucketMaxSpanSeconds changes under us.
                         *
                         * {$expr:
                         *   {$lte: [
                         *     {$subtract: [$control.max.timeField, $control.min.timeField]},
                         *     {$const: bucketMaxSpanSeconds, in milliseconds}
                         * ]}}
                         */
                        auto minTime = unpack->getMinTimeField();
                        auto maxTime = unpack->getMaxTimeField();
                        auto match = std::make_unique<ExprMatchExpression>(
                            // This produces {$lte: ... }
                            make_intrusive<ExpressionCompare>(
                                expCtx.get(),
                                ExpressionCompare::CmpOp::LTE,
                                // This produces [...]
                                makeVector<boost::intrusive_ptr<Expression>>(
                                    // This produces {$subtract: ... }
                                    make_intrusive<ExpressionSubtract>(
                                        expCtx.get(),
                                        // This produces [...]
                                        makeVector<boost::intrusive_ptr<Expression>>(
                                            // This produces "$control.max.timeField"
                                            ExpressionFieldPath::createPathFromString(
                                                expCtx.get(), maxTime, expCtx->variablesParseState),
                                            // This produces "$control.min.timeField"
                                            ExpressionFieldPath::createPathFromString(
                                                expCtx.get(),
                                                minTime,
                                                expCtx->variablesParseState))),
                                    // This produces {$const: maxBucketSpanSeconds}
                                    make_intrusive<ExpressionConstant>(
                                        expCtx.get(),
                                        Value{static_cast<long long>(
                                                  unpack->getBucketMaxSpanSeconds()) *
                                              1000}))),
                            expCtx);
                        pipeline->_sources.insert(
                            unpackIter,
                            make_intrusive<DocumentSourceMatch>(std::move(match), expCtx));
                    }
                    // Ensure we're erasing the sort source.
                    tassert(6434901,
                            "we must erase a $sort stage and replace it with a bounded sort stage",
                            strcmp((*iter)->getSourceName(),
                                   DocumentSourceSort::kStageName.rawData()) == 0);
                    pipeline->_sources.erase(iter);
                    pipeline->stitch();
                }
            }
        }
    }

    const auto cursorType = shouldProduceEmptyDocs
        ? DocumentSourceCursor::CursorType::kEmptyDocuments
        : DocumentSourceCursor::CursorType::kRegular;

    // If this is a change stream pipeline or a resharding resume token has been requested, make
    // sure that we tell DSCursor to track the oplog time.
    const bool trackOplogTS =
        (pipeline->peekFront() && pipeline->peekFront()->constraints().isChangeStreamStage()) ||
        (aggRequest && aggRequest->getRequestReshardingResumeToken());

    auto resumeTrackingType = DocumentSourceCursor::ResumeTrackingType::kNone;
    if (trackOplogTS) {
        resumeTrackingType = DocumentSourceCursor::ResumeTrackingType::kOplog;
    } else if (aggRequest && aggRequest->getRequestResumeToken()) {
        resumeTrackingType = DocumentSourceCursor::ResumeTrackingType::kNonOplog;
    }

    auto attachExecutorCallback = [cursorType, resumeTrackingType](
                                      const MultipleCollectionAccessor& collections,
                                      std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> exec,
                                      Pipeline* pipeline) {
        auto cursor = DocumentSourceCursor::create(
            collections, std::move(exec), pipeline->getContext(), cursorType, resumeTrackingType);
        pipeline->addInitialSource(std::move(cursor));
    };
    return {std::move(exec), std::move(attachExecutorCallback), {}};
}

PipelineD::BuildQueryExecutorResult PipelineD::buildInnerQueryExecutorGeoNear(
    const MultipleCollectionAccessor& collections,
    const NamespaceString& nss,
    const AggregateCommandRequest* aggRequest,
    Pipeline* pipeline) {
    // $geoNear can only run over the main collection.
    const auto& collection = collections.getMainCollection();
    uassert(ErrorCodes::NamespaceNotFound,
            str::stream() << "$geoNear requires a geo index to run, but "
                          << nss.toStringForErrorMsg() << " does not exist",
            collection);

    Pipeline::SourceContainer& sources = pipeline->_sources;
    auto expCtx = pipeline->getContext();
    const auto geoNearStage = dynamic_cast<DocumentSourceGeoNear*>(sources.front().get());
    invariant(geoNearStage);

    // If the user specified a "key" field, use that field to satisfy the "near" query. Otherwise,
    // look for a geo-indexed field in 'collection' that can.
    auto nearFieldName =
        (geoNearStage->getKeyField() ? geoNearStage->getKeyField()->fullPath()
                                     : extractGeoNearFieldFromIndexes(expCtx->opCtx, collection))
            .toString();

    // Create a PlanExecutor whose query is the "near" predicate on 'nearFieldName' combined with
    // the optional "query" argument in the $geoNear stage.
    BSONObj fullQuery = geoNearStage->asNearQuery(nearFieldName);

    bool shouldProduceEmptyDocs = false;
    auto exec = uassertStatusOK(
        prepareExecutor(expCtx,
                        collections,
                        nss,
                        pipeline,
                        nullptr, /* sortStage */
                        nullptr, /* rewrittenGroupStage */
                        DepsTracker::kDefaultUnavailableMetadata & ~DepsTracker::kAllGeoNearData,
                        fullQuery,
                        SkipThenLimit{boost::none, boost::none},
                        aggRequest,
                        Pipeline::kGeoNearMatcherFeatures,
                        &shouldProduceEmptyDocs,
                        false /* timeseriesBoundedSortOptimization */));

    auto attachExecutorCallback = [distanceField = geoNearStage->getDistanceField(),
                                   locationField = geoNearStage->getLocationField(),
                                   distanceMultiplier =
                                       geoNearStage->getDistanceMultiplier().value_or(1.0)](
                                      const MultipleCollectionAccessor& collections,
                                      std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> exec,
                                      Pipeline* pipeline) {
        auto cursor = DocumentSourceGeoNearCursor::create(collections,
                                                          std::move(exec),
                                                          pipeline->getContext(),
                                                          distanceField,
                                                          locationField,
                                                          distanceMultiplier);
        pipeline->addInitialSource(std::move(cursor));
    };
    // Remove the initial $geoNear; it will be replaced by $geoNearCursor.
    sources.pop_front();
    return {std::move(exec), std::move(attachExecutorCallback), {}};
}

StatusWith<std::unique_ptr<PlanExecutor, PlanExecutor::Deleter>> PipelineD::prepareExecutor(
    const intrusive_ptr<ExpressionContext>& expCtx,
    const MultipleCollectionAccessor& collections,
    const NamespaceString& nss,
    Pipeline* pipeline,
    const boost::intrusive_ptr<DocumentSourceSort>& sortStage,
    std::unique_ptr<GroupFromFirstDocumentTransformation> rewrittenGroupStage,
    QueryMetadataBitSet unavailableMetadata,
    const BSONObj& queryObj,
    SkipThenLimit skipThenLimit,
    const AggregateCommandRequest* aggRequest,
    const MatchExpressionParser::AllowedFeatureSet& matcherFeatures,
    bool* shouldProduceEmptyDocs,
    bool timeseriesBoundedSortOptimization,
    QueryPlannerParams plannerOpts) {
    invariant(shouldProduceEmptyDocs);

    bool isChangeStream =
        pipeline->peekFront() && pipeline->peekFront()->constraints().isChangeStreamStage();
    if (isChangeStream) {
        invariant(expCtx->tailableMode == TailableModeEnum::kTailableAndAwaitData);
        plannerOpts.options |= (QueryPlannerParams::TRACK_LATEST_OPLOG_TS |
                                QueryPlannerParams::ASSERT_MIN_TS_HAS_NOT_FALLEN_OFF_OPLOG);
    }

    // The $_requestReshardingResumeToken parameter is only valid for an oplog scan.
    if (aggRequest && aggRequest->getRequestReshardingResumeToken()) {
        plannerOpts.options |= (QueryPlannerParams::TRACK_LATEST_OPLOG_TS |
                                QueryPlannerParams::ASSERT_MIN_TS_HAS_NOT_FALLEN_OFF_OPLOG);
    }

    // If there is a sort stage eligible for pushdown, serialize its SortPattern to a BSONObj. The
    // BSONObj format is currently necessary to request that the sort is computed by the query layer
    // inside the inner PlanExecutor. We also remove the $sort stage from the Pipeline, since it
    // will be handled instead by PlanStage execution.
    BSONObj sortObj;
    if (sortStage) {
        sortObj = sortStage->getSortKeyPattern()
                      .serialize(SortPattern::SortKeySerialization::kForPipelineSerialization)
                      .toBson();

        pipeline->popFrontWithName(DocumentSourceSort::kStageName);

        // Now that we've pushed down the sort, see if there is a $limit and $skip to push down
        // also. We should not already have a limit or skip here, otherwise it would be incorrect
        // for the caller to pass us a sort stage to push down, since the order matters.
        invariant(!skipThenLimit.getLimit());
        invariant(!skipThenLimit.getSkip());

        // Since all $limit stages were already pushdowned to the sort stage, we are only looking
        // for $skip stages.
        auto skip = extractSkipForPushdown(pipeline);

        // Since the limit from $sort is going before the extracted $skip stages, we construct
        // 'LimitThenSkip' object and then convert it 'SkipThenLimit'.
        skipThenLimit = LimitThenSkip(sortStage->getLimit(), skip).flip();
    }

    // Perform dependency analysis. In order to minimize the dependency set, we only analyze the
    // stages that remain in the pipeline after pushdown. In particular, any dependencies for a
    // $match or $sort pushed down into the query layer will not be reflected here.
    auto deps = pipeline->getDependencies(unavailableMetadata);
    *shouldProduceEmptyDocs = deps.hasNoRequirements();

    BSONObj projObj;
    if (!*shouldProduceEmptyDocs) {
        // Build a BSONObj representing a projection eligible for pushdown. If there is an inclusion
        // projection at the front of the pipeline, it will be removed and handled by the PlanStage
        // layer. If a projection cannot be pushed down, an empty BSONObj will be returned.

        // In most cases .find() behaves as if it evaluates in a predictable order:
        //     predicate, sort, skip, limit, projection.
        // But there is at least one case where it runs the projection before the sort/skip/limit:
        // when the predicate has a rooted $or.  (In that case we plan each branch of the $or
        // separately, using Subplan, and include the projection on each branch.)

        // To work around this behavior, don't allow pushing down expressions if we are also going
        // to push down a sort, skip or limit. We don't want the expressions to be evaluated on any
        // documents that the sort/skip/limit would have filtered out. (The sort stage can be a
        // top-k sort, which both sorts and limits.)
        bool allowExpressions = !sortStage && !skipThenLimit.getSkip() && !skipThenLimit.getLimit();
        projObj = buildProjectionForPushdown(
            deps, pipeline, allowExpressions, timeseriesBoundedSortOptimization);

        plannerOpts.options |= QueryPlannerParams::RETURN_OWNED_DATA;
    }

    if (rewrittenGroupStage) {
        // See if the query system can handle the $group and $sort stage using a DISTINCT_SCAN
        // (SERVER-9507).
        auto swExecutorGrouped =
            attemptToGetExecutor(expCtx,
                                 collections,
                                 nss,
                                 createFindCommand(expCtx,
                                                   nss,
                                                   queryObj,
                                                   projObj,
                                                   sortObj,
                                                   SkipThenLimit{boost::none, boost::none},
                                                   aggRequest),
                                 deps.metadataDeps(),
                                 rewrittenGroupStage.get(),
                                 plannerOpts,
                                 matcherFeatures,
                                 pipeline,
                                 *shouldProduceEmptyDocs /* isCountLike */);

        if (swExecutorGrouped.isOK()) {
            // Any $limit stage before the $group stage should make the pipeline ineligible for this
            // optimization.
            invariant(!sortStage || !sortStage->hasLimit());

            // We remove the $sort and $group stages that begin the pipeline, because the executor
            // will handle the sort, and the groupTransform (added below) will handle the $group
            // stage.
            pipeline->popFrontWithName(DocumentSourceSort::kStageName);
            pipeline->popFrontWithName(rewrittenGroupStage->originalStageName());

            boost::intrusive_ptr<DocumentSource> groupTransform(
                new DocumentSourceSingleDocumentTransformation(
                    expCtx,
                    std::move(rewrittenGroupStage),
                    "$groupByDistinctScan",
                    false /* independentOfAnyCollection */));
            pipeline->addInitialSource(groupTransform);

            return swExecutorGrouped;
        } else if (swExecutorGrouped != ErrorCodes::NoQueryExecutionPlans) {
            return swExecutorGrouped.getStatus().withContext(
                "Failed to determine whether query system can provide a "
                "DISTINCT_SCAN grouping");
        }
    }

    // If this pipeline is a change stream, then the cursor must use the simple collation, so we
    // temporarily switch the collator on the ExpressionContext to nullptr. We do this here because
    // by this point, all the necessary pipeline analyses and optimizations have already been
    // performed. Note that 'collatorStash' restores the original collator when it leaves scope.
    std::unique_ptr<CollatorInterface> collatorForCursor = nullptr;
    auto collatorStash =
        isChangeStream ? expCtx->temporarilyChangeCollator(std::move(collatorForCursor)) : nullptr;

    auto executor = attemptToGetExecutor(
        expCtx,
        collections,
        nss,
        createFindCommand(expCtx, nss, queryObj, projObj, sortObj, skipThenLimit, aggRequest),
        deps.metadataDeps(),
        nullptr, /* groupForDistinctScan */
        plannerOpts,
        matcherFeatures,
        pipeline,
        *shouldProduceEmptyDocs /* isCountLike */);

    // While constructing the executor, some stages might have been lowered from the 'pipeline' into
    // the executor, so we need to recheck whether the executor's layer can still produce an empty
    // document.
    *shouldProduceEmptyDocs = pipeline->getDependencies(unavailableMetadata).hasNoRequirements();
    if (executor.isOK()) {
        executor.getValue()->setReturnOwnedData(!*shouldProduceEmptyDocs);
    }

    return executor;
}

Timestamp PipelineD::getLatestOplogTimestamp(const Pipeline* pipeline) {
    if (auto docSourceCursor =
            dynamic_cast<DocumentSourceCursor*>(pipeline->_sources.front().get())) {
        return docSourceCursor->getLatestOplogTimestamp();
    }
    return Timestamp();
}

BSONObj PipelineD::getPostBatchResumeToken(const Pipeline* pipeline) {
    if (auto docSourceCursor =
            dynamic_cast<DocumentSourceCursor*>(pipeline->_sources.front().get())) {
        return docSourceCursor->getPostBatchResumeToken();
    }
    return BSONObj{};
}

bool PipelineD::isSearchPresentAndEligibleForSbe(const Pipeline* pipeline) {
    auto expCtx = pipeline->getContext();

    auto firstStageIsSearch =
        getSearchHelpers(expCtx->opCtx->getServiceContext())->isSearchPipeline(pipeline) ||
        getSearchHelpers(expCtx->opCtx->getServiceContext())->isSearchMetaPipeline(pipeline);

    // (Ignore FCV check): FCV checking is unnecessary because SBE execution is local to a given
    // node.
    auto searchInSbeEnabled = feature_flags::gFeatureFlagSearchInSbe.isEnabledAndIgnoreFCVUnsafe();
    auto forceClassicEngine =
        QueryKnobConfiguration::decoration(expCtx->opCtx).getInternalQueryFrameworkControlForOp() ==
        QueryFrameworkControlEnum::kForceClassicEngine;

    return firstStageIsSearch && searchInSbeEnabled && !forceClassicEngine;
}
}  // namespace mongo
