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

#pragma once

#include <cstddef>
#include <memory>

#include <boost/optional/optional.hpp>

#include "mongo/db/query/optimizer/cascades/interfaces.h"
#include "mongo/db/query/optimizer/cascades/logical_rewriter.h"
#include "mongo/db/query/optimizer/cascades/memo.h"
#include "mongo/db/query/optimizer/cascades/memo_defs.h"
#include "mongo/db/query/optimizer/cascades/rewriter_rules.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/metadata.h"
#include "mongo/db/query/optimizer/node_defs.h"
#include "mongo/db/query/optimizer/props.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"
#include "mongo/db/query/optimizer/utils/utils.h"

namespace mongo::optimizer::cascades {

class PhysicalRewriter {
    friend class PropEnforcerVisitor;
    friend class ImplementationVisitor;

public:
    struct OptimizeGroupResult {
        OptimizeGroupResult();
        OptimizeGroupResult(size_t index, CostType cost);

        OptimizeGroupResult(const OptimizeGroupResult& other) = default;
        OptimizeGroupResult(OptimizeGroupResult&& other) = default;

        bool _success;
        size_t _index;
        CostType _cost;
    };

    PhysicalRewriter(const Metadata& _metadata,
                     Memo& memo,
                     PrefixId& prefixId,
                     GroupIdType rootGroupid,
                     const DebugInfo& debugInfo,
                     const QueryHints& hints,
                     const RIDProjectionsMap& ridProjections,
                     const CostEstimator& costEstimator,
                     const PathToIntervalFn& pathToInterval,
                     std::unique_ptr<LogicalRewriter>& logicalRewriter);

    // This is a transient structure. We do not allow copying or moving.
    PhysicalRewriter(const PhysicalRewriter& /*other*/) = delete;
    PhysicalRewriter(PhysicalRewriter&& /*other*/) = delete;
    PhysicalRewriter& operator=(const PhysicalRewriter& /*other*/) = delete;
    PhysicalRewriter& operator=(PhysicalRewriter&& /*other*/) = delete;

    /**
     * Main entry point for physical optimization.
     * Optimize a logical plan rooted at a RootNode, and return an index into the winner's circle if
     * successful.
     */
    OptimizeGroupResult optimizeGroup(GroupIdType groupId,
                                      properties::PhysProps physProps,
                                      CostType costLimit);

private:
    void costAndRetainBestNode(std::unique_ptr<ABT> node,
                               ChildPropsType childProps,
                               NodeCEMap nodeCEMap,
                               PhysicalRewriteType rule,
                               GroupIdType groupId,
                               PhysOptimizationResult& bestResult);

    boost::optional<CostType> optimizeChildren(CostType nodeCost,
                                               ChildPropsType childProps,
                                               CostType costLimit);

    SpoolIdGenerator _spoolId;

    // We don't own any of this.
    const Metadata& _metadata;
    Memo& _memo;
    PrefixId& _prefixId;
    const GroupIdType _rootGroupId;
    const CostEstimator& _costEstimator;
    const DebugInfo& _debugInfo;
    const QueryHints& _hints;
    const RIDProjectionsMap& _ridProjections;
    const PathToIntervalFn& _pathToInterval;
    // If set, we'll perform logical rewrites as part of OptimizeGroup().
    std::unique_ptr<LogicalRewriter>& _logicalRewriter;
};

}  // namespace mongo::optimizer::cascades
