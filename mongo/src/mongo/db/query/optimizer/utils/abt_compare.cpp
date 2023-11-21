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

#include "mongo/db/query/optimizer/utils/abt_compare.h"

#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include <absl/container/node_hash_map.h>

#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/query/optimizer/bool_expression.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/utils/strong_alias.h"
#include "mongo/util/assert_util.h"


namespace mongo::optimizer {

/**
 * Smaller containers sort first. For equal sets we perform lexicographical comparison.
 * Custom comparator is supplied.
 */
template <class T, class Fn>
int compareContainers(const T& n1, const T& n2, const Fn& fn) {
    if (n1.size() < n2.size()) {
        return -1;
    }
    if (n1.size() > n2.size()) {
        return 1;
    }

    auto i2 = n2.begin();
    for (auto i1 = n1.begin(); i1 != n1.end(); i1++, i2++) {
        const int cmp = fn(*i1, *i2);
        if (cmp != 0) {
            return cmp;
        }
    }

    return 0;
}

/**
 * Used to compare strings and strong string aliases.
 */
template <class T>
int compareStrings(const T& v1, const T& v2) {
    return v1.compare(v2);
}

/**
 * Helper class used to compare ABTs.
 */
class ABTCompareTransporter {
public:
    template <typename T>
    int operator()(const ABT& n, const T& node, const ABT& other) {
        if (const auto otherPtr = other.cast<T>()) {
            // If the types are the same, route to method which compares them.
            return compare(node, *otherPtr);
        }

        // When types are different, sort based on tags.
        const auto t1 = n.tagOf();
        const auto t2 = other.tagOf();
        return (t1 == t2) ? 0 : ((t1 < t2) ? -1 : 1);
    }

    int compareNodes(const ABT& n1, const ABT& n2) {
        return n1.visit(*this, n2);
    }

private:
    int compare(const Blackhole& node, const Blackhole& other) {
        return 0;
    }

    int compare(const Constant& node, const Constant& other) {
        const auto [tag, val] = node.get();
        const auto [otherTag, otherVal] = other.get();
        const auto [compareTag, compareVal] = compareValue(tag, val, otherTag, otherVal);
        uassert(
            7086703, "Invalid comparison result", compareTag == sbe::value::TypeTags::NumberInt32);
        return sbe::value::bitcastTo<int32_t>(compareVal);
    }

    int compare(const Variable& node, const Variable& other) {
        return node.name().compare(other.name());
    }

    int compare(const UnaryOp& node, const UnaryOp& other) {
        if (node.op() < other.op()) {
            return -1;
        } else if (node.op() > other.op()) {
            return 1;
        }
        return compareNodes(node.getChild(), other.getChild());
    }

    int compare(const BinaryOp& node, const BinaryOp& other) {
        if (node.op() < other.op()) {
            return -1;
        } else if (node.op() > other.op()) {
            return 1;
        }

        const int cmp = compareNodes(node.getLeftChild(), other.getLeftChild());
        if (cmp != 0) {
            return cmp;
        }

        return compareNodes(node.getRightChild(), other.getRightChild());
    }

    int compare(const If& node, const If& other) {
        int cmp = compareNodes(node.getCondChild(), other.getCondChild());
        if (cmp != 0) {
            return cmp;
        }

        cmp = compareNodes(node.getThenChild(), other.getThenChild());
        if (cmp != 0) {
            return cmp;
        }

        return compareNodes(node.getElseChild(), other.getElseChild());
    }

    int compare(const Let& node, const Let& other) {
        int cmp = node.varName().compare(other.varName());
        if (cmp != 0) {
            return cmp;
        }

        cmp = compareNodes(node.bind(), other.bind());
        if (cmp != 0) {
            return cmp;
        }

        return compareNodes(node.in(), other.in());
    }

    int compare(const LambdaAbstraction& node, const LambdaAbstraction& other) {
        const int cmp = node.varName().compare(other.varName());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getBody(), other.getBody());
    }

