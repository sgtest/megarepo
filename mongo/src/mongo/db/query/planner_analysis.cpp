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


#include "mongo/db/query/planner_analysis.h"

// IWYU pragma: no_include "boost/container/detail/std_fwd.hpp"
#include <boost/cstdint.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <cstdint>
#include <cstring>
#include <s2cellid.h>
// IWYU pragma: no_include "ext/alloc_traits.h"
#include <algorithm>
#include <ostream>
#include <set>
#include <type_traits>
#include <vector>

#include "mongo/base/checked_cast.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/simple_bsonelement_comparator.h"
#include "mongo/db/basic_types.h"
#include "mongo/db/bson/dotted_path_support.h"
#include "mongo/db/exec/document_value/document_metadata_fields.h"
#include "mongo/db/index/expression_params.h"
#include "mongo/db/index/multikey_paths.h"
#include "mongo/db/index/s2_common.h"
#include "mongo/db/index_names.h"
#include "mongo/db/matcher/expression.h"
#include "mongo/db/matcher/expression_geo.h"
#include "mongo/db/pipeline/dependencies.h"
#include "mongo/db/pipeline/field_path.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/index_bounds.h"
#include "mongo/db/query/interval.h"
#include "mongo/db/query/interval_evaluation_tree.h"
#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/projection.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/query/query_planner_common.h"
#include "mongo/db/query/query_request_helper.h"
#include "mongo/db/query/query_solution.h"
#include "mongo/db/query/stage_types.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo {

using std::endl;
using std::pair;
using std::string;
using std::unique_ptr;
using std::vector;

namespace dps = ::mongo::dotted_path_support;

//
// Helpers for bounds explosion AKA quick-and-dirty SERVER-1205.
//

