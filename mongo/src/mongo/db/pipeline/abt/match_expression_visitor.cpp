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

#include "mongo/db/pipeline/abt/match_expression_visitor.h"

#include <cstddef>
#include <utility>
#include <vector>

#include <absl/container/node_hash_map.h>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/exec/docval_to_sbeval.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/field_ref.h"
#include "mongo/db/matcher/expression_always_boolean.h"
#include "mongo/db/matcher/expression_array.h"
#include "mongo/db/matcher/expression_expr.h"
#include "mongo/db/matcher/expression_geo.h"
#include "mongo/db/matcher/expression_internal_bucket_geo_within.h"
#include "mongo/db/matcher/expression_internal_eq_hashed_key.h"
#include "mongo/db/matcher/expression_internal_expr_comparison.h"
#include "mongo/db/matcher/expression_leaf.h"
#include "mongo/db/matcher/expression_path.h"
#include "mongo/db/matcher/expression_text.h"
#include "mongo/db/matcher/expression_text_noop.h"
#include "mongo/db/matcher/expression_tree.h"
#include "mongo/db/matcher/expression_type.h"
#include "mongo/db/matcher/expression_visitor.h"
#include "mongo/db/matcher/expression_where.h"
#include "mongo/db/matcher/expression_where_noop.h"
#include "mongo/db/matcher/match_expression_walker.h"
#include "mongo/db/matcher/matcher_type_set.h"
#include "mongo/db/matcher/schema/expression_internal_schema_all_elem_match_from_index.h"
#include "mongo/db/matcher/schema/expression_internal_schema_allowed_properties.h"
#include "mongo/db/matcher/schema/expression_internal_schema_cond.h"
#include "mongo/db/matcher/schema/expression_internal_schema_eq.h"
#include "mongo/db/matcher/schema/expression_internal_schema_fmod.h"
#include "mongo/db/matcher/schema/expression_internal_schema_match_array_index.h"
#include "mongo/db/matcher/schema/expression_internal_schema_max_items.h"
#include "mongo/db/matcher/schema/expression_internal_schema_max_length.h"
#include "mongo/db/matcher/schema/expression_internal_schema_max_properties.h"
#include "mongo/db/matcher/schema/expression_internal_schema_min_items.h"
#include "mongo/db/matcher/schema/expression_internal_schema_min_length.h"
#include "mongo/db/matcher/schema/expression_internal_schema_min_properties.h"
#include "mongo/db/matcher/schema/expression_internal_schema_object_match.h"
#include "mongo/db/matcher/schema/expression_internal_schema_root_doc_eq.h"
#include "mongo/db/matcher/schema/expression_internal_schema_unique_items.h"
#include "mongo/db/matcher/schema/expression_internal_schema_xor.h"
#include "mongo/db/pipeline/abt/agg_expression_visitor.h"
#include "mongo/db/pipeline/abt/expr_algebrizer_context.h"
#include "mongo/db/pipeline/abt/utils.h"
#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/optimizer/comparison_op.h"
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/utils/path_utils.h"
#include "mongo/db/query/tree_walker.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

namespace mongo::optimizer {
namespace {
/**
 * Return the minimum or maximum value for the "class" of values represented by the input
 * constant. Used to support type bracketing. Takes into account both the type tag and value of
 * the input constant.
 * Return format is <min/max value, bool inclusive>
 */
std::pair<boost::optional<ABT>, bool> getMinMaxBoundForValue(const bool isMin,
                                                             const sbe::value::TypeTags& tag,
                                                             const sbe::value::Value& val) {
    if (sbe::value::isNaN(tag, val)) {
        return {Constant::fromDouble(std::numeric_limits<double>::quiet_NaN()), true};
    }
    return getMinMaxBoundForType(isMin, tag);
}
}  // namespace

class ABTMatchExpressionPreVisitor : public SelectiveMatchExpressionVisitorBase<true> {
    using SelectiveMatchExpressionVisitorBase<true>::visit;

public:
    ABTMatchExpressionPreVisitor(ExpressionAlgebrizerContext& ctx) : _ctx(ctx) {}

