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


#include "mongo/db/exec/multi_plan.h"

#include <algorithm>
#include <boost/move/utility_core.hpp>
#include <deque>
#include <fmt/format.h>
#include <memory>
#include <string>
#include <utility>
#include <variant>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/db/commands/server_status_metric.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/exec/histogram_server_status_metric.h"
#include "mongo/db/exec/trial_period_utils.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/classic_plan_cache.h"
#include "mongo/db/query/collection_query_info.h"
#include "mongo/db/query/multiple_collection_accessor.h"
#include "mongo/db/query/plan_cache_debug_info.h"
#include "mongo/db/query/plan_cache_key_factory.h"
#include "mongo/db/query/plan_explainer.h"
#include "mongo/db/query/plan_explainer_factory.h"
#include "mongo/db/query/plan_ranker.h"
#include "mongo/db/query/plan_ranker_util.h"
#include "mongo/db/query/plan_ranking_decision.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/service_context.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/platform/atomic_proxy.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/duration.h"
#include "mongo/util/str.h"
#include "mongo/util/tick_source.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo {

using std::unique_ptr;

// static
const char* MultiPlanStage::kStageType = "MULTI_PLAN";

namespace {
void markShouldCollectTimingInfoOnSubtree(PlanStage* root) {
    root->markShouldCollectTimingInfo();
    for (auto&& child : root->getChildren()) {
        markShouldCollectTimingInfoOnSubtree(child.get());
    }
}

/**
 * Aggregation of the total number of microseconds spent (in the classic multiplanner).
 */
auto& classicMicrosTotal = *MetricBuilder<Counter64>{"query.multiPlanner.classicMicros"};

/**
 * Aggregation of the total number of "works" performed (in the classic multiplanner).
 */
auto& classicWorksTotal = *MetricBuilder<Counter64>{"query.multiPlanner.classicWorks"};

/**
 * Aggregation of the total number of invocations (of the classic multiplanner).
 */
auto& classicCount = *MetricBuilder<Counter64>{"query.multiPlanner.classicCount"};

/**
 * An element in this histogram is the number of microseconds spent in an invocation (of the
 * classic multiplanner).
 */
auto& classicMicrosHistogram =
    *MetricBuilder<HistogramServerStatusMetric>{"query.multiPlanner.histograms.classicMicros"}.bind(
        HistogramServerStatusMetric::pow(11, 1024, 4));

/**
 * An element in this histogram is the number of "works" performed during an invocation (of the
 * classic multiplanner).
 */
auto& classicWorksHistogram =
    *MetricBuilder<HistogramServerStatusMetric>{"query.multiPlanner.histograms.classicWorks"}.bind(
        HistogramServerStatusMetric::pow(9, 128, 2));

/**
 * An element in this histogram is the number of plans in the candidate set of an invocation (of the
 * classic multiplanner).
 */
auto& classicNumPlansHistogram =
    *MetricBuilder<HistogramServerStatusMetric>{"query.multiPlanner.histograms.classicNumPlans"}
         .bind(HistogramServerStatusMetric::pow(5, 2, 2));

}  // namespace

MultiPlanStage::MultiPlanStage(ExpressionContext* expCtx,
                               VariantCollectionPtrOrAcquisition collection,
                               CanonicalQuery* cq,
                               PlanCachingMode cachingMode)
    : RequiresCollectionStage(kStageType, expCtx, collection),
      _cachingMode(cachingMode),
      _query(cq),
      _bestPlanIdx(kNoSuchPlan),
      _backupPlanIdx(kNoSuchPlan) {}

void MultiPlanStage::addPlan(std::unique_ptr<QuerySolution> solution,
                             std::unique_ptr<PlanStage> root,
                             WorkingSet* ws) {
    _children.emplace_back(std::move(root));
    _candidates.push_back({std::move(solution), _children.back().get(), ws});

    // Tell the new candidate plan that it must collect timing info. This timing info will
    // later be stored in the plan cache, and may be used for explain output.
    PlanStage* newChild = _children.back().get();
    markShouldCollectTimingInfoOnSubtree(newChild);
}

bool MultiPlanStage::isEOF() {
    // If _bestPlanIdx hasn't been found, can't be at EOF
    if (!bestPlanChosen()) {
        return false;
    }

    // We must have returned all our cached results
    // and there must be no more results from the best plan.
    auto& bestPlan = _candidates[_bestPlanIdx];
    return bestPlan.results.empty() && bestPlan.root->isEOF();
}

