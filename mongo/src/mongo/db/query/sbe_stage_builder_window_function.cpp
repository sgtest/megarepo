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

#include "mongo/db/query/sbe_stage_builder_window_function.h"
#include "mongo/db/query/sbe_stage_builder_accumulator.h"

namespace mongo::stage_builder {

template <int N>
std::vector<std::unique_ptr<sbe::EExpression>> emptyInitializer(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> argExpr) {
    return std::vector<std::unique_ptr<sbe::EExpression>>{N};
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAddSum(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggRemovableSumAdd", std::move(arg)));
    return exprs;
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemoveSum(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggRemovableSumRemove", std::move(arg)));
    return exprs;
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizeSum(StageBuilderState& state,
                                                         const WindowFunctionStatement& stmt,
                                                         sbe::value::SlotVector slots) {
    sbe::EExpression::Vector exprs;
    for (auto slot : slots) {
        exprs.push_back(makeVariable(slot));
    }
    return makeE<sbe::EFunction>("aggRemovableSumFinalize", std::move(exprs));
}

AccumulationStatement createFakeAccumulationStatement(StageBuilderState& state,
                                                      const WindowFunctionStatement& stmt) {
    NamespaceString nss;
    auto expCtx = make_intrusive<ExpressionContext>(state.opCtx, nullptr, nss);
    AccumulationExpression accExpr{ExpressionConstant::create(expCtx.get(), Value(BSONNULL)),
                                   stmt.expr->input(),
                                   []() { return nullptr; },
                                   stmt.expr->getOpName()};
    return {"", accExpr};
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAddCovariance(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildAccumulator(
        accStmt, std::move(args), {} /* collatorSlot */, *state.frameIdGenerator);
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemoveCovariance(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    tassert(7820811, "Incorrect number of arguments", args.size() == 2);

    auto it = args.find(AccArgs::kCovarianceX);
    tassert(7820812,
            str::stream() << "Window function expects '" << AccArgs::kCovarianceX << "' argument",
            it != args.end());
    auto argX = std::move(it->second);

    it = args.find(AccArgs::kCovarianceY);
    tassert(7820813,
            str::stream() << "Window function expects '" << AccArgs::kCovarianceY << "' argument",
            it != args.end());
    auto argY = std::move(it->second);

    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggCovarianceRemove", std::move(argX), std::move(argY)));
    return exprs;
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizeCovarianceSamp(
    StageBuilderState& state, const WindowFunctionStatement& stmt, sbe::value::SlotVector slots) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildFinalize(
        state, accStmt, std::move(slots), {} /* collatorSlot */, *state.frameIdGenerator);
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizeCovariancePop(
    StageBuilderState& state, const WindowFunctionStatement& stmt, sbe::value::SlotVector slots) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildFinalize(
        state, accStmt, std::move(slots), {} /* collatorSlot */, *state.frameIdGenerator);
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAddPush(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggRemovablePushAdd", std::move(arg)));
    return exprs;
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemovePush(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggRemovablePushRemove"));
    return exprs;
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizePush(StageBuilderState& state,
                                                          const WindowFunctionStatement& stmt,
                                                          sbe::value::SlotVector slots) {
    sbe::EExpression::Vector exprs;
    for (auto slot : slots) {
        exprs.push_back(makeVariable(slot));
    }
    return makeE<sbe::EFunction>("aggRemovablePushFinalize", std::move(exprs));
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowInitializeIntegral(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> unitExpr) {
    std::vector<std::unique_ptr<sbe::EExpression>> aggs;
    aggs.push_back(makeFunction("aggIntegralInit", std::move(unitExpr), makeBoolConstant(false)));
    return aggs;
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAddIntegral(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildAccumulator(
        accStmt, std::move(args), {} /* collatorSlot */, *state.frameIdGenerator);
}


std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemoveIntegral(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    tassert(7996814, "Incorrect number of arguments", args.size() == 2);

    auto it = args.find(AccArgs::kInput);
    tassert(7996815,
            str::stream() << "Window function expects '" << AccArgs::kInput << "' argument",
            it != args.end());
    auto input = std::move(it->second);

    it = args.find(AccArgs::kSortBy);
    tassert(7996816,
            str::stream() << "Window function expects '" << AccArgs::kSortBy << "' argument",
            it != args.end());
    auto sortBy = std::move(it->second);

    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggIntegralRemove", std::move(input), std::move(sortBy)));
    return exprs;
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizeIntegral(StageBuilderState& state,
                                                              const WindowFunctionStatement& stmt,
                                                              sbe::value::SlotVector slots) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildFinalize(
        state, accStmt, std::move(slots), {} /* collatorSlot */, *state.frameIdGenerator);
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowInitializeDerivative(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> unitExpr) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildInitialize(accStmt, std::move(unitExpr), *state.frameIdGenerator);
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAddDerivative(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildAccumulator(
        accStmt, std::move(args), {} /* collatorSlot */, *state.frameIdGenerator);
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemoveDerivative(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    tassert(7996817, "Incorrect number of arguments", args.size() == 2);

    auto it = args.find(AccArgs::kInput);
    tassert(7996818,
            str::stream() << "Window function expects '" << AccArgs::kInput << "' argument",
            it != args.end());
    auto input = std::move(it->second);

    it = args.find(AccArgs::kSortBy);
    tassert(7996819,
            str::stream() << "Window function expects '" << AccArgs::kSortBy << "' argument",
            it != args.end());
    auto sortBy = std::move(it->second);

    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggDerivativeRemove", std::move(input), std::move(sortBy)));
    return exprs;
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizeDerivative(StageBuilderState& state,
                                                                const WindowFunctionStatement& stmt,
                                                                sbe::value::SlotVector slots) {
    auto accStmt = createFakeAccumulationStatement(state, stmt);
    return buildFinalize(
        state, accStmt, std::move(slots), {} /* collatorSlot */, *state.frameIdGenerator);
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAddStdDev(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggRemovableStdDevAdd", std::move(arg)));
    return exprs;
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemoveStdDev(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    std::vector<std::unique_ptr<sbe::EExpression>> exprs;
    exprs.push_back(makeFunction("aggRemovableStdDevRemove", std::move(arg)));
    return exprs;
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizeStdDevSamp(StageBuilderState& state,
                                                                const WindowFunctionStatement& stmt,
                                                                sbe::value::SlotVector slots) {
    tassert(8019606, "Incorrect number of arguments", slots.size() == 1);
    sbe::EExpression::Vector exprs;
    for (auto slot : slots) {
        exprs.push_back(makeVariable(slot));
    }
    return makeE<sbe::EFunction>("aggRemovableStdDevSampFinalize", std::move(exprs));
}

std::unique_ptr<sbe::EExpression> buildWindowFinalizeStdDevPop(StageBuilderState& state,
                                                               const WindowFunctionStatement& stmt,
                                                               sbe::value::SlotVector slots) {
    tassert(8019607, "Incorrect number of arguments", slots.size() == 1);
    sbe::EExpression::Vector exprs;
    for (auto slot : slots) {
        exprs.push_back(makeVariable(slot));
    }
    return makeE<sbe::EFunction>("aggRemovableStdDevPopFinalize", std::move(exprs));
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowInit(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    using BuildInitFn = std::function<std::vector<std::unique_ptr<sbe::EExpression>>(
        StageBuilderState&, const WindowFunctionStatement&, std::unique_ptr<sbe::EExpression>)>;

    static const StringDataMap<BuildInitFn> kWindowFunctionBuilders = {
        {"$sum", &emptyInitializer<1>},
        {"$covarianceSamp", &emptyInitializer<1>},
        {"$covariancePop", &emptyInitializer<1>},
        {"$push", &emptyInitializer<1>},
        {"$integral", &buildWindowInitializeIntegral},
        {"$derivative", &buildWindowInitializeDerivative},
        {"$stdDevSamp", &emptyInitializer<1>},
        {"$stdDevPop", &emptyInitializer<1>},
    };

    auto opName = stmt.expr->getOpName();
    uassert(7914603,
            str::stream() << "Unsupported window function in SBE stage builder: " << opName,
            kWindowFunctionBuilders.find(opName) != kWindowFunctionBuilders.end());

    return std::invoke(kWindowFunctionBuilders.at(opName), state, stmt, std::move(arg));
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAdd(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    using BuildAddFn = std::function<std::vector<std::unique_ptr<sbe::EExpression>>(
        StageBuilderState&, const WindowFunctionStatement&, std::unique_ptr<sbe::EExpression>)>;

    static const StringDataMap<BuildAddFn> kWindowFunctionBuilders = {
        {"$sum", &buildWindowAddSum},
        {"$push", &buildWindowAddPush},
        {"$stdDevSamp", &buildWindowAddStdDev},
        {"$stdDevPop", &buildWindowAddStdDev},
    };

    auto opName = stmt.expr->getOpName();
    uassert(7914604,
            str::stream() << "Unsupported window function in SBE stage builder: " << opName,
            kWindowFunctionBuilders.find(opName) != kWindowFunctionBuilders.end());

    return std::invoke(kWindowFunctionBuilders.at(opName), state, stmt, std::move(arg));
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowAdd(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    using BuildAddFn = std::function<std::vector<std::unique_ptr<sbe::EExpression>>(
        StageBuilderState&,
        const WindowFunctionStatement&,
        StringDataMap<std::unique_ptr<sbe::EExpression>>)>;

    static const StringDataMap<BuildAddFn> kWindowFunctionBuilders = {
        {"$covarianceSamp", &buildWindowAddCovariance},
        {"$covariancePop", &buildWindowAddCovariance},
        {"$integral", &buildWindowAddIntegral},
        {"$derivative", &buildWindowAddDerivative},
    };

    auto opName = stmt.expr->getOpName();
    uassert(7820816,
            str::stream() << "Unsupported window function in SBE stage builder: " << opName,
            kWindowFunctionBuilders.find(opName) != kWindowFunctionBuilders.end());

    return std::invoke(kWindowFunctionBuilders.at(opName), state, stmt, std::move(args));
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemove(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    std::unique_ptr<sbe::EExpression> arg) {
    using BuildRemoveFn = std::function<std::vector<std::unique_ptr<sbe::EExpression>>(
        StageBuilderState&, const WindowFunctionStatement&, std::unique_ptr<sbe::EExpression>)>;

    static const StringDataMap<BuildRemoveFn> kWindowFunctionBuilders = {
        {"$sum", &buildWindowRemoveSum},
        {"$push", &buildWindowRemovePush},
        {"$stdDevSamp", &buildWindowRemoveStdDev},
        {"$stdDevPop", &buildWindowRemoveStdDev},
    };

    auto opName = stmt.expr->getOpName();
    uassert(7914605,
            str::stream() << "Unsupported window function in SBE stage builder: " << opName,
            kWindowFunctionBuilders.find(opName) != kWindowFunctionBuilders.end());

    return std::invoke(kWindowFunctionBuilders.at(opName), state, stmt, std::move(arg));
}

std::vector<std::unique_ptr<sbe::EExpression>> buildWindowRemove(
    StageBuilderState& state,
    const WindowFunctionStatement& stmt,
    StringDataMap<std::unique_ptr<sbe::EExpression>> args) {
    using BuildRemoveFn = std::function<std::vector<std::unique_ptr<sbe::EExpression>>(
        StageBuilderState&,
        const WindowFunctionStatement&,
        StringDataMap<std::unique_ptr<sbe::EExpression>>)>;

    static const StringDataMap<BuildRemoveFn> kWindowFunctionBuilders = {
        {"$covarianceSamp", &buildWindowRemoveCovariance},
        {"$covariancePop", &buildWindowRemoveCovariance},
        {"$integral", &buildWindowRemoveIntegral},
        {"$derivative", &buildWindowRemoveDerivative},
    };

    auto opName = stmt.expr->getOpName();
    uassert(7820817,
            str::stream() << "Unsupported window function in SBE stage builder: " << opName,
            kWindowFunctionBuilders.find(opName) != kWindowFunctionBuilders.end());

    return std::invoke(kWindowFunctionBuilders.at(opName), state, stmt, std::move(args));
}

std::unique_ptr<sbe::EExpression> buildWindowFinalize(StageBuilderState& state,
                                                      const WindowFunctionStatement& stmt,
                                                      sbe::value::SlotVector values) {
    using BuildFinalizeFn = std::function<std::unique_ptr<sbe::EExpression>(
        StageBuilderState&, const WindowFunctionStatement&, sbe::value::SlotVector values)>;

    static const StringDataMap<BuildFinalizeFn> kWindowFunctionBuilders = {
        {"$sum", &buildWindowFinalizeSum},
        {"$covarianceSamp", &buildWindowFinalizeCovarianceSamp},
        {"$covariancePop", &buildWindowFinalizeCovariancePop},
        {"$push", &buildWindowFinalizePush},
        {"$integral", &buildWindowFinalizeIntegral},
        {"$derivative", &buildWindowFinalizeDerivative},
        {"$stdDevSamp", &buildWindowFinalizeStdDevSamp},
        {"$stdDevPop", &buildWindowFinalizeStdDevPop},
    };

    auto opName = stmt.expr->getOpName();
    uassert(7914606,
            str::stream() << "Unsupported window function in SBE stage builder: " << opName,
            kWindowFunctionBuilders.find(opName) != kWindowFunctionBuilders.end());

    return std::invoke(kWindowFunctionBuilders.at(opName), state, stmt, std::move(values));
}
}  // namespace mongo::stage_builder
