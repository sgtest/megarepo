/**
 *    Copyright (C) 2024-present MongoDB, Inc.
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

#include <vector>

#include <boost/optional/optional.hpp>

#include "mongo/db/exec/sbe/sbe_unittest.h"
#include "mongo/db/exec/sbe/values/block_interface.h"
#include "mongo/db/exec/sbe/values/value.h"

namespace mongo::sbe {
// Helper that's used to make tests easier to write (and read). Not all tests have been changed
// to use this, but see the block hashagg unit test for an example.
struct CopyableValueBlock {
    CopyableValueBlock() = default;
    CopyableValueBlock(std::unique_ptr<value::ValueBlock> vb) : _block(std::move(vb)) {
        invariant(_block);
    }
    CopyableValueBlock(const CopyableValueBlock& o) : _block(o._block->clone()) {}

    value::ValueBlock* operator->() {
        return _block.get();
    }

    value::ValueBlock& operator*() {
        return *_block;
    }

    const value::ValueBlock* operator->() const {
        return _block.get();
    }

    const value::ValueBlock& operator*() const {
        return *_block;
    }
    CopyableValueBlock& operator=(const CopyableValueBlock& o) {
        _block = o._block->clone();
        return *this;
    }
    CopyableValueBlock& operator=(CopyableValueBlock&& o) {
        // Doesn't actually move, as we don't care about optimizing copies in tests.
        _block = o._block->clone();
        return *this;
    }

    std::unique_ptr<value::ValueBlock> _block;
};

static std::vector<std::pair<value::TypeTags, value::Value>> makeInt32s(
    std::vector<int32_t> values) {
    std::vector<std::pair<value::TypeTags, value::Value>> ints;
    for (auto v : values) {
        ints.push_back(makeInt32(v));
    }
    return ints;
}

static CopyableValueBlock makeMonoBlock(std::pair<value::TypeTags, value::Value> tv, size_t ct) {
    return CopyableValueBlock(std::make_unique<value::MonoBlock>(ct, tv.first, tv.second));
}

static CopyableValueBlock makeInt32sBlock(const std::vector<int32_t>& vals) {
    auto block = std::make_unique<value::HeterogeneousBlock>();
    for (auto v : vals) {
        block->push_back(value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(v));
    }
    return CopyableValueBlock(std::move(block));
}

static std::unique_ptr<value::HeterogeneousBlock> makeHeterogeneousBlock(
    std::vector<std::pair<value::TypeTags, value::Value>> vals) {
    auto block = std::make_unique<value::HeterogeneousBlock>();
    for (auto [t, v] : vals) {
        block->push_back(t, v);
    }
    return block;
}

static std::unique_ptr<value::ValueBlock> makeHeterogeneousBoolBlock(std::vector<bool> bools) {
    auto block = std::make_unique<value::HeterogeneousBlock>();
    for (auto b : bools) {
        block->push_back(value::TypeTags::Boolean, value::bitcastFrom<bool>(b));
    }
    return block;
}

static std::unique_ptr<value::ValueBlock> makeBoolBlock(std::vector<bool> bools) {
    return std::make_unique<value::BoolBlock>(bools);
}

static void release2dValueVector(const std::vector<TypedValues>& vals) {
    for (size_t i = 0; i < vals.size(); ++i) {
        for (size_t j = 0; j < vals[j].size(); ++j) {
            releaseValue(vals[i][j].first, vals[i][j].second);
        }
    }
}

template <typename T>
static std::vector<T> makeNumbers(int magnitude = 1, bool multipleNaNs = true) {
    std::vector<T> nums;
    if (std::is_same_v<T, bool>) {
        nums.push_back(false);
        nums.push_back(true);
        return nums;
    }
    if (std::is_same_v<T, double>) {
        nums.push_back(std::numeric_limits<double>::quiet_NaN());
        if (multipleNaNs) {
            nums.push_back(std::numeric_limits<double>::signaling_NaN());
        }
        nums.push_back(std::numeric_limits<double>::infinity() * -1);
        nums.push_back(std::numeric_limits<double>::infinity());
    }
    nums.push_back(-1 * magnitude);
    nums.push_back(0);
    nums.push_back(1 * magnitude);
    nums.push_back(std::numeric_limits<T>::min());
    nums.push_back(std::numeric_limits<T>::max());
    return nums;
}

template <typename BlockType, typename T>
static std::unique_ptr<BlockType> makeTestHomogeneousBlock(bool inclNothing = true,
                                                           bool multipleNaNs = true) {
    std::unique_ptr<BlockType> homogeneousTestBlock = std::make_unique<BlockType>();
    auto nums = makeNumbers<T>(1, multipleNaNs);
    for (auto num : nums) {
        homogeneousTestBlock->push_back(value::bitcastFrom<T>(num));
    }
    if (inclNothing) {
        homogeneousTestBlock->pushNothing();
    }
    return homogeneousTestBlock;
}

static std::unique_ptr<value::ValueBlock> makeTestNothingBlock(size_t valsNum) {
    std::unique_ptr<value::Int32Block> testHomogeneousBlock = std::make_unique<value::Int32Block>();
    for (size_t i = 0; i < valsNum; ++i) {
        testHomogeneousBlock->pushNothing();
    }
    return testHomogeneousBlock;
}

static TypedValues makeInterestingValues() {
    TypedValues vals;
    vals.push_back(makeNull());

    vals.push_back(makeBsonArray(BSON_ARRAY(2 << 3 << 4 << 4)));
    vals.push_back(makeArray(BSON_ARRAY(3 << 3 << 4 << 5)));
    vals.push_back(makeArraySet(BSON_ARRAY(4 << 5 << 6)));
    vals.push_back(makeBsonObject(BSON("b" << 7)));
    vals.push_back(makeObject(BSON("b" << 8)));

    auto int32s = makeNumbers<int32_t>(10 /* magnitude */);
    for (auto int32 : int32s) {
        vals.push_back(makeInt32(int32));
    }
    auto int64s = makeNumbers<int64_t>(100 /* magnitude */);
    for (auto int64 : int64s) {
        vals.push_back(makeInt64(int64));
    }
    auto dates = makeNumbers<int64_t>(500 /* magnitude */);
    for (auto dt : dates) {
        vals.push_back(std::pair{value::TypeTags::Date, value::bitcastFrom<int64_t>(dt)});
    }
    auto doubles = makeNumbers<double>(1000 /* magnitude */);
    for (auto dbl : doubles) {
        vals.push_back(makeDouble(dbl));
    }

    vals.push_back(makeBool(false));
    vals.push_back(makeBool(true));

    vals.push_back(value::makeNewString("regular string"_sd));  // StringBig
    vals.push_back(value::makeNewString("tinystr"_sd));         // StringSmall

    vals.push_back(makeDecimal("-1234.5678"));
    vals.push_back(makeDecimal("1234.5678"));
    vals.push_back(makeDecimal("somethingE200"));    // NaN
    vals.push_back(makeDecimal("200E9999999999"));   // +Inf
    vals.push_back(makeDecimal("-200E9999999999"));  // -Inf

    vals.push_back(makeTimestamp(Timestamp(992391600, 0)));
    vals.push_back(makeTimestamp(Timestamp(992391600, 1234)));
    vals.push_back(makeTimestamp(Timestamp::min()));
    vals.push_back(makeTimestamp(Timestamp::max()));

    return vals;
}
}  // namespace mongo::sbe