    int compare(const LambdaApplication& node, const LambdaApplication& other) {
        const int cmp = node.getLambda().visit(*this, other.getLambda());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getArgument(), other.getArgument());
    }

    int compare(const FunctionCall& node, const FunctionCall& other) {
        const int cmp = node.name().compare(other.name());
        if (cmp != 0) {
            return cmp;
        }
        return compareContainers(node.nodes(), other.nodes(), compareExprAndPaths);
    }

    int compare(const EvalPath& node, const EvalPath& other) {
        const int cmp = compareNodes(node.getInput(), other.getInput());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getPath(), other.getPath());
    }

    int compare(const EvalFilter& node, const EvalFilter& other) {
        const int cmp = compareNodes(node.getInput(), other.getInput());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getPath(), other.getPath());
    }

    int compare(const Source& node, const Source& other) {
        return 0;
    }

    int compare(const PathConstant& node, const PathConstant& other) {
        return compareNodes(node.getConstant(), other.getConstant());
    }

    int compare(const PathLambda& node, const PathLambda& other) {
        return compareNodes(node.getLambda(), other.getLambda());
    }

    int compare(const PathIdentity& node, const PathIdentity& other) {
        return 0;
    }

    int compare(const PathDefault& node, const PathDefault& other) {
        return compareNodes(node.getDefault(), other.getDefault());
    }

    int compare(const PathCompare& node, const PathCompare& other) {
        if (node.op() < other.op()) {
            return -1;
        } else if (node.op() > other.op()) {
            return 1;
        }
        return compareNodes(node.getVal(), other.getVal());
    }

    int compare(const PathDrop& node, const PathDrop& other) {
        return compareContainers(node.getNames(), other.getNames(), compareStrings<FieldNameType>);
    }

    int compare(const PathKeep& node, const PathKeep& other) {
        return compareContainers(node.getNames(), other.getNames(), compareStrings<FieldNameType>);
    }

    int compare(const PathObj& node, const PathObj& other) {
        return 0;
    }

    int compare(const PathArr& node, const PathArr& other) {
        return 0;
    }

    int compare(const PathTraverse& node, const PathTraverse& other) {
        if (node.getMaxDepth() < other.getMaxDepth()) {
            return -1;
        } else if (node.getMaxDepth() > other.getMaxDepth()) {
            return 1;
        }
        return compareNodes(node.getPath(), other.getPath());
    }

    int compare(const PathField& node, const PathField& other) {
        const int cmp = node.name().compare(other.name());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getPath(), other.getPath());
    }

    int compare(const PathGet& node, const PathGet& other) {
        const int cmp = node.name().compare(other.name());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getPath(), other.getPath());
    }

    int compare(const PathComposeM& node, const PathComposeM& other) {
        const int cmp = compareNodes(node.getPath1(), other.getPath1());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getPath2(), other.getPath2());
    }

    int compare(const PathComposeA& node, const PathComposeA& other) {
        const int cmp = compareNodes(node.getPath1(), other.getPath1());
        if (cmp != 0) {
            return cmp;
        }
        return compareNodes(node.getPath2(), other.getPath2());
    }

    int compare(const ExpressionBinder& node, const ExpressionBinder& other) {
        const int cmp =
            compareContainers(node.names(), other.names(), compareStrings<ProjectionName>);
        if (cmp != 0) {
            return cmp;
        }
        return compareContainers(node.exprs(), other.exprs(), compareExprAndPaths);
    }

    int compare(const References& node, const References& other) {
        return compareContainers(node.nodes(), other.nodes(), compareExprAndPaths);
    }

    template <class T>
    int compare(const T& node, const T& other) {
        static_assert(std::is_base_of_v<Node, T>,
                      "Expressions and Paths must implement comparisons");
        return 0;
    }
};

int compareExprAndPaths(const ABT& n1, const ABT& n2) {
    ABTCompareTransporter instance;
    return n1.visit(instance, n2);
}

