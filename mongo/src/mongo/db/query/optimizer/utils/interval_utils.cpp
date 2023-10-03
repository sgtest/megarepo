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

#include "mongo/db/query/optimizer/utils/interval_utils.h"

#include <absl/container/node_hash_map.h>
#include <algorithm>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <iterator>
#include <memory>
#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/db/query/optimizer/algebra/operator.h"
#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/optimizer/bool_expression.h"
#include "mongo/db/query/optimizer/comparison_op.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"
#include "mongo/db/query/optimizer/utils/abt_compare.h"
#include "mongo/util/assert_util.h"


namespace mongo::optimizer {

ABT minABT(const ABT& v1, const ABT& v2) {
    return make<If>(make<BinaryOp>(Operations::Lte, v1, v2), v1, v2);
};

ABT maxABT(const ABT& v1, const ABT& v2) {
    return make<If>(make<BinaryOp>(Operations::Gte, v1, v2), v1, v2);
};

void constFoldInterval(IntervalRequirement& interval, const ConstFoldFn& constFold) {
    constFold(interval.getLowBound().getBound());
    constFold(interval.getHighBound().getBound());
}

// Returns true if the interval can be proven to be empty. If no conclusion can be made, or the
// interval is provably not empty, returns false.

bool isIntervalEmpty(const IntervalRequirement& interval) {
    if (interval.getLowBound() == BoundRequirement{false, Constant::maxKey()} ||
        interval.getHighBound() == BoundRequirement{false, Constant::minKey()}) {
        return true;
    }

    CmpResult boundsCmp = cmp3wFast(
        Operations::Cmp3w, interval.getLowBound().getBound(), interval.getHighBound().getBound());
    // Can't make any conclusions about the comparison between the bounds. We don't know for sure
    // that it's empty.
    if (boundsCmp == CmpResult::kIncomparable) {
        return false;
    }
    const bool hasExclusiveBound =
        !interval.getLowBound().isInclusive() || !interval.getHighBound().isInclusive();
    // If lower bound greater than upper bound, or the bounds are equal but the interval is
    // not completely inclusive, we have an empty interval.
    return boundsCmp == CmpResult::kGt || (boundsCmp == CmpResult::kEq && hasExclusiveBound);
}

std::vector<IntervalRequirement> unionTwoIntervals(const IntervalRequirement& int1,
                                                   const IntervalRequirement& int2,
                                                   const ConstFoldFn& constFold) {
    /*
     * If we have two intervals, we can convert [a, b] U [c, d] to:
     * overlap_indicator = a < d && c < b && int1NonEmpty && int2NonEmpty
     * [overlap_indicator ? min(a, c) : a, overlap_indicator ? max(b, d) : b]
     * U [c, overlap_indicator ? -inf : d]
     * If the intervals overlap, they become [min(a,c), max(b,d)] U [c, -inf]
     * If they do not overlap, then we have the original intervals [a, b] U [c, d]
     */
    const auto& int1Low = int1.getLowBound();
    const auto& int1High = int1.getHighBound();

    const auto& int2Low = int2.getLowBound();
    const auto& int2High = int2.getHighBound();

    const auto& a = int1Low.getBound();
    const bool aInc = int1Low.isInclusive();
    const auto& b = int1High.getBound();
    const bool bInc = int1High.isInclusive();
    const auto& c = int2Low.getBound();
    const bool cInc = int2Low.isInclusive();
    const auto& d = int2High.getBound();
    const bool dInc = int2High.isInclusive();

    /*
     * We'll be adding auxiliary intervals to deal with open/closed bounds. If we have an
     * interval [a, b), and add an auxiliary interval to account for the inclusivity of `a`,
     * we only want this aux interval to simplify to [a, a] if a < b. If a >= b, then we
     * have something like [2,1) = empty set, or [1, 1) which is also the empty set. If the
     * original interval is the empty set, we want the aux interval to be empty as well.
     * Therefore we use have the indicators below to tell us if the intervals are non-empty.
     * If both bounds are inclusive, we allow equality (since [1, 1] is non-empty).
     * Otherwise we only use less-than (since (1, 1] is empty).
     */
    const ABT int1NonEmpty = make<BinaryOp>(aInc && bInc ? Operations::Lte : Operations::Lt, a, b);
    const ABT int2NonEmpty = make<BinaryOp>(cInc && dInc ? Operations::Lte : Operations::Lt, c, d);

    /*
     * Whether or not these intervals overlap is dependent on the inclusivity of
     * the bounds. For example, [2, 3] U [3, 4] does overlap, while [2, 3) U (3, 4] does
     * not.
     * The intervals overlap if a < d && c < b, with the actual comparison instead being LTE if
     * either bound is inclusive.
     * For another example, we consider [2, 3) U [3, 4] to "overlap" because they can be
     * combined into one contiguous interval, even though they have no points in common.
     */
    const Operations cmpAD = aInc || dInc ? Operations::Lte : Operations::Lt;
    const Operations cmpBC = bInc || cInc ? Operations::Lte : Operations::Lt;
    const ABT overlapCondition =
        make<BinaryOp>(Operations::And, make<BinaryOp>(cmpAD, a, d), make<BinaryOp>(cmpBC, c, b));
    ABT overlapAndNonEmptyCond =
        make<BinaryOp>(Operations::And,
                       overlapCondition,
                       make<BinaryOp>(Operations::And, int1NonEmpty, int2NonEmpty));

    // Add the primary intervals.
    std::vector<IntervalRequirement> result;
    IntervalRequirement primaryInt1 =
        IntervalRequirement{{aInc && cInc, make<If>(overlapAndNonEmptyCond, minABT(a, c), a)},
                            {bInc && dInc, make<If>(overlapAndNonEmptyCond, maxABT(b, d), b)}};
    IntervalRequirement primaryInt2 = IntervalRequirement{
        {cInc, c}, {dInc, make<If>(std::move(overlapAndNonEmptyCond), Constant::minKey(), d)}};
    constFoldInterval(primaryInt1, constFold);
    constFoldInterval(primaryInt2, constFold);
    if (!isIntervalEmpty(primaryInt1)) {
        result.push_back(std::move(primaryInt1));
    }
    if (!isIntervalEmpty(primaryInt2)) {
        result.push_back(std::move(primaryInt2));
    }

    // Take a constant non-empty auxiliary interval, and look for its corresponding primary
    // interval to merge with.
    const auto mergeAuxWithPrimary = [&result](const ABT& bound) {
        for (auto& interval : result) {
            BoundRequirement& low = interval.getLowBound();
            if (bound == low.getBound()) {
                low = {true, bound};
                return;
            }
            BoundRequirement& high = interval.getHighBound();
            if (bound == high.getBound()) {
                high = {true, bound};
                return;
            }
        }
        // We need this case, for when we have something like [1,1] U [2,3). Our primary intervals
        // would be [1,1) U [2,3). These would be unioned with an aux interval. However the primary
        // interval corresponding with the aux one would be removed, since [1,1) is empty. Then when
        // this function is called, we won't find the primary interval it belongs with, so instead
        // of merging we add it to the primary intervals.
        result.emplace_back(BoundRequirement{true, bound}, BoundRequirement{true, bound});
    };

    // Analyze an aux interval for const-ness or emptiness, and add it to our result.
    const auto addAuxInterval = [&](IntervalRequirement auxInterval) {
        constFoldInterval(auxInterval, constFold);
        if (!isIntervalEmpty(auxInterval)) {
            if (auxInterval.isConstant()) {
                invariant(auxInterval.isEquality());
                // Find the primary interval and merge with it.
                mergeAuxWithPrimary(auxInterval.getLowBound().getBound());
            } else {
                // It's still a variable interval after const folding, so we can't merge it with
                // a primary. Instead just add it to the result.
                result.push_back(std::move(auxInterval));
            }
        }
        // If it's empty ignore it.
    };

    // If `a` and `c` agree on inclusivity, then the primary interval will have the same
    // inclusivity. If they disagree, we make the primary interval exclusive, and add an aux
    // interval.
    if (aInc != cInc) {
        // Add aux interval.
        if (aInc) {
            // The aux interval should be [a,a] if
            // int1NonEmpty && (!overlap_indicator || a <= c)
            // We only want the aux interval to be non-empty if the interval it originated from is
            // non-empty and the bound it corresponds to ("a" in this case) wins, OR the intervals
            // end up not overlapping. Example for the non-overlapping case might be [1,2] U (3,4]
            // -> (1,2] U (3,4] U aux so we need 1 to still be inclusive, by making the aux a
            // non-empty point [1,1].
            ABT auxCondition =
                make<BinaryOp>(Operations::And,
                               int1NonEmpty,
                               make<BinaryOp>(Operations::Or,
                                              make<UnaryOp>(Operations::Not, overlapCondition),
                                              make<BinaryOp>(Operations::Lte, a, c)));
            addAuxInterval(IntervalRequirement{
                {true, a}, {true, make<If>(std::move(auxCondition), a, Constant::minKey())}});
        } else {
            // The aux interval should be [c,c] if
            // int2NonEmpty && (!overlap_indicator || c <= a)
            ABT auxCondition =
                make<BinaryOp>(Operations::And,
                               int2NonEmpty,
                               make<BinaryOp>(Operations::Or,
                                              make<UnaryOp>(Operations::Not, overlapCondition),
                                              make<BinaryOp>(Operations::Lte, c, a)));
            addAuxInterval(IntervalRequirement{
                {true, c}, {true, make<If>(std::move(auxCondition), c, Constant::minKey())}});
        }
    }
    if (bInc != dInc) {
        // Add aux interval.
        if (bInc) {
            // The interval should be [b,b] if
            // int1NonEmpty && (!overlap_indicator || b >= d)
            ABT auxCondition =
                make<BinaryOp>(Operations::And,
                               int1NonEmpty,
                               make<BinaryOp>(Operations::Or,
                                              make<UnaryOp>(Operations::Not, overlapCondition),
                                              make<BinaryOp>(Operations::Gte, b, d)));
            addAuxInterval(IntervalRequirement{
                {true, b}, {true, make<If>(std::move(auxCondition), b, Constant::minKey())}});
        } else {
            // The aux interval should be [d,d] if
            // int2NonEmpty && (!overlap_indicator || d >= b)
            ABT auxCondition =
                make<BinaryOp>(Operations::And,
                               int2NonEmpty,
                               make<BinaryOp>(Operations::Or,
                                              make<UnaryOp>(Operations::Not, overlapCondition),
                                              make<BinaryOp>(Operations::Gte, d, b)));
            addAuxInterval(IntervalRequirement{
                {true, d}, {true, make<If>(std::move(auxCondition), d, Constant::minKey())}});
        }
    }

    return result;
}

boost::optional<IntervalReqExpr::Node> unionDNFIntervals(const IntervalReqExpr::Node& intervalDNF,
                                                         const ConstFoldFn& constFold) {
    BoolExprBuilder<IntervalRequirement> builder;
    builder.pushDisj();

    // Since our input intervals are sorted, constDisjIntervals will be sorted as well.
    std::vector<IntervalRequirement> constDisjIntervals;
    const auto& disjNodes = intervalDNF.cast<IntervalReqExpr::Disjunction>()->nodes();
    for (const auto& disjunct : disjNodes) {
        const auto& conjNodes = disjunct.cast<IntervalReqExpr::Conjunction>()->nodes();
        tassert(
            7117500,
            "Conjunctions were not simplified before unioning. Found conjunction not of size 1.",
            conjNodes.size() == 1);
        const auto& interval = conjNodes.front().cast<IntervalReqExpr::Atom>()->getExpr();
        if (interval.isConstant()) {
            // We've found a constant disjunct.
            constDisjIntervals.push_back(interval);
        } else {
            // The bound is not constant, so we won't simplify.
            builder.pushConj().atom(interval).pop();
        }
    }

    // Remove empty intervals.
    for (auto it = constDisjIntervals.begin(); it != constDisjIntervals.end();) {
        if (isIntervalEmpty(*it)) {
            it = constDisjIntervals.erase(it);
        } else {
            it++;
        }
    }

    // For as long as it can, it1 will eat the interval in front of it. When it can't eat anymore,
    // advance it1.
    for (auto it1 = constDisjIntervals.begin(); it1 != constDisjIntervals.end(); it1++) {
        while (std::next(it1) != constDisjIntervals.end()) {
            auto it2 = std::next(it1);
            std::vector<IntervalRequirement> result = unionTwoIntervals(*it1, *it2, constFold);
            if (result.size() != 1) {
                // They were not merged.
                break;
            }
            // They were merged, delete the it2 interval.
            constDisjIntervals.erase(it2);
            *it1 = std::move(result.front());
        }
    }

    // This check happens after simplification in case the simplifying reveals a fully open interval
    // to us. For example, (-inf, 5) U (3, inf) would become (-inf, inf).
    for (const auto& interval : constDisjIntervals) {
        if (interval.isFullyOpen()) {
            return IntervalReqExpr::makeSingularDNF(interval);
        }
    }

    // Add our simplified constant disjuncts to the final result.
    for (auto& interval : constDisjIntervals) {
        builder.pushConj().atom(std::move(interval)).pop();
    }

    // If we have an empty result, .finish() will return boost::none.
    return builder.finish();
}

void combineIntervalsDNF(const bool intersect,
                         IntervalReqExpr::Node& target,
                         const IntervalReqExpr::Node& source) {
    if (target == source) {
        // Intervals are the same. Leave target unchanged.
        return;
    }

    if (isIntervalReqFullyOpenDNF(target)) {
        // Intersecting with fully open interval is redundant.
        // Unioning with fully open interval results in a fully-open interval.
        if (intersect) {
            target = source;
        }
        return;
    }

    if (isIntervalReqFullyOpenDNF(source)) {
        // Intersecting with fully open interval is redundant.
        // Unioning with fully open interval results in a fully-open interval.
        if (!intersect) {
            target = source;
        }
        return;
    }

    BoolExprBuilder<IntervalRequirement> builder;
    builder.pushDisj();

    const auto pushConjNodesFn = [&builder](const IntervalReqExpr::Node& conj) {
        for (const auto& interval : conj.cast<IntervalReqExpr::Conjunction>()->nodes()) {
            builder.atom(interval.cast<IntervalReqExpr::Atom>()->getExpr());
        }
    };

    // Integrate both compound bounds.
    if (intersect) {
        // Intersection is analogous to polynomial multiplication. Using '.' to denote intersection
        // and '+' to denote union. (a.b + c.d) . (e+f) = a.b.e + c.d.e + a.b.f + c.d.f
        // TODO: in certain cases we can simplify further. For example if we only have scalars, we
        // can simplify (-inf, 10) ^ (5, +inf) to (5, 10), but this does not work with arrays.

        for (const auto& sourceConjunction : source.cast<IntervalReqExpr::Disjunction>()->nodes()) {
            for (const auto& targetConjunction :
                 target.cast<IntervalReqExpr::Disjunction>()->nodes()) {
                // TODO: handle case with targetConjunct  fully open
                // TODO: handle case with targetConjunct half-open and sourceConjunct equality.
                // TODO: handle case with both targetConjunct and sourceConjunct equalities
                // (different consts).

                builder.pushConj();
                pushConjNodesFn(sourceConjunction);
                pushConjNodesFn(targetConjunction);
                builder.pop();
            }
        }
    } else {
        // Unioning is analogous to polynomial addition.
        // (a.b + c.d) + (e+f) = a.b + c.d + e + f
        for (const auto& conj : target.cast<IntervalReqExpr::Disjunction>()->nodes()) {
            builder.pushConj();
            pushConjNodesFn(conj);
            builder.pop();
        }
        for (const auto& conj : source.cast<IntervalReqExpr::Disjunction>()->nodes()) {
            builder.pushConj();
            pushConjNodesFn(conj);
            builder.pop();
        }
    }
    target = std::move(*builder.finish());
}

static std::vector<IntervalRequirement> intersectIntervals(const IntervalRequirement& i1,
                                                           const IntervalRequirement& i2,
                                                           const ConstFoldFn& constFold) {
    // Handle trivial cases of intersection.
    if (i1.isFullyOpen()) {
        return {i2};
    }
    if (i2.isFullyOpen()) {
        return {i1};
    }

    const ABT& low1 = i1.getLowBound().getBound();
    const ABT& high1 = i1.getHighBound().getBound();
    const ABT& low2 = i2.getLowBound().getBound();
    const ABT& high2 = i2.getHighBound().getBound();

    const auto foldFn = [&constFold](ABT expr) {
        constFold(expr);
        return expr;
    };

    // In the simplest case our bound is (max(low1, low2), min(high1, high2)) if none of the bounds
    // are inclusive.
    ABT maxLow = foldFn(maxABT(low1, low2));
    ABT minHigh = foldFn(minABT(high1, high2));
    if (cmp3wFast(Operations::Gt, maxLow, minHigh) == CmpResult::kTrue) {
        //   Low bound is greater than high bound.
        return {};
    }

    const bool low1Inc = i1.getLowBound().isInclusive();
    const bool high1Inc = i1.getHighBound().isInclusive();
    const bool low2Inc = i2.getLowBound().isInclusive();
    const bool high2Inc = i2.getHighBound().isInclusive();

    // We form a "main" result interval which is closed on any side with "agreement" between the two
    // intervals. For example [low1, high1] ^ [low2, high2) -> [max(low1, low2), min(high1, high2))
    BoundRequirement lowBoundPrimary(low1Inc && low2Inc, maxLow);
    BoundRequirement highBoundPrimary(high1Inc && high2Inc, minHigh);

    auto boundsEqual = cmpEqFast(maxLow, minHigh);
    if (boundsEqual == CmpResult::kTrue) {
        if (low1Inc && high1Inc && low2Inc && high2Inc) {
            // Point interval.
            return {{std::move(lowBoundPrimary), std::move(highBoundPrimary)}};
        }
        if ((!low1Inc && !low2Inc) || (!high1Inc && !high2Inc)) {
            // Fully open on both sides.
            return {};
        }
    }
    if (low1Inc == low2Inc && high1Inc == high2Inc) {
        // Inclusion matches on both sides.
        return {{std::move(lowBoundPrimary), std::move(highBoundPrimary)}};
    }

    // At this point we have intervals without inclusion agreement, for example
    // [low1, high1) ^ (low2, high2]. We have the primary interval which in this case is the open
    // (max(low1, low2), min(high1, high2)). Then we add an extra closed interval for each side with
    // disagreement. For example for the lower sides we add: [indicator ? low1 : MaxKey, low1]. This
    // is a closed interval which would reduce to [low1, low1] if low1 > low2 and the intervals
    // intersect and are non-empty. If low2 >= low1 the interval reduces to an empty one,
    // [MaxKey, low1], which will return no results from an index scan. We do not know that in
    // general if we do not have constants (we cannot fold).
    //
    // If we can fold the aux interval, we combine the aux interval into the primary one, which
    // would yield [low1, min(high1, high2)) if we can prove that low1 > low2. Then we create a
    // similar auxiliary interval for the right side if there is disagreement on the inclusion.
    // We'll attempt to fold both intervals. Should we conclude definitively that they are
    // point intervals, we update the inclusion of the main interval for the respective side.

    std::vector<IntervalRequirement> result;
    const auto addAuxInterval = [&](ABT low, ABT high, BoundRequirement& bound) {
        IntervalRequirement interval{{true, low}, {true, high}};

        CmpResult comparison = cmp3wFast(Operations::Lte, low, high);
        if (comparison == CmpResult::kTrue) {
            if (interval.isEquality()) {
                // We can determine the two bounds are equal.
                bound = {true /*inclusive*/, bound.getBound()};
            } else {
                result.push_back(std::move(interval));
            }
        } else if (comparison == CmpResult::kIncomparable) {
            // We cannot determine statically how the two bounds compare.
            result.push_back(std::move(interval));
        }
    };

    /*
     * An auxiliary interval should resolve to a non-empty interval if the original intervals we're
     * intersecting overlap and produce something non-empty. Below we create an overlap indicator,
     * which tells us if the intervals overlap.
     *
     * For intersection, the pair [1,2) and [2, 3] does not overlap, while [1,2] and [2, 3] does. So
     * we need to adjust our comparisons depending on if the bounds are both inclusive or not.
     */
    const Operations cmpLows = low1Inc && low2Inc ? Operations::Lte : Operations::Lt;
    const Operations cmpLow1High2 = low1Inc && high2Inc ? Operations::Lte : Operations::Lt;
    const Operations cmpLow2High1 = low2Inc && high1Inc ? Operations::Lte : Operations::Lt;
    const Operations cmpHighs = high1Inc && high2Inc ? Operations::Lte : Operations::Lt;
    /*
     * Our final overlap indicator is as follows (using < or <= depending on inclusiveness)
     * (low1,high1) ^ (low2,high2) overlap if:
     * low2 < low1 < high2 || low2 < high1 < high2 || low1 < low2 < high1 || low1 < high2 < high1
     * As long as both intervals are non-empty.
     *
     * This covers the four cases:
     *      1. int1 intersects int2 from below, ex: (1,3) ^ (2,4)
     *      2. int1 intersects int2 from above, ex: (2,4) ^ (1,3)
     *      3. int1 is a subset of int2, ex: (2,3) ^ (1,4)
     *      4. int2 is a subset of int1, ex: (1,4) ^ (2,3)
     */
    ABT int1NonEmpty =
        make<BinaryOp>(low1Inc && high1Inc ? Operations::Lte : Operations::Lt, low1, high1);
    ABT int2NonEmpty =
        make<BinaryOp>(low2Inc && high2Inc ? Operations::Lte : Operations::Lt, low2, high2);
    ABT overlapCondition =
        make<BinaryOp>(Operations::Or,
                       make<BinaryOp>(Operations::Or,
                                      make<BinaryOp>(Operations::And,
                                                     make<BinaryOp>(cmpLows, low2, low1),
                                                     make<BinaryOp>(cmpLow1High2, low1, high2)),
                                      make<BinaryOp>(Operations::And,
                                                     make<BinaryOp>(cmpLow2High1, low2, high1),
                                                     make<BinaryOp>(cmpHighs, high1, high2))),
                       make<BinaryOp>(Operations::Or,
                                      make<BinaryOp>(Operations::And,
                                                     make<BinaryOp>(cmpLows, low1, low2),
                                                     make<BinaryOp>(cmpLow2High1, low2, high1)),
                                      make<BinaryOp>(Operations::And,
                                                     make<BinaryOp>(cmpLow1High2, low1, high2),
                                                     make<BinaryOp>(cmpHighs, high2, high1))));
    overlapCondition = make<BinaryOp>(
        Operations::And,
        std::move(overlapCondition),
        make<BinaryOp>(Operations::And, std::move(int1NonEmpty), std::move(int2NonEmpty)));

    /*
     * It's possible our aux indicators could be simplified. For example, a more concise indicator
     * for [low1, high1] ^ (low2, high2] might be int1_nonempty && (int2 contains low1). This
     * condition implies the intervals are non-empty and overlap, meaning the intersection is
     * non-empty. It also implies that low1 > low2, meaning the inclusive bound wins.
     */
    if (low1Inc != low2Inc) {
        ABT incBound = low1Inc ? low1 : low2;
        ABT nonIncBound = low1Inc ? low2 : low1;

        // Our aux interval should be non-empty if overlap_indicator && (incBound > nonIncBound)
        ABT auxCondition =
            make<BinaryOp>(Operations::And,
                           overlapCondition,
                           make<BinaryOp>(Operations::Gt, incBound, std::move(nonIncBound)));
        ABT low = foldFn(make<If>(std::move(auxCondition), incBound, Constant::maxKey()));
        ABT high = std::move(incBound);
        addAuxInterval(std::move(low), std::move(high), lowBoundPrimary);
    }

    if (high1Inc != high2Inc) {
        ABT incBound = high1Inc ? high1 : high2;
        ABT nonIncBound = high1Inc ? high2 : high1;

        ABT low = incBound;
        // Our aux interval should be non-empty if overlap_indicator && (incBound < nonIncBound)
        ABT auxCondition =
            make<BinaryOp>(Operations::And,
                           overlapCondition,
                           make<BinaryOp>(Operations::Lt, incBound, std::move(nonIncBound)));
        ABT high =
            foldFn(make<If>(std::move(auxCondition), std::move(incBound), Constant::minKey()));
        addAuxInterval(std::move(low), std::move(high), highBoundPrimary);
    }

    if (boundsEqual == CmpResult::kIncomparable || boundsEqual == CmpResult::kFalse ||
        (lowBoundPrimary.isInclusive() && highBoundPrimary.isInclusive())) {
        //  We add the main interval to the result as long as it is a valid point interval, or
        //  the bounds are not equal.
        result.emplace_back(std::move(lowBoundPrimary), std::move(highBoundPrimary));
    }
    return result;
}

boost::optional<IntervalReqExpr::Node> intersectDNFIntervals(
    const IntervalReqExpr::Node& intervalDNF, const ConstFoldFn& constFold) {

    struct IntervalComparator {
        bool operator()(const IntervalReqExpr::Node& i1, const IntervalReqExpr::Node& i2) const {
            return compareIntervalExpr(i1, i2) < 0;
        }
    };
    struct IntervalSimplifier {
        using DefaultSimplifier = DefaultSimplifyAndCreateNode<IntervalRequirement>;

        DefaultSimplifier::Result operator()(const BuilderNodeType type,
                                             std::vector<IntervalReqExpr::Node> v,
                                             const bool hasTrue,
                                             const bool hasFalse) const {
            // Deduplicate via sort + unique.
            std::sort(v.begin(), v.end(), IntervalComparator{});
            auto end = std::unique(v.begin(), v.end());
            v.erase(end, v.end());

            return DefaultSimplifier{}(type, std::move(v), hasTrue, hasFalse);
        }
    };

    BoolExprBuilder<IntervalRequirement, IntervalSimplifier> builder;
    builder.pushDisj();

    for (const auto& disjunct : intervalDNF.cast<IntervalReqExpr::Disjunction>()->nodes()) {
        const auto& conjuncts = disjunct.cast<IntervalReqExpr::Conjunction>()->nodes();
        uassert(6624149, "Empty disjunct in interval DNF.", !conjuncts.empty());

        std::vector<IntervalRequirement> intersectedIntervalDisjunction;
        bool isEmpty = false;
        bool isFirst = true;

        for (const auto& conjunct : conjuncts) {
            const auto& interval = conjunct.cast<IntervalReqExpr::Atom>()->getExpr();
            if (isFirst) {
                isFirst = false;
                intersectedIntervalDisjunction = {interval};
            } else {
                std::vector<IntervalRequirement> newResult;
                for (const auto& intersectedInterval : intersectedIntervalDisjunction) {
                    auto intersectionResult =
                        intersectIntervals(intersectedInterval, interval, constFold);
                    newResult.insert(newResult.end(),
                                     make_move_iterator(intersectionResult.begin()),
                                     make_move_iterator(intersectionResult.end()));
                }
                if (newResult.empty()) {
                    // The intersection is empty, there is no need to process the remaining
                    // conjuncts
                    isEmpty = true;
                    break;
                }
                std::swap(intersectedIntervalDisjunction, newResult);
            }
        }
        if (isEmpty) {
            continue;  // The whole conjunct is false (empty interval), skip it.
        }

        for (auto& interval : intersectedIntervalDisjunction) {
            // We will remove duplicate conjuncts here.
            builder.pushConj().atom(std::move(interval)).pop();
        }
    }

    // If we have an empty result, .finish() will return boost::none.
    return builder.finish();
}

boost::optional<IntervalReqExpr::Node> simplifyDNFIntervals(const IntervalReqExpr::Node& interval,
                                                            const ConstFoldFn& constFold) {
    if (const auto simplified = intersectDNFIntervals(interval, constFold)) {
        return unionDNFIntervals(*simplified, constFold);
    }
    // Empty interval.
    return boost::none;
}

bool combineCompoundIntervalsDNF(CompoundIntervalReqExpr::Node& targetIntervals,
                                 const IntervalReqExpr::Node& sourceIntervals,
                                 bool reverseSource) {
    BoolExprBuilder<CompoundIntervalRequirement> builder;
    builder.pushDisj();

    for (const auto& sourceConjunction :
         sourceIntervals.cast<IntervalReqExpr::Disjunction>()->nodes()) {
        for (const auto& targetConjunction :
             targetIntervals.cast<CompoundIntervalReqExpr::Disjunction>()->nodes()) {
            builder.pushConj();

            for (const auto& sourceConjunct :
                 sourceConjunction.cast<IntervalReqExpr::Conjunction>()->nodes()) {
                const auto& sourceInterval =
                    sourceConjunct.cast<IntervalReqExpr::Atom>()->getExpr();
                for (const auto& targetConjunct :
                     targetConjunction.cast<CompoundIntervalReqExpr::Conjunction>()->nodes()) {
                    const auto& targetInterval =
                        targetConjunct.cast<CompoundIntervalReqExpr::Atom>()->getExpr();
                    if (!targetInterval.isEquality() && !sourceInterval.isFullyOpen()) {
                        // We do not have an equality prefix. Reject.
                        return false;
                    }

                    auto newInterval = targetInterval;
                    if (reverseSource) {
                        auto newSource = sourceInterval;
                        newSource.reverse();
                        newInterval.push_back(std::move(newSource));
                    } else {
                        newInterval.push_back(sourceInterval);
                    }
                    builder.atom(std::move(newInterval));
                }
            }

            builder.pop();
        }
    }

    targetIntervals = std::move(*builder.finish());
    return true;
}

void padCompoundIntervalsDNF(CompoundIntervalReqExpr::Node& targetIntervals,
                             const bool reverseSource) {
    BoolExprBuilder<CompoundIntervalRequirement> builder;
    builder.pushDisj();

    for (const auto& targetConjunction :
         targetIntervals.cast<CompoundIntervalReqExpr::Disjunction>()->nodes()) {
        builder.pushConj();

        for (const auto& targetConjunct :
             targetConjunction.cast<CompoundIntervalReqExpr::Conjunction>()->nodes()) {
            auto targetInterval = targetConjunct.cast<CompoundIntervalReqExpr::Atom>()->getExpr();

            // For the low bound, if we are inclusive and not reversed, then we append MinKey key in
            // order to include all the values for the previously constrained fields. For example,
            // for a compound index on (a, b) if we constrain a >= 1 then we need to construct a
            // compound bound [{1, MinKey}, ...). Conversely on the upper side if we are inclusive
            // we need to append a MaxKey in order to include the previously constrained values. If
            // we are not inclusive, then we do not want to include any of the values for the
            // previously constrained fields, and thus append MaxKey for the lower side. On the same
            // compound index (a, b), we encode the condition a > 1 as {(1, MaxKey), ...}.
            // Conversely of the upper side we append MinKey. If we are reversing, the lower and
            // upper bounds effectively switch sides, and we append MinKey where we would have
            // appended MaxKey, and vice versa.
            const bool lowInclusive = targetInterval.getLowBound().isInclusive();
            BoundRequirement lowBound{lowInclusive,
                                      (lowInclusive == reverseSource) ? Constant::maxKey()
                                                                      : Constant::minKey()};
            const bool highInclusive = targetInterval.getHighBound().isInclusive();
            BoundRequirement highBound{highInclusive,
                                       (highInclusive == reverseSource) ? Constant::minKey()
                                                                        : Constant::maxKey()};

            targetInterval.push_back({std::move(lowBound), std::move(highBound)});
            builder.atom(std::move(targetInterval));
        }

        builder.pop();
    }

    targetIntervals = std::move(*builder.finish());
}

/**
 * Transport which updates an interval to be in a normal form. Children of each conjunction and
 * disjunction node are consistently ordered. We order the Atoms first by low bound, then by high
 * bound.
 */
class IntervalNormalizer {
public:
    void transport(const IntervalReqExpr::Atom& node) {
        // Noop.
    }