namespace {

/**
 * Walk the tree 'root' and output all leaf nodes into 'leafNodes'.
 */
void getLeafNodes(QuerySolutionNode* root, vector<QuerySolutionNode*>* leafNodes) {
    if (0 == root->children.size()) {
        leafNodes->push_back(root);
    } else {
        for (size_t i = 0; i < root->children.size(); ++i) {
            getLeafNodes(root->children[i].get(), leafNodes);
        }
    }
}

/**
 * Determines if the query solution node 'node' is a FETCH node with an IXSCAN child node.
 */
bool isFetchNodeWithIndexScanChild(const QuerySolutionNode* node) {
    return (STAGE_FETCH == node->getType() && node->children.size() == 1 &&
            STAGE_IXSCAN == node->children[0]->getType());
}

/**
 * Walks the tree 'root' and outputs all nodes that can be considered for explosion for sort.
 * Outputs FETCH nodes with an IXSCAN node as a child as well as singular IXSCAN leaves without a
 * FETCH as a parent into 'explodableNodes'.
 */
void getExplodableNodes(QuerySolutionNode* root, vector<QuerySolutionNode*>* explodableNodes) {
    if (STAGE_IXSCAN == root->getType() || isFetchNodeWithIndexScanChild(root)) {
        explodableNodes->push_back(root);
    } else {
        for (auto&& childNode : root->children) {
            getExplodableNodes(childNode.get(), explodableNodes);
        }
    }
}

/**
 * Returns the IXSCAN node from the tree 'node' that can be either a IXSCAN node or a FETCH node
 * with an IXSCAN node as a child.
 */
const IndexScanNode* getIndexScanNode(const QuerySolutionNode* node) {
    if (STAGE_IXSCAN == node->getType()) {
        return static_cast<const IndexScanNode*>(node);
    } else if (isFetchNodeWithIndexScanChild(node)) {
        return static_cast<const IndexScanNode*>(node->children[0].get());
    }
    MONGO_UNREACHABLE;
    return nullptr;
}

/**
 * Returns true if every interval in 'oil' is a point, false otherwise.
 */
bool isUnionOfPoints(const OrderedIntervalList& oil) {
    // We can't explode if there are empty bounds. Don't consider the
    // oil a union of points if there are no intervals.
    if (0 == oil.intervals.size()) {
        return false;
    }

    for (size_t i = 0; i < oil.intervals.size(); ++i) {
        if (!oil.intervals[i].isPoint()) {
            return false;
        }
    }

    return true;
}

/**
 * Returns true if we are safe to explode the 'oil' with the corresponding 'iet' that will be
 * evaluated on different input parameters.
 */
bool isOilExplodable(const OrderedIntervalList& oil,
                     const boost::optional<interval_evaluation_tree::IET>& iet) {
    if (!isUnionOfPoints(oil)) {
        return false;
    }

    if (iet) {
        // In order for the IET to be evaluated to the same number of point intervals given any set
        // of input parameters, the IET needs to be either a const node, or an $eq/$in eval node.
        // Having union or intersection may result in different number of point intervals when the
        // IET is evaluated.
        if (const auto* ietEval = iet->cast<interval_evaluation_tree::EvalNode>(); ietEval) {
            return ietEval->matchType() == MatchExpression::EQ ||
                ietEval->matchType() == MatchExpression::MATCH_IN;
        } else if (iet->is<interval_evaluation_tree::ConstNode>()) {
            return true;
        } else {
            return false;
        }
    }

    return true;
}

/**
 * Should we try to expand the index scan(s) in 'solnRoot' to pull out an indexed sort?
 *
 * Returns the node which should be replaced by the merge sort of exploded scans, or nullptr
 * if there is no such node. The result is a pointer to a unique_ptr so that the caller
 * can replace the unique_ptr.
 */
std::unique_ptr<QuerySolutionNode>* structureOKForExplode(
    std::unique_ptr<QuerySolutionNode>* solnRoot) {
    // For now we only explode if we *know* we will pull the sort out.  We can look at
    // more structure (or just explode and recalculate properties and see what happens)
    // but for now we just explode if it's a sure bet.
    //
    // TODO: Can also try exploding if root is AND_HASH (last child dictates order.),
    // or other less obvious cases...

    // Skip over a sharding filter stage.
    if (STAGE_SHARDING_FILTER == (*solnRoot)->getType()) {
        solnRoot = &(*solnRoot)->children[0];
    }

    if (STAGE_IXSCAN == (*solnRoot)->getType()) {
        return solnRoot;
    }

    if (isFetchNodeWithIndexScanChild(solnRoot->get())) {
        return &(*solnRoot)->children[0];
    }

    // If we have a STAGE_OR, we can explode only when all children are either IXSCANs or FETCHes
    // that have an IXSCAN as a child.
    if (STAGE_OR == (*solnRoot)->getType()) {
        for (auto&& child : (*solnRoot)->children) {
            if (STAGE_IXSCAN != child->getType() && !isFetchNodeWithIndexScanChild(child.get())) {
                return nullptr;
            }
        }
        return solnRoot;
    }

    return nullptr;
}

/**
 * A pair of <PointPrefix, PrefixIndices> are returned from the Cartesian product function,
 * where PointPrefix is the list of point intervals that has been exploded, and the PrefixIndices is
 * the list of indices of each point interval in the original union of points OIL.
 *
 * For example, if the index bounds is {a: [[1, 1], [2, 2]], b: [[3, 3], c: [[MinKey, MaxKey]]},
 * then the two PointPrefix are: [[1, 1], [3, 3]] and [[2, 2], [3, 3]].
 * The two PrefixIndices are [0, 0] and [1, 0].
 */
typedef vector<Interval> PointPrefix;
typedef vector<size_t> PrefixIndices;

/**
 * The first 'fieldsToExplode' fields of 'bounds' are points.  Compute the Cartesian product
 * of those fields and place it in 'prefixOut'.
 */
void makeCartesianProduct(const IndexBounds& bounds,
                          size_t fieldsToExplode,
                          vector<pair<PointPrefix, PrefixIndices>>* prefixOut) {
    vector<pair<PointPrefix, PrefixIndices>> prefixForScans;

    // We dump the Cartesian product of bounds into prefixForScans, starting w/the first
    // field's points.
    invariant(fieldsToExplode >= 1);
    const OrderedIntervalList& firstOil = bounds.fields[0];
    invariant(firstOil.intervals.size() >= 1);
    for (size_t i = 0; i < firstOil.intervals.size(); ++i) {
        const Interval& ival = firstOil.intervals[i];
        invariant(ival.isPoint());
        PointPrefix pfix;
        pfix.push_back(ival);
        PrefixIndices pfixIndices;
        pfixIndices.push_back(i);
        prefixForScans.emplace_back(std::make_pair(std::move(pfix), std::move(pfixIndices)));
    }

    // For each subsequent field...
    for (size_t i = 1; i < fieldsToExplode; ++i) {
        vector<pair<PointPrefix, PrefixIndices>> newPrefixForScans;
        const OrderedIntervalList& oil = bounds.fields[i];
        invariant(oil.intervals.size() >= 1);
        // For each point interval in that field (all ivals must be points)...
        for (size_t j = 0; j < oil.intervals.size(); ++j) {
            const Interval& ival = oil.intervals[j];
            invariant(ival.isPoint());
            // Make a new scan by appending it to all scans in prefixForScans.
            for (size_t k = 0; k < prefixForScans.size(); ++k) {
                auto pfix = prefixForScans[k].first;
                pfix.push_back(ival);
                auto pfixIndicies = prefixForScans[k].second;
                pfixIndicies.push_back(j);
                newPrefixForScans.emplace_back(
                    std::make_pair(std::move(pfix), std::move(pfixIndicies)));
            }
        }
        // And update prefixForScans.
        newPrefixForScans.swap(prefixForScans);
    }

    prefixOut->swap(prefixForScans);
}

/**
 * Takes the provided 'node' (identified by 'nodeIndex'), either an IndexScanNode or FetchNode with
 * a direct child that is an IndexScanNode. Produces a list of new nodes, which are logically
 * equivalent to 'node' if joined by a MergeSort. Inserts these new nodes at the end of
 * 'explosionResult'.
 *
 * 'fieldsToExplode' is a count of how many fields in the scan's bounds are the union of point
 * intervals.  This is computed beforehand and provided as a small optimization.
 *
 * Example:
 *
 * For the query find({a: {$in: [1,2]}}).sort({b: 1}) using the index {a:1, b:1}:
 * 'node' will be a scan with multi-interval bounds a: [[1, 1], [2, 2]], b: [MinKey, MaxKey]
 * 'sort' will be {b: 1}
 * 'fieldsToExplode' will be 1 (as only one field isUnionOfPoints).
 *
 * The return value is whether the original index scan needs to be deduplicated.
 *
 * On return, 'explosionResult' will contain the following two scans:
 * a: [[1, 1]], b: [MinKey, MaxKey]
 * a: [[2, 2]], b: [MinKey, MaxKey]
 */
bool explodeNode(const QuerySolutionNode* node,
                 const size_t nodeIndex,
                 const BSONObj& sort,
                 size_t fieldsToExplode,
                 vector<std::unique_ptr<QuerySolutionNode>>* explosionResult) {
    // Get the 'isn' from either the FetchNode or IndexScanNode.
    const IndexScanNode* isn = getIndexScanNode(node);

    // Turn the compact bounds in 'isn' into a bunch of points...
    vector<pair<PointPrefix, PrefixIndices>> prefixForScans;
    makeCartesianProduct(isn->bounds, fieldsToExplode, &prefixForScans);

    for (size_t i = 0; i < prefixForScans.size(); ++i) {
        const PointPrefix& prefix = prefixForScans[i].first;
        const PrefixIndices& prefixIndices = prefixForScans[i].second;
        invariant(prefix.size() == fieldsToExplode);
        invariant(prefixIndices.size() == fieldsToExplode);

        // Copy boring fields into new child.
        auto child = std::make_unique<IndexScanNode>(isn->index);
        child->direction = isn->direction;
        child->addKeyMetadata = isn->addKeyMetadata;
        child->queryCollator = isn->queryCollator;

        // Set up the IET of children when the original index scan has IET.
        if (!isn->iets.empty()) {
            // Set the explosion index for the exploded IET so that they can be evaluated to the
            // correct point interval. When present, the caller should already have verified that
            // the IETs are the correct shape (i.e. derived from an $in or $eq predicate) so that
            // they are safe to explode.
            for (size_t pidx = 0; pidx < prefixIndices.size(); pidx++) {
                invariant(pidx < isn->iets.size());
                const auto& iet = isn->iets[pidx];
                auto needsExplodeNode = [&]() {
                    if (const auto* ietEval = iet.cast<interval_evaluation_tree::EvalNode>();
                        ietEval) {
                        return ietEval->matchType() == MatchExpression::MATCH_IN;
                    } else if (const auto* ietConst =
                                   iet.cast<interval_evaluation_tree::ConstNode>();
                               ietConst) {
                        return ietConst->oil.intervals.size() > 1;
                    }
                    MONGO_UNREACHABLE;
                }();

                if (needsExplodeNode) {
                    auto ietExplode =
                        interval_evaluation_tree::IET::make<interval_evaluation_tree::ExplodeNode>(
                            iet, std::make_pair(nodeIndex, pidx), prefixIndices[pidx]);
                    child->iets.push_back(std::move(ietExplode));
                } else {
                    child->iets.push_back(std::move(iet));
                }
            }
            // Copy the rest of the unexploded IETs directly into the new child.
            for (size_t pidx = prefixIndices.size(); pidx < isn->iets.size(); pidx++) {
                child->iets.push_back(isn->iets[pidx]);
            }
        }

        // Copy the filter, if there is one.
        if (isn->filter.get()) {
            child->filter = isn->filter->clone();
        }

        // Create child bounds.
        child->bounds.fields.resize(isn->bounds.fields.size());
        for (size_t j = 0; j < fieldsToExplode; ++j) {
            child->bounds.fields[j].intervals.push_back(prefix[j]);
            child->bounds.fields[j].name = isn->bounds.fields[j].name;
        }
        for (size_t j = fieldsToExplode; j < isn->bounds.fields.size(); ++j) {
            child->bounds.fields[j] = isn->bounds.fields[j];
        }

        // If the explosion is on a FetchNode, make a copy and add the 'isn' as a child.
        if (STAGE_FETCH == node->getType()) {
            auto origFetchNode = static_cast<const FetchNode*>(node);
            auto newFetchNode = std::make_unique<FetchNode>();

            // Copy the FETCH's filter, if it exists.
            if (origFetchNode->filter.get()) {
                newFetchNode->filter = origFetchNode->filter->clone();
            }

            // Add the 'child' IXSCAN under the FETCH stage, and the FETCH stage to the result set.
            newFetchNode->children.push_back(std::move(child));
            explosionResult->push_back(std::move(newFetchNode));
        } else {
            explosionResult->push_back(std::move(child));
        }
    }

    return isn->shouldDedup;
}

void geoSkipValidationOn(const std::set<StringData>& twoDSphereFields,
                         QuerySolutionNode* solnRoot) {
    // If there is a GeoMatchExpression in the tree on a field with a 2dsphere index,
    // we can skip validation since it was validated on insertion. This only applies to
    // 2dsphere index version >= 3.
    //
    // This does not mean that there is necessarily an IXSCAN using this 2dsphere index,
    // only that there exists a 2dsphere index on this field.
    MatchExpression* expr = solnRoot->filter.get();
    if (expr) {
        StringData nodeField = expr->path();
        if (expr->matchType() == MatchExpression::GEO &&
            twoDSphereFields.find(nodeField) != twoDSphereFields.end()) {
            GeoMatchExpression* gme = static_cast<GeoMatchExpression*>(expr);
            gme->setCanSkipValidation(true);
        }
    }

    for (auto&& child : solnRoot->children) {
        geoSkipValidationOn(twoDSphereFields, child.get());
    }
}

/**
 * If any field is missing from the list of fields the projection wants, we are not covered.
 */
auto providesAllFields(const OrderedPathSet& fields, const QuerySolutionNode& solnRoot) {
    for (auto&& field : fields) {
        if (!solnRoot.hasField(field))
            return false;
    }
    return true;
}

/**
 * If 'solnRoot' is returning index key data from a single index, returns the associated index key
 * pattern. Otherwise, returns an empty object.
 */
auto produceCoveredKeyObj(QuerySolutionNode* solnRoot) {
    vector<QuerySolutionNode*> leafNodes;
    getLeafNodes(solnRoot, &leafNodes);

    // Both the IXSCAN and DISTINCT stages provide covered key data.
    if (1 == leafNodes.size()) {
        if (STAGE_IXSCAN == leafNodes[0]->getType()) {
            IndexScanNode* ixn = static_cast<IndexScanNode*>(leafNodes[0]);
            return ixn->index.keyPattern;
        } else if (STAGE_DISTINCT_SCAN == leafNodes[0]->getType()) {
            DistinctNode* dn = static_cast<DistinctNode*>(leafNodes[0]);
            return dn->index.keyPattern;
        }
    }
    return BSONObj();
}

/**
 * Adds a stage to generate the sort key metadata if there's no sort stage but we have a sortKey
 * meta-projection.
 */
std::unique_ptr<QuerySolutionNode> addSortKeyGeneratorStageIfNeeded(
    const CanonicalQuery& query, bool hasSortStage, std::unique_ptr<QuerySolutionNode> solnRoot) {
    if (!hasSortStage && query.metadataDeps()[DocumentMetadataFields::kSortKey]) {
        auto keyGenNode = std::make_unique<SortKeyGeneratorNode>();
        keyGenNode->sortSpec = query.getFindCommandRequest().getSort();
        keyGenNode->children.push_back(std::move(solnRoot));
        return keyGenNode;
    }
    return solnRoot;
}

/**
 * Returns a pointer to a COLUMN_SCAN node if there is one. Returns nullptr if it cannot be found or
 * if there is any branching in the tree that would lead to more than one leaf node.
 */
const ColumnIndexScanNode* treeSourceIsColumnScan(const QuerySolutionNode* root) {
    if (root->getType() == StageType::STAGE_COLUMN_SCAN) {
        return static_cast<const ColumnIndexScanNode*>(root);
    }

    // Non-branching trees only, intentionally ignore >1 child.
    if (root->children.size() == 1) {
        return treeSourceIsColumnScan(root->children[0].get());
    }
    return nullptr;
}

/**
 * When a projection needs to be added to the solution tree, this function chooses between the
 * default implementation and one of the fast paths.
 */
std::unique_ptr<QuerySolutionNode> analyzeProjection(const CanonicalQuery& query,
                                                     std::unique_ptr<QuerySolutionNode> solnRoot,
                                                     const bool hasSortStage) {
    LOGV2_DEBUG(20949, 5, "PROJECTION: Current plan", "plan"_attr = redact(solnRoot->toString()));

    const auto& projection = *query.getProj();

    // If the projection requires the entire document we add a fetch stage if not present. Otherwise
    // we add a fetch stage if we are not covered.
    if (!solnRoot->fetched() &&
        (projection.requiresDocument() ||
         !providesAllFields(projection.getRequiredFields(), *solnRoot))) {
        auto fetch = std::make_unique<FetchNode>();
        fetch->children.push_back(std::move(solnRoot));
        solnRoot = std::move(fetch);
    }

    // With the previous fetch analysis we know we have all the required fields. We know we have a
    // projection specified, so we may need a projection node in the tree for any or multiple of the
    // following reasons:
    // - We have provided too many fields. Maybe we have the full document from a FETCH, or the
    //   index scan is compound and has an extra field or two, or maybe some fields were needed
    //   internally that the client didn't request.
    // - We have a projection which computes new values using expressions - a "computed projection".
    // - Finally, we could have the right data, but not in the format required to return to the
    //   client. As one example: The format of data returned in index keys is meant for internal
    //   consumers and is not returnable to a user.

    // A generic "ProjectionNodeDefault" will take care of any of the three, but is slower due to
    // its generic nature. We will attempt to avoid that for some "fast paths" first.
    // All fast paths can only apply to "simple" projections - see the implementation for details.
    if (projection.isSimple()) {
        const bool isInclusionOnly = projection.isInclusionOnly();
        // First fast path: We have a COLUMN_SCAN providing the data, there are no computed
        // expressions, and the requested fields are provided exactly. For 'simple' projections
        // which must have only top-level fields, A COLUMN_SCAN can provide data in a format safe to
        // return to the client, so it is safe to elide any projection if the COLUMN_SCAN is
        // outputting exactly the set of fields that the user required. This may not be the case all
        // the time if say we needed an extra field for a sort or for shard filtering.
        const auto* columnScan = treeSourceIsColumnScan(solnRoot.get());
        if (columnScan && isInclusionOnly &&
            columnScan->outputFields.size() == projection.getRequiredFields().size()) {
            // No projection needed. We already checked that all necessary fields are provided, so
            // if the set sizes match, they match exactly.
            return solnRoot;
        }

        // Next fast path: A ProjectionNodeSimple fast-path for plans that have a materialized
        // object from a FETCH or COLUMN_SCAN stage.
        if (solnRoot->fetched() || columnScan != nullptr) {
            // COLUMN_SCAN may fall into this case if it provided all the necessary data but had
            // too many fields output, so we need to trim them down.
            return std::make_unique<ProjectionNodeSimple>(
                addSortKeyGeneratorStageIfNeeded(query, hasSortStage, std::move(solnRoot)),
                query.root(),
                projection);
        }
        if (isInclusionOnly) {
            auto coveredKeyObj = produceCoveredKeyObj(solnRoot.get());
            if (!coveredKeyObj.isEmpty()) {
                // Final fast path: ProjectionNodeCovered for plans with an index scan that the
                // projection can cover.
                return std::make_unique<ProjectionNodeCovered>(
                    addSortKeyGeneratorStageIfNeeded(query, hasSortStage, std::move(solnRoot)),
                    query.root(),
                    projection,
                    std::move(coveredKeyObj));
            }
        }
    }

    // No fast path available, we need to add this generic projection node.
    return std::make_unique<ProjectionNodeDefault>(
        addSortKeyGeneratorStageIfNeeded(query, hasSortStage, std::move(solnRoot)),
        query.root(),
        projection);
}

/**
 * Given the solution tree 'root', attempts to push a projection at the root of the tree beneath a
 * SORT node. Returns the tree with this optimization applied, or the unmodified tree if the
 * optimization was not legal.
 *
 * Applying the projection before the sort is beneficial when it reduces the amount of data that
 * needs to be sorted.
 */
std::unique_ptr<QuerySolutionNode> tryPushdownProjectBeneathSort(
    std::unique_ptr<QuerySolutionNode> root) {
    if (!isProjectionStageType(root->getType())) {
        // There's no projection to push down.
        return root;
    }

    auto projectNode = static_cast<ProjectionNode*>(root.get());
    if (projectNode->proj.hasExpressions()) {
        // If the projection has any expressions, then we refrain from moving it underneath the
        // sort. It's possible that the addition of computed fields increases the size of the data
        // to sort, in which case it would be better to sort first and then project.
        return root;
    }

    // There could be a situation when there is a SKIP stage between PROJECT and SORT:
    //   PROJECT => SKIP => SORT
    // In this case we still want to push PROJECT beneath SORT.
    bool hasSkipBetween = false;
    QuerySolutionNode* sortNodeCandidate = projectNode->children[0].get();
    if (sortNodeCandidate->getType() == STAGE_SKIP) {
        hasSkipBetween = true;
        sortNodeCandidate = sortNodeCandidate->children[0].get();
    }

    if (!isSortStageType(sortNodeCandidate->getType())) {
        return root;
    }

    auto sortNode = static_cast<SortNode*>(sortNodeCandidate);

    // Don't perform this optimization if the sort is a top-k sort. We would be wasting work
    // computing projections for documents that are discarded since they are not in the top-k set.
    if (sortNode->limit > 0) {
        return root;
    }

    // It is only legal to push down the projection it if preserves all of the fields on which we
    // need to sort.
    for (auto&& sortComponent : sortNode->pattern) {
        if (!projectNode->hasField(sortComponent.fieldNameStringData().toString())) {
            return root;
        }
    }

    // Perform the swap. We are starting with the following structure:
    //   PROJECT => SORT => CHILD
    // Or if there is a SKIP stage between PROJECT and SORT:
    //   PROJECT => SKIP => SORT => CHILD
    //
    // This needs to be transformed to the following:
    //   SORT => PROJECT => CHILD
    // Or to the following in case of SKIP:
    //   SKIP => SORT => PROJECT => CHILD
    //
    // First, detach the bottom of the tree. This part is CHILD in the comment above.
    std::unique_ptr<QuerySolutionNode> restOfTree = std::move(sortNode->children[0]);
    invariant(sortNode->children.size() == 1u);
    sortNode->children.clear();

    // Next, detach the input from the projection and assume ownership of it.
    // The projection input is either this structure:
    //   SORT
    // Or this if we have SKIP:
    //   SKIP => SORT
    std::unique_ptr<QuerySolutionNode> ownedProjectionInput = std::move(projectNode->children[0]);
    sortNode = nullptr;
    invariant(projectNode->children.size() == 1u);
    projectNode->children.clear();

    // Attach the lower part of the tree as the child of the projection.
    // We want to get the following structure:
    //   PROJECT => CHILD
    std::unique_ptr<QuerySolutionNode> ownedProjectionNode = std::move(root);
    ownedProjectionNode->children.push_back(std::move(restOfTree));

    // Attach the projection as the child of the sort stage.
    if (hasSkipBetween) {
        // In this case 'ownedProjectionInput' points to the structure:
        //   SKIP => SORT
        // And to attach PROJECT => CHILD to it, we need to access children of SORT stage.
        ownedProjectionInput->children[0]->children.push_back(std::move(ownedProjectionNode));
    } else {
        // In this case 'ownedProjectionInput' points to the structure:
        //   SORT
        // And we can just add PROJECT => CHILD to its children.
        ownedProjectionInput->children.push_back(std::move(ownedProjectionNode));
    }

    // Re-compute properties so that they reflect the new structure of the tree.
    ownedProjectionInput->computeProperties();

    return ownedProjectionInput;
}

bool canUseSimpleSort(const QuerySolutionNode& solnRoot,
                      const CanonicalQuery& cq,
                      const QueryPlannerParams& plannerParams) {
    // The simple sort stage discards any metadata other than sort key metadata. It can only be used
    // if there are no metadata dependencies, or the only metadata dependency is a 'kSortKey'
    // dependency.
    const bool metadataDepsCompatible = cq.metadataDeps().none() ||
        (cq.metadataDeps().count() == 1u && cq.metadataDeps()[DocumentMetadataFields::kSortKey]);

    return solnRoot.fetched() && metadataDepsCompatible &&
        // For performance, the simple sort stage discards any incoming record ids. Carrying the
        // record ids along through the sorting process is wasted work when these ids will never be
        // consumed later in the execution of the query. If the record ids are needed, however, then
        // we can't use the simple sort stage.
        !cq.getForceGenerateRecordId();
}

boost::optional<const projection_ast::Projection*> attemptToGetProjectionFromQuerySolution(
    const QuerySolutionNode& projectNodeCandidate) {
    switch (projectNodeCandidate.getType()) {
        case StageType::STAGE_PROJECTION_DEFAULT:
            return &static_cast<const ProjectionNodeDefault*>(&projectNodeCandidate)->proj;
        case StageType::STAGE_PROJECTION_SIMPLE:
            return &static_cast<const ProjectionNodeSimple*>(&projectNodeCandidate)->proj;
        default:
            return boost::none;
    }
}

/**
 * Returns true if 'setL' is a non-strict subset of 'setR'.
 *
 * The types of the sets are permitted to be different to allow checking something with compatible
 * but different types e.g. std::set<std::string> and StringDataUnorderedMap.
 */
template <typename SetL, typename SetR>
bool isSubset(const SetL& setL, const SetR& setR) {
    return setL.size() <= setR.size() &&
        std::all_of(setL.begin(), setL.end(), [&setR](auto&& lElem) {
               return setR.find(lElem) != setR.end();
           });
}

void removeInclusionProjectionBelowGroupRecursive(QuerySolutionNode* solnRoot) {
    if (solnRoot == nullptr) {
        return;
    }

    // Look for a GROUP => PROJECTION_SIMPLE where the dependency set of the PROJECTION_SIMPLE
    // is a super set of the dependency set of the GROUP. If so, the projection isn't needed and
    // it can be elminated.
    if (solnRoot->getType() == StageType::STAGE_GROUP) {
        auto groupNode = static_cast<GroupNode*>(solnRoot);

        QuerySolutionNode* projectNodeCandidate = groupNode->children[0].get();
        if (auto projection = attemptToGetProjectionFromQuerySolution(*projectNodeCandidate);
            // only eliminate inclusion projections
            projection && projection.value()->isInclusionOnly() &&
            // only eliminate when group depends on a subset of fields
            !groupNode->needWholeDocument &&
            // only eliminate projections which preserve all fields used by the group
            isSubset(groupNode->requiredFields, projection.value()->getRequiredFields())) {
            // Attach the projectNode's child directly as the groupNode's child, eliminating the
            // project node.
            groupNode->children[0] = std::move(projectNodeCandidate->children[0]);
        }
    }

    // Keep traversing the tree in search of GROUP stages.
    for (size_t i = 0; i < solnRoot->children.size(); ++i) {
        removeInclusionProjectionBelowGroupRecursive(solnRoot->children[i].get());
    }
}

// Determines whether 'index' is eligible for executing the right side of a pushed down $lookup over
// 'foreignField'.
bool isIndexEligibleForRightSideOfLookupPushdown(const IndexEntry& index,
                                                 const CollatorInterface* collator,
                                                 const std::string& foreignField) {
    return (index.type == INDEX_BTREE || index.type == INDEX_HASHED) &&
        index.keyPattern.firstElement().fieldName() == foreignField && !index.filterExpr &&
        !index.sparse && CollatorInterface::collatorsMatch(collator, index.collator);
}

/**
 * Sets the lowPriority parameter on the given node if it is an unbounded collection scan.
 */
void deprioritizeUnboundedCollectionScan(QuerySolutionNode* solnRoot,
                                         const FindCommandRequest& findCommand) {
    if (solnRoot->getType() != StageType::STAGE_COLLSCAN) {
        return;
    }

    auto sort = findCommand.getSort();
    if (findCommand.getLimit() &&
        (sort.isEmpty() || sort[query_request_helper::kNaturalSortField])) {
        // There is a limit with either no sort or the natural sort.
        return;
    }

    auto collScan = checked_cast<CollectionScanNode*>(solnRoot);
    if (collScan->minRecord || collScan->maxRecord) {
        // This scan is not unbounded.
        return;
    }

    collScan->lowPriority = true;
}
}  // namespace