PlanStage::StageState MultiPlanStage::doWork(WorkingSetID* out) {
    auto& bestPlan = _candidates[_bestPlanIdx];

    // Look for an already produced result that provides the data the caller wants.
    if (!bestPlan.results.empty()) {
        *out = bestPlan.results.front();
        bestPlan.results.pop_front();
        return PlanStage::ADVANCED;
    }

    // best plan had no (or has no more) cached results

    StageState state;
    try {
        state = bestPlan.root->work(out);
    } catch (const ExceptionFor<ErrorCodes::QueryExceededMemoryLimitNoDiskUseAllowed>&) {
        // The winning plan ran out of memory. If we have a backup plan with no blocking states,
        // then switch to it.
        if (!hasBackupPlan()) {
            throw;
        }

        LOGV2_DEBUG(20588, 5, "Best plan errored, switching to backup plan");

        CollectionQueryInfo::get(collectionPtr())
            .getPlanCache()
            ->remove(plan_cache_key_factory::make<PlanCacheKey>(*_query, collectionPtr()));

        switchToBackupPlan();
        return _candidates[_bestPlanIdx].root->work(out);
    }

    if (hasBackupPlan() && PlanStage::ADVANCED == state) {
        LOGV2_DEBUG(20589, 5, "Best plan had a blocking stage, became unblocked");
        removeBackupPlan();
    }

    return state;
}

void MultiPlanStage::tryYield(PlanYieldPolicy* yieldPolicy) {
    // These are the conditions which can cause us to yield:
    //   1) The yield policy's timer elapsed, or
    //   2) some stage requested a yield, or
    //   3) we need to yield and retry due to a WriteConflictException.
    // In all cases, the actual yielding happens here.
    if (yieldPolicy->shouldYieldOrInterrupt(expCtx()->opCtx)) {
        uassertStatusOK(yieldPolicy->yieldOrInterrupt(expCtx()->opCtx));
    }
}

Status MultiPlanStage::pickBestPlan(PlanYieldPolicy* yieldPolicy) {
    if (bestPlanChosen()) {
        return Status::OK();
    }

    // Adds the amount of time taken by pickBestPlan() to executionTime. There's lots of execution
    // work that happens here, so this is needed for the time accounting to make sense.
    auto optTimer = getOptTimer();

    auto tickSource = opCtx()->getServiceContext()->getTickSource();
    auto startTicks = tickSource->getTicks();

    classicNumPlansHistogram.increment(_candidates.size());
    classicCount.increment();

    const size_t numWorks =
        trial_period::getTrialPeriodMaxWorks(opCtx(),
                                             collectionPtr(),
                                             internalQueryPlanEvaluationWorks.load(),
                                             internalQueryPlanEvaluationCollFraction.load());
    size_t numResults = trial_period::getTrialPeriodNumToReturn(*_query);

    try {
        // Work the plans, stopping when a plan hits EOF or returns some fixed number of results.
        size_t ix = 0;
        for (; ix < numWorks; ++ix) {
            bool moreToDo = workAllPlans(numResults, yieldPolicy);
            if (!moreToDo) {
                break;
            }
        }
        auto totalWorks = ix * _candidates.size();
        classicWorksHistogram.increment(totalWorks);
        classicWorksTotal.increment(totalWorks);
    } catch (DBException& e) {
        return e.toStatus().withContext("error while multiplanner was selecting best plan");
    }

    auto durationMicros = durationCount<Microseconds>(
        tickSource->ticksTo<Microseconds>(tickSource->getTicks() - startTicks));
    classicMicrosHistogram.increment(durationMicros);
    classicMicrosTotal.increment(durationMicros);

    // After picking best plan, ranking will own plan stats from candidate solutions (winner and
    // losers).
    auto statusWithRanking = plan_ranker::pickBestPlan<PlanStageStats>(_candidates);
    if (!statusWithRanking.isOK()) {
        return statusWithRanking.getStatus();
    }

    _ranking = std::move(statusWithRanking.getValue());
    // Since the status was ok there should be a ranking containing at least one successfully ranked
    // plan.
    invariant(_ranking);
    _bestPlanIdx = _ranking->candidateOrder[0];

    MONGO_verify(_bestPlanIdx >= 0 && _bestPlanIdx < static_cast<int>(_candidates.size()));

    auto& bestCandidate = _candidates[_bestPlanIdx];
    const auto& alreadyProduced = bestCandidate.results;
    const auto& bestSolution = bestCandidate.solution;

    LOGV2_DEBUG(20590,
                5,
                "Winning solution",
                "bestSolution"_attr = redact(bestSolution->toString()),
                "bestSolutionHash"_attr = bestSolution->hash());

    auto explainer =
        plan_explainer_factory::make(bestCandidate.root, bestSolution->_enumeratorExplainInfo);
    LOGV2_DEBUG(20591, 2, "Winning plan", "planSummary"_attr = explainer->getPlanSummary());

    _backupPlanIdx = kNoSuchPlan;
    if (bestSolution->hasBlockingStage && (0 == alreadyProduced.size())) {
        LOGV2_DEBUG(20592, 5, "Winner has blocking stage, looking for backup plan...");
        for (auto&& ix : _ranking->candidateOrder) {
            if (!_candidates[ix].solution->hasBlockingStage) {
                LOGV2_DEBUG(20593, 5, "Backup child", "ix"_attr = ix);
                _backupPlanIdx = ix;
                break;
            }
        }
    }

    const auto& coll = collection();
    auto multipleCollection = coll.isAcquisition()
        ? MultipleCollectionAccessor{coll.getAcquisition()}
        : MultipleCollectionAccessor{coll.getCollectionPtr()};

    if (_cachingMode != PlanCachingMode::NeverCache) {
        plan_cache_util::updateClassicPlanCacheFromClassicCandidates(expCtx()->opCtx,
                                                                     multipleCollection,
                                                                     _cachingMode,
                                                                     *_query,
                                                                     std::move(_ranking),
                                                                     _candidates);
    }

    removeRejectedPlans();

    return Status::OK();
}