    void transport(IntervalReqExpr::Conjunction& node,
                   std::vector<IntervalReqExpr::Node>& children) {
        sortChildren(children);
    }

    void transport(IntervalReqExpr::Disjunction& node,
                   std::vector<IntervalReqExpr::Node>& children) {
        sortChildren(children);
    }

    void normalize(IntervalReqExpr::Node& intervals) {
        return algebra::transport<false>(intervals, *this);
    }

private:
    void sortChildren(std::vector<IntervalReqExpr::Node>& children) {
        struct Comparator {
            bool operator()(const IntervalReqExpr::Node& i1,
                            const IntervalReqExpr::Node& i2) const {
                return compareIntervalExpr(i1, i2) < 0;
            }
        };
        std::sort(children.begin(), children.end(), Comparator{});
    }
};

void normalizeIntervals(IntervalReqExpr::Node& intervals) {
    IntervalNormalizer{}.normalize(intervals);
}

bool isSimpleRange(const CompoundIntervalReqExpr::Node& interval) {
    if (const auto singularInterval = CompoundIntervalReqExpr::getSingularDNF(interval);
        singularInterval && !singularInterval->isEquality()) {
        return true;
    }
    return false;
}

class CheckMultikeyness {
public:
    bool walk(const PathIdentity& identity, const MultikeynessTrie& trie, bool seenParentTraverse) {
        return trie.isMultiKey;
    }

