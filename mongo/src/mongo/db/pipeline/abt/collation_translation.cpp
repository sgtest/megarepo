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

#include "mongo/db/pipeline/abt/collation_translation.h"

#include <utility>
#include <vector>

#include <absl/container/node_hash_map.h>
#include <boost/optional/optional.hpp>

#include "mongo/db/pipeline/abt/utils.h"
#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/props.h"
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"


namespace mongo::optimizer {

void generateCollationNode(AlgebrizerContext& ctx, const SortPattern& sortPattern) {
    ProjectionCollationSpec collationSpec;
    auto rootProjection = ctx.getNode()._rootProjection;
    // Create Evaluation node for each sort field.
    for (const auto& part : sortPattern) {
        if (!part.fieldPath.has_value()) {
            continue;
        }
        auto sortProjName = ctx.getNextId("sort");
        collationSpec.emplace_back(
            sortProjName, part.isAscending ? CollationOp::Ascending : CollationOp::Descending);

        ABT sortPath = translateFieldPath(
            *part.fieldPath,
            make<PathIdentity>(),
            [](FieldNameType fieldName, const bool /*isLastElement*/, ABT input) {
                return make<PathGet>(std::move(fieldName), std::move(input));
            });

        ctx.setNode<EvaluationNode>(
            rootProjection,
            std::move(sortProjName),
            make<EvalPath>(std::move(sortPath), make<Variable>(rootProjection)),
            std::move(ctx.getNode()._node));
    }
    if (collationSpec.empty()) {
        return;
    }
    ctx.setNode<CollationNode>(std::move(rootProjection),
                               properties::CollationRequirement(std::move(collationSpec)),
                               std::move(ctx.getNode()._node));
}

}  // namespace mongo::optimizer