bool QueryPlannerAnalysis::isEligibleForHashJoin(const SecondaryCollectionInfo& foreignCollInfo) {
    return !internalQueryDisableLookupExecutionUsingHashJoin.load() && foreignCollInfo.exists &&
        foreignCollInfo.stats.noOfRecords <=
        internalQueryCollectionMaxNoOfDocumentsToChooseHashJoin.load() &&
        foreignCollInfo.stats.approximateDataSizeBytes <=
        internalQueryCollectionMaxDataSizeBytesToChooseHashJoin.load() &&
        foreignCollInfo.stats.storageSizeBytes <=
        internalQueryCollectionMaxStorageSizeBytesToChooseHashJoin.load();
}

// static
std::unique_ptr<QuerySolution> QueryPlannerAnalysis::removeInclusionProjectionBelowGroup(
    std::unique_ptr<QuerySolution> soln) {
    auto root = soln->extractRoot();

    removeInclusionProjectionBelowGroupRecursive(root.get());

    soln->setRoot(std::move(root));
    return soln;
}

// static
void QueryPlannerAnalysis::removeUselessColumnScanRowStoreExpression(QuerySolutionNode& root) {
    // If a group or projection's child is a COLUMN_SCAN node, try to eliminate the
    // expression that projects documents retrieved from row store fallback. In other words, the
    // COLUMN_SCAN's rowStoreExpression can be removed if it does not affect the group or
    // project above.
    for (auto& child : root.children) {
        if (child->getType() == STAGE_COLUMN_SCAN) {
            auto childColumnScan = static_cast<ColumnIndexScanNode*>(child.get());

            // Look for group above column scan.
            if (root.getType() == STAGE_GROUP) {
                auto& parentGroup = static_cast<GroupNode&>(root);
                // A row store expression that preserves all fields used by the parent $group is
                // redundant and can be removed.
                if (!childColumnScan->extraFieldsPermitted &&
                    isSubset(parentGroup.requiredFields, childColumnScan->outputFields)) {
                    childColumnScan->extraFieldsPermitted = true;
                }
            }
            // Look for projection above column scan.
            else if (root.getType() == STAGE_PROJECTION_SIMPLE ||
                     root.getType() == STAGE_PROJECTION_DEFAULT) {
                auto& parentProjection = static_cast<ProjectionNode&>(root);
                // A row store expression that preserves all fields used by the parent projection is
                // redundant and can be removed.
                if (!childColumnScan->extraFieldsPermitted &&
                    isSubset(parentProjection.proj.getRequiredFields(),
                             childColumnScan->outputFields)) {
                    childColumnScan->extraFieldsPermitted = true;
                }
            }
        }
        // Recur on child.
        removeUselessColumnScanRowStoreExpression(*child);
    }
}