    bool walk(const PathTraverse& traverse,
              const MultikeynessTrie& trie,
              bool seenParentTraverse,
              const ABT& child,
              const ABT& refs) {
        // If there is a traverse node, we assume that the key is already multikey.

        // If the traverse node is single level, and its immediate parent isn't another traverse
        // node, then we can check the next node.
        if (traverse.getMaxDepth() == PathTraverse::kSingleLevel && !seenParentTraverse) {
            return algebra::walk<false>(child, *this, trie, true /*seenParentTraverse*/);
        }

        // The multikeynessTrie contains metadata only on consecutively nested fields.
        // e.g., {a: [[ {b: [5]} ]]} the multikeyness trie would *correctly* respond that path
        // "Get [a] Traverse [1] Get [b] Id" is non-multikey.
        // Traverse [1] flattens only one level of nesting, and field "b" is nested in two levels.
        //
        // Currently, if the traverse node is multi level (Traverse [N], N > 1), or we have multiple
        // subsequent traverse nodes, we assume that the path is multikey.

        // We have no information about multikeyness of the child path, i.e., it is multikey.
        return true;
    }

    bool walk(const PathGet& get,
              const MultikeynessTrie& trie,
              bool seenParentTraverse,
              const ABT& child) {
        // If there is no trie node for the PathGet field, then the field is multikey.
        auto it = trie.children.find(get.name());
        if (it == trie.children.end()) {
            return true;
        }

        return algebra::walk<false>(child, *this, it->second, false /*seenParentTraverse*/);
    }

