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

#include <string>

#include <absl/container/node_hash_map.h>

#include "mongo/base/string_data.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/query/optimizer/algebra/operator.h"
#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/optimizer/comparison_op.h"
#include "mongo/db/query/optimizer/utils/unit_test_utils.h"
#include "mongo/db/query/sbe_stage_builder_sbexpr.h"
#include "mongo/db/query/sbe_stage_builder_vectorizer.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo::stage_builder {
namespace {

using namespace optimizer;

TEST(VectorizerTest, ConvertGt) {
    auto tree1 = make<BinaryOp>(Operations::Gt, make<Variable>("inputVar"), Constant::int32(9));

    sbe::value::FrameIdGenerator generator;
    Vectorizer::VariableTypes bindings;
    bindings.emplace("inputVar"_sd,
                     TypeSignature::kBlockType.include(TypeSignature::kAnyScalarType));

    auto processed = Vectorizer{&generator, Vectorizer::Purpose::Filter}.vectorize(tree1, bindings);

    ASSERT_EXPLAIN_V2_AUTO(
        "FunctionCall [valueBlockGt]\n"
        "|   Const [9]\n"
        "Variable [inputVar]\n",
        *processed.expr);
}

TEST(VectorizerTest, ConvertGtOnCell) {
    auto tree1 = make<BinaryOp>(Operations::Gt, make<Variable>("inputVar"), Constant::int32(9));

    sbe::value::FrameIdGenerator generator;
    Vectorizer::VariableTypes bindings;
    bindings.emplace("inputVar"_sd,
                     TypeSignature::kCellType.include(TypeSignature::kAnyScalarType));

    auto processed = Vectorizer{&generator, Vectorizer::Purpose::Filter}.vectorize(tree1, bindings);

    ASSERT_EXPLAIN_V2_AUTO(
        "FunctionCall [cellFoldValues_F]\n"
        "|   Variable [inputVar]\n"
        "FunctionCall [valueBlockGt]\n"
        "|   Const [9]\n"
        "FunctionCall [cellBlockGetFlatValuesBlock]\n"
        "Variable [inputVar]\n",
        *processed.expr);
}

TEST(VectorizerTest, ConvertBooleanOpOnCell) {
    auto tree1 = make<BinaryOp>(
        Operations::And,
        make<BinaryOp>(Operations::Lte, make<Variable>("inputVar"), Constant::int32(59)),
        make<BinaryOp>(Operations::Gt, make<Variable>("inputVar"), Constant::int32(9)));

    sbe::value::FrameIdGenerator generator;
    Vectorizer::VariableTypes bindings;
    bindings.emplace("inputVar"_sd,
                     TypeSignature::kCellType.include(TypeSignature::kAnyScalarType));

    auto processed = Vectorizer{&generator, Vectorizer::Purpose::Filter}.vectorize(tree1, bindings);

    ASSERT_EXPLAIN_V2_AUTO(
        "Let [__l1_0]\n"
        "|   FunctionCall [valueBlockLogicalAnd]\n"
        "|   |   FunctionCall [cellFoldValues_F]\n"
        "|   |   |   Variable [inputVar]\n"
        "|   |   FunctionCall [valueBlockGt]\n"
        "|   |   |   Const [9]\n"
        "|   |   FunctionCall [cellBlockGetFlatValuesBlock]\n"
        "|   |   Variable [inputVar]\n"
        "|   Variable [__l1_0]\n"
        "FunctionCall [cellFoldValues_F]\n"
        "|   Variable [inputVar]\n"
        "FunctionCall [valueBlockLte]\n"
        "|   Const [59]\n"
        "FunctionCall [cellBlockGetFlatValuesBlock]\n"
        "Variable [inputVar]\n",
        *processed.expr);
}

}  // namespace
}  // namespace mongo::stage_builder