// static
std::pair<EqLookupNode::LookupStrategy, boost::optional<IndexEntry>>
QueryPlannerAnalysis::determineLookupStrategy(
    const NamespaceString& foreignCollName,
    const std::string& foreignField,
    const std::map<NamespaceString, SecondaryCollectionInfo>& collectionsInfo,
    bool allowDiskUse,
    const CollatorInterface* collator) {
    auto foreignCollItr = collectionsInfo.find(foreignCollName);
    tassert(5842600,
            str::stream() << "Expected collection info, but found none; target collection: "
                          << foreignCollName.toStringForErrorMsg(),
            foreignCollItr != collectionsInfo.end());

    // Check if an eligible index exists for indexed loop join strategy.
    const auto foreignIndex = [&]() -> boost::optional<IndexEntry> {
        // Sort indexes by (# of components, index type, index key pattern) tuple.
        auto indexes = foreignCollItr->second.indexes;
        std::sort(
            indexes.begin(), indexes.end(), [](const IndexEntry& left, const IndexEntry& right) {
                const auto nFieldsLeft = left.keyPattern.nFields();
                const auto nFieldsRight = right.keyPattern.nFields();
                if (nFieldsLeft != nFieldsRight) {
                    return nFieldsLeft < nFieldsRight;
                } else if (left.type != right.type) {
                    // Here we rely on the fact that 'INDEX_BTREE < INDEX_HASHED'.
                    return left.type < right.type;
                }

                // This is a completely arbitrary tie breaker to make the selection algorithm
                // deterministic.
                return left.keyPattern.woCompare(right.keyPattern) < 0;
            });

        for (const auto& index : indexes) {
            if (isIndexEligibleForRightSideOfLookupPushdown(index, collator, foreignField)) {
                return index;
            }
        }

        return boost::none;
    }();

    if (!foreignCollItr->second.exists) {
        return {EqLookupNode::LookupStrategy::kNonExistentForeignCollection, boost::none};
    } else if (foreignIndex) {
        return {EqLookupNode::LookupStrategy::kIndexedLoopJoin, std::move(foreignIndex)};
    } else if (allowDiskUse && isEligibleForHashJoin(foreignCollItr->second)) {
        return {EqLookupNode::LookupStrategy::kHashJoin, boost::none};
    } else {
        return {EqLookupNode::LookupStrategy::kNestedLoopJoin, boost::none};
    }
}

