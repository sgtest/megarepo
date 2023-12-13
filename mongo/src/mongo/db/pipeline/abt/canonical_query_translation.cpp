/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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

#include "mongo/db/pipeline/abt/canonical_query_translation.h"

#include <utility>

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/db/pipeline/abt/algebrizer_context.h"
#include "mongo/db/pipeline/abt/collation_translation.h"
#include "mongo/db/pipeline/abt/match_expression_visitor.h"
#include "mongo/db/pipeline/abt/transformer_visitor.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/props.h"
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/utils/path_utils.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo::optimizer {

ABT translateCanonicalQueryToABT(const Metadata& metadata,
                                 const CanonicalQuery& canonicalQuery,
                                 ProjectionName scanProjName,
                                 ABT initialNode,
                                 PrefixId& prefixId,
                                 QueryParameterMap& queryParameters,
                                 size_t maxFilterDepth) {
    auto matchExpr = generateMatchExpression(canonicalQuery.getPrimaryMatchExpression(),
                                             true /* allowAggExpression */,
                                             scanProjName,
                                             prefixId,
                                             queryParameters);

    {
        // Decompose conjunction in the filter into a serial chain of FilterNodes.
        auto result = decomposeToFilterNodes(
            initialNode, matchExpr, make<Variable>(scanProjName), 1 /*minDepth*/, maxFilterDepth);
        initialNode = std::move(*result);
    }

    AlgebrizerContext ctx{prefixId, {scanProjName, std::move(initialNode)}, queryParameters};

    if (auto sortPattern = canonicalQuery.getSortPattern()) {
        generateCollationNode(ctx, *sortPattern);
    }

    if (auto proj = canonicalQuery.getProj()) {
        translateProjection(ctx, scanProjName, canonicalQuery.getExpCtx(), proj);
    }

    auto skipAmount = canonicalQuery.getFindCommandRequest().getSkip();
    auto limitAmount = canonicalQuery.getFindCommandRequest().getLimit();

    if (limitAmount || skipAmount) {
        ctx.setNode<LimitSkipNode>(
            std::move(ctx.getNode()._rootProjection),
            properties::LimitSkipRequirement(
                limitAmount.value_or(properties::LimitSkipRequirement::kMaxVal),
                skipAmount.value_or(0)),
            std::move(ctx.getNode()._node));
    }

    return make<RootNode>(properties::ProjectionRequirement{ProjectionNameVector{
                              std::move(ctx.getNode()._rootProjection)}},
                          std::move(ctx.getNode()._node));
}

}  // namespace mongo::optimizer