bool MultiPlanStage::workAllPlans(size_t numResults, PlanYieldPolicy* yieldPolicy) {
    bool doneWorking = false;

    for (size_t ix = 0; ix < _candidates.size(); ++ix) {
        auto& candidate = _candidates[ix];
        if (!candidate.status.isOK()) {
            continue;
        }

        // Might need to yield between calls to work due to the timer elapsing.
        tryYield(yieldPolicy);

        WorkingSetID id = WorkingSet::INVALID_ID;
        PlanStage::StageState state;
        try {
            state = candidate.root->work(&id);
        } catch (const ExceptionFor<ErrorCodes::QueryExceededMemoryLimitNoDiskUseAllowed>& ex) {
            // If a candidate fails due to exceeding allowed resource consumption, then mark the
            // candidate as failed but proceed with the multi-plan trial period. The MultiPlanStage
            // as a whole only fails if _all_ candidates hit their resource consumption limit, or if
            // a different, query-fatal error code is thrown.
            candidate.status = ex.toStatus();
            ++_failureCount;

            // If all children have failed, then rethrow. Otherwise, swallow the error and move onto
            // the next candidate plan.
            if (_failureCount == _candidates.size()) {
                throw;
            }

            continue;
        }

        if (PlanStage::ADVANCED == state) {
            // Save result for later.
            WorkingSetMember* member = candidate.data->get(id);
            // Ensure that the BSONObj underlying the WorkingSetMember is owned in case we choose to
            // return the results from the 'candidate' plan.
            member->makeObjOwnedIfNeeded();
            candidate.results.push_back(id);

            // Once a plan returns enough results, stop working.
            if (candidate.results.size() >= numResults) {
                doneWorking = true;
            }
        } else if (PlanStage::IS_EOF == state) {
            // First plan to hit EOF wins automatically.  Stop evaluating other plans.
            // Assumes that the ranking will pick this plan.
            doneWorking = true;
        } else if (PlanStage::NEED_YIELD == state) {
            invariant(id == WorkingSet::INVALID_ID);
            // Run-time plan selection occurs before a WriteUnitOfWork is opened and it's not
            // subject to TemporarilyUnavailableException's.
            invariant(!expCtx()->getTemporarilyUnavailableException());
            if (!yieldPolicy->canAutoYield()) {
                throwWriteConflictException(
                    "Write conflict during multi-planning selection period "
                    "and yielding is disabled.");
            }

            if (yieldPolicy->canAutoYield()) {
                yieldPolicy->forceYield();
            }

            tryYield(yieldPolicy);
        }
    }

    return !doneWorking;
}

