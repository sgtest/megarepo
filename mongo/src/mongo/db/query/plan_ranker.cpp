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


#include <queue>
#include <vector>

#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/db/exec/plan_stats.h"
#include "mongo/db/query/plan_ranker.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo::plan_ranker {
namespace log_detail {
void logScoreFormula(std::function<std::string()> formula,
                     double score,
                     double baseScore,
                     double productivity,
                     double noFetchBonus,
                     double noSortBonus,
                     double noIxisectBonus,
                     double tieBreakers) {
    LOGV2_DEBUG(
        20961, 2, "Score formula", "formula"_attr = [&]() {
            StringBuilder sb;
            sb << "score(" << str::convertDoubleToString(score) << ") = baseScore("
               << str::convertDoubleToString(baseScore) << ")"
               << " + productivity(" << formula() << " = "
               << str::convertDoubleToString(productivity) << ")"
               << " + tieBreakers(" << str::convertDoubleToString(noFetchBonus)
               << " noFetchBonus + " << str::convertDoubleToString(noSortBonus) << " noSortBonus + "
               << str::convertDoubleToString(noIxisectBonus)
               << " noIxisectBonus = " << str::convertDoubleToString(tieBreakers) << ")";
            return sb.str();
        }());
}

void logScoreBoost(double score) {
    LOGV2_DEBUG(20962, 5, "Score boosted due to intersection forcing", "newScore"_attr = score);
}

void logScoringPlan(std::function<std::string()> solution,
                    std::function<std::string()> explain,
                    std::function<std::string()> planSummary,
                    size_t planIndex,
                    bool isEOF) {
    LOGV2_DEBUG(20956,
                5,
                "Scoring plan",
                "planIndex"_attr = planIndex,
                "querySolution"_attr = redact(solution()),
                "stats"_attr = redact(explain()));
    LOGV2_DEBUG(20957,
                2,
                "Scoring query plan",
                "planSummary"_attr = planSummary(),
                "planHitEOF"_attr = isEOF);
}

void logScore(double score) {
    LOGV2_DEBUG(20958, 5, "Basic plan score", "score"_attr = score);
}

void logEOFBonus(double eofBonus) {
    LOGV2_DEBUG(20959, 5, "Adding EOF bonus to score", "eofBonus"_attr = eofBonus);
}

void logFailedPlan(std::function<std::string()> planSummary) {
    LOGV2_DEBUG(
        20960, 2, "Not scoring a plan because the plan failed", "planSummary"_attr = planSummary());
}

void logTieBreaking(double score,
                    double docsExaminedBonus,
                    double indexPrefixBonus,
                    bool isPlanTied) {
    LOGV2_DEBUG(
        8027500, 2, "Tie breaking heuristics", "formula"_attr = [&]() {
            StringBuilder sb;
            sb << "isPlanTied: " << isPlanTied << ". finalScore("
               << str::convertDoubleToString(score + docsExaminedBonus + indexPrefixBonus)
               << ") = score(" << str::convertDoubleToString(score) << ") + docsExaminedBonus("
               << str::convertDoubleToString(docsExaminedBonus) << ") + indexPrefixBonus("
               << str::convertDoubleToString(indexPrefixBonus) << ")";
            return sb.str();
        }());
}
}  // namespace log_detail

namespace {
/**
 * A plan scorer for the classic plan stage tree. Defines the plan productivity as a number
 * of intermediate results returned, or advanced, by the root stage, divided by the "unit of works"
 * which the plan performed. Each call to work(...) counts as one unit.
 */
class DefaultPlanScorer final : public PlanScorer<PlanStageStats> {
protected:
    double calculateProductivity(const PlanStageStats* stats) const final {
        invariant(stats->common.works != 0);
        return static_cast<double>(stats->common.advanced) /
            static_cast<double>(stats->common.works);
    }

    std::string getProductivityFormula(const PlanStageStats* stats) const {
        StringBuilder sb;
        sb << "(" << stats->common.advanced << " advanced)/(" << stats->common.works << " works)";
        return sb.str();
    }

    double getNumberOfAdvances(const PlanStageStats* stats) const final {
        return stats->common.advanced;
    }

