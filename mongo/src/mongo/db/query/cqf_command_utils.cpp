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

#include "mongo/db/query/cqf_command_utils.h"

#include <memory>
#include <set>
#include <vector>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <s2cellid.h>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/basic_types.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/exec/add_fields_projection_executor.h"
#include "mongo/db/exec/exclusion_projection_executor.h"
#include "mongo/db/exec/inclusion_projection_executor.h"
#include "mongo/db/exec/projection_executor.h"
#include "mongo/db/exec/projection_executor_builder.h"
#include "mongo/db/field_ref.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/index_names.h"
#include "mongo/db/matcher/expression.h"
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
#include "mongo/db/pipeline/accumulator.h"
#include "mongo/db/pipeline/accumulator_multi.h"
#include "mongo/db/pipeline/accumulator_percentile.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/pipeline/dependencies.h"
#include "mongo/db/pipeline/document_source_match.h"
#include "mongo/db/pipeline/document_source_replace_root.h"
#include "mongo/db/pipeline/document_source_single_document_transformation.h"
#include "mongo/db/pipeline/expression.h"
#include "mongo/db/pipeline/expression_visitor.h"
#include "mongo/db/pipeline/expression_walker.h"
#include "mongo/db/pipeline/group_from_first_document_transformation.h"
#include "mongo/db/pipeline/transformer_interface.h"
#include "mongo/db/pipeline/visitors/document_source_visitor_registry_mongod.h"
#include "mongo/db/pipeline/visitors/document_source_walker.h"
#include "mongo/db/pipeline/visitors/transformer_interface_visitor.h"
#include "mongo/db/pipeline/visitors/transformer_interface_walker.h"
#include "mongo/db/query/ce_mode_parameter.h"
#include "mongo/db/query/expression_walker.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/projection_policies.h"
#include "mongo/db/query/query_decorations.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/query/tree_walker.h"
#include "mongo/db/server_parameter.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/string_map.h"
#include "mongo/util/synchronized_value.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery
namespace mongo {

using namespace optimizer;

namespace {

/**
 * Visitor that is responsible for indicating whether a MatchExpression is eligible for Bonsai
 * by setting the '_eligibility' member variable. Expressions which are "test-only" and not
 * officially supported should set _eligibility to Ineligible.
 */
class ABTMatchExpressionVisitor : public MatchExpressionConstVisitor {
public:
    ABTMatchExpressionVisitor(bool queryHasNaturalHint)
        : _eligibility(BonsaiEligibility::FullyEligible),
          _queryHasNaturalHint(queryHasNaturalHint) {}

    void visit(const LTEMatchExpression* expr) override {
        assertSupportedComparisonMatchExpression(expr);
    }
    void visit(const LTMatchExpression* expr) override {
        assertSupportedComparisonMatchExpression(expr);
    }
    void visit(const ElemMatchObjectMatchExpression* expr) override {
        assertSupportedPathExpression(expr);
    }
    void visit(const ElemMatchValueMatchExpression* expr) override {
        assertSupportedPathExpression(expr);
    }
    void visit(const EqualityMatchExpression* expr) override {
        assertSupportedComparisonMatchExpression(expr);
    }
    void visit(const GTEMatchExpression* expr) override {
        assertSupportedComparisonMatchExpression(expr);
    }
    void visit(const GTMatchExpression* expr) override {
        assertSupportedComparisonMatchExpression(expr);
    }
    void visit(const InMatchExpression* expr) override {
        assertSupportedPathExpression(expr);

        // Dotted path equality to null is not supported.
        const auto fieldRef = expr->fieldRef();
        if (fieldRef && fieldRef->numParts() > 1) {
            _eligibility.setIneligibleIf(std::any_of(expr->getEqualities().begin(),
                                                     expr->getEqualities().end(),
                                                     [](auto&& elt) { return elt.isNull(); }));
        }

        // $in over a regex predicate is not supported.
        _eligibility.setIneligibleIf(!expr->getRegexes().empty());
    }
    void visit(const ExistsMatchExpression* expr) override {
        assertSupportedPathExpression(expr);
    }
    void visit(const AndMatchExpression* expr) override {}
    void visit(const OrMatchExpression* expr) override {}