int compareIntervals(const IntervalRequirement& i1, const IntervalRequirement& i2) {
    const auto& low1 = i1.getLowBound();
    const auto& high1 = i1.getHighBound();
    const auto& low2 = i2.getLowBound();
    const auto& high2 = i2.getHighBound();

    // Sort constant intervals first.
    if (i1.isConstant() && !i2.isConstant()) {
        return -1;
    } else if (!i1.isConstant() && i2.isConstant()) {
        return 1;
    }

    // By lower bound expression.
    const int cmpLow = compareExprAndPaths(low1.getBound(), low2.getBound());
    if (cmpLow != 0) {
        return cmpLow;
    }

    // By high bound expression.
    const int cmpHigh = compareExprAndPaths(high1.getBound(), high2.getBound());
    if (cmpHigh != 0) {
        return cmpHigh;
    }

    // Sort first by inclusive lower bounds.
    if (low1.isInclusive() && !low2.isInclusive()) {
        return -1;
    } else if (!low1.isInclusive() && low2.isInclusive()) {
        return 1;
    }

    // Then by inclusive high bounds.
    if (high1.isInclusive() && !high2.isInclusive()) {
        return -1;
    } else if (!high1.isInclusive() && high2.isInclusive()) {
        return 1;
    }
    return 0;
}

class IntervalExprComparator {
public:
    template <typename T>
    int operator()(const IntervalReqExpr::Node& n,
                   const T& node,
                   const IntervalReqExpr::Node& other) {
        if (const auto otherPtr = other.cast<T>()) {
            // If the types are the same, route to method which compares them.
            return compare(node, *otherPtr);
        }

        // When types are different, sort based on tags.
        const auto t1 = n.tagOf();
        const auto t2 = other.tagOf();
        return (t1 == t2) ? 0 : ((t1 < t2) ? -1 : 1);
    }

    int compareIntExpr(const IntervalReqExpr::Node& i1, const IntervalReqExpr::Node& i2) {
        return i1.visit(*this, i2);
    }

private:
    int compare(const IntervalReqExpr::Atom& node, const IntervalReqExpr::Atom& other) {
        return compareIntervals(node.getExpr(), other.getExpr());
    }

    int compare(const IntervalReqExpr::Conjunction& node,
                const IntervalReqExpr::Conjunction& other) {
        return compareContainers(node.nodes(), other.nodes(), compareIntervalExpr);
    }

    int compare(const IntervalReqExpr::Disjunction& node,
                const IntervalReqExpr::Disjunction& other) {
        return compareContainers(node.nodes(), other.nodes(), compareIntervalExpr);
    }
};

int compareIntervalExpr(const IntervalReqExpr::Node& i1, const IntervalReqExpr::Node& i2) {
    return IntervalExprComparator{}.compareIntExpr(i1, i2);
}

class PSRExprComparator {
public:
    template <typename T>
    int operator()(const PSRExpr::Node& n, const T& node, const PSRExpr::Node& other) {
        if (const auto otherPtr = other.cast<T>()) {
            // If the types are the same, route to method which compares them.
            return compare(node, *otherPtr);
        }

        // When types are different, sort based on tags.
        const auto t1 = n.tagOf();
        const auto t2 = other.tagOf();
        return (t1 < t2) ? -1 : 1;
    }

    int comparePSRExpr(const PSRExpr::Node& n1, const PSRExpr::Node& n2) {
        return n1.visit(*this, n2);
    }

private:
    int compare(const PSRExpr::Atom& node, const PSRExpr::Atom& other) {
        return PartialSchemaEntryComparator::Cmp3W{}(node.getExpr(), other.getExpr());
    }

    int compare(const PSRExpr::Conjunction& node, const PSRExpr::Conjunction& other) {
        return compareContainers(node.nodes(), other.nodes(), comparePartialSchemaRequirementsExpr);
    }

    int compare(const PSRExpr::Disjunction& node, const PSRExpr::Disjunction& other) {
        return compareContainers(node.nodes(), other.nodes(), comparePartialSchemaRequirementsExpr);
    }
};