// static
void QueryPlannerAnalysis::analyzeGeo(const QueryPlannerParams& params,
                                      QuerySolutionNode* solnRoot) {
    // Get field names of all 2dsphere indexes with version >= 3.
    std::set<StringData> twoDSphereFields;
    for (const IndexEntry& indexEntry : params.indices) {
        if (indexEntry.type != IndexType::INDEX_2DSPHERE) {
            continue;
        }

        S2IndexingParams params;
        ExpressionParams::initialize2dsphereParams(
            indexEntry.infoObj, indexEntry.collator, &params);

        if (params.indexVersion < S2_INDEX_VERSION_3) {
            continue;
        }

        for (auto& elt : indexEntry.keyPattern) {
            if (elt.type() == BSONType::String && elt.String() == "2dsphere") {
                twoDSphereFields.insert(elt.fieldName());
            }
        }
    }
    if (twoDSphereFields.size() > 0) {
        geoSkipValidationOn(twoDSphereFields, solnRoot);
    }
}

BSONObj QueryPlannerAnalysis::getSortPattern(const BSONObj& indexKeyPattern) {
    BSONObjBuilder sortBob;
    BSONObjIterator kpIt(indexKeyPattern);
    while (kpIt.more()) {
        BSONElement elt = kpIt.next();
        if (elt.type() == mongo::String) {
            break;
        }
        // The canonical check as to whether a key pattern element is "ascending" or "descending" is
        // (elt.number() >= 0). This is defined by the Ordering class.
        int sortOrder = (elt.number() >= 0) ? 1 : -1;
        sortBob.append(elt.fieldName(), sortOrder);
    }
    return sortBob.obj();
}

