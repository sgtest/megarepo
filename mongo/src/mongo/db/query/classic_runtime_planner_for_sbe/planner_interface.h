/**
 *    Copyright (C) 2024-present MongoDB, Inc.
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

#include "mongo/db/exec/multi_plan.h"
#include "mongo/db/exec/plan_cache_util.h"
#include "mongo/db/exec/sbe/stages/stages.h"
#include "mongo/db/exec/subplan.h"
#include "mongo/db/exec/working_set.h"
#include "mongo/db/query/plan_executor.h"
#include "mongo/db/query/plan_yield_policy.h"
#include "mongo/db/query/plan_yield_policy_sbe.h"
#include "mongo/db/query/planner_interface.h"
#include "mongo/db/query/query_planner_params.h"
#include "mongo/db/query/query_solution.h"
#include "mongo/db/query/sbe_plan_cache.h"
#include "mongo/db/query/sbe_stage_builder_plan_data.h"

namespace mongo::classic_runtime_planner_for_sbe {

/**
 * Data that any runtime planner needs to perform the planning.
 */
struct PlannerDataForSBE final : public PlannerData {
    PlannerDataForSBE(OperationContext* opCtx,
                      CanonicalQuery* cq,
                      std::unique_ptr<WorkingSet> workingSet,
                      const MultipleCollectionAccessor& collections,
                      QueryPlannerParams plannerParams,
                      PlanYieldPolicy::YieldPolicy yieldPolicy,
                      boost::optional<size_t> cachedPlanHash,
                      std::unique_ptr<PlanYieldPolicySBE> sbeYieldPolicy)
        : PlannerData(opCtx,
                      cq,
                      std::move(workingSet),
                      collections,
                      std::move(plannerParams),
                      yieldPolicy,
                      cachedPlanHash),
          sbeYieldPolicy(std::move(sbeYieldPolicy)) {}

    std::unique_ptr<PlanYieldPolicySBE> sbeYieldPolicy;
};

class PlannerBase : public PlannerInterface {
public:
    PlannerBase(PlannerDataForSBE plannerData);

protected:
    /**
     * Function that prepares 'sbePlanAndData' for execution and passes the correct arguments to a
     * new instance of PlanExecutorSBE and returns it. Note that the classicRuntimePlannerStage is
     * only passed to PlanExecutorSBE so that it can be plumbed through to a PlanExplainer to
     * generate the correct explain output when using the classic multiplanner with SBE.
     */
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> prepareSbePlanExecutor(
        std::unique_ptr<CanonicalQuery> canonicalQuery,
        std::unique_ptr<QuerySolution> solution,
        std::pair<std::unique_ptr<sbe::PlanStage>, stage_builder::PlanStageData> sbePlanAndData,
        bool isFromPlanCache,
        boost::optional<size_t> cachedPlanHash,
        std::unique_ptr<MultiPlanStage> classicRuntimePlannerStage);

    OperationContext* opCtx() {
        return _plannerData.opCtx;
    }

    CanonicalQuery* cq() {
        return _plannerData.cq;
    }

    const CanonicalQuery* cq() const {
        return _plannerData.cq;
    }

    const MultipleCollectionAccessor& collections() const {
        return _plannerData.collections;
    }

    PlanYieldPolicy::YieldPolicy yieldPolicy() const {
        return _plannerData.yieldPolicy;
    }

    PlanYieldPolicySBE* sbeYieldPolicy() {
        return _plannerData.sbeYieldPolicy.get();
    }

    std::unique_ptr<PlanYieldPolicySBE> extractSbeYieldPolicy() {
        return std::move(_plannerData.sbeYieldPolicy);
    }

    size_t plannerOptions() const {
        return _plannerData.plannerParams.options;
    }

    boost::optional<size_t> cachedPlanHash() const {
        return _plannerData.cachedPlanHash;
    }

    WorkingSet* ws() {
        return _plannerData.workingSet.get();
    }

    std::unique_ptr<WorkingSet> extractWs() {
        return std::move(_plannerData.workingSet);
    }

    const QueryPlannerParams& plannerParams() const {
        return _plannerData.plannerParams;
    }

    PlannerDataForSBE extractPlannerData() {
        return std::move(_plannerData);
    }

private:
    PlannerDataForSBE _plannerData;
};