    bool hasStage(StageType type, const PlanStageStats* root) const final {
        std::queue<const PlanStageStats*> remaining;
        remaining.push(root);

        while (!remaining.empty()) {
            auto stats = remaining.front();
            remaining.pop();

            if (stats->stageType == type) {
                return true;
            }

            for (auto&& child : stats->children) {
                remaining.push(child.get());
            }
        }
        return false;
    }
};

/**
 * Return true if the nodes have the same type and the same number of children.
 */
bool areNodesCompatible(const std::vector<const QuerySolutionNode*>& nodes) {
    for (size_t i = 1; i < nodes.size(); ++i) {
        if (nodes[i - 1]->getType() != nodes[i]->getType()) {
            return false;
        }

        if (nodes[i - 1]->children.size() != nodes[i]->children.size()) {
            return false;
        }
    }

    return true;
}

/**
 * Returns true if the value can serve as a type lower bound for the purposes of type bracketing.
 * The function is designed to work with the 'interesting' for index prefix heuristic types only:
 * Number, String, Date, Timestamp, Boolean, Object, Array, ObjectId. For other types it may return
 * false positive results. The code of the function is based on index bounds build logic from
 * 'index_bounds_builder.cpp'.
 */
bool isLowerBound(const BSONElement& value, bool isInclusive) {
    switch (value.type()) {
        case NumberInt:
        case NumberDouble:
        case NumberLong:
        case NumberDecimal:
            // Lower bound value for numbers.
            return (std::isinf(value.numberDouble()) || std::isnan(value.numberDouble())) &&
                isInclusive == true;
        case String:
            // Lower bound value for strings.
            return value.str().empty() && isInclusive == true;
        case Date:
            // Lower bound value for dates.
            return value.date() == Date_t::min() && isInclusive == true;
        case bsonTimestamp:
            // Lower bound value for timestamps.
            return value.timestamp() == Timestamp::min() && isInclusive == true;
        case jstOID:
            // Lower bound value for ObjectID.
            return value.OID() == OID() && isInclusive == true;
        case Object:
        case Array:
            // Lower bound value for Object and Array.
            return value.Obj().isEmpty() && isInclusive == true;
        case BinData:
        case EOO:
        case MinKey:
        case MaxKey:
        case Bool:  // Boolean bounds are considered always open since they are non-selective.
        case jstNULL:
        case Undefined:
        case Symbol:
        case RegEx:
        case DBRef:
        case Code:
        case CodeWScope:
            return true;
    }

    MONGO_UNREACHABLE_TASSERT(8102100);
}

/**
 * Returns true if the value can serve as a type upper bound for the purposes of type bracketing.
 * The function is designed to work with the 'interesting' for index prefix heuristic types only:
 * Number, String, Date, Timestamp, Boolean, Object, Array, ObjectId. For other types it may return
 * false positive results. The code of the function is based on index bounds build logic from
 * 'index_bounds_builder.cpp'.
 */
bool isUpperBound(const BSONElement& value, bool isInclusive) {
    switch (value.type()) {
        case NumberInt:
        case NumberDouble:
        case NumberLong:
        case NumberDecimal:
            // Upper bound value for numbers.
            return std::isinf(value.numberDouble()) && isInclusive == true;
        case String:
            // A string value cannot be an upper bound value.
            return false;
        case Date:
            // Upper bound value for Date.
            return value.date() == Date_t::max() && isInclusive == true;
        case bsonTimestamp:
            // Upper bound value for Timestamp.
            return value.timestamp() == Timestamp::max() && isInclusive == true;
        case jstOID:
            // Upper bound value for ObjectID.
            return value.OID() == OID::max() && isInclusive == true;
        case Object:
            // Upper bound value for String.
            return value.Obj().isEmpty() && isInclusive == false;
        case Array:
            // Upper bound value for Object.
            return value.Obj().isEmpty() && isInclusive == false;
        case BinData:
            // Upper bound value for Array.
            return value.valuesize() == 0 && isInclusive == false;
        case EOO:
        case MinKey:
        case MaxKey:
        case Bool:  // Boolean bounds are considered always open since they are non-selective.
        case jstNULL:
        case Undefined:
        case Symbol:
        case RegEx:
        case DBRef:
        case Code:
        case CodeWScope:
            return true;
    }

    MONGO_UNREACHABLE_TASSERT(8102101);
}

/**
 * The function tries to detect if the interval is closed on both ends. Can return false
 * positive results for the types not mentioned in the comment to 'isMinMaxValue' function.
 */
bool isClosedInterval(const Interval& interval) {
    // If the bound types are different the interval is considered to be open.
    if (interval.start.type() != interval.end.type()) {
        return false;
    }

    switch (interval.getDirection()) {
        // Point intervals, empty intervals, and null intervals have no direction.
        case Interval::Direction::kDirectionNone:
            return true;
        case Interval::Direction::kDirectionAscending:
            return !isLowerBound(interval.start, interval.startInclusive) &&
                !isUpperBound(interval.end, interval.endInclusive);
        case Interval::Direction::kDirectionDescending:
            return !isUpperBound(interval.start, interval.startInclusive) &&
                !isLowerBound(interval.end, interval.endInclusive);
    }

    MONGO_UNREACHABLE_TASSERT(8102102);
}

/**
 * Returns true if this OIL contains only closed intervals.
 */
bool containsOnlyClosedIntervals(const OrderedIntervalList& oil) {
    for (const auto& interval : oil.intervals) {
        if (!isClosedInterval(interval)) {
            return false;
        }
    }

    return true;
}

/**
 * Calculates score for the given index bounds. The score reflects the following rules:
 * - IndexBounds that has longest single point interval prefix wins,
 * - if winner is not defined on the previous step then IndexBounds with the longest point
 * interval prefix wins,
 * - if winner is not defined on the previous step then IndexBounds with the longest closed
 * interval prefix wins,
 * - if winner is not defined, then IndexBounds with longest interval prefix wins
 * - if winner is not defined, them IndexBounds with shortest index key pattern wins.
 */
uint64_t getIndexBoundsScore(const IndexBounds& bounds) {
    const uint64_t indexKeyLength = static_cast<uint64_t>(bounds.fields.size());
    uint64_t singlePointIntervalPrefix = 0;
    uint64_t pointsIntervalPrefix = 0;
    uint64_t closedIntervalPrefix = 0;
    uint64_t intervalLength = 0;

    for (const auto& field : bounds.fields) {
        // Skip the $** index virtual field, as it's not part of the actual index key.
        if (field.name == "$_path") {
            continue;
        }

        // Stop scoring index bounds as soon as we see an all-values interval.
        if (field.isMinToMax() || field.isMaxToMin()) {
            break;
        }

        if (intervalLength == singlePointIntervalPrefix && field.isPoint()) {
            ++singlePointIntervalPrefix;
        }

        if (intervalLength == pointsIntervalPrefix && field.containsOnlyPointIntervals()) {
            ++pointsIntervalPrefix;
        }

        if (intervalLength == closedIntervalPrefix && containsOnlyClosedIntervals(field)) {
            ++closedIntervalPrefix;
        }

        ++intervalLength;
    }

    // We pack calculated stats into one value to make their comparison simplier. For every
    // prefix length we allocate 12 bits (4096 values) which is more then enough since an index
    // can have no more than 32 fields (see "MongoDB Limits and Thresholds" reference).
    // 'indexKeyLength' is treated differently because, unlike others, we prefer shorter index
    // key prefix length (see the comment to the function for details).
    uint64_t result = (singlePointIntervalPrefix << 52) | (pointsIntervalPrefix << 40) |
        (closedIntervalPrefix << 28) | (intervalLength << 16) |
        (std::numeric_limits<uint16_t>::max() - indexKeyLength);

    return result;
}

/**
 * Calculates scores for the given IndexBounds and add 1 to every winner's resultScores. i-th
 * position in resultScores corresponds to i-th field in IndexBound.
 */
void scoreIndexBounds(const std::vector<const IndexBounds*>& bounds,
                      std::vector<size_t>& resultScores) {
    const size_t nfields = bounds.size();

    std::vector<uint64_t> scores{};
    scores.reserve(nfields);
    for (size_t i = 0; i < bounds.size(); ++i) {
        scores.emplace_back(getIndexBoundsScore(*bounds[i]));
    }

    auto topScore = max_element(scores.begin(), scores.end());
    for (size_t i = 0; i < nfields; ++i) {
        if (*topScore == scores[i]) {
            resultScores[i] += 1;
        }
    }
}
}  // namespace