// static
bool QueryPlannerAnalysis::explodeForSort(const CanonicalQuery& query,
                                          const QueryPlannerParams& params,
                                          std::unique_ptr<QuerySolutionNode>* solnRoot) {
    vector<QuerySolutionNode*> explodableNodes;

    std::unique_ptr<QuerySolutionNode>* toReplace = structureOKForExplode(solnRoot);
    if (!toReplace)
        return false;

    // Find explodable nodes in the subtree rooted at 'toReplace'.
    getExplodableNodes(toReplace->get(), &explodableNodes);

    const BSONObj& desiredSort = query.getFindCommandRequest().getSort();

    // How many scan leaves will result from our expansion?
    size_t totalNumScans = 0;

    // The value of entry i is how many scans we want to blow up for explodableNodes[i]. We
    // calculate this in the loop below and might as well reuse it if we blow up that scan.
    vector<size_t> fieldsToExplode;

    // The sort order we're looking for has to possibly be provided by each of the index scans
    // upon explosion.
    for (auto&& explodableNode : explodableNodes) {
        // We can do this because structureOKForExplode is only true if the leaves are index
        // scans.
        IndexScanNode* isn = const_cast<IndexScanNode*>(getIndexScanNode(explodableNode));
        const IndexBounds& bounds = isn->bounds;

        // Not a point interval prefix, can't try to rewrite.
        if (bounds.isSimpleRange) {
            return false;
        }

        if (isn->index.multikey && isn->index.multikeyPaths.empty()) {
            // The index is multikey but has no path-level multikeyness metadata. In this case, the
            // index can never provide a sort.
            return false;
        }

        // How many scans will we create if we blow up this ixscan?
        size_t numScans = 1;

        // Skip every field that is a union of point intervals. When the index scan is
        // parameterized, we need to check IET instead of the index bounds alone because we need to
        // make sure the same number of exploded index scans will result given any set of input
        // parameters. So that when the plan is recovered from cache and parameterized, we will be
        // sure to have the same number of sort merge branches.
        BSONObjIterator kpIt(isn->index.keyPattern);
        size_t boundsIdx = 0;
        while (kpIt.more()) {
            const auto& oil = bounds.fields[boundsIdx];
            boost::optional<interval_evaluation_tree::IET> iet;
            if (!isn->iets.empty()) {
                invariant(boundsIdx < isn->iets.size());
                iet = isn->iets[boundsIdx];
            }
            if (!isOilExplodable(oil, iet)) {
                break;
            }
            numScans *= oil.intervals.size();
            kpIt.next();
            ++boundsIdx;
        }

        // There's no sort order left to gain by exploding.  Just go home.  TODO: verify nothing
        // clever we can do here.
        if (!kpIt.more()) {
            return false;
        }

        // Only explode if there's at least one field to explode for this scan.
        if (0 == boundsIdx) {
            return false;
        }

        // The rest of the fields define the sort order we could obtain by exploding
        // the bounds.
        BSONObjBuilder resultingSortBob;
        while (kpIt.more()) {
            auto elem = kpIt.next();
            if (isn->multikeyFields.find(elem.fieldNameStringData()) != isn->multikeyFields.end()) {
                // One of the indexed fields providing the sort is multikey. It is not correct for a
                // field with multikey components to provide a sort, so bail out.
                return false;
            }
            resultingSortBob.append(elem);
        }

        // See if it's the order we're looking for.
        BSONObj possibleSort = resultingSortBob.obj();
        if (!desiredSort.isPrefixOf(possibleSort, SimpleBSONElementComparator::kInstance)) {
            // We can't get the sort order from the index scan. See if we can
            // get the sort by reversing the scan.
            BSONObj reversePossibleSort = QueryPlannerCommon::reverseSortObj(possibleSort);
            if (!desiredSort.isPrefixOf(reversePossibleSort,
                                        SimpleBSONElementComparator::kInstance)) {
                // Can't get the sort order from the reversed index scan either. Give up.
                return false;
            } else {
                // We can get the sort order we need if we reverse the scan.
                QueryPlannerCommon::reverseScans(isn);
            }
        }

        // An index whose collation does not match the query's cannot provide a sort if sort-by
        // fields can contain collatable values.
        if (!CollatorInterface::collatorsMatch(isn->index.collator, query.getCollator())) {
            auto fieldsWithStringBounds =
                IndexScanNode::getFieldsWithStringBounds(bounds, isn->index.keyPattern);
            for (auto&& element : desiredSort) {
                if (fieldsWithStringBounds.count(element.fieldNameStringData()) > 0) {
                    // The field can contain collatable values and therefore we cannot use the index
                    // to provide the sort.
                    return false;
                }
            }
        }

        // Do some bookkeeping to see how many ixscans we'll create total.
        totalNumScans += numScans;

        // And for this scan how many fields we expand.
        fieldsToExplode.push_back(boundsIdx);
    }

    // Too many ixscans spoil the performance.
    if (totalNumScans > (size_t)internalQueryMaxScansToExplode.load()) {
        (*solnRoot)->hitScanLimit = true;
        LOGV2_DEBUG(
            20950,
            5,
            "Could expand ixscans to pull out sort order but resulting scan count is too high",
            "numScans"_attr = totalNumScans);
        return false;
    }

    // If we're here, we can (probably?  depends on how restrictive the structure check is)
    // get our sort order via ixscan blow-up.
    auto merge = std::make_unique<MergeSortNode>();
    merge->sort = desiredSort;

    // Exploded nodes all take different point prefix so they should produce disjoint results.
    // We only deduplicate if some original index scans need to deduplicate.
    merge->dedup = false;
    for (size_t i = 0; i < explodableNodes.size(); ++i) {
        if (explodeNode(explodableNodes[i], i, desiredSort, fieldsToExplode[i], &merge->children)) {
            merge->dedup = true;
        }
    }

    merge->computeProperties();

    *toReplace = std::move(merge);

    return true;
}

