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

#include <string>


#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/db/query/sbe_plan_ranker.h"
#include "mongo/db/query/stage_types.h"
#include "mongo/util/assert_util_core.h"

namespace mongo::sbe::plan_ranker {
namespace {
/**
 * A plan ranker for the SBE plan stage tree. Defines productivity as a cumulative number of
 * physical reads from the storage performed by all stages in the plan which can read from the
 * storage, divided by the total number of advances of the root stage, which corresponds to the
 * number of returned documents.
 */
class DefaultPlanScorer final : public mongo::plan_ranker::PlanScorer<PlanStageStats> {
public:
    DefaultPlanScorer(const QuerySolution* solution) : _solution{solution} {
        invariant(_solution);
    }

protected:
    double calculateProductivity(const mongo::sbe::PlanStageStats* root) const final {
        return plan_ranker::calculateProductivity(root->common.advances,
                                                  calculateNumberOfReads(root));
    }

    std::string getProductivityFormula(const mongo::sbe::PlanStageStats* root) const final {
        auto numReads{calculateNumberOfReads(root)};
        StringBuilder sb;

        sb << "(" << (root->common.advances) << " advances + 1)/(" << numReads << " numReads + 1)";

        return sb.str();
    }

    double getNumberOfAdvances(const mongo::sbe::PlanStageStats* stats) const final {
        return stats->common.advances;
    }

    bool hasStage(StageType type, const mongo::sbe::PlanStageStats* stats) const final {
        // In SBE a plan stage doesn't map 1-to-1 to a solution node, and can expand into a subtree
        // of plan stages, each having its own plan stage stats. So, to answer whether an SBE plan
        // stage stats tree contains a stage of the given 'type', we need to look into the solution
        // tree instead.
        return _solution->hasNode(type);
    }

private:
    const QuerySolution* _solution;
};
}  // namespace

std::unique_ptr<mongo::plan_ranker::PlanScorer<PlanStageStats>> makePlanScorer(
    const QuerySolution* solution) {
    return std::make_unique<DefaultPlanScorer>(solution);
}

double calculateProductivity(const size_t advances, const size_t numReads) {
    // We add one to the number of advances so that plans which returned zero documents still
    // have a productivity of non-zero. This allows us to compare productivity scores between
    // plans with zero advances. For example, a plan which did zero advances but examined ten
    // documents would have a score of (0 + 1)/10, while a plan which did zero advances but
    // examined a hundred documents would have a score of (0 + 1)/100.
    //
    // Similarly, we add one to the number of reads in case 0 reads were performed. This could
    // happen if a plan encounters EOF right away, for example.
    return static_cast<double>(advances + 1) / static_cast<double>(numReads + 1);
}

}  // namespace mongo::sbe::plan_ranker
