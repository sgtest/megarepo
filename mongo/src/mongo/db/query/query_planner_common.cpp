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


#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <memory>
#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/base/exact_cast.h"
#include "mongo/base/string_data.h"
#include "mongo/db/catalog/clustered_collection_options_gen.h"
#include "mongo/db/catalog/clustered_collection_util.h"
#include "mongo/db/exec/document_value/document_metadata_fields.h"
#include "mongo/db/pipeline/expression.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/db/query/index_bounds.h"
#include "mongo/db/query/index_entry.h"
#include "mongo/db/query/projection_ast.h"
#include "mongo/db/query/projection_ast_path_tracking_visitor.h"
#include "mongo/db/query/projection_ast_visitor.h"
#include "mongo/db/query/query_planner_common.h"
#include "mongo/db/query/query_solution.h"
#include "mongo/db/query/stage_types.h"
#include "mongo/db/query/tree_walker.h"
#include "mongo/logv2/redaction.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo {

bool QueryPlannerCommon::scanDirectionsEqual(QuerySolutionNode* node, int direction) {
    StageType type = node->getType();

    boost::optional<int> scanDir;
    if (STAGE_IXSCAN == type) {
        IndexScanNode* isn = static_cast<IndexScanNode*>(node);
        scanDir = isn->direction;
    } else if (STAGE_DISTINCT_SCAN == type) {
        DistinctNode* dn = static_cast<DistinctNode*>(node);
        scanDir = dn->direction;
    } else if (STAGE_COLLSCAN == type) {
        CollectionScanNode* collScan = static_cast<CollectionScanNode*>(node);
        scanDir = collScan->direction;
    } else {
        // We shouldn't encounter a sort stage.
        invariant(!isSortStageType(type));
    }

    // If we found something with a direction, and the direction doesn't match, we return false.
    if (scanDir && scanDir != direction) {
        return false;
    }

    for (size_t i = 0; i < node->children.size(); ++i) {
        if (!scanDirectionsEqual(node->children[i].get(), direction)) {
            return false;
        }
    }
    return true;
}

void QueryPlannerCommon::reverseScans(QuerySolutionNode* node, bool reverseCollScans) {
    StageType type = node->getType();

    if (STAGE_IXSCAN == type) {
        IndexScanNode* isn = static_cast<IndexScanNode*>(node);
        isn->direction *= -1;

        isn->bounds = isn->bounds.reverse();

        invariant(isn->bounds.isValidFor(isn->index.keyPattern, isn->direction),
                  str::stream() << "Invalid bounds: "
                                << redact(isn->bounds.toString(isn->index.collator != nullptr)));

        // TODO: we can just negate every value in the already computed properties.
        isn->computeProperties();
    } else if (STAGE_DISTINCT_SCAN == type) {
        DistinctNode* dn = static_cast<DistinctNode*>(node);
        dn->direction *= -1;

        dn->bounds = dn->bounds.reverse();

        invariant(dn->bounds.isValidFor(dn->index.keyPattern, dn->direction),
                  str::stream() << "Invalid bounds: "
                                << redact(dn->bounds.toString(dn->index.collator != nullptr)));

        dn->computeProperties();
    } else if (STAGE_SORT_MERGE == type) {
        // reverse direction of comparison for merge
        MergeSortNode* msn = static_cast<MergeSortNode*>(node);
        msn->sort = reverseSortObj(msn->sort);
    } else if (reverseCollScans && STAGE_COLLSCAN == type) {
        CollectionScanNode* collScan = static_cast<CollectionScanNode*>(node);
        collScan->direction *= -1;
    } else {
        // Reversing scans is done in order to determine whether or not we need to add an explicit
        // SORT stage. There shouldn't already be one present in the plan.
        invariant(!isSortStageType(type));
    }

    for (size_t i = 0; i < node->children.size(); ++i) {
        reverseScans(node->children[i].get(), reverseCollScans);
    }
}

namespace {

struct MetaFieldData {
    std::vector<FieldPath> metaPaths;
};

using MetaFieldVisitorContext = projection_ast::PathTrackingVisitorContext<MetaFieldData>;

/**
 * Visitor which produces a list of paths where $meta expressions are.
 */
class MetaFieldVisitor final : public projection_ast::ProjectionASTConstVisitor {
public:
    MetaFieldVisitor(MetaFieldVisitorContext* context) : _context(context) {}


    void visit(const projection_ast::ExpressionASTNode* node) final {
        const auto* metaExpr = exact_pointer_cast<const ExpressionMeta*>(node->expressionRaw());
        if (!metaExpr || metaExpr->getMetaType() != DocumentMetadataFields::MetaType::kSortKey) {
            return;
        }

        _context->data().metaPaths.push_back(_context->fullPath());
    }

    void visit(const projection_ast::ProjectionPositionalASTNode* node) final {}
    void visit(const projection_ast::ProjectionSliceASTNode* node) final {}
    void visit(const projection_ast::ProjectionElemMatchASTNode* node) final {}
    void visit(const projection_ast::BooleanConstantASTNode* node) final {}
    void visit(const projection_ast::ProjectionPathASTNode* node) final {}
    void visit(const projection_ast::MatchExpressionASTNode* node) final {}

private:
    MetaFieldVisitorContext* _context;
};
}  // namespace

std::vector<FieldPath> QueryPlannerCommon::extractSortKeyMetaFieldsFromProjection(
    const projection_ast::Projection& proj) {

    MetaFieldVisitorContext ctx;
    MetaFieldVisitor visitor(&ctx);
    projection_ast::PathTrackingConstWalker<MetaFieldData> walker{&ctx, {&visitor}, {}};
    tree_walker::walk<true, projection_ast::ASTNode>(proj.root(), &walker);

    return std::move(ctx.data().metaPaths);
}

boost::optional<int> QueryPlannerCommon::determineClusteredScanDirection(
    const CanonicalQuery& query, const QueryPlannerParams& params) {
    if (params.clusteredInfo && query.getSortPattern() &&
        CollatorInterface::collatorsMatch(params.clusteredCollectionCollator,
                                          query.getCollator())) {
        BSONObj kp = clustered_util::getSortPattern(params.clusteredInfo->getIndexSpec());
        if (QueryPlannerCommon::providesSort(query, kp)) {
            return 1;
        } else if (QueryPlannerCommon::providesSort(query,
                                                    QueryPlannerCommon::reverseSortObj(kp))) {
            return -1;
        }
    }

    return boost::none;
}

}  // namespace mongo
