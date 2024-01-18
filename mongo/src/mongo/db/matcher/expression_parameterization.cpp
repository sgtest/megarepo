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

#include "mongo/db/matcher/expression_parameterization.h"

#include <cmath>

#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/platform/decimal128.h"
#include "mongo/util/assert_util.h"

namespace mongo {
void MatchExpressionParameterizationVisitor::visitBitTestExpression(BitTestMatchExpression* expr) {
    if (_context->availableParamIds(2)) {
        expr->setBitPositionsParamId(_context->nextInputParamId(expr));
        expr->setBitMaskParamId(_context->nextInputParamId(expr));
    }
}

void MatchExpressionParameterizationVisitor::visit(BitsAllClearMatchExpression* expr) {
    visitBitTestExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(BitsAllSetMatchExpression* expr) {
    visitBitTestExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(BitsAnyClearMatchExpression* expr) {
    visitBitTestExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(BitsAnySetMatchExpression* expr) {
    visitBitTestExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(EqualityMatchExpression* expr) {
    visitComparisonMatchExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(GTEMatchExpression* expr) {
    visitComparisonMatchExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(GTMatchExpression* expr) {
    visitComparisonMatchExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(LTEMatchExpression* expr) {
    visitComparisonMatchExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(LTMatchExpression* expr) {
    visitComparisonMatchExpression(expr);
}

void MatchExpressionParameterizationVisitor::visit(ModMatchExpression* expr) {
    if (_context->availableParamIds(2)) {
        expr->setDivisorInputParamId(_context->nextInputParamId(expr));
        expr->setRemainderInputParamId(_context->nextInputParamId(expr));
    }
}

void MatchExpressionParameterizationVisitor::visit(RegexMatchExpression* expr) {
    if (_context->availableParamIds(2)) {
        expr->setSourceRegexInputParamId(_context->nextInputParamId(expr));
        expr->setCompiledRegexInputParamId(_context->nextInputParamId(expr));
    }
}

void MatchExpressionParameterizationVisitor::visit(SizeMatchExpression* expr) {
    expr->setInputParamId(_context->nextInputParamId(expr));
}

void MatchExpressionParameterizationVisitor::visit(WhereMatchExpression* expr) {
    expr->setInputParamId(_context->nextInputParamId(expr));
}

void MatchExpressionParameterizationVisitor::visitComparisonMatchExpression(
    ComparisonMatchExpressionBase* expr) {
    auto type = expr->getData().type();
    switch (type) {
        case BSONType::MinKey:
        case BSONType::EOO:
        case BSONType::jstNULL:
        case BSONType::Array:
        case BSONType::DBRef:
        case BSONType::MaxKey:
        case BSONType::Undefined:
        case BSONType::Object:
            break;

        case BSONType::String:
        case BSONType::BinData:
        case BSONType::jstOID:
        case BSONType::Bool:
        case BSONType::RegEx:
        case BSONType::Code:
        case BSONType::Symbol:
        case BSONType::CodeWScope:
            expr->setInputParamId(_context->nextReusableInputParamId(expr));
            break;
        case BSONType::bsonTimestamp:
            if (expr->getData().timestamp() != Timestamp::max() &&
                expr->getData().timestamp() != Timestamp::min()) {
                expr->setInputParamId(_context->nextReusableInputParamId(expr));
            }
            break;
        case BSONType::Date:
            if (expr->getData().Date() != Date_t::max() &&
                expr->getData().Date() != Date_t::min()) {
                expr->setInputParamId(_context->nextReusableInputParamId(expr));
            }
            break;
        case BSONType::NumberInt:
            if (expr->getData().numberInt() != std::numeric_limits<int>::max() &&
                expr->getData().numberInt() != std::numeric_limits<int>::min()) {
                expr->setInputParamId(_context->nextReusableInputParamId(expr));
            }
            break;
        case BSONType::NumberLong:
            if (expr->getData().numberLong() != std::numeric_limits<long long>::max() &&
                expr->getData().numberLong() != std::numeric_limits<long long>::min()) {
                expr->setInputParamId(_context->nextReusableInputParamId(expr));
            }
            break;
        case BSONType::NumberDouble: {
            auto doubleVal = expr->getData().numberDouble();
            if (!std::isnan(doubleVal) && doubleVal != std::numeric_limits<double>::max() &&
                doubleVal != std::numeric_limits<double>::min() &&
                doubleVal != std::numeric_limits<double>::infinity() &&
                doubleVal != -std::numeric_limits<double>::infinity()) {
                expr->setInputParamId(_context->nextReusableInputParamId(expr));
            }
            break;
        }
        case BSONType::NumberDecimal:
            if (!expr->getData().numberDecimal().isNaN() &&
                !expr->getData().numberDecimal().isInfinite()) {
                expr->setInputParamId(_context->nextReusableInputParamId(expr));
            }
            break;
    }
}

void MatchExpressionParameterizationVisitor::visit(InMatchExpression* expr) {
    // We don't set inputParamId if a InMatchExpression contains a regex.
    if (!expr->getRegexes().empty()) {
        return;
    }

    // We don't set inputParamId if there's just one element because it could end up with a single
    // interval index bound that may be eligible for fast COUNT_SCAN plan. However, a
    // multiple-element $in query has more than one (point) intervals for the index bounds, which is
    // ineligible for COUNT_SCAN. This is to make sure that $in queries with multiple elements will
    // not share the same query shape with any other single-element $in query.
    if (auto inList = expr->getInList(); inList && inList->hasSingleElement()) {
        return;
    }

    if (expr->hasNull() || expr->hasArray() || expr->hasObject()) {
        // We don't set inputParamId if an InMatchExpression contains null, arrays, or objects.
        return;
    }

    expr->setInputParamId(_context->nextReusableInputParamId(expr));
}

void MatchExpressionParameterizationVisitor::visit(TypeMatchExpression* expr) {
    // TODO SERVER-64776: reenable auto-parameterization for $type expressions.
}
}  // namespace mongo