    void visit(const ElemMatchObjectMatchExpression* expr) override {
        _ctx.enterElemMatch(expr->matchType());
    }

    void visit(const ElemMatchValueMatchExpression* expr) override {
        _ctx.enterElemMatch(expr->matchType());
    }

private:
    ExpressionAlgebrizerContext& _ctx;
};

class ABTMatchExpressionVisitor : public MatchExpressionConstVisitor {
public:
    ABTMatchExpressionVisitor(ExpressionAlgebrizerContext& ctx, const bool allowAggExpressions)
        : _allowAggExpressions(allowAggExpressions), _ctx(ctx) {}

    void visit(const AlwaysFalseMatchExpression* expr) override {
        generateBoolConstant(false);
    }

    void visit(const AlwaysTrueMatchExpression* expr) override {
        generateBoolConstant(true);
    }

    void visit(const AndMatchExpression* expr) override {
        visitAndOrExpression<PathComposeM, true>(expr);
    }

    void visit(const BitsAllClearMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const BitsAllSetMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const BitsAnyClearMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const BitsAnySetMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const ElemMatchObjectMatchExpression* expr) override {
        generateElemMatch<false /*isValueElemMatch*/>(expr);
        _ctx.exitElemMatch();
    }

    void visit(const ElemMatchValueMatchExpression* expr) override {
        generateElemMatch<true /*isValueElemMatch*/>(expr);
        _ctx.exitElemMatch();
    }

    void visit(const EqualityMatchExpression* expr) override {
        generateSimpleComparison(expr, Operations::Eq);
    }

    void visit(const ExistsMatchExpression* expr) override {
        assertSupportedPathExpression(expr);

        ABT result = make<PathDefault>(Constant::boolean(false));
        if (shouldGeneratePath(expr)) {
            result = translateFieldRef(*(expr->fieldRef()), std::move(result));
        }
        _ctx.push(std::move(result));
    }

    void visit(const ExprMatchExpression* expr) override {
        uassert(6624246, "Cannot generate an agg expression in this context", _allowAggExpressions);

        ABT result = generateAggExpression(
            expr->getExpression().get(), _ctx.getRootProjection(), _ctx.getPrefixId());

        if (auto filterPtr = result.cast<EvalFilter>();
            filterPtr != nullptr && filterPtr->getInput() == _ctx.getRootProjVar()) {
            // If we have an EvalFilter, just return the path.
            _ctx.push(std::move(filterPtr->getPath()));
        } else {
            _ctx.push<PathConstant>(std::move(result));
        }
    }

    void visit(const GTEMatchExpression* expr) override {
        generateSimpleComparison(expr, Operations::Gte);
    }

    void visit(const GTMatchExpression* expr) override {
        generateSimpleComparison(expr, Operations::Gt);
    }