    template <class N, class... Ts>
    bool walk(const N& node, Ts&&...) {
        return true;
    }

    static bool checkMultikeyness(const ABT& path, const MultikeynessTrie& multikeynessTrie) {
        CheckMultikeyness instance;
        return algebra::walk<false>(path, instance, multikeynessTrie, false);
    }
};

static bool requiresArray(const IntervalReqExpr::Node& intervals,
                          const PathToIntervalFn& pathToInterval) {

    // Check whether the requirement interval is a requirement for an array.

    // Create a single requirement interval for an array.
    IntervalReqExpr::Node arrayIntervalDNF = pathToInterval(make<PathArr>()).get();

    // Intersect the single requirement for an array with the input interval requirements.
    // This will update in-place 'arrayIntervalDNF', which depending on the intervals, will
    // result to either a subset of a single requirement for an array or an empty set.
    combineIntervalsDNF(true /*intersect*/, arrayIntervalDNF, intervals);

    // Comparing the result of the intersection with the array requirement will return
    // true only if the input requirements contain a subset of array requirement.
    return intervals == arrayIntervalDNF;
}

bool requiresArrayOnNonMultikeyPath(const ABT& path,
                                    const IntervalReqExpr::Node& intervals,
                                    const MultikeynessTrie& multikeynessTrie,
                                    const PathToIntervalFn& pathToInterval) {
    return requiresArray(intervals, pathToInterval) &&
        !CheckMultikeyness::checkMultikeyness(path, multikeynessTrie);
}

bool mayContainNull(const IntervalReqExpr::Atom& node, const ConstFoldFn& constFold) {
    const auto& interval = node.getExpr();

    const auto foldFn = [&constFold](ABT expr) {
        constFold(expr);
        return expr;
    };
    if (const auto& lowBound = interval.getLowBound();
        foldFn(make<BinaryOp>(lowBound.isInclusive() ? Operations::Gt : Operations::Gte,
                              lowBound.getBound(),
                              Constant::null())) == Constant::boolean(true)) {
        // Lower bound is strictly larger than null, or equal to null but not inclusive.
        return false;
    }
    if (const auto& highBound = interval.getHighBound();
        foldFn(make<BinaryOp>(highBound.isInclusive() ? Operations::Lt : Operations::Lte,
                              highBound.getBound(),
                              Constant::null())) == Constant::boolean(true)) {
        // Upper bound is strictly smaller than null, or equal to null but not inclusive.
        return false;
    }

    return true;
}

/**
 * Transport that replaces the intervals containing nulls in-place in order to preserve the DNF/CNF
 * structure. If 'excludeNull' is true, the interval is updated to (null, HighBound] (i.e.
 * intersecting {>Const [null]}). Otherwise, it is updated to [LowBound, null] (i.e. intersecting
 * {<=Const [null]}).
 */
class SplitNullTransport {
public:
    void transport(IntervalReqExpr::Node& n,
                   const IntervalReqExpr::Atom& atom,
                   const ConstFoldFn& constFold,
                   const bool excludeNull) {
        if (mayContainNull(atom, constFold)) {
            const IntervalRequirement& expr = atom.getExpr();
            if (excludeNull) {
                n.cast<IntervalReqExpr::Atom>()->getExpr() = IntervalRequirement{
                    {false /*inclusive*/, Constant::null()}, expr.getHighBound()};
            } else {
                n.cast<IntervalReqExpr::Atom>()->getExpr() =
                    IntervalRequirement{expr.getLowBound(), {true /*inclusive*/, Constant::null()}};
            }
        }
    }
    void transport(IntervalReqExpr::Node& n,
                   const IntervalReqExpr::Conjunction&,
                   const ConstFoldFn&,
                   const bool excludeNull,
                   std::vector<IntervalReqExpr::Node>&) {}
    void transport(IntervalReqExpr::Node& n,
                   const IntervalReqExpr::Disjunction&,
                   const ConstFoldFn&,
                   const bool excludeNull,
                   std::vector<IntervalReqExpr::Node>&) {}


    void split(IntervalReqExpr::Node& n, const ConstFoldFn& constFold, const bool excludeNull) {
        algebra::transport<true>(n, *this, constFold, excludeNull);
    }
};

boost::optional<std::pair<IntervalReqExpr::Node, IntervalReqExpr::Node>> splitNull(
    const IntervalReqExpr::Node& interval, const ConstFoldFn& constFold) {
    // Requires all the intervals to be constant and does not have a high bound with null
    // inclusively. Having a low bound with null is acceptable because it's beneficial to split
    // [null, HighBound] into [null, null] and (null, HighBound].
    if (IntervalReqExpr::any(interval, [](const IntervalRequirement& requirement) {
            return !requirement.isConstant() ||
                requirement.getHighBound() == BoundRequirement{true, Constant::null()};
        })) {
        return boost::none;
    }
    IntervalReqExpr::Node nullExcluded = interval;
    IntervalReqExpr::Node nullIncluded = interval;

    SplitNullTransport{}.split(nullExcluded, constFold, true /* excludeNull */);
    SplitNullTransport{}.split(nullIncluded, constFold, false /* excludeNull */);

    return std::make_pair(nullExcluded, nullIncluded);
}

}  // namespace mongo::optimizer