int comparePartialSchemaRequirementsExpr(const PSRExpr::Node& n1, const PSRExpr::Node& n2) {
    return PSRExprComparator{}.comparePSRExpr(n1, n2);
}

namespace {

/**
 * Returns true if the given ABT is NaN, otherwise returns false.
 */
bool isNaN(const ABT& abt) {
    // Only perform NaN check if the ABT is a Constant.
    auto c = abt.cast<Constant>();
    if (!c)
        return false;

    auto [tag, val] = c->get();
    return sbe::value::isNaN(tag, val);
}

// Returns true if the given ABT represent a query parameter, otherwise returns false.
bool isParameter(const ABT& abt) {
    auto p = abt.cast<FunctionCall>();
    if (!p) {
        return false;
    }
    return p->name() == kParameterFunctionName;
}

// Given an ABT representing a query parameter, return the type tag of the parameter.
sbe::value::TypeTags parameterType(const ABT& abt) {
    // See defintion of 'kParameterFunctionName' for details about representation of query
    // parameters in ABT.
    return static_cast<sbe::value::TypeTags>(
        abt.cast<FunctionCall>()->nodes()[1].cast<Constant>()->get().second);
}

/**
 * Compare a numeric, non-NaN FunctionCall[getParam] node to NaN. NaN will always be smaller.
 */
CmpResult cmpNumericParamToNaN(Operations op, const ABT& lhs) {
    auto lhsIsNaN = isNaN(lhs);
    switch (op) {
        case Operations::Lt:
        case Operations::Lte:
            return lhsIsNaN ? CmpResult::kTrue : CmpResult::kFalse;
        case Operations::Gt:
        case Operations::Gte:
            return lhsIsNaN ? CmpResult::kFalse : CmpResult::kTrue;
        case Operations::Cmp3w:
            return lhsIsNaN ? CmpResult::kLt : CmpResult::kGt;
        default:
            MONGO_UNREACHABLE;
    }
}

// Compare two type tags for the purposes of constant evaluation of FunctionCall[getParam]
// expressions which are guarenteed to evaluate to the specified SBE type.
// This function returns kIncomparable if the given type tags are of the same canonical BSON type.
// This is becuase we cannot determine anything about two expressions that are of the same type.
// If the two tags are of different canonical BSON types, this function will compare them according
// to the specified operation. For example, in the BSON order, integers are always less than
// strings.
// For comparisons between the Constant NaN and a FunctionCall[getParam] node of a different
// canonical type, cmpTags will handle constant folding because NaN falls under the numeric type
// bucket.
CmpResult cmpTags(Operations op, sbe::value::TypeTags lhsTag, sbe::value::TypeTags rhsTag) {
    auto lhsBsonType = tagToType(lhsTag);
    auto rhsBsonType = tagToType(rhsTag);
    auto result = canonicalizeBSONType(lhsBsonType) - canonicalizeBSONType(rhsBsonType);
    // If the lhs and rhs have the same type, return incomparable since we have no information about
    // their values.
    if (result == 0) {
        return CmpResult::kIncomparable;
    }
    // By this point, there is no difference betwen Lt/Lte and Gt/Gte since we know the types
    // are different.
    switch (op) {
        case Operations::Lt:
        case Operations::Lte:
            return (result < 0) ? CmpResult::kTrue : CmpResult::kFalse;
        case Operations::Gt:
        case Operations::Gte:
            return (result > 0) ? CmpResult::kTrue : CmpResult::kFalse;
        case Operations::Cmp3w:
            return (result > 0) ? CmpResult::kGt : CmpResult::kLt;
        default:
            MONGO_UNREACHABLE;
    }
}

}  // namespace

