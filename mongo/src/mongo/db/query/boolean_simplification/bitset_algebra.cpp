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

#include <absl/container/node_hash_set.h>
#include <boost/dynamic_bitset/dynamic_bitset.hpp>
#include <boost/preprocessor/control/iif.hpp>
// IWYU pragma: no_include "ext/alloc_traits.h"
#include <algorithm>
#include <ostream>
#include <utility>

#include "mongo/stdx/unordered_set.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/stream_utils.h"

namespace mongo::boolean_simplification {
Maxterm::Maxterm(size_t size) : _numberOfBits(size) {}

Maxterm::Maxterm(std::initializer_list<Minterm> init)
    : minterms(std::move(init)), _numberOfBits(0) {
    tassert(7507918, "Maxterm cannot be initilized with empty list of minterms", !minterms.empty());
    for (auto& minterm : minterms) {
        _numberOfBits = std::max(minterm.size(), _numberOfBits);
    }

    for (auto& minterm : minterms) {
        if (_numberOfBits > minterm.size()) {
            minterm.resize(_numberOfBits);
        }
    }
}

std::string Maxterm::toString() const {
    std::ostringstream oss{};
    oss << *this;
    return oss.str();
}

Maxterm& Maxterm::operator|=(const Minterm& rhs) {
    minterms.emplace_back(rhs);
    return *this;
}

Maxterm Maxterm::operator~() const {
    if (minterms.empty()) {
        return {Minterm{numberOfBits()}};
    }

    Maxterm result = ~minterms.front();
    for (size_t i = 1; i < minterms.size(); ++i) {
        result &= ~minterms[i];
    }

    return result;
}

void Maxterm::removeRedundancies() {
    stdx::unordered_set<Minterm> seen{};
    std::vector<Minterm> newMinterms{};
    for (const auto& minterm : minterms) {
        const bool isAlwaysTrue = minterm.mask.none();
        if (isAlwaysTrue) {
            newMinterms.clear();
            newMinterms.emplace_back(minterm);
            break;
        }
        auto [it, isInserted] = seen.insert(minterm);
        if (isInserted) {
            newMinterms.push_back(minterm);
        }
    }

    minterms.swap(newMinterms);
}

void Maxterm::append(size_t bitIndex, bool val) {
    minterms.emplace_back(_numberOfBits, bitIndex, val);
}

void Maxterm::appendEmpty() {
    minterms.emplace_back(_numberOfBits);
}

Maxterm Minterm::operator~() const {
    Maxterm result{size()};
    for (size_t i = 0; i < mask.size(); ++i) {
        if (mask[i]) {
            result |= Minterm(mask.size(), i, !predicates[i]);
        }
    }
    return result;
}

bool operator==(const Minterm& lhs, const Minterm& rhs) {
    return lhs.predicates == rhs.predicates && lhs.mask == rhs.mask;
}

std::ostream& operator<<(std::ostream& os, const Minterm& minterm) {
    os << '(' << minterm.predicates << ", " << minterm.mask << ")";
    return os;
}

Maxterm& Maxterm::operator|=(const Maxterm& rhs) {
    for (auto& right : rhs.minterms) {
        *this |= right;
    }
    return *this;
}

Maxterm& Maxterm::operator&=(const Maxterm& rhs) {
    Maxterm result = *this & rhs;
    minterms.swap(result.minterms);
    return *this;
}

bool operator==(const Maxterm& lhs, const Maxterm& rhs) {
    return lhs.minterms == rhs.minterms;
}

std::ostream& operator<<(std::ostream& os, const Maxterm& maxterm) {
    using mongo::operator<<;
    return os << maxterm.minterms;
}
}  // namespace mongo::boolean_simplification