    void visit(const GeoMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const GeoNearMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InMatchExpression* expr) override {
        uassert(ErrorCodes::InternalErrorNotSupported,
                "$in with regexes is not supported.",
                expr->getRegexes().empty());

        assertSupportedPathExpression(expr);

        const auto& equalities = expr->getEqualities();

        // $in with an empty equalities list matches nothing; replace with constant false.
        if (equalities.empty()) {
            generateBoolConstant(false);
            return;
        }

        ABT result = make<PathIdentity>();

        const auto [tagTraverse, valTraverse] = sbe::value::makeNewArray();
        auto arrTraversePtr = sbe::value::getArrayView(valTraverse);
        arrTraversePtr->reserve(equalities.size());

        const auto [tagArraysOnly, valArraysOnly] = sbe::value::makeNewArray();
        sbe::value::ValueGuard arrOnlyGuard{tagArraysOnly, valArraysOnly};
        auto arraysOnlyPtr = sbe::value::getArrayView(valArraysOnly);
        bool addNullPathDefault = false;
        arraysOnlyPtr->reserve(equalities.size());

        for (const auto& pred : equalities) {
            const auto [tag, val] = sbe::value::makeValue(Value(pred));
            arrTraversePtr->push_back(tag, val);

            if (tag == sbe::value::TypeTags::Null) {
                addNullPathDefault = true;
            } else if (tag == sbe::value::TypeTags::Array) {
                const auto [tag2, val2] = sbe::value::copyValue(tag, val);
                arraysOnlyPtr->push_back(tag2, val2);
            }
        }

        if (expr->getInputParamId()) {
            result = make<FunctionCall>(
                kParameterFunctionName,
                makeSeq(make<Constant>(sbe::value::TypeTags::NumberInt32, *expr->getInputParamId()),
                        make<Constant>(sbe::value::TypeTags::NumberInt32,
                                       static_cast<int>(tagTraverse))));
            _ctx.getQueryParameters().emplace(*expr->getInputParamId(),
                                              Constant(tagTraverse, valTraverse));
        } else {
            result = make<Constant>(tagTraverse, valTraverse);
        }
        result = make<PathCompare>(Operations::EqMember, std::move(result));

        if (addNullPathDefault) {
            maybeComposePath<PathComposeA>(result, make<PathDefault>(Constant::boolean(true)));
        }

        // Do not insert a traverse if within an $elemMatch; traversal will be handled by the
        // $elemMatch expression itself.
        if (shouldGeneratePath(expr)) {
            // When the path we are comparing is a path to an array, the comparison is
            // considered true if it evaluates to true for the array itself or for any of the
            // array’s elements. 'result' evaluates comparison on the array elements, and
            // 'arraysOnly' evaluates the comparison on the array itself.
            result = make<PathTraverse>(PathTraverse::kSingleLevel, std::move(result));

            if (arraysOnlyPtr->size() == 1) {
                const auto [tagSingle, valSingle] = sbe::value::copyValue(
                    arraysOnlyPtr->getAt(0).first, arraysOnlyPtr->getAt(0).second);
                maybeComposePath<PathComposeA>(
                    result,
                    make<PathCompare>(Operations::Eq, make<Constant>(tagSingle, valSingle)));
            } else if (arraysOnlyPtr->size() > 0) {
                maybeComposePath<PathComposeA>(
                    result,
                    make<PathCompare>(Operations::EqMember,
                                      make<Constant>(tagArraysOnly, valArraysOnly)));
                arrOnlyGuard.reset();
            }
            result = translateFieldRef(*(expr->fieldRef()), std::move(result));
        }
        _ctx.push(std::move(result));
    }

    void visit(const InternalBucketGeoWithinMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalExprEqMatchExpression* expr) override {
        // Ignored. Translate to "true".
        _ctx.push(make<PathConstant>(Constant::boolean(true)));
    }

    void visit(const InternalExprGTMatchExpression* expr) override {
        // Ignored. Translate to "true".
        _ctx.push(make<PathConstant>(Constant::boolean(true)));
    }

    void visit(const InternalExprGTEMatchExpression* expr) override {
        // Ignored. Translate to "true".
        _ctx.push(make<PathConstant>(Constant::boolean(true)));
    }

    void visit(const InternalExprLTMatchExpression* expr) override {
        // Ignored. Translate to "true".
        _ctx.push(make<PathConstant>(Constant::boolean(true)));
    }

    void visit(const InternalExprLTEMatchExpression* expr) override {
        // Ignored. Translate to "true".
        _ctx.push(make<PathConstant>(Constant::boolean(true)));
    }

    void visit(const InternalEqHashedKey* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaAllElemMatchFromIndexMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaAllowedPropertiesMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaBinDataEncryptedTypeExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaBinDataFLE2EncryptedTypeExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaBinDataSubTypeExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaCondMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaEqMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaFmodMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaMatchArrayIndexMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaMaxItemsMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaMaxLengthMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaMaxPropertiesMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaMinItemsMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaMinLengthMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaMinPropertiesMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaObjectMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaRootDocEqMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaTypeExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaUniqueItemsMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalSchemaXorMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const LTEMatchExpression* expr) override {
        generateSimpleComparison(expr, Operations::Lte);
    }

    void visit(const LTMatchExpression* expr) override {
        generateSimpleComparison(expr, Operations::Lt);
    }