    void visit(const GeoMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const GeoNearMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalBucketGeoWithinMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalExprEqMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalExprGTMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalExprGTEMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalExprLTMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const InternalExprLTEMatchExpression* expr) override {
        unsupportedExpression(expr);
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

    void visit(const ModMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const NorMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const NotMatchExpression* expr) override {}

    void visit(const RegexMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const SizeMatchExpression* expr) override {
        unsupportedExpression(expr);
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

    void visit(const WhereMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const WhereNoOpMatchExpression* expr) override {
        unsupportedExpression(expr);
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

    void visit(const TypeMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const AlwaysFalseMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const AlwaysTrueMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    void visit(const ExprMatchExpression* expr) override {
        unsupportedExpression(expr);
    }

    const BonsaiEligibility& eligibility() const {
        return _eligibility;
    }

private:
    void unsupportedExpression(const MatchExpression* expr) {
        _eligibility.setIneligible();
    }

    void assertSupportedComparisonMatchExpression(const ComparisonMatchExpression* expr) {
        assertSupportedPathExpression(expr);

        // Dotted path equality to null is not supported.
        const auto fieldRef = expr->fieldRef();
        _eligibility.setIneligibleIf(fieldRef && fieldRef->numParts() > 1 &&
                                     expr->getData().isNull());
    }

    void assertSupportedPathExpression(const PathMatchExpression* expr) {
        const auto fieldRef = FieldRef(expr->path());
        _eligibility.setIneligibleIf(fieldRef.hasNumericPathComponents());

        // Match expressions which compare against _id are only fully eligible if the query has a
        // $natural hint. Otherwise, they could use the _id index.
        if (!fieldRef.empty() && fieldRef.getPart(0) == "_id" && !_queryHasNaturalHint) {
            _eligibility.minOf(BonsaiEligibility::Experimental);
        }
    }

    BonsaiEligibility _eligibility;
    bool _queryHasNaturalHint;
};

class ABTUnsupportedAggExpressionVisitor : public ExpressionConstVisitor {
public:
    ABTUnsupportedAggExpressionVisitor() : _eligibility(BonsaiEligibility::FullyEligible) {}

    void visit(const ExpressionConstant* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionAbs* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionAdd* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionAllElementsTrue* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionAnd* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionAnyElementTrue* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionArray* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionArrayElemAt* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionBitAnd* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionBitOr* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionBitXor* expr) override final {
        unsupportedExpression();
    }
    void visit(const ExpressionBitNot* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFirst* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionLast* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionObjectToArray* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionArrayToObject* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionBsonSize* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionCeil* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionCoerceToBool* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionCompare* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionConcat* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionConcatArrays* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionCond* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateFromString* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateFromParts* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateDiff* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateToParts* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateToString* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateTrunc* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDivide* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionExp* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFieldPath* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFilter* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFloor* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIfNull* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIn* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIndexOfArray* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIndexOfBytes* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIndexOfCP* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIsNumber* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionLet* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionLn* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionLog* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionLog10* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalFLEEqual* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalFLEBetween* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionMap* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionMeta* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionMod* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionMultiply* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionNot* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionObject* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionOr* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionPow* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionRange* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionReduce* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionReplaceOne* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionReplaceAll* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSetDifference* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSetEquals* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSetIntersection* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSetIsSubset* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSetUnion* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSize* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionReverseArray* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSortArray* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSlice* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIsArray* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalFindAllValuesAtPath* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionRound* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSplit* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSqrt* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionStrcasecmp* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSubstrBytes* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSubstrCP* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionStrLenBytes* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionBinarySize* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionStrLenCP* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSubtract* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSwitch* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionTestApiVersion* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionToLower* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionToUpper* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionTrim* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionTrunc* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionType* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionZip* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionConvert* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionRegexFind* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionRegexFindAll* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionRegexMatch* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionCosine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionTangent* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionArcCosine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionArcSine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionArcTangent* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionArcTangent2* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionHyperbolicArcTangent* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionHyperbolicArcCosine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionHyperbolicArcSine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionHyperbolicTangent* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionHyperbolicCosine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionHyperbolicSine* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDegreesToRadians* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionRadiansToDegrees* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDayOfMonth* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDayOfWeek* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDayOfYear* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionHour* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionMillisecond* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionMinute* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionMonth* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSecond* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionWeek* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIsoWeekYear* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIsoDayOfWeek* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionIsoWeek* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionYear* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulator<AccumulatorAvg>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulatorN<AccumulatorFirstN>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulatorN<AccumulatorLastN>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulator<AccumulatorMax>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulator<AccumulatorMin>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulatorN<AccumulatorMaxN>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulatorN<AccumulatorMinN>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulatorQuantile<AccumulatorMedian>* expr) override final {
        unsupportedExpression();
    }

    void visit(
        const ExpressionFromAccumulatorQuantile<AccumulatorPercentile>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulator<AccumulatorStdDevPop>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulator<AccumulatorStdDevSamp>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulator<AccumulatorSum>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFromAccumulator<AccumulatorMergeObjects>* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionTests::Testable* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalJsEmit* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalFindSlice* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalFindPositional* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalFindElemMatch* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionFunction* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionRandom* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionToHashedIndexKey* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateAdd* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionDateSubtract* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionSetField* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionGetField* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionTsSecond* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionTsIncrement* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalOwningShard* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalIndexKey* expr) override final {
        unsupportedExpression();
    }

    void visit(const ExpressionInternalKeyStringValue* expr) override final {
        unsupportedExpression();
    }

    const BonsaiEligibility& eligibility() const {
        return _eligibility;
    }

private:
    void unsupportedExpression() {
        _eligibility.setIneligible();
    }

    BonsaiEligibility _eligibility;
};

class ABTTransformerVisitor : public TransformerInterfaceConstVisitor {
public:
    ABTTransformerVisitor() : _eligibility(BonsaiEligibility::FullyEligible) {}

    void visit(const projection_executor::ExclusionProjectionExecutor* transformer) override {
        checkUnsupportedInclusionExclusion(transformer);
    }

    void visit(const projection_executor::InclusionProjectionExecutor* transformer) override {
        checkUnsupportedInclusionExclusion(transformer);
    }

    void visit(const projection_executor::AddFieldsProjectionExecutor* transformer) override {
        unsupportedTransformer(transformer);
    }

    void visit(const GroupFromFirstDocumentTransformation* transformer) override {
        unsupportedTransformer(transformer);
    }

    void visit(const ReplaceRootTransformation* transformer) override {
        unsupportedTransformer(transformer);
    }

    const BonsaiEligibility& eligibility() const {
        return _eligibility;
    }

private:
    void unsupportedTransformer(const TransformerInterface* transformer) {
        _eligibility.setIneligible();
    }

    template <typename T>
    void checkUnsupportedInclusionExclusion(const T* transformer) {
        OrderedPathSet computedPaths;
        StringMap<std::string> renamedPaths;
        transformer->getRoot()->reportComputedPaths(&computedPaths, &renamedPaths);

        // Non-simple projections are supported under test only.
        if (computedPaths.size() > 0 || renamedPaths.size() > 0) {
            unsupportedTransformer(transformer);
            return;
        }

        OrderedPathSet preservedPaths;
        transformer->getRoot()->reportProjectedPaths(&preservedPaths);

        for (const std::string& path : preservedPaths) {
            if (FieldRef(path).hasNumericPathComponents()) {
                unsupportedTransformer(transformer);
                return;
            }
        }

        ABTUnsupportedAggExpressionVisitor aggVisitor;
        stage_builder::ExpressionWalker walker{&aggVisitor, nullptr, nullptr};
        expression_walker::walk(transformer->rootReplacementExpression().get(), &walker);
        _eligibility.minOf(aggVisitor.eligibility());
    }

    BonsaiEligibility _eligibility;
};

//
// Check unsupported command options.
//
template <class RequestType>
BonsaiEligibility eligibilityOfCommandOptions(const RequestType& request) {
    // The FindCommandRequest defaults some parameters to BSONObj() instead of boost::none.
    auto hasParam = [&](auto param) {
        if constexpr (std::is_same_v<decltype(param), boost::optional<BSONObj>>) {
            return param && !param->isEmpty();
        } else {
            return !param.isEmpty();
        }
    };
    if (hasParam(request.getResumeAfter()) || request.getRequestResumeToken() ||
        request.getLegacyRuntimeConstants()) {
        return BonsaiEligibility::Ineligible;
    }

    // Below we enforce that the collection collation is empty (aka, "simple"). Therefore we can
    // support either empty collation or simple collation on the query.
    auto hasNonSimpleCollation = [](auto param) {
        if constexpr (std::is_same_v<decltype(param), boost::optional<BSONObj>>) {
            return param && !param->isEmpty() &&
                SimpleBSONObjComparator::kInstance.evaluate(*param != CollationSpec::kSimpleSpec);
        } else {
            return !param.isEmpty() &&
                SimpleBSONObjComparator::kInstance.evaluate(param != CollationSpec::kSimpleSpec);
        }
    }(request.getCollation());
    if (hasNonSimpleCollation) {
        return BonsaiEligibility::Ineligible;
    }

    return BonsaiEligibility::FullyEligible;
}

BonsaiEligibility eligibilityOfNonNaturalIndexHint(const boost::optional<BSONObj>& param) {
    if (param) {
        if (!param->isEmpty() &&
            param->firstElementFieldNameStringData() != query_request_helper::kNaturalSortField) {
            // $natural is the only hint fully supported.
            return BonsaiEligibility::Experimental;
        }
    }
    return BonsaiEligibility::FullyEligible;
}

BonsaiEligibility eligibilityOfIndexes(IndexCatalog::IndexIterator* indexIterator) {
    auto eligibility = BonsaiEligibility{BonsaiEligibility::FullyEligible};

    while (indexIterator->more()) {
        const IndexDescriptor& descriptor = *indexIterator->next()->descriptor();
        if (descriptor.hidden()) {
            // An index that is hidden will not be considered by the optimizer, so we don't need
            // to check its eligibility further.
            continue;
        }

        // In M2, allow {id: 'hashed'} index for test coverage purposes, but we don't add it to
        // the metadata.
        if (descriptor.isHashedIdIndex()) {
            continue;
        }

        // When any non-hidden, non-_id index is present, a query is only fully eligible with a
        // $natural hint.
        if (!descriptor.isIdIndex()) {
            eligibility.minOf(BonsaiEligibility::Experimental);
        }

        if (descriptor.getIndexType() != IndexType::INDEX_BTREE) {
            // Return early if ineligible.
            return BonsaiEligibility::Ineligible;
        }

        if (descriptor.infoObj().hasField(IndexDescriptor::kExpireAfterSecondsFieldName) ||
            descriptor.isPartial() || descriptor.isSparse() || !descriptor.collation().isEmpty()) {
            return BonsaiEligibility::Ineligible;
        }
    }

    return eligibility;
}

// Eligibility determination common across FindCommandRequest and AggregateCommandRequest.
template <class RequestType>
BonsaiEligibility determineEligibilityCommon(const RequestType& request,
                                             OperationContext* opCtx,
                                             const CollectionPtr& collection) {
    auto eligibility = BonsaiEligibility{BonsaiEligibility::FullyEligible};

    return eligibility.minOf([&]() { return eligibilityOfCommandOptions(request); })
        .minOf([&]() { return eligibilityOfNonNaturalIndexHint(request.getHint()); })
        .minOf([&]() {
            // Check unsupported index types.
            if (!collection)
                return BonsaiEligibility{BonsaiEligibility::FullyEligible};

            auto hint = request.getHint();
            auto queryHasNaturalHint = [&hint]() {
                if constexpr (std::is_same_v<decltype(hint), boost::optional<BSONObj>>) {
                    return hint && !hint->isEmpty() &&
                        hint->firstElementFieldNameStringData() ==
                        query_request_helper::kNaturalSortField;
                } else {
                    return !hint.isEmpty() &&
                        hint.firstElementFieldNameStringData() ==
                        query_request_helper::kNaturalSortField;
                }
            }();

            // If the query has a hint specifying $natural, then there is no need to inspect the
            // index catalog since we know we will generate a collection scan plan.
            if (!queryHasNaturalHint) {
                const IndexCatalog& indexCatalog = *collection->getIndexCatalog();
                auto indexIterator =
                    indexCatalog.getIndexIterator(opCtx, IndexCatalog::InclusionPolicy::kReady);

                return eligibilityOfIndexes(indexIterator.get());
            } else {
                return BonsaiEligibility{BonsaiEligibility::FullyEligible};
            }
        })
        // Check unsupported collection types.
        .setIneligibleIf(collection &&
                         (collection->isClustered() ||
                          !collection->getCollectionOptions().collation.isEmpty() ||
                          collection->getTimeseriesOptions() || collection->isCapped()))
        // Check notablescan.
        .setIneligibleIf(storageGlobalParams.noTableScan.load());
}

boost::optional<bool> shouldForceEligibility(QueryFrameworkControlEnum frameworkControl) {
    // We don't need to consult the feature flag here, since the framework control knob can only
    // be set to enable bonsai if featureFlagCommonQueryFramework is enabled.
    LOGV2_DEBUG(7325101,
                4,
                "logging internalQueryFrameworkControl",
                "knob"_attr = QueryFrameworkControl_serializer(frameworkControl));

    switch (frameworkControl) {
        case QueryFrameworkControlEnum::kForceClassicEngine:
        case QueryFrameworkControlEnum::kTrySbeEngine:
        case QueryFrameworkControlEnum::kTrySbeRestricted:
            return false;
        case QueryFrameworkControlEnum::kTryBonsai:
        case QueryFrameworkControlEnum::kTryBonsaiExperimental:
            // Return boost::none to indicate that we should not force eligibility of bonsai nor
            // the classic engine.
            return boost::none;
        case QueryFrameworkControlEnum::kForceBonsai:
            return true;
    }

    MONGO_UNREACHABLE;
}

BonsaiEligibility checkSupportedFeatures(ServiceContext* serviceCtx,
                                         const Pipeline& pipeline,
                                         bool queryHasNaturalHint) {
    ABTUnsupportedDocumentSourceVisitorContext visitorCtx{queryHasNaturalHint};
    auto& reg = getDocumentSourceVisitorRegistry(serviceCtx);
    DocumentSourceWalker walker(reg, &visitorCtx);
    walker.walk(pipeline);
    return visitorCtx.eligibility;
}

BonsaiEligibility checkSupportedFeatures(const CanonicalQuery& cq) {
    auto expression = cq.getPrimaryMatchExpression();

    auto hint = cq.getFindCommandRequest().getHint();
    bool hasNaturalHint = !hint.isEmpty() &&
        hint.firstElementFieldNameStringData() == query_request_helper::kNaturalSortField;

    ABTMatchExpressionVisitor visitor(hasNaturalHint);
    MatchExpressionWalker walker(nullptr /*preVisitor*/, nullptr /*inVisitor*/, &visitor);
    tree_walker::walk<true, MatchExpression>(expression, &walker);
    auto eligibility = visitor.eligibility();

    return eligibility.minOf([&]() {
        if (cq.getProj()) {
            auto projExecutor = projection_executor::buildProjectionExecutor(
                cq.getExpCtx(),
                cq.getProj(),
                ProjectionPolicies::findProjectionPolicies(),
                projection_executor::BuilderParamsBitSet{
                    projection_executor::kDefaultBuilderParams});
            ABTTransformerVisitor visitor{};
            TransformerInterfaceWalker walker(&visitor);
            walker.walk(projExecutor.get());
            return visitor.eligibility();
        } else {
            return BonsaiEligibility{BonsaiEligibility::FullyEligible};
        }
    });
}

/**
 * Use the framework control to determine the minimum required eligibility level.
 */
BonsaiEligibility::Eligibility getMinRequiredEligibility(OperationContext* opCtx) {
    auto frameworkControl =
        QueryKnobConfiguration::decoration(opCtx).getInternalQueryFrameworkControlForOp();
    switch (frameworkControl) {
        case QueryFrameworkControlEnum::kForceBonsai:
            return BonsaiEligibility::Ineligible;
        case QueryFrameworkControlEnum::kTryBonsaiExperimental:
            return BonsaiEligibility::Experimental;
        default:
            return BonsaiEligibility::FullyEligible;
    }
}
}  // namespace

MONGO_FAIL_POINT_DEFINE(enableExplainInBonsai);

BonsaiEligibility determineBonsaiEligibility(OperationContext* opCtx,
                                             const CollectionPtr& collection,
                                             const AggregateCommandRequest& request,
                                             const Pipeline& pipeline) {
    return BonsaiEligibility{BonsaiEligibility::FullyEligible, getMinRequiredEligibility(opCtx)}
        .minOf([&]() { return determineEligibilityCommon(request, opCtx, collection); })
        .setIneligibleIf(request.getRequestReshardingResumeToken().has_value())
        .setIneligibleIf(request.getExchange().has_value())
        .minOf([&]() {
            auto hint = request.getHint();
            bool hasNaturalHint = hint.has_value() && !hint->isEmpty() &&
                hint->firstElementFieldNameStringData() == query_request_helper::kNaturalSortField;

            return checkSupportedFeatures(opCtx->getServiceContext(), pipeline, hasNaturalHint);
        });
}

BonsaiEligibility determineBonsaiEligibility(OperationContext* opCtx,
                                             const CollectionPtr& collection,
                                             const CanonicalQuery& cq) {
    auto request = cq.getFindCommandRequest();
    return BonsaiEligibility{BonsaiEligibility::FullyEligible, getMinRequiredEligibility(opCtx)}
        .setIneligibleIf(!cq.useCqfIfEligible())
        .minOf([&]() { return determineEligibilityCommon(request, opCtx, collection); })
        .setIneligibleIf(!request.getSort().isEmpty())
        .setIneligibleIf(!request.getMin().isEmpty())
        .setIneligibleIf(!request.getMax().isEmpty())
        .setIneligibleIf(request.getReturnKey())
        .setIneligibleIf(request.getSingleBatch())
        .setIneligibleIf(request.getTailable())
        .setIneligibleIf(request.getSkip().has_value())
        .setIneligibleIf(request.getLimit().has_value())
        .setIneligibleIf(request.getNoCursorTimeout())
        .setIneligibleIf(request.getAllowPartialResults())
        .setIneligibleIf(request.getAllowSpeculativeMajorityRead())
        .setIneligibleIf(request.getAwaitData())
        .setIneligibleIf(request.getReadOnce())
        .setIneligibleIf(request.getShowRecordId())
        .setIneligibleIf(request.getTerm().has_value())
        .minOf([&]() { return checkSupportedFeatures(cq); });
}

bool isEligibleForBonsaiUnderFrameworkControl(OperationContext* opCtx,
                                              bool isExplain,
                                              BonsaiEligibility eligibility) {
    auto frameworkControl =
        QueryKnobConfiguration::decoration(opCtx).getInternalQueryFrameworkControlForOp();

    // Explain is not currently supported but is allowed if the failpoint is set
    // for testing purposes.
    // TODO SERVER-77719: eventually explain should be permitted by default with tryBonsai, but
    // we will still want to fall back on explain commands with tryBonsaiExperimental.
    auto satisfiesExplainRequirements = [&]() {
        return !isExplain || MONGO_unlikely(enableExplainInBonsai.shouldFail());
    };
    switch (frameworkControl) {
        case QueryFrameworkControlEnum::kTryBonsai:
            return eligibility.isFullyEligible() && satisfiesExplainRequirements();
        case QueryFrameworkControlEnum::kTryBonsaiExperimental:
            return eligibility.isExperimentallyEligible() && satisfiesExplainRequirements();
        case QueryFrameworkControlEnum::kForceBonsai:
            return true;
        default:
            return false;
    }
}

bool isEligibleForBonsai(OperationContext* opCtx,
                         const CollectionPtr& collection,
                         const AggregateCommandRequest& request,
                         const Pipeline& pipeline) {
    auto eligibility = determineBonsaiEligibility(opCtx, collection, request, pipeline);
    return isEligibleForBonsaiUnderFrameworkControl(
        opCtx, request.getExplain().has_value(), eligibility);
}

bool isEligibleForBonsai(OperationContext* opCtx,
                         const CollectionPtr& collection,
                         const CanonicalQuery& cq) {
    auto eligibility = determineBonsaiEligibility(opCtx, collection, cq);
    return isEligibleForBonsaiUnderFrameworkControl(
        opCtx, cq.getExplain().has_value(), eligibility);
}

BonsaiEligibility isEligibleForBonsai_forTesting(const CanonicalQuery& cq) {
    return checkSupportedFeatures(cq);
}

BonsaiEligibility isEligibleForBonsai_forTesting(ServiceContext* serviceCtx,
                                                 const Pipeline& pipeline) {
    return checkSupportedFeatures(serviceCtx, pipeline, false /* queryHasNaturalHint */);
}

}  // namespace mongo

namespace mongo::optimizer {
// Templated visit function to mark unsupported DocumentSources as ineligible for CQF.
template <typename T>
void visit(ABTUnsupportedDocumentSourceVisitorContext* ctx, const T&) {
    ctx->eligibility.setIneligible();
}

void visit(ABTUnsupportedDocumentSourceVisitorContext* ctx, const DocumentSourceMatch& source) {
    ABTMatchExpressionVisitor visitor(ctx->queryHasNaturalHint);
    MatchExpressionWalker walker(nullptr, nullptr, &visitor);
    tree_walker::walk<true, MatchExpression>(source.getMatchExpression(), &walker);
    ctx->eligibility.minOf(visitor.eligibility());
}

void visit(ABTUnsupportedDocumentSourceVisitorContext* ctx,
           const DocumentSourceSingleDocumentTransformation& source) {
    ABTTransformerVisitor visitor;
    TransformerInterfaceWalker walker(&visitor);
    walker.walk(&source.getTransformer());
    ctx->eligibility.minOf(visitor.eligibility());
}

const ServiceContext::ConstructorActionRegisterer abtUnsupportedRegisterer{
    "ABTUnsupportedRegisterer", [](ServiceContext* service) {
        registerMongodVisitor<ABTUnsupportedDocumentSourceVisitorContext>(service);
    }};
}  // namespace mongo::optimizer