// This function is used to check if the given index pattern and direction in the traversal
// preference can be used to satisfy the given sort pattern (specifically for time series
// collections).
bool sortMatchesTraversalPreference(const TraversalPreference& traversalPreference,
                                    const BSONObj& indexPattern) {
    BSONObjIterator sortIter(traversalPreference.sortPattern);
    BSONObjIterator indexIter(indexPattern);
    while (sortIter.more() && indexIter.more()) {
        BSONElement sortPart = sortIter.next();
        BSONElement indexPart = indexIter.next();

        if (!sortPart.isNumber() || !indexPart.isNumber()) {
            return false;
        }

        // If the field doesn't match or the directions don't match, we return false.
        if (strcmp(sortPart.fieldName(), indexPart.fieldName()) != 0 ||
            (sortPart.safeNumberInt() > 0) != (indexPart.safeNumberInt() > 0)) {
            return false;
        }
    }

    if (!indexIter.more() && sortIter.more()) {
        // The sort still has more, so it cannot be a prefix of the index.
        return false;
    }
    return true;
}

bool isShardedCollScan(QuerySolutionNode* solnRoot) {
    return solnRoot->getType() == StageType::STAGE_SHARDING_FILTER &&
        solnRoot->children.size() == 1 &&
        solnRoot->children[0]->getType() == StageType::STAGE_COLLSCAN;
}

// static
std::unique_ptr<QuerySolutionNode> QueryPlannerAnalysis::analyzeSort(
    const CanonicalQuery& query,
    const QueryPlannerParams& params,
    std::unique_ptr<QuerySolutionNode> solnRoot,
    bool* blockingSortOut) {
    *blockingSortOut = false;

    const FindCommandRequest& findCommand = query.getFindCommandRequest();
    if (params.traversalPreference) {
        // If we've been passed a traversal preference, we might want to reverse the order we scan
        // the data to avoid a blocking sort later in the pipeline.
        auto providedSorts = solnRoot->providedSorts();

        BSONObj solnSortPattern;
        if (solnRoot->getType() == StageType::STAGE_COLLSCAN || isShardedCollScan(solnRoot.get())) {
            BSONObjBuilder builder;
            builder.append(params.traversalPreference->clusterField, 1);
            solnSortPattern = builder.obj();
        } else {
            solnSortPattern = providedSorts.getBaseSortPattern();
        }

        if (sortMatchesTraversalPreference(params.traversalPreference.value(), solnSortPattern) &&
            QueryPlannerCommon::scanDirectionsEqual(solnRoot.get(),
                                                    -params.traversalPreference->direction)) {
            QueryPlannerCommon::reverseScans(solnRoot.get(), true);
            return solnRoot;
        }
    }

    const BSONObj& sortObj = findCommand.getSort();

    if (sortObj.isEmpty()) {
        return solnRoot;
    }

    // TODO: We could check sortObj for any projections other than :1 and :-1
    // and short-cut some of this.

    // If the sort is $natural, we ignore it, assuming that the caller has detected that and
    // outputted a collscan to satisfy the desired order.
    if (sortObj[query_request_helper::kNaturalSortField]) {
        return solnRoot;
    }

    // See if solnRoot gives us the sort.  If so, we're done.
    auto providedSorts = solnRoot->providedSorts();
    if (providedSorts.contains(sortObj)) {
        return solnRoot;
    }

    // Sort is not provided.  See if we provide the reverse of our sort pattern.
    // If so, we can reverse the scan direction(s).
    BSONObj reverseSort = QueryPlannerCommon::reverseSortObj(sortObj);
    // The only collection scan that includes a sort order in 'providedSorts' is a scan on a
    // clustered collection. However, we cannot reverse this scan if its direction is specified by a
    // $natural hint.
    const bool naturalCollScan = solnRoot->getType() == StageType::STAGE_COLLSCAN &&
        query.getFindCommandRequest().getHint()[query_request_helper::kNaturalSortField];
    if (providedSorts.contains(reverseSort) && !naturalCollScan) {
        QueryPlannerCommon::reverseScans(solnRoot.get());
        LOGV2_DEBUG(20951,
                    5,
                    "Reversing ixscan to provide sort",
                    "newPlan"_attr = redact(solnRoot->toString()));
        return solnRoot;
    }

    // Sort not provided, can't reverse scans to get the sort.  One last trick: We can "explode"
    // index scans over point intervals to an OR of sub-scans in order to pull out a sort.
    // Let's try this.
    if (explodeForSort(query, params, &solnRoot)) {
        return solnRoot;
    }

    // If we're here, we need to add a sort stage.

    if (!solnRoot->fetched()) {
        const bool sortIsCovered = std::all_of(sortObj.begin(), sortObj.end(), [&](BSONElement e) {
            // If the index has the collation that the query is expecting then kCollatedProvided
            // will be returned hence we can use the index for sorting and grouping (distinct_scan)
            // but need to add a fetch to retrieve a proper value of the key.
            auto fieldAvailability = solnRoot->getFieldAvailability(e.fieldName());
            return fieldAvailability == FieldAvailability::kCollatedProvided ||
                fieldAvailability == FieldAvailability::kFullyProvided;
        });

        if (!sortIsCovered) {
            auto fetch = std::make_unique<FetchNode>();
            fetch->children.push_back(std::move(solnRoot));
            solnRoot = std::move(fetch);
        }
    }

    std::unique_ptr<SortNode> sortNode;
    if (canUseSimpleSort(*solnRoot, query, params)) {
        sortNode = std::make_unique<SortNodeSimple>();
    } else {
        sortNode = std::make_unique<SortNodeDefault>();
    }
    sortNode->pattern = sortObj;
    sortNode->children.push_back(std::move(solnRoot));
    sortNode->addSortKeyMetadata = query.metadataDeps()[DocumentMetadataFields::kSortKey];
    // When setting the limit on the sort, we need to consider both
    // the limit N and skip count M. The sort should return an ordered list
    // N + M items so that the skip stage can discard the first M results.
    if (findCommand.getLimit()) {
        // The limit can be combined with the SORT stage.
        sortNode->limit = static_cast<size_t>(*findCommand.getLimit()) +
            static_cast<size_t>(findCommand.getSkip().value_or(0));
    } else {
        sortNode->limit = 0;
    }
    solnRoot = std::move(sortNode);

    *blockingSortOut = true;

    return solnRoot;
}

