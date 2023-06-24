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

#include <boost/optional/optional.hpp>
#include <cstddef>
#include <vector>

#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/simple_bsonelement_comparator.h"
#include "mongo/db/jsobj.h"
#include "mongo/db/matcher/expression.h"
#include "mongo/db/pipeline/field_path.h"
#include "mongo/db/query/canonical_query.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/projection.h"
#include "mongo/db/query/query_planner_params.h"
#include "mongo/db/query/query_solution.h"

namespace mongo {

/**
 * Methods used by several parts of the planning process.
 */
class QueryPlannerCommon {
public:
    /**
     * Does the tree rooted at 'root' have a node with matchType 'type'?
     *
     * If 'out' is not NULL, sets 'out' to the first node of type 'type' encountered.
     */
    static bool hasNode(const MatchExpression* root,
                        MatchExpression::MatchType type,
                        const MatchExpression** out = nullptr) {
        if (type == root->matchType()) {
            if (nullptr != out) {
                *out = root;
            }
            return true;
        }

        for (size_t i = 0; i < root->numChildren(); ++i) {
            if (hasNode(root->getChild(i), type, out)) {
                return true;
            }
        }
        return false;
    }

    /**
     * Returns a count of 'type' nodes in expression tree.
     */
    static size_t countNodes(const MatchExpression* root, MatchExpression::MatchType type) {
        size_t sum = 0;
        if (type == root->matchType()) {
            sum = 1;
        }
        for (size_t i = 0; i < root->numChildren(); ++i) {
            sum += countNodes(root->getChild(i), type);
        }
        return sum;
    }

    /**
     * Assumes the provided BSONObj is of the form {field1: -+1, ..., field2: -+1}
     * Returns a BSONObj with the values negated.
     */
    static BSONObj reverseSortObj(const BSONObj& sortObj) {
        BSONObjBuilder reverseBob;
        BSONObjIterator it(sortObj);
        while (it.more()) {
            BSONElement elt = it.next();
            reverseBob.append(elt.fieldName(), elt.numberInt() * -1);
        }
        return reverseBob.obj();
    }

    /**
     * Traverses the tree rooted at 'node'. Tests scan directions recursively to see if they are
     * equal to the given direction argument. Returns true if they are and false otherwise.
     */
    static bool scanDirectionsEqual(QuerySolutionNode* node, int direction);

    /**
     * Traverses the tree rooted at 'node'.  For every STAGE_IXSCAN encountered, reverse
     * the scan direction and index bounds, unless reverseCollScans equals true, in which case
     * STAGE_COLLSCAN is reversed as well.
     */
    static void reverseScans(QuerySolutionNode* node, bool reverseCollScans = false);

    /**
     * Extracts all field names for the sortKey meta-projection and stores them in the returned
     * array. Returns an empty array if there were no sortKey meta-projection specified in the
     * given projection 'proj'. For example, given a projection {a:1, b: {$meta: "sortKey"},
     * c: {$meta: "sortKey"}}, the returned vector will contain two elements ["b", "c"].
     */
    static std::vector<FieldPath> extractSortKeyMetaFieldsFromProjection(
        const projection_ast::Projection& proj);

    static bool providesSort(const CanonicalQuery& query, const BSONObj& kp) {
        return query.getFindCommandRequest().getSort().isPrefixOf(
            kp, SimpleBSONElementComparator::kInstance);
    }

    /**
     * Determine whether this query has a sort that can be provided by the collection's clustering
     * index, if so, which direction the scan should be. If the collection is not clustered, or the
     * sort cannot be provided, returns 'boost::none'.
     */
    static boost::optional<int> determineClusteredScanDirection(const CanonicalQuery& query,
                                                                const QueryPlannerParams& params);
};

}  // namespace mongo