void MultiPlanStage::removeRejectedPlans() {
    // Move the best plan and the backup plan to the front of 'children'.
    if (_bestPlanIdx != 0) {
        std::swap(_children[_bestPlanIdx], _children[0]);
        std::swap(_candidates[_bestPlanIdx], _candidates[0]);
        if (_backupPlanIdx == 0) {
            _backupPlanIdx = _bestPlanIdx;
        }
        _bestPlanIdx = 0;
    }
    size_t startIndex = 1;
    if (_backupPlanIdx != kNoSuchPlan) {
        if (_backupPlanIdx != 1) {
            std::swap(_children[_backupPlanIdx], _children[1]);
            std::swap(_candidates[_backupPlanIdx], _candidates[1]);
            _backupPlanIdx = 1;
        }
        startIndex = 2;
    }

    _rejected.reserve(_children.size() - startIndex);
    for (size_t i = startIndex; i < _children.size(); ++i) {
        rejectPlan(i);
    }
    _children.resize(startIndex);
}

void MultiPlanStage::switchToBackupPlan() {
    std::swap(_children[_backupPlanIdx], _children[_bestPlanIdx]);
    std::swap(_candidates[_backupPlanIdx], _candidates[_bestPlanIdx]);
    removeBackupPlan();
}

void MultiPlanStage::rejectPlan(size_t planIdx) {
    auto rejectedPlan = std::move(_children[planIdx]);
    if (opCtx() != nullptr) {
        rejectedPlan->saveState();
        rejectedPlan->detachFromOperationContext();
    }
    _rejected.emplace_back(std::move(rejectedPlan));
}

void MultiPlanStage::removeBackupPlan() {
    rejectPlan(_backupPlanIdx);
    _children.resize(1);
    _backupPlanIdx = kNoSuchPlan;
}

bool MultiPlanStage::hasBackupPlan() const {
    return kNoSuchPlan != _backupPlanIdx;
}

bool MultiPlanStage::bestPlanChosen() const {
    return kNoSuchPlan != _bestPlanIdx;
}

boost::optional<size_t> MultiPlanStage::bestPlanIdx() const {
    return {bestPlanChosen(), static_cast<size_t>(_bestPlanIdx)};
}

const QuerySolution* MultiPlanStage::bestSolution() const {
    if (_bestPlanIdx == kNoSuchPlan)
        return nullptr;

    return _candidates[_bestPlanIdx].solution.get();
}

std::unique_ptr<QuerySolution> MultiPlanStage::extractBestSolution() {
    if (_bestPlanIdx == kNoSuchPlan)
        return nullptr;

    _bestPlanScore = _candidates[_bestPlanIdx].solution->score;
    return std::move(_candidates[_bestPlanIdx].solution);
}

bool MultiPlanStage::bestSolutionEof() const {
    tassert(8523500, "The best plan is not chosen by the multi-planner", bestPlanChosen());
    auto& bestPlan = _candidates[_bestPlanIdx];
    return bestPlan.root->isEOF();
}

unique_ptr<PlanStageStats> MultiPlanStage::getStats() {
    _commonStats.isEOF = isEOF();
    unique_ptr<PlanStageStats> ret =
        std::make_unique<PlanStageStats>(_commonStats, STAGE_MULTI_PLAN);
    ret->specific = std::make_unique<MultiPlanStats>(_specificStats);
    for (auto&& child : _children) {
        ret->children.emplace_back(child->getStats());
    }
    for (auto&& child : _rejected) {
        ret->children.emplace_back(child->getStats());
    }
    return ret;
}

const SpecificStats* MultiPlanStage::getSpecificStats() const {
    return &_specificStats;
}

const plan_ranker::CandidatePlan& MultiPlanStage::getCandidate(size_t candidateIdx) const {
    tassert(8223800,
            str::stream() << "Invalid candidate plan index: " << candidateIdx
                          << ", size: " << _candidates.size(),
            candidateIdx < _candidates.size());
    return _candidates[candidateIdx];
}

boost::optional<double> MultiPlanStage::getCandidateScore(size_t candidateIdx) const {
    tassert(5408301,
            str::stream() << "Invalid candidate plan index: " << candidateIdx
                          << ", size: " << _candidates.size(),
            candidateIdx < _candidates.size());
    if (candidateIdx == static_cast<size_t>(_bestPlanIdx) && !_candidates[candidateIdx].solution) {
        return _bestPlanScore;
    }
    return _candidates[candidateIdx].solution->score;
}

}  // namespace mongo