std::unique_ptr<QuerySolution> QueryPlannerAnalysis::analyzeDataAccess(
    const CanonicalQuery& query,
    const QueryPlannerParams& params,
    std::unique_ptr<QuerySolutionNode> solnRoot) {
    auto soln = std::make_unique<QuerySolution>();
    soln->indexFilterApplied = params.indexFiltersApplied;

    solnRoot->computeProperties();

    analyzeGeo(params, solnRoot.get());

    const FindCommandRequest& findCommand = query.getFindCommandRequest();

    deprioritizeUnboundedCollectionScan(solnRoot.get(), findCommand);

    // solnRoot finds all our results.  Let's see what transformations we must perform to the
    // data.

    // If we're answering a query on a sharded system, we need to drop documents that aren't
    // logically part of our shard.
    if (params.options & QueryPlannerParams::INCLUDE_SHARD_FILTER) {
        if (!solnRoot->fetched()) {
            // See if we need to fetch information for our shard key.
            // NOTE: Solution nodes only list ordinary, non-transformed index keys for now

            bool fetch = false;
            for (auto&& shardKeyField : params.shardKey) {
                auto fieldAvailability = solnRoot->getFieldAvailability(shardKeyField.fieldName());
                if (fieldAvailability == FieldAvailability::kNotProvided ||
                    fieldAvailability == FieldAvailability::kCollatedProvided) {
                    // One of the shard key fields are not  or only a collated version are provided
                    // by an index. We need to fetch the full documents prior to shard filtering. In
                    // the case of kCollatedProvided the fetch is needed to get a non-ICU encoded
                    // value from the collection. Else the IDXScan would only return non-readable
                    // ICU encoded values.
                    fetch = true;
                    break;
                }
                if (fieldAvailability == FieldAvailability::kHashedValueProvided &&
                    shardKeyField.valueStringDataSafe() != IndexNames::HASHED) {
                    // The index scan provides the hash of a field, but the shard key field is _not_
                    // hashed. We need to fetch prior to shard filtering in order to recover the raw
                    // value of the field.
                    fetch = true;
                    break;
                }
            }

            if (fetch) {
                auto fetchNode = std::make_unique<FetchNode>();
                fetchNode->children.push_back(std::move(solnRoot));
                solnRoot = std::move(fetchNode);
            }
        }

        auto sfn = std::make_unique<ShardingFilterNode>();
        sfn->children.push_back(std::move(solnRoot));
        solnRoot = std::move(sfn);
    }

    bool hasSortStage = false;
    solnRoot = analyzeSort(query, params, std::move(solnRoot), &hasSortStage);

    // This can happen if we need to create a blocking sort stage and we're not allowed to.
    if (!solnRoot) {
        return nullptr;
    }

    // A solution can be blocking if it has a blocking sort stage or
    // a hashed AND stage.
    bool hasAndHashStage = solnRoot->hasNode(STAGE_AND_HASH);
    soln->hasBlockingStage = hasSortStage || hasAndHashStage;

    if (findCommand.getSkip()) {
        auto skip = std::make_unique<SkipNode>();
        skip->skip = *findCommand.getSkip();
        skip->children.push_back(std::move(solnRoot));
        solnRoot = std::move(skip);
    }

    // Project the results.
    if (findCommand.getReturnKey()) {
        // We don't need a projection stage if returnKey was requested since the intended behavior
        // is that the projection is ignored when returnKey is specified.
        solnRoot = std::make_unique<ReturnKeyNode>(
            addSortKeyGeneratorStageIfNeeded(query, hasSortStage, std::move(solnRoot)),
            query.getProj()
                ? QueryPlannerCommon::extractSortKeyMetaFieldsFromProjection(*query.getProj())
                : std::vector<FieldPath>{});
    } else if (query.getProj()) {
        solnRoot = analyzeProjection(query, std::move(solnRoot), hasSortStage);
    } else {
        // Even if there's no projection, the client may want sort key metadata.
        solnRoot = addSortKeyGeneratorStageIfNeeded(query, hasSortStage, std::move(solnRoot));

        // If there's no projection, we must fetch, as the user wants the entire doc.
        if (!solnRoot->fetched() && !query.isCountLike()) {
            auto fetch = std::make_unique<FetchNode>();
            fetch->children.push_back(std::move(solnRoot));
            solnRoot = std::move(fetch);
        }
    }

    // When there is both a blocking sort and a limit, the limit will be enforced by the blocking
    // sort. Otherwise, we will have to enforce the limit ourselves since it's not handled inside
    // SORT.
    if (!hasSortStage && findCommand.getLimit()) {
        auto limit = std::make_unique<LimitNode>();
        limit->limit = *findCommand.getLimit();
        limit->children.push_back(std::move(solnRoot));
        solnRoot = std::move(limit);
    }

    solnRoot = tryPushdownProjectBeneathSort(std::move(solnRoot));

    QueryPlannerAnalysis::removeUselessColumnScanRowStoreExpression(*solnRoot);

    soln->setRoot(std::move(solnRoot));
    return soln;
}

}  // namespace mongo
