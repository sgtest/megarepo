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

#pragma once

#include "mongo/db/query/sbe_stage_builder_abt_helpers.h"
#include "mongo/db/query/sbe_stage_builder_helpers.h"
#include "mongo/db/query/sbe_stage_builder_sbexpr.h"

namespace mongo::stage_builder {
class SbExprBuilder {
public:
    using CaseValuePair = SbExpr::CaseValuePair;

    SbExprBuilder(StageBuilderState& state) : _state(state) {}

    SbExpr cloneExpr(const SbExpr& expr) {
        return expr.clone();
    }

    SbExpr makeVariable(SbVar var) {
        return var;
    }

    SbExpr makeVariable(sbe::FrameId frameId, sbe::value::SlotId slotId) {
        return SbVar(frameId, slotId);
    }

    SbExpr makeNot(SbExpr e);

    SbExpr makeUnaryOp(sbe::EPrimUnary::Op unaryOp, SbExpr e);
    SbExpr makeUnaryOp(optimizer::Operations unaryOp, SbExpr e);

    SbExpr makeBinaryOp(sbe::EPrimBinary::Op unaryOp, SbExpr lhs, SbExpr rhs);
    SbExpr makeBinaryOp(optimizer::Operations unaryOp, SbExpr lhs, SbExpr rhs);
    SbExpr makeBinaryOpWithCollation(sbe::EPrimBinary::Op unaryOp, SbExpr lhs, SbExpr rhs);
    SbExpr makeBinaryOpWithCollation(optimizer::Operations unaryOp, SbExpr lhs, SbExpr rhs);

    SbExpr makeConstant(sbe::value::TypeTags tag, sbe::value::Value val);
    SbExpr makeNothingConstant();
    SbExpr makeNullConstant();
    SbExpr makeBoolConstant(bool boolVal);
    SbExpr makeInt32Constant(int32_t num);
    SbExpr makeInt64Constant(int64_t num);
    SbExpr makeDoubleConstant(double num);
    SbExpr makeDecimalConstant(const Decimal128& num);
    SbExpr makeStrConstant(StringData str);

    SbExpr makeFunction(StringData name, SbExpr::Vector args);

    template <typename... Args>
    inline SbExpr makeFunction(StringData name, Args&&... args) {
        return makeFunction(name, SbExpr::makeSeq(std::forward<Args>(args)...));
    }

    SbExpr makeIf(SbExpr condExpr, SbExpr thenExpr, SbExpr elseExpr);

    SbExpr makeLet(sbe::FrameId frameId, SbExpr::Vector binds, SbExpr expr);

    SbExpr makeLocalLambda(sbe::FrameId frameId, SbExpr expr);

    SbExpr makeNumericConvert(SbExpr expr, sbe::value::TypeTags tag);

    SbExpr makeFail(ErrorCodes::Error error, StringData errorMessage);

    SbExpr makeFillEmptyFalse(SbExpr expr);
    SbExpr makeFillEmptyTrue(SbExpr expr);
    SbExpr makeFillEmptyNull(SbExpr expr);
    SbExpr makeFillEmptyUndefined(SbExpr expr);

    SbExpr makeIfNullExpr(SbExpr::Vector values);

    SbExpr generateNullOrMissing(SbExpr expr);
    SbExpr generatePositiveCheck(SbExpr expr);

    SbExpr generateNullOrMissing(SbVar var);
    SbExpr generateNonStringCheck(SbVar var);
    SbExpr generateNonTimestampCheck(SbVar var);
    SbExpr generateNegativeCheck(SbVar var);
    SbExpr generateNonPositiveCheck(SbVar var);
    SbExpr generateNonNumericCheck(SbVar var);
    SbExpr generateLongLongMinCheck(SbVar var);
    SbExpr generateNonArrayCheck(SbVar var);
    SbExpr generateNonObjectCheck(SbVar var);
    SbExpr generateNullishOrNotRepresentableInt32Check(SbVar var);
    SbExpr generateNaNCheck(SbVar var);
    SbExpr generateInfinityCheck(SbVar var);
    SbExpr generateInvalidRoundPlaceArgCheck(SbVar var);

    SbExpr buildMultiBranchConditional(SbExpr defaultCase) {
        return defaultCase;
    }

    template <typename... Ts>
    SbExpr buildMultiBranchConditional(SbExpr::CaseValuePair headCase, Ts... rest) {
        return makeIf(std::move(headCase.first),
                      std::move(headCase.second),
                      buildMultiBranchConditional(std::forward<Ts>(rest)...));
    }

    SbExpr buildMultiBranchConditionalFromCaseValuePairs(
        std::vector<SbExpr::CaseValuePair> caseValPairs, SbExpr defaultVal) {
        SbExpr result = std::move(defaultVal);

        for (size_t i = caseValPairs.size(); i > 0;) {
            --i;
            result = makeIf(std::move(caseValPairs[i].first),
                            std::move(caseValPairs[i].second),
                            std::move(result));
        }

        return result;
    }

    SbExpr makeBalancedBooleanOpTree(sbe::EPrimBinary::Op logicOp, SbExpr::Vector leaves) {
        return stage_builder::makeBalancedBooleanOpTree(logicOp, std::move(leaves), _state);
    }

    SbExpr makeBalancedBooleanOpTree(optimizer::Operations logicOp, SbExpr::Vector leaves) {
        return stage_builder::makeBalancedBooleanOpTree(
            getEPrimBinaryOp(logicOp), std::move(leaves), _state);
    }

private:
    std::unique_ptr<sbe::EExpression> extractExpr(SbExpr& e);

    sbe::EExpression::Vector extractExpr(SbExpr::Vector& sbExprs);

    StageBuilderState& _state;
};
}  // namespace mongo::stage_builder
