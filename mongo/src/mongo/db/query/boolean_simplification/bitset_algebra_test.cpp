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

#include "mongo/db/query/boolean_simplification/bitset_algebra.h"

#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo::boolean_simplification {
constexpr size_t nbits = 64;

TEST(MintermOperationsTest, AAndB) {
    Minterm a{"01", "01"};
    Minterm b{"10", "10"};
    Maxterm expectedResult{{"11", "11"}};

    auto result = a & b;
    ASSERT_EQ(expectedResult, result);
}

TEST(MintermOperationsTest, AAndNotB) {
    Minterm a{"01", "01"};
    Minterm b{"00", "10"};
    Maxterm expectedResult{{"01", "11"}};

    auto result = a & b;
    ASSERT_EQ(expectedResult, result);
}

TEST(MintermOperationsTest, AAndNotA) {
    Minterm a{"1", "1"};
    Minterm na{"0", "1"};
    Maxterm expectedResult{a.size()};

    auto result = a & na;
    ASSERT_EQ(expectedResult, result);
}

TEST(MintermOperationsTest, AAndA) {
    Minterm a1{"1", "1"};
    Minterm a2{"1", "1"};
    Maxterm expectedResult{{"1", "1"}};

    auto result = a1 & a2;
    ASSERT_EQ(expectedResult, result);
}

TEST(MintermOperationsTest, ACDAndB) {
    Minterm acd{"1101", "1101"};
    Minterm b{"0010", "0010"};
    Maxterm expectedResult{{"1111", "1111"}};

    auto result = acd & b;
    ASSERT_EQ(expectedResult, result);
}

TEST(MintermOperationsTest, ComplexExpr) {
    Minterm acnbd{"1101", "1111"};
    Minterm b{"0010", "0010"};
    Maxterm expectedResult{b.size()};

    auto result = acnbd & b;
    ASSERT_EQ(expectedResult, result);
}

TEST(MintermOperationsTest, Not) {
    Minterm a{"00010001", "00110011"};
    Maxterm expectedResult({
        {"00000000", "00000001"},
        {"00000010", "00000010"},
        {"00000000", "00010000"},
        {"00100000", "00100000"},
    });

    auto result = ~a;
    ASSERT_EQ(expectedResult, result);
}

TEST(MaxtermOperationsTest, ABOrC) {
    Maxterm ab{{"011", "011"}};
    Maxterm c{{"100", "100"}};
    Maxterm expectedResult{
        {"011", "011"},
        {"100", "100"},
    };

    ab |= c;
    ASSERT_EQ(ab, expectedResult);
}

TEST(MaxtermOperationsTest, ABOrA) {
    Maxterm ab{{"11", "11"}};
    Maxterm a{{"01", "01"}};
    Maxterm expectedResult{
        {"11", "11"},
        {"01", "01"},
    };

    ab |= a;
    ASSERT_EQ(ab, expectedResult);
}

// (AB | A ) |= (~AC | BD)
TEST(MaxtermOperationsTest, ComplexOr) {
    Maxterm abOrA{
        {"0011", "0011"},
        {"0001", "0001"},
    };
    Maxterm nacOrBd{
        {"0100", "0101"},
        {"1010", "1010"},
    };
    Maxterm expectedResult{
        {"0011", "0011"},  // A & B
        {"0001", "0001"},  // A
        {"0100", "0101"},  // ~A & C
        {"1010", "1010"},  // B & D
    };

    abOrA |= nacOrBd;
    ASSERT_EQ(abOrA, expectedResult);
}

// (A | B) & C
TEST(MaxtermOperationsTest, ComplexAnd) {
    Maxterm aOrB{
        {"001", "001"},
        {"010", "010"},
    };

    Maxterm c{
        {"100", "100"},
    };

    Maxterm expectedResult{
        {"101", "101"},
        {"110", "110"},
    };

    auto result = aOrB & c;
    ASSERT_EQ(expectedResult, result);
}

// "(A | B) &= C"
TEST(MaxtermOperationsTest, ComplexUsingAndAssignmentOperator) {
    Maxterm aOrB{
        {"001", "001"},
        {"010", "010"},
    };

    Maxterm c{
        {"100", "100"},
    };

    Maxterm expectedResult{
        {"101", "101"},
        {"110", "110"},
    };

    aOrB &= c;
    ASSERT_EQ(expectedResult, aOrB);
}

// (A | B) & (C | ~D)
TEST(MaxtermOperationsTest, ComplexAnd2) {
    Maxterm aOrB{
        {"0001", "0001"},
        {"0010", "0010"},
    };

    Maxterm cOrNd{
        {"0100", "0100"},
        {"0000", "1000"},
    };

    Maxterm expectedResult{
        {"0101", "0101"},  // A & C
        {"0001", "1001"},  // A & ~D
        {"0110", "0110"},  // B & C
        {"0010", "1010"},  // B & ~D
    };

    auto result = aOrB & cOrNd;
    ASSERT_EQ(expectedResult, result);
}

// not (BC | A~D)
TEST(MaxtermOperationsTest, ComplexNot) {
    Maxterm bcOrAnd{
        {"0110", "0110"},
        {"0001", "1001"},
    };

    Maxterm expectedResult{
        {"0000", "0011"},  // ~A & ~B
        {"1000", "1010"},  // ~B & D
        {"0000", "0101"},  // ~A & ~C
        {"1000", "1100"},  // ~C & D
    };

    auto result = ~bcOrAnd;
    ASSERT_EQ(expectedResult, result);
}
}  // namespace mongo::boolean_simplification
