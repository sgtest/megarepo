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

#include "mongo/db/matcher/expression.h"

namespace mongo {
/**
 * MatchExpression's hash function designed to be consistent with `MatchExpression::equivalent()`.
 * The function does not support $jsonSchema and will tassert() if provided an input that contains
 * any $jsonSchema-related nodes. 'maxNumberOfInElementsToHash' is the maximum number of equalities
 * or regexes to hash to avoid performance issues related to hashing of large '$in's.
 */
size_t calculateHash(const MatchExpression& expr, size_t maxNumberOfInElementsToHash);

/**
 * MatchExpression's hash functor implementation compatible with unordered containers. Designed to
 * be consistent with 'MatchExpression::equivalent()'. The functor does not support $jsonSchema and
 * will tassert() if provided an input that contains any $jsonSchema-related nodes.
 */
struct MatchExpressionHasher {
    /**
     * 'maxNumberOfInElementsToHash' is the maximum number of equalities or regexes to hash to avoid
     * performance issues related to hashing of large '$in's.
     */
    explicit MatchExpressionHasher(size_t maxNumberOfInElementsToHash = 20)
        : _maxNumberOfInElementsToHash(maxNumberOfInElementsToHash) {}

    size_t operator()(const MatchExpression* expr) const {
        return calculateHash(*expr, _maxNumberOfInElementsToHash);
    }

private:
    const size_t _maxNumberOfInElementsToHash;
};

/**
 * MatchExpression's equality functor implementation compatible with unordered containers. It uses
 * 'MatchExpression::equivalent()' under the hood and compatible with 'MatchExpressionHasher'
 * defined above.
 */
struct MatchExpressionEq {
    bool operator()(const MatchExpression* lhs, const MatchExpression* rhs) const {
        return lhs->equivalent(rhs);
    }
};

}  // namespace mongo
