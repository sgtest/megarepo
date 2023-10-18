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

#include <bitset>
#include <initializer_list>
#include <iosfwd>
#include <string>
#include <vector>

#include "mongo/base/string_data.h"

namespace mongo::boolean_simplification {

/**
 * This file defines Maxterm and Minterm classes and operations over them. Maxterm/Minterms are used
 * to represent a boolean expression in a canonical form. For example, for Disjunctive Normal Form,
 * a Maxterm is used to represent the top disjunctive term and minterms are used to represent the
 * children conjunctive terms.
 */

constexpr size_t kBitsetNumberOfBits = 64;
using Bitset = std::bitset<kBitsetNumberOfBits>;

inline Bitset operator""_b(const char* bits, size_t len) {
    return Bitset{std::string{bits, len}};
}

/**
 * Represent a conjunctive or disjunctive term in a condensed bitset form.
 */
struct BitsetTerm {
    explicit BitsetTerm() : predicates(0ul), mask(0ul) {}

    BitsetTerm(Bitset bitset, Bitset mask) : predicates(bitset), mask(mask) {}

    BitsetTerm(size_t bitIndex, bool val) : predicates(0ul), mask(0ul) {
        set(bitIndex, val);
    }

    void set(size_t bitIndex, bool value) {
        mask.set(bitIndex, true);
        predicates.set(bitIndex, value);
    }

    size_t size() const {
        return mask.size();
    }

    /**
     * Predicates bitset, if a predicate takes part in the conjunction its corresponding bit in the
     * predicates bitset set to 1 if the predicate in true form or to 0 otherwise.
     */
    Bitset predicates;

    /**
     * Predicates mask, if a predicate takes part in the conjunction its corresponding bit set to 1.
     */
    Bitset mask;
};

struct Minterm;

/**
 * Maxterm represents top disjunction of an expression in Disjunctive Normal Form and consists of a
 * list of children conjunctions. Each child conjunction is represented as a Minterm.
 */
struct Maxterm {
    Maxterm() = default;
    Maxterm(std::initializer_list<Minterm> init);

    Maxterm& operator|=(const Minterm& rhs);
    Maxterm& operator|=(const Maxterm& rhs);
    Maxterm& operator&=(const Maxterm& rhs);
    Maxterm operator~() const;

    bool isAlwaysTrue() const;

    bool isAlwaysFalse() const;

    /**
     * Removes redundant minterms from the maxterm. A minterm might be redundant if it can be
     * absorbed by another term. For example, 'a' absorbs 'a & b'. See Absorption law for details.
     */
    void removeRedundancies();

    /**
     * Appends a new minterm with the bit at 'bitIndex' set to 'val' and all other bits unset.
     */
    void append(size_t bitIndex, bool val);

    /**
     * Appends empty minterm.
     */
    void appendEmpty();

    std::string toString() const;

    std::vector<Minterm> minterms;

private:
    friend Maxterm operator&(const Maxterm& lhs, const Maxterm& rhs);
};

/**
 * Identify and extract common predicates from the given booleean expression in DNF. Returns the
 * pair of the extracted predicates and the expression without predicates. If there is no common
 * predicates the first element of the pair will be empty minterm.
 */
std::pair<Minterm, Maxterm> extractCommonPredicates(Maxterm maxterm);

/**
 * Minterms represent a conjunction of an expression in Disjunctive Normal Form and consists of
 * predicates which can be in true (for a predicate A, true form is just A) of false forms (for
 * a predicate A the false form is the negation of A: ~A). Every predicate is represented by a
 * bit in the predicates bitset.
 */
struct Minterm : private BitsetTerm {
    using BitsetTerm::BitsetTerm;
    using BitsetTerm::mask;
    using BitsetTerm::predicates;
    using BitsetTerm::set;
    using BitsetTerm::size;

    Minterm(StringData bits, StringData mask)
        : Minterm{Bitset{bits.toString()}, Bitset{mask.toString()}} {}

    /**
     * Returns the set of bits in which the conflicting bits of the minterms are set. The bits
     * of two minterms are conflicting if in one minterm the bit is set to 1 and in another to
     * 0.
     */
    inline Bitset getConflicts(const Minterm& other) const {
        return (predicates ^ other.predicates) & (mask & other.mask);
    }

    Maxterm operator~() const;

    /**
     * Returns true if the current minterm can absorb the other minterm. For example, 'a' absorbs 'a
     * & b'. See Absorption law for details.
     */
    bool canAbsorb(const Minterm& other) const {
        return mask == (mask & other.mask) && predicates == (mask & other.predicates);
    }

    bool isAlwaysTrue() const {
        return mask.none();
    }

    /**
     * Flip the value of every predicate in the minterm.
     */
    void flip();
};

inline Maxterm operator&(const Minterm& lhs, const Minterm& rhs) {
    if (lhs.getConflicts(rhs).any()) {
        return Maxterm{};
    }
    return {{Minterm(lhs.predicates | rhs.predicates, lhs.mask | rhs.mask)}};
}

inline Maxterm operator&(const Maxterm& lhs, const Maxterm& rhs) {
    Maxterm result{};
    result.minterms.reserve(lhs.minterms.size() * rhs.minterms.size());
    for (const auto& left : lhs.minterms) {
        for (const auto& right : rhs.minterms) {
            result |= left & right;
        }
    }
    return result;
}

bool operator==(const BitsetTerm& lhs, const BitsetTerm& rhs);
std::ostream& operator<<(std::ostream& os, const BitsetTerm& term);
bool operator==(const Minterm& lhs, const Minterm& rhs);
std::ostream& operator<<(std::ostream& os, const Minterm& minterm);
bool operator==(const Maxterm& lhs, const Maxterm& rhs);
std::ostream& operator<<(std::ostream& os, const Maxterm& maxterm);

template <typename H>
H AbslHashValue(H h, const Minterm& mt) {
    return H::combine(std::move(h), mt.predicates, mt.mask);
}
}  // namespace mongo::boolean_simplification
