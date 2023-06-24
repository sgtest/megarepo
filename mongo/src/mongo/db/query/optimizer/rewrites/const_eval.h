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
#include <functional>
#include <vector>

#include <absl/container/node_hash_map.h>

#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/optimizer/containers.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/reference_tracker.h"
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"
#include "mongo/db/query/optimizer/utils/abt_hash.h"


namespace mongo::optimizer {

// Handler which should return a boolean indicating if we are allowed to inline an EvaluationNode.
// If the handler returns "true" we can inline, otherwise we are not allowed to.
using CanInlineEvalFn = std::function<bool(const EvaluationNode& node)>;

// Handler which is called when we erase an unused projection name.
using ErasedProjFn = std::function<void(const ProjectionName& erasedProjName)>;

// Handler which is called when we inline a projection name (target) with another projection name
// (source).
using RenamedProjFn =
    std::function<void(const ProjectionName& target, const ProjectionName& source)>;

/**
 * This is an example rewriter that does constant evaluation in-place.
 */
class ConstEval {
public:
    ConstEval(VariableEnvironment& env,
              const CanInlineEvalFn& canInlineEval = {},
              const ErasedProjFn& erasedProj = {},
              const RenamedProjFn& renamedProj = {})
        : _env(env),
          _canInlineEval(canInlineEval),
          _erasedProj(erasedProj),
          _renamedProj(renamedProj) {}

    // The default noop transport. Note the first ABT& parameter.
    template <typename T, typename... Ts>
    void transport(ABT&, const T&, Ts&&...) {}

    void transport(ABT& n, const Variable& var);

    void prepare(ABT&, const Let& let);
    void transport(ABT& n, const Let& let, ABT&, ABT& in);
    void transport(ABT& n, const LambdaApplication& app, ABT& lam, ABT& arg);
    void prepare(ABT&, const LambdaAbstraction&);
    void transport(ABT&, const LambdaAbstraction&, ABT&);

    void transport(ABT& n, const UnaryOp& op, ABT& child);
    // Specific transport for binary operation
    // The const correctness is probably wrong (as const ABT& lhs, const ABT& rhs does not work for
    // some reason but we can fix it later).
    void transport(ABT& n, const BinaryOp& op, ABT& lhs, ABT& rhs);
    void transport(ABT& n, const FunctionCall& op, std::vector<ABT>& args);
    void transport(ABT& n, const If& op, ABT& cond, ABT& thenBranch, ABT& elseBranch);

    void transport(ABT& n, const EvalPath& op, ABT& path, ABT& input);
    void transport(ABT& n, const EvalFilter& op, ABT& path, ABT& input);

    void transport(ABT& n, const FilterNode& op, ABT& child, ABT& expr);
    void transport(ABT& n, const EvaluationNode& op, ABT& child, ABT& expr);

    void prepare(ABT&, const PathTraverse&);
    void transport(ABT&, const PathTraverse&, ABT&);

    void transport(ABT& n, const PathComposeM& op, ABT& lhs, ABT& rhs);
    void transport(ABT& n, const PathComposeA& op, ABT& lhs, ABT& rhs);

    void prepare(ABT&, const References& refs);
    void transport(ABT& n, const References& op, std::vector<ABT>&);

    // The tree is passed in as NON-const reference as we will be updating it.
    bool optimize(ABT& n);

    // Provides constant folding interface.
    static void constFold(ABT& n);

private:
    struct EvalNodeHash {
        size_t operator()(const EvaluationNode* node) const {
            return ABTHashGenerator::generate(node->getProjection());
        }
    };

    struct EvalNodeCompare {
        size_t operator()(const EvaluationNode* lhs, const EvaluationNode* rhs) const {
            return lhs->getProjection() == rhs->getProjection();
        }
    };

    struct RefHash {
        size_t operator()(const ABT::reference_type& nodeRef) const {
            return nodeRef.hash();
        }
    };

    void swapAndUpdate(ABT& n, ABT newN);
    void removeUnusedEvalNodes();

    VariableEnvironment& _env;

    // Handler which controls inlining of EvalNodes.
    const CanInlineEvalFn& _canInlineEval;
    // Handler called when a projection is erased.
    const ErasedProjFn& _erasedProj;
    // Handler called when a projection is renamed.
    const RenamedProjFn& _renamedProj;

    opt::unordered_set<const Variable*> _singleRef;
    opt::unordered_set<const EvaluationNode*> _noRefProj;
    opt::unordered_map<const Let*, std::vector<const Variable*>> _letRefs;
    opt::unordered_map<const EvaluationNode*, std::vector<const Variable*>> _projectRefs;
    opt::unordered_set<const EvaluationNode*, EvalNodeHash, EvalNodeCompare> _seenProjects;
    opt::unordered_set<ABT::reference_type, RefHash> _inlinedDefs;
    opt::unordered_map<ABT::reference_type, ABT::reference_type, RefHash> _staleDefs;
    // We collect old ABTs in order to avoid the ABA problem.
    std::vector<ABT> _staleABTs;

    bool _inRefBlock{false};
    size_t _inCostlyCtx{0};
    bool _changed{false};
};

}  // namespace mongo::optimizer
