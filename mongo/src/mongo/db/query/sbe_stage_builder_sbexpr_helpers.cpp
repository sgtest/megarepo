/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include "mongo/db/query/sbe_stage_builder_sbexpr_helpers.h"

#include "mongo/db/query/sbe_stage_builder_abt_holder_impl.h"

namespace mongo::stage_builder {
namespace {
inline bool hasABT(const SbExpr& e) {
    return e.hasABT();
}

inline bool hasABT(const SbExpr::Vector& exprs) {
    return std::all_of(exprs.begin(), exprs.end(), [](auto&& e) { return hasABT(e); });
}

template <typename... Ts>
inline bool hasABT(const SbExpr& head, Ts&&... rest) {
    return hasABT(head) && hasABT(std::forward<Ts>(rest)...);
}

template <typename... Ts>
inline bool hasABT(const SbExpr::Vector& head, Ts&&... rest) {
    return hasABT(head) && hasABT(std::forward<Ts>(rest)...);
}

inline optimizer::ABT extractABT(SbExpr& e) {
    return abt::unwrap(e.extractABT());
}

inline optimizer::ABTVector extractABT(SbExpr::Vector& exprs) {
    // Convert the SbExpr vector to an ABT vector.
    optimizer::ABTVector abtExprs;
    for (auto& e : exprs) {
        abtExprs.emplace_back(extractABT(e));
    }

    return abtExprs;
}

inline optimizer::Operations getOptimizerOp(sbe::EPrimUnary::Op op) {
    switch (op) {
        case sbe::EPrimUnary::negate:
            return optimizer::Operations::Neg;
        case sbe::EPrimUnary::logicNot:
            return optimizer::Operations::Not;
        default:
            MONGO_UNREACHABLE;
    }
}

inline optimizer::Operations getOptimizerOp(sbe::EPrimBinary::Op op) {
    switch (op) {
        case sbe::EPrimBinary::eq:
            return optimizer::Operations::Eq;
        case sbe::EPrimBinary::neq:
            return optimizer::Operations::Neq;
        case sbe::EPrimBinary::greater:
            return optimizer::Operations::Gt;
        case sbe::EPrimBinary::greaterEq:
            return optimizer::Operations::Gte;
        case sbe::EPrimBinary::less:
            return optimizer::Operations::Lt;
        case sbe::EPrimBinary::lessEq:
            return optimizer::Operations::Lte;
        case sbe::EPrimBinary::add:
            return optimizer::Operations::Add;
        case sbe::EPrimBinary::sub:
            return optimizer::Operations::Sub;
        case sbe::EPrimBinary::fillEmpty:
            return optimizer::Operations::FillEmpty;
        case sbe::EPrimBinary::logicAnd:
            return optimizer::Operations::And;
        case sbe::EPrimBinary::logicOr:
            return optimizer::Operations::Or;
        case sbe::EPrimBinary::cmp3w:
            return optimizer::Operations::Cmp3w;
        case sbe::EPrimBinary::div:
            return optimizer::Operations::Div;
        case sbe::EPrimBinary::mul:
            return optimizer::Operations::Mult;
        default:
            MONGO_UNREACHABLE;
    }
}
}  // namespace

std::unique_ptr<sbe::EExpression> SbExprBuilder::extractExpr(SbExpr& e) {
    return e.extractExpr(_state).expr;
}

sbe::EExpression::Vector SbExprBuilder::extractExpr(SbExpr::Vector& sbExprs) {
    // Convert the SbExpr vector to an EExpression vector.
    sbe::EExpression::Vector exprs;
    for (auto& e : sbExprs) {
        exprs.emplace_back(extractExpr(e));
    }

    return exprs;
}

SbExpr SbExprBuilder::makeNot(SbExpr e) {
    if (hasABT(e)) {
        return abt::wrap(stage_builder::makeNot(extractABT(e)));
    } else {
        return stage_builder::makeNot(extractExpr(e));
    }
}

SbExpr SbExprBuilder::makeUnaryOp(sbe::EPrimUnary::Op unaryOp, SbExpr e) {
    if (hasABT(e)) {
        return abt::wrap(stage_builder::makeUnaryOp(getOptimizerOp(unaryOp), extractABT(e)));
    } else {
        return stage_builder::makeUnaryOp(unaryOp, extractExpr(e));
    }
}

SbExpr SbExprBuilder::makeUnaryOp(optimizer::Operations unaryOp, SbExpr e) {
    if (hasABT(e)) {
        return abt::wrap(stage_builder::makeUnaryOp(unaryOp, extractABT(e)));
    } else {
        return stage_builder::makeUnaryOp(getEPrimUnaryOp(unaryOp), extractExpr(e));
    }
}

SbExpr SbExprBuilder::makeBinaryOp(sbe::EPrimBinary::Op binaryOp, SbExpr lhs, SbExpr rhs) {
    if (hasABT(lhs, rhs)) {
        return abt::wrap(stage_builder::makeBinaryOp(
            getOptimizerOp(binaryOp), extractABT(lhs), extractABT(rhs)));
    } else {
        return stage_builder::makeBinaryOp(binaryOp, extractExpr(lhs), extractExpr(rhs));
    }
}

SbExpr SbExprBuilder::makeBinaryOp(optimizer::Operations binaryOp, SbExpr lhs, SbExpr rhs) {
    if (hasABT(lhs, rhs)) {
        return abt::wrap(stage_builder::makeBinaryOp(binaryOp, extractABT(lhs), extractABT(rhs)));
    } else {
        return stage_builder::makeBinaryOp(
            getEPrimBinaryOp(binaryOp), extractExpr(lhs), extractExpr(rhs));
    }
}

SbExpr SbExprBuilder::makeBinaryOpWithCollation(sbe::EPrimBinary::Op binaryOp,
                                                SbExpr lhs,
                                                SbExpr rhs) {
    auto collatorSlot = _state.getCollatorSlot();
    if (!collatorSlot) {
        return makeBinaryOp(binaryOp, std::move(lhs), std::move(rhs));
    }

    return sbe::makeE<sbe::EPrimBinary>(
        binaryOp, extractExpr(lhs), extractExpr(rhs), sbe::makeE<sbe::EVariable>(*collatorSlot));
}

SbExpr SbExprBuilder::makeBinaryOpWithCollation(optimizer::Operations binaryOp,
                                                SbExpr lhs,
                                                SbExpr rhs) {
    auto collatorSlot = _state.getCollatorSlot();
    if (!collatorSlot) {
        return makeBinaryOp(binaryOp, std::move(lhs), std::move(rhs));
    }

    return sbe::makeE<sbe::EPrimBinary>(getEPrimBinaryOp(binaryOp),
                                        extractExpr(lhs),
                                        extractExpr(rhs),
                                        sbe::makeE<sbe::EVariable>(*collatorSlot));
}

SbExpr SbExprBuilder::makeConstant(sbe::value::TypeTags tag, sbe::value::Value val) {
    return abt::wrap(optimizer::make<optimizer::Constant>(tag, val));
}

SbExpr SbExprBuilder::makeNothingConstant() {
    return abt::wrap(optimizer::Constant::nothing());
}

SbExpr SbExprBuilder::makeNullConstant() {
    return abt::wrap(optimizer::Constant::null());
}

SbExpr SbExprBuilder::makeBoolConstant(bool boolVal) {
    return abt::wrap(optimizer::Constant::boolean(boolVal));
}

SbExpr SbExprBuilder::makeInt32Constant(int32_t num) {
    return abt::wrap(optimizer::Constant::int32(num));
}

SbExpr SbExprBuilder::makeInt64Constant(int64_t num) {
    return abt::wrap(optimizer::Constant::int64(num));
}

SbExpr SbExprBuilder::makeDoubleConstant(double num) {
    return abt::wrap(optimizer::Constant::fromDouble(num));
}

SbExpr SbExprBuilder::makeDecimalConstant(const Decimal128& num) {
    return abt::wrap(optimizer::Constant::fromDecimal(num));
}

SbExpr SbExprBuilder::makeStrConstant(StringData str) {
    return abt::wrap(optimizer::Constant::str(str));
}

SbExpr SbExprBuilder::makeFunction(StringData name, SbExpr::Vector args) {
    if (hasABT(args)) {
        return abt::wrap(stage_builder::makeABTFunction(name, extractABT(args)));
    } else {
        return stage_builder::makeFunction(name, extractExpr(args));
    }
}

SbExpr SbExprBuilder::makeIf(SbExpr condExpr, SbExpr thenExpr, SbExpr elseExpr) {
    if (hasABT(condExpr, thenExpr, elseExpr)) {
        return abt::wrap(stage_builder::makeIf(
            extractABT(condExpr), extractABT(thenExpr), extractABT(elseExpr)));
    } else {
        return stage_builder::makeIf(
            extractExpr(condExpr), extractExpr(thenExpr), extractExpr(elseExpr));
    }
}

SbExpr SbExprBuilder::makeLet(sbe::FrameId frameId, SbExpr::Vector binds, SbExpr expr) {
    if (hasABT(expr, binds)) {
        return abt::wrap(stage_builder::makeLet(frameId, extractABT(binds), extractABT(expr)));
    } else {
        return stage_builder::makeLet(frameId, extractExpr(binds), extractExpr(expr));
    }
}

SbExpr SbExprBuilder::makeLocalLambda(sbe::FrameId frameId, SbExpr expr) {
    if (hasABT(expr)) {
        return abt::wrap(stage_builder::makeLocalLambda(frameId, extractABT(expr)));
    } else {
        return stage_builder::makeLocalLambda(frameId, extractExpr(expr));
    }
}

SbExpr SbExprBuilder::makeNumericConvert(SbExpr expr, sbe::value::TypeTags tag) {
    if (hasABT(expr)) {
        return abt::wrap(stage_builder::makeNumericConvert(extractABT(expr), tag));
    } else {
        return stage_builder::makeNumericConvert(extractExpr(expr), tag);
    }
}

SbExpr SbExprBuilder::makeFail(ErrorCodes::Error error, StringData errorMessage) {
    return abt::wrap(stage_builder::makeABTFail(error, errorMessage));
}

SbExpr SbExprBuilder::makeFillEmptyFalse(SbExpr expr) {
    if (hasABT(expr)) {
        return abt::wrap(stage_builder::makeFillEmptyFalse(extractABT(expr)));
    } else {
        return stage_builder::makeFillEmptyFalse(extractExpr(expr));
    }
}

SbExpr SbExprBuilder::makeFillEmptyTrue(SbExpr expr) {
    if (hasABT(expr)) {
        return abt::wrap(stage_builder::makeFillEmptyTrue(extractABT(expr)));
    } else {
        return stage_builder::makeFillEmptyTrue(extractExpr(expr));
    }
}

SbExpr SbExprBuilder::makeFillEmptyNull(SbExpr expr) {
    if (hasABT(expr)) {
        return abt::wrap(stage_builder::makeFillEmptyNull(extractABT(expr)));
    } else {
        return stage_builder::makeFillEmptyNull(extractExpr(expr));
    }
}

SbExpr SbExprBuilder::makeFillEmptyUndefined(SbExpr expr) {
    if (hasABT(expr)) {
        return abt::wrap(stage_builder::makeFillEmptyUndefined(extractABT(expr)));
    } else {
        return stage_builder::makeFillEmptyUndefined(extractExpr(expr));
    }
}

SbExpr SbExprBuilder::makeIfNullExpr(SbExpr::Vector values) {
    if (hasABT(values)) {
        return abt::wrap(
            stage_builder::makeIfNullExpr(extractABT(values), _state.frameIdGenerator));
    } else {
        return stage_builder::makeIfNullExpr(extractExpr(values), _state.frameIdGenerator);
    }
}

SbExpr SbExprBuilder::generateNullOrMissing(SbExpr expr) {
    if (hasABT(expr)) {
        return abt::wrap(stage_builder::generateABTNullOrMissing(extractABT(expr)));
    } else {
        return stage_builder::generateNullOrMissing(extractExpr(expr));
    }
}

SbExpr SbExprBuilder::generatePositiveCheck(SbExpr expr) {
    return abt::wrap(stage_builder::generateABTPositiveCheck(extractABT(expr)));
}

SbExpr SbExprBuilder::generateNullOrMissing(SbVar var) {
    return abt::wrap(stage_builder::generateABTNullOrMissing(var.getABTName()));
}

SbExpr SbExprBuilder::generateNonStringCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNonStringCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateNonTimestampCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNonTimestampCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateNegativeCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNegativeCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateNonPositiveCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNonPositiveCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateNonNumericCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNonNumericCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateLongLongMinCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTLongLongMinCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateNonArrayCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNonArrayCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateNonObjectCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNonObjectCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateNullishOrNotRepresentableInt32Check(SbVar var) {
    return abt::wrap(
        stage_builder::generateABTNullishOrNotRepresentableInt32Check(var.getABTName()));
}

SbExpr SbExprBuilder::generateNaNCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTNaNCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateInfinityCheck(SbVar var) {
    return abt::wrap(stage_builder::generateABTInfinityCheck(var.getABTName()));
}

SbExpr SbExprBuilder::generateInvalidRoundPlaceArgCheck(SbVar var) {
    return abt::wrap(stage_builder::generateInvalidRoundPlaceArgCheck(var.getABTName()));
}
}  // namespace mongo::stage_builder