    void visit(const ModMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const NorMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const NotMatchExpression* expr) override {
        ABT result = _ctx.pop();

        // If this $not expression is a child of an $elemMatch, then we need to use a PathLambda to
        // ensure that the value stream (variable) corresponding to the inner path element is passed
        // into the inner EvalFilter.
        //
        // Examples:
        // find({"a.b": {$not: {$eq: 1}}}): The input into the not expression are documents from the
        // Scan. The EvalFilter expression will encapsulate the "a.b" path traversal.
        //
        // find({"a": {$elemMatch: {b: {$not: {$eq: 1}}}}}): The outer EvalFilter expression
        // encapsulates the "a" path traversal. However, we need the input to the not expression to
        // be the value of the "b" field, rather than those of "a". We use the PathLambda expression
        // to achieve this.
        if (_ctx.inElemMatch()) {
            auto notProjName = _ctx.getNextId("not");
            _ctx.push(make<PathLambda>(make<LambdaAbstraction>(
                notProjName,
                make<UnaryOp>(Operations::Not,
                              make<EvalFilter>(std::move(result), make<Variable>(notProjName))))));
            return;
        }
        _ctx.push(make<PathConstant>(make<UnaryOp>(
            Operations::Not,
            make<EvalFilter>(std::move(result), make<Variable>(_ctx.getRootProjection())))));
    }

    void visit(const OrMatchExpression* expr) override {
        visitAndOrExpression<PathComposeA, false>(expr);
    }

    void visit(const RegexMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const SizeMatchExpression* expr) override {
        assertSupportedPathExpression(expr);

        const ProjectionName lambdaProjName{_ctx.getNextId("lambda_sizeMatch")};
        auto result = [&]() {
            if (expr->getInputParamId()) {
                _ctx.getQueryParameters().emplace(
                    *expr->getInputParamId(),
                    Constant(sbe::value::TypeTags::NumberInt64,
                             sbe::value::bitcastFrom<int64_t>(*expr->getInputParamId())));
                return make<FunctionCall>(
                    kParameterFunctionName,
                    makeSeq(
                        make<Constant>(sbe::value::TypeTags::NumberInt32, *expr->getInputParamId()),
                        make<Constant>(sbe::value::TypeTags::NumberInt32,
                                       static_cast<int>(sbe::value::TypeTags::NumberInt32))));
            } else {
                return Constant::int64(expr->getData());
            }
        }();
        result = make<PathLambda>(make<LambdaAbstraction>(
            lambdaProjName,
            make<BinaryOp>(
                Operations::Eq,
                make<FunctionCall>("getArraySize", makeSeq(make<Variable>(lambdaProjName))),
                std::move(result))));
        if (shouldGeneratePath(expr)) {
            result = translateFieldRef(*(expr->fieldRef()), std::move(result));
        }
        _ctx.push(std::move(result));
    }

    void visit(const TextMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const TextNoOpMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const TwoDPtInAnnulusExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const TypeMatchExpression* expr) override {
        assertSupportedPathExpression(expr);

        const ProjectionName lambdaProjName{_ctx.getNextId("lambda_typeMatch")};
        ABT result = make<PathLambda>(make<LambdaAbstraction>(
            lambdaProjName,
            make<FunctionCall>("typeMatch",
                               makeSeq(make<Variable>(lambdaProjName),
                                       Constant::int32(expr->typeSet().getBSONTypeMask())))));

        if (shouldGeneratePath(expr)) {
            result = make<PathTraverse>(PathTraverse::kSingleLevel, std::move(result));
            if (expr->typeSet().hasType(BSONType::Array)) {
                // If we are testing against array type, insert a comparison against the
                // non-traversed path (the array itself if we have one).
                result = make<PathComposeA>(make<PathArr>(), std::move(result));
            }

            result = translateFieldRef(*(expr->fieldRef()), std::move(result));
        }
        _ctx.push(std::move(result));
    }

