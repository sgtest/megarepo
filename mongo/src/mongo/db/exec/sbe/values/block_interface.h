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

#include <boost/optional/optional.hpp>
#include <cstddef>
#include <memory>

#include "mongo/db/exec/sbe/values/cell_interface.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/util/assert_util.h"

namespace mongo::sbe::value {
/**
 * Deblocked tags and values for a ValueBlock.
 *
 * Note: Deblocked values are read-only and must not be modified.
 */
struct DeblockedTagVals {
    // 'tags' and 'vals' point to an array of 'count' elements respectively.
    DeblockedTagVals(size_t count, const TypeTags* tags, const Value* vals)
        : count(count), tags(tags), vals(vals) {
        tassert(7888701, "Values must exist", count > 0 && tags != nullptr && vals != nullptr);
    }

    size_t count;
    const TypeTags* tags;
    const Value* vals;
};

/**
 * Interface for accessing a sequence of SBE Values independent of their backing storage.
 *
 * Currently we only support getting all of the deblocked values via 'extract()' but PM-3168 will
 * extend the interface to allow for other operations to be applied which may run directly on the
 * underlying format or take advantage of precomputed summaries.
 */
struct ValueBlock {
    virtual ~ValueBlock() = default;

    /**
     * Returns the unowned deblocked values. The return value is only valid as long as the block
     * remains alive. The returned values must be dense, meaning that there are always same
     * number of values as the count() of this block. The 'DeblockedTagVals.count' must always be
     * equal to this block's count().
     */
    virtual DeblockedTagVals extract() = 0;

    /**
     * Returns a copy of this block.
     */
    virtual std::unique_ptr<ValueBlock> clone() const = 0;

    /**
     * Returns the number of values in this block in O(1) time, otherwise returns boost::none.
     */
    virtual boost::optional<size_t> tryCount() const = 0;
};

/**
 * A block that is a run of repeated values.
 */
class MonoBlock final : public ValueBlock {
public:
    MonoBlock(size_t count, TypeTags tag, Value val) : _count(count) {
        tassert(7962102, "The number of values must be > 0", count > 0);
        std::tie(_tag, _val) = value::copyValue(tag, val);
    }
    MonoBlock(const MonoBlock& o) : _count(o._count) {
        std::tie(_tag, _val) = value::copyValue(o._tag, o._val);
    }
    MonoBlock(MonoBlock&& o) : _tag(o._tag), _val(o._val), _count(o._count) {
        o._tag = TypeTags::Nothing;
        o._val = 0;
    }
    MonoBlock& operator=(const MonoBlock&) = delete;
    MonoBlock& operator=(MonoBlock&&) = delete;
    ~MonoBlock() {
        releaseValue(_tag, _val);
    }

    std::unique_ptr<ValueBlock> clone() const override {
        return std::make_unique<MonoBlock>(*this);
    }

    DeblockedTagVals extract() override {
        if (_deblockedTags.size() != _count) {
            _deblockedTags.resize(_count, _tag);
            _deblockedVals.resize(_count, _val);
        }

        return {_count, _deblockedTags.data(), _deblockedVals.data()};
    }

    boost::optional<size_t> tryCount() const override {
        return _count;
    }

private:
    // Always owned.
    TypeTags _tag;
    Value _val;

    // To lazily extract the values, we need to remember the number of values which is supposed to
    // exist in this block.
    size_t _count;

    // These are always a view onto '_tag' and '_val', materialized lazily if the caller requests
    // it.
    std::vector<TypeTags> _deblockedTags;
    std::vector<Value> _deblockedVals;
};
}  // namespace mongo::sbe::value
