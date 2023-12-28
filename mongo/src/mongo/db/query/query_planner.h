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

#pragma once

#include <cstddef>
#include <functional>
#include <map>
#include <memory>
#include <vector>

#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/matcher/expression.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/canonical_query.h"
#include "mongo/db/query/classic_plan_cache.h"
#include "mongo/db/query/index_entry.h"
#include "mongo/db/query/multiple_collection_accessor.h"
#include "mongo/db/query/query_planner_params.h"
#include "mongo/db/query/query_solution.h"

namespace mongo {
// The logging facility enforces the rule that logging should not be done in a header file. Since
// template classes and functions below must be defined in the header file and since they use the
// logging facility, we have to define the helper functions below to perform the actual logging
// operation from template code.
namespace log_detail {
void logSubplannerIndexEntry(const IndexEntry& entry, size_t childIndex);
void logCachedPlanFound(size_t numChildren, size_t childIndex);
void logCachedPlanNotFound(size_t numChildren, size_t childIndex);
void logNumberOfSolutions(size_t numSolutions);
}  // namespace log_detail

class Collection;
class CollectionPtr;

/**
 * QueryPlanner's job is to provide an entry point to the query planning and optimization
 * process.
 */
class QueryPlanner {
public:
    /**
     * Holds the result of subqueries planning for rooted $or queries.
     */
    struct SubqueriesPlanningResult {
        /**
         * A class used internally in order to keep track of the results of planning
         * a particular $or branch.
         */
        struct BranchPlanningResult {
            // A parsed version of one branch of the $or.
            std::unique_ptr<CanonicalQuery> canonicalQuery;

            // If there is cache data available, then we store it here rather than generating
            // a set of alternate plans for the branch. The index tags from the cache data
            // can be applied directly to the parent $or MatchExpression when generating the
            // composite solution.
            std::unique_ptr<SolutionCacheData> cachedData;

            // Query solutions resulting from planning the $or branch.
            std::vector<std::unique_ptr<QuerySolution>> solutions;
        };

        // The copy of the query that we will annotate with tags and use to construct the composite
        // solution. Must be a rooted $or query, or a contained $or that has been rewritten to a
        // rooted $or.
        std::unique_ptr<MatchExpression> orExpression;

        // Holds a list of the results from planning each branch.
        std::vector<std::unique_ptr<BranchPlanningResult>> branches;

        // We need this to extract cache-friendly index data from the index assignments.
        std::map<IndexEntry::Identifier, size_t> indexMap;
    };

    static std::unique_ptr<QuerySolution> extendWithAggPipeline(
        CanonicalQuery& query,
        std::unique_ptr<QuerySolution>&& solution,
        const std::map<NamespaceString, SecondaryCollectionInfo>& secondaryCollInfos);

    /**
     * Returns the list of possible query solutions for the provided 'query' for multi-planning.
     * Uses the indices and other data in 'params' to determine the set of available plans.
     */
    static StatusWith<std::vector<std::unique_ptr<QuerySolution>>> plan(
        const CanonicalQuery& query, const QueryPlannerParams& params);

    /**
     * Generates and returns a query solution, given data retrieved from the plan cache.
     *
     * @param query -- query for which we are generating a plan
     * @param params -- planning parameters
     * @param cachedSoln -- the CachedSolution retrieved from the plan cache.
     */
    static StatusWith<std::unique_ptr<QuerySolution>> planFromCache(
        const CanonicalQuery& query,
        const QueryPlannerParams& params,
        const SolutionCacheData& solnCacheData);

    /**
     * Plan each branch of the rooted $or query independently, and return the resulting
     * lists of query solutions in 'SubqueriesPlanningResult'.
     *
     * The 'createPlanCacheKey' callback is used to create a plan cache key of the specified
     * 'KeyType' for each of the branches to look up the plan in the 'planCache'.
     */
    static StatusWith<SubqueriesPlanningResult> planSubqueries(
        OperationContext* opCtx,
        std::function<std::unique_ptr<SolutionCacheData>(
            const CanonicalQuery& cq, const CollectionPtr& coll)> getSolutionCachedData,
        const CollectionPtr& collection,
        const CanonicalQuery& query,
        const QueryPlannerParams& params);

    /**
     * Generates and returns the index tag tree that will be inserted into the plan cache. This data
     * gets stashed inside a QuerySolution until it can be inserted into the cache proper.
     *
     * @param taggedTree -- a MatchExpression with index tags that has been
     *   produced by the enumerator.
     * @param relevantIndices -- a list of the index entries used to tag
     *   the tree (i.e. index numbers in the tags refer to entries in this vector)
     */
    static StatusWith<std::unique_ptr<PlanCacheIndexTree>> cacheDataFromTaggedTree(
        const MatchExpression* taggedTree, const std::vector<IndexEntry>& relevantIndices);

    /**
     * @param filter -- an untagged MatchExpression
     * @param indexTree -- a tree structure retrieved from the
     *   cache with index tags that indicates how 'filter' should
     *   be tagged.
     * @param indexMap -- needed in order to put the proper index
     *   numbers inside the index tags
     *
     * On success, 'filter' is mutated so that it has all the
     * index tags needed in order for the access planner to recreate
     * the cached plan.
     *
     * On failure, the tag state attached to the nodes of 'filter'
     * is invalid. Planning from the cache should be aborted.
     *
     * Does not take ownership of either filter or indexTree.
     */
    static Status tagAccordingToCache(MatchExpression* filter,
                                      const PlanCacheIndexTree* indexTree,
                                      const std::map<IndexEntry::Identifier, size_t>& indexMap);

    /**
     * Uses the query planning results from QueryPlanner::planSubqueries() and the multi planner
     * callback to select the best plan for each branch.
     *
     * On success, returns a composite solution obtained by planning each $or branch independently.
     */
    static StatusWith<std::unique_ptr<QuerySolution>> choosePlanForSubqueries(
        const CanonicalQuery& query,
        const QueryPlannerParams& params,
        QueryPlanner::SubqueriesPlanningResult planningResult,
        std::function<StatusWith<std::unique_ptr<QuerySolution>>(
            CanonicalQuery* cq, std::vector<std::unique_ptr<QuerySolution>>)> multiplanCallback);
};

}  // namespace mongo