    void visit(const WhereMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const WhereNoOpMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

private:
    void generateBoolConstant(const bool value) {
        _ctx.push<PathConstant>(Constant::boolean(value));
    }

    template <bool isValueElemMatch>
    void generateElemMatch(const ArrayMatchingMatchExpression* expr) {
        assertSupportedPathExpression(expr);

        // Returns true if at least one sub-objects matches the condition.

        const size_t childCount = expr->numChildren();
        tassert(
            7021700, "ArrayMatchingMatchExpression must have at least one child", childCount > 0);

        _ctx.ensureArity(childCount);
        ABT result = _ctx.pop();
        for (size_t i = 1; i < childCount; i++) {
            maybeComposePath(result, _ctx.pop());
        }
        if constexpr (!isValueElemMatch) {
            // Make sure we consider only objects or arrays as elements of the array.
            maybeComposePath(result, make<PathComposeA>(make<PathObj>(), make<PathArr>()));
        }
        result = make<PathTraverse>(PathTraverse::kSingleLevel, std::move(result));

        // Make sure we consider only arrays fields on the path.
        maybeComposePath(result, make<PathArr>());

        if (shouldGeneratePath(expr)) {
            result = translateFieldRef(*(expr->fieldRef()), std::move(result));
        }

        _ctx.push(std::move(result));
    }

    void assertSupportedPathExpression(const PathMatchExpression* expr) {
        uassert(ErrorCodes::InternalErrorNotSupported,
                "Expression contains a numeric path component",
                !FieldRef(expr->path()).hasNumericPathComponents());
    }

    void generateSimpleComparison(const ComparisonMatchExpressionBase* expr, const Operations op) {
        assertSupportedPathExpression(expr);

        auto [tag, val] = sbe::value::makeValue(Value(expr->getData()));
        auto result = ABT{make<PathIdentity>()};
        if (expr->getInputParamId()) {
            result = make<FunctionCall>(
                kParameterFunctionName,
                makeSeq(make<Constant>(sbe::value::TypeTags::NumberInt32, *expr->getInputParamId()),
                        make<Constant>(sbe::value::TypeTags::NumberInt32, static_cast<int>(tag))));
            _ctx.getQueryParameters().emplace(*expr->getInputParamId(), Constant(tag, val));
        } else {
            result = make<Constant>(tag, val);
        }
        result = make<PathCompare>(op, std::move(result));

        bool tagNullMatchMissingField =
            tag == sbe::value::TypeTags::Null && (op == Operations::Lte || op == Operations::Gte);

        switch (op) {
            case Operations::Lt:
            case Operations::Lte: {
                auto&& [constant, inclusive] = getMinMaxBoundForValue(true /*isMin*/, tag, val);
                if (constant) {
                    maybeComposePath(result,
                                     make<PathCompare>(inclusive ? Operations::Gte : Operations::Gt,
                                                       std::move(constant.get())));
                }
                // Handle null and missing semantics
                // find({a: {$lt: MaxKey()}}) matches {a: null} and {b: 1}
                // find({a: {$lte: null}}) matches {a: null} and {b: 1})
                if (tag == sbe::value::TypeTags::MaxKey || tagNullMatchMissingField) {
                    maybeComposePath<PathComposeA>(result,
                                                   make<PathDefault>(Constant::boolean(true)));
                }
                break;
            }

            case Operations::Gt:
            case Operations::Gte: {
                auto&& [constant, inclusive] = getMinMaxBoundForValue(false /*isMin*/, tag, val);
                if (constant) {
                    maybeComposePath(result,
                                     make<PathCompare>(inclusive ? Operations::Lte : Operations::Lt,
                                                       std::move(constant.get())));
                }
                // Handle null and missing semantics
                // find({a: {$gt: MinKey()}}) matches {a: null} and {b: 1}
                // find({a: {$gte: null}}) matches {a: null} and {b: 1})
                if (tag == sbe::value::TypeTags::MinKey || tagNullMatchMissingField) {
                    maybeComposePath<PathComposeA>(result,
                                                   make<PathDefault>(Constant::boolean(true)));
                }
                break;
            }

            case Operations::Eq: {
                if (tag == sbe::value::TypeTags::Null) {
                    // Handle null and missing semantics. Matching against null also implies
                    // matching against missing.
                    result = make<PathComposeA>(make<PathDefault>(Constant::boolean(true)),
                                                std::move(result));
                }
                break;
            }

            default:
                tasserted(7021701,
                          str::stream()
                              << "Cannot generate comparison for operation: " << toStringData(op));
        }

        if (shouldGeneratePath(expr)) {
            if (tag == sbe::value::TypeTags::Array ||
                (op != Operations::Eq &&
                 (tag == sbe::value::TypeTags::MinKey || tag == sbe::value::TypeTags::MaxKey))) {
                // The behavior of PathTraverse when it encounters an array is to apply its subpath
                // to every element of the array and not the array itself. When we do a comparison
                // to an array, or an inequality comparison to minKey/maxKey, we need to ensure
                // that these comparisons happen to every element of the array and the array itself.
                //
                // For example:
                // find({a: [1]})
                //   matches {a: [1]} and {a: [[1]]}
                // find({a: {$gt: MinKey()}})
                //   matches {a: []} and {a: [MinKey()]}
                //   but not {a: MinKey()}
                result = make<PathComposeA>(make<PathTraverse>(PathTraverse::kSingleLevel, result),
                                            result);
            } else {
                result = make<PathTraverse>(PathTraverse::kSingleLevel, std::move(result));
            }

            result = translateFieldRef(*(expr->fieldRef()), std::move(result));
        }

        _ctx.push(std::move(result));
    }

    template <class Composition, bool defaultResult>
    void visitAndOrExpression(const ListOfMatchExpression* expr) {
        const size_t childCount = expr->numChildren();
        if (childCount == 0) {
            generateBoolConstant(defaultResult);
            return;
        }
        if (childCount == 1) {
            return;
        }

        ABTVector nodes;
        for (size_t i = 0; i < childCount; i++) {
            nodes.push_back(_ctx.pop());
        }

        // Construct a balanced composition tree.
        maybeComposePaths<Composition>(nodes);
        _ctx.push(std::move(nodes.front()));
    }

    void unsupportedExpression(const MatchExpression* expr) const {
        uasserted(ErrorCodes::InternalErrorNotSupported,
                  str::stream() << "Match expression is not supported: " << expr->matchType());
    }

    /**
     * Returns whether the currently visiting expression should consider the path it's operating on
     * and build the appropriate ABT. This can return false for expressions within an $elemMatch
     * that operate against each value in an array (aka "elemMatch value").
     */
    bool shouldGeneratePath(const PathMatchExpression* expr) const {
        // The only case where any expression, including $elemMatch, should ignore it's path is if
        // it's directly under a value $elemMatch. The 'elemMatchStack' includes 'expr' if it's an
        // $elemMatch, so we need to look back an extra element.
        if (expr->matchType() == MatchExpression::MatchType::ELEM_MATCH_OBJECT ||
            expr->matchType() == MatchExpression::MatchType::ELEM_MATCH_VALUE) {
            return _ctx.shouldGeneratePathForElemMatch();
        }

        return _ctx.shouldGeneratePath();
    }

    // If we are parsing a partial index filter, we don't allow agg expressions.
    const bool _allowAggExpressions;

    // We don't own this
    ExpressionAlgebrizerContext& _ctx;
};

ABT generateMatchExpression(const MatchExpression* expr,
                            const bool allowAggExpressions,
                            const ProjectionName& rootProjection,
                            PrefixId& prefixId,
                            QueryParameterMap& queryParameters) {
    ExpressionAlgebrizerContext ctx(false /*assertExprSort*/,
                                    true /*assertPathSort*/,
                                    rootProjection,
                                    prefixId,
                                    queryParameters);
    ABTMatchExpressionPreVisitor preVisitor(ctx);
    ABTMatchExpressionVisitor postVisitor(ctx, allowAggExpressions);
    MatchExpressionWalker walker(&preVisitor, nullptr /*inVisitor*/, &postVisitor);
    tree_walker::walk<true, MatchExpression>(expr, &walker);
    return ctx.pop();
}

}  // namespace mongo::optimizer