CmpResult cmpEqFast(const ABT& lhs, const ABT& rhs) {
    if (lhs == rhs) {
        // If the subtrees are equal, we can conclude that their result is equal because we
        // have only pure functions.
        return CmpResult::kTrue;
    } else if (lhs.is<Constant>() && rhs.is<Constant>()) {
        // We have two constants which are not equal.
        return CmpResult::kFalse;
    } else if ((isParameter(lhs) && isNaN(rhs)) || (isParameter(rhs) && isNaN(lhs))) {
        // We are comparing FunctionCall[getParam] with a NaN Constant - they will never be equal.
        return CmpResult::kFalse;
    }
    return CmpResult::kIncomparable;
}

CmpResult cmp3wFast(Operations op, const ABT& lhs, const ABT& rhs) {
    const auto lhsConst = lhs.cast<Constant>();
    const auto rhsConst = rhs.cast<Constant>();
    const bool isLhsParam = isParameter(lhs);
    const bool isRhsParam = isParameter(rhs);

    if (lhsConst) {
        const auto [lhsTag, lhsVal] = lhsConst->get();

        if (rhsConst) {
            const auto [rhsTag, rhsVal] = rhsConst->get();

            const auto [compareTag, compareVal] =
                sbe::value::compareValue(lhsTag, lhsVal, rhsTag, rhsVal);
            uassert(7086701,
                    "Invalid comparison result",
                    compareTag == sbe::value::TypeTags::NumberInt32);
            const auto cmpVal = sbe::value::bitcastTo<int32_t>(compareVal);

            switch (op) {
                case Operations::Lt:
                    return (cmpVal < 0) ? CmpResult::kTrue : CmpResult::kFalse;
                case Operations::Lte:
                    return (cmpVal <= 0) ? CmpResult::kTrue : CmpResult::kFalse;
                case Operations::Gt:
                    return (cmpVal > 0) ? CmpResult::kTrue : CmpResult::kFalse;
                case Operations::Gte:
                    return (cmpVal >= 0) ? CmpResult::kTrue : CmpResult::kFalse;
                case Operations::Cmp3w:
                    return cmpVal > 0 ? CmpResult::kGt
                                      : (cmpVal < 0 ? CmpResult::kLt : CmpResult::kEq);
                default:
                    MONGO_UNREACHABLE;
            }
        } else if (isRhsParam) {
            auto rhsType = parameterType(rhs);
            if (isNaN(lhs) && isNumber(rhsType)) {
                return cmpNumericParamToNaN(op, lhs);
            }
            return cmpTags(op, lhsTag, rhsType);
        } else {
            if (lhsTag == sbe::value::TypeTags::MinKey) {
                switch (op) {
                    case Operations::Lte:
                        return CmpResult::kTrue;
                    case Operations::Gt:
                        return CmpResult::kFalse;
                    default:
                        break;
                }
            } else if (lhsTag == sbe::value::TypeTags::MaxKey) {
                switch (op) {
                    case Operations::Lt:
                        return CmpResult::kFalse;
                    case Operations::Gte:
                        return CmpResult::kTrue;
                    default:
                        break;
                }
            }
        }
    } else if (rhsConst) {
        const auto [rhsTag, rhsVal] = rhsConst->get();

        if (isLhsParam) {
            auto lhsType = parameterType(lhs);
            if (isNumber(lhsType) && isNaN(rhs)) {
                return cmpNumericParamToNaN(op, lhs);
            }
            return cmpTags(op, parameterType(lhs), rhsTag);
        }

        if (rhsTag == sbe::value::TypeTags::MinKey) {
            switch (op) {
                case Operations::Lt:
                    return CmpResult::kFalse;
                case Operations::Gte:
                    return CmpResult::kTrue;
                default:
                    break;
            }
        } else if (rhsTag == sbe::value::TypeTags::MaxKey) {
            switch (op) {
                case Operations::Lte:
                    return CmpResult::kTrue;
                case Operations::Gt:
                    return CmpResult::kFalse;
                default:
                    break;
            }
        }
    }

    if (isLhsParam && isRhsParam) {
        return cmpTags(op, parameterType(lhs), parameterType(rhs));
    }

    return CmpResult::kIncomparable;
}
}  // namespace mongo::optimizer