std::unique_ptr<PlanScorer<PlanStageStats>> makePlanScorer() {
    return std::make_unique<DefaultPlanScorer>();
}

std::vector<size_t> applyIndexPrefixHeuristic(const std::vector<const QuerySolution*>& solutions) {
    std::vector<size_t> solutionScores(solutions.size(), 0);

    std::vector<std::vector<const QuerySolutionNode*>> stack{};
    stack.emplace_back();
    stack.back().reserve(solutions.size());
    for (auto solution : solutions) {
        stack.back().emplace_back(solution->root());
    }

    while (!stack.empty()) {
        auto top = std::move(stack.back());
        stack.pop_back();

        if (!areNodesCompatible(top)) {
            return {};
        }

        // Compatible nodes have the same number of children, see comment to 'areNodesCompatible'
        // function.
        for (size_t childIndex = 0; childIndex < top.front()->children.size(); ++childIndex) {
            stack.emplace_back();
            stack.back().reserve(solutions.size());
            for (auto node : top) {
                stack.back().emplace_back(node->children[childIndex].get());
            }
        }

        if (top.front()->getType() == STAGE_IXSCAN) {
            std::vector<const IndexBounds*> bounds{};
            bounds.reserve(solutions.size());

            for (auto node : top) {
                bounds.emplace_back(&static_cast<const IndexScanNode*>(node)->bounds);
            }

            scoreIndexBounds(bounds, solutionScores);
        }
    }

    std::vector<size_t> winningSolutionIndices{};
    winningSolutionIndices.reserve(solutions.size());
    const auto topScore = max_element(solutionScores.begin(), solutionScores.end());
    for (size_t index = 0; index < solutionScores.size(); ++index) {
        if (solutionScores[index] == *topScore) {
            winningSolutionIndices.emplace_back(index);
        }
    }

    return winningSolutionIndices;
}
}  // namespace mongo::plan_ranker