/**
 * Trivial planner that just creates an executor when there is only one QuerySolution present.
 */
class SingleSolutionPassthroughPlanner final : public PlannerBase {
public:
    SingleSolutionPassthroughPlanner(PlannerDataForSBE plannerData,
                                     std::unique_ptr<QuerySolution> solution);

    /**
     * Builds and caches SBE plan for the given solution and returns PlanExecutor for it.
     */
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> makeExecutor(
        std::unique_ptr<CanonicalQuery> canonicalQuery) override;

private:
    std::unique_ptr<QuerySolution> _solution;
};

class CachedPlanner final : public PlannerBase {
public:
    CachedPlanner(PlannerDataForSBE plannerData,
                  PlanYieldPolicy::YieldPolicy yieldPolicy,
                  std::unique_ptr<sbe::CachedPlanHolder> cachedPlanHolder);

    /**
     * Recovers SBE plan from cache and performs trial on it. When the trial period ends, this
     * function checks the stats to determine if the number of reads during the trial meets the
     * criteria for replanning and replans if necessary.
     *
     * Returns PlanExecutor for the final selected plan.
     */
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> makeExecutor(
        std::unique_ptr<CanonicalQuery> canonicalQuery) override;

private:
    /**
     * Executes the "trial" portion of a single plan until it
     *   - reaches EOF,
     *   - reaches the 'maxNumResults' limit,
     *   - early exits via the TrialRunTracker, or
     *   - returns a failure Status.
     *
     * All documents returned by the plan are enqueued into the 'CandidatePlan->results' queue.
     */
    sbe::plan_ranker::CandidatePlan _collectExecutionStatsForCachedPlan(
        std::unique_ptr<sbe::PlanStage> root,
        stage_builder::PlanStageData data,
        size_t maxTrialPeriodNumReads);

    /**
     * Uses the QueryPlanner and the MultiPlanner to re-generate candidate plans for this
     * query and select a new winner.
     *
     * The plan cache is modified only if 'shouldCache' is true. The 'replanReason' string is used
     * to indicate the reason for replanning, which can be included, for example, into plan stats
     * summary.
     */
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> _replan(
        std::unique_ptr<CanonicalQuery> canonicalQuery, bool shouldCache, std::string replanReason);

    PlanYieldPolicy::YieldPolicy _yieldPolicy;
    std::unique_ptr<sbe::CachedPlanHolder> _cachedPlanHolder;
};

class MultiPlanner final : public PlannerBase {
public:
    MultiPlanner(PlannerDataForSBE plannerData,
                 std::vector<std::unique_ptr<QuerySolution>> candidatePlans,
                 PlanCachingMode cachingMode,
                 boost::optional<std::string> replanReason = boost::none);

    /**
     * Picks the best plan given by the classic engine multiplanner and returns a plan executor. If
     * the planner finished running the best solution during multiplanning, we return the documents
     * and exit, otherwise we pick the best plan and return the SBE plan executor.
     */
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> makeExecutor(
        std::unique_ptr<CanonicalQuery> canonicalQuery) override;

private:
    using SbePlanAndData = std::pair<std::unique_ptr<sbe::PlanStage>, stage_builder::PlanStageData>;
    SbePlanAndData _buildSbePlanAndUpdatePlanCache(const QuerySolution* winningSolution,
                                                   const plan_ranker::PlanRankingDecision& ranking);

    bool _shouldUseEofOptimization() const;

    std::unique_ptr<MultiPlanStage> _multiPlanStage;
    PlanCachingMode _cachingMode;
    boost::optional<std::string> _replanReason;
};

class SubPlanner final : public PlannerBase {
public:
    SubPlanner(PlannerDataForSBE plannerData);

    /**
     * Picks the composite solution given by the classic engine subplanner, extends the composite
     * solution with the cq pipeline, creates a pinned plan cache entry containing the resulting SBE
     * plan, and returns a plan executor.
     */
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> makeExecutor(
        std::unique_ptr<CanonicalQuery> canonicalQuery) override;

private:
    std::unique_ptr<SubplanStage> _subplanStage;
};

}  // namespace mongo::classic_runtime_planner_for_sbe
