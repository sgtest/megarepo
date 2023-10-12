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

#include "mongo/db/exec/sbe/values/block_interface.h"

namespace mongo::sbe::value {

template <class Stream>
Stream& streamInsertionImpl(Stream& s, const DeblockedTagVals& deblocked) {
    for (size_t i = 0; i < deblocked.count; ++i) {
        s << std::pair(deblocked.tags[i], deblocked.vals[i]) << " ";
    }
    return s;
}

std::ostream& operator<<(std::ostream& stream, const DeblockedTagVals& vals) {
    return streamInsertionImpl(stream, vals);
}

str::stream& operator<<(str::stream& stream, const DeblockedTagVals& vals) {
    return streamInsertionImpl(stream, vals);
}

void DeblockedTagValStorage::copyValuesFrom(const DeblockedTagValStorage& other) {
    if (other.owned) {
        owned = true;
        tags.resize(other.tags.size(), value::TypeTags::Nothing);
        vals.resize(other.vals.size(), 0);

        for (size_t i = 0; i < other.tags.size(); ++i) {
            auto [cpyTag, cpyVal] = copyValue(other.tags[i], other.vals[i]);
            tags[i] = cpyTag;
            vals[i] = cpyVal;
        }
    } else {
        owned = false;
        tags = other.tags;
        vals = other.vals;
    }
}

void DeblockedTagValStorage::release() {
    if (owned) {
        owned = false;
        for (size_t i = 0; i < tags.size(); ++i) {
            releaseValue(tags[i], vals[i]);
        }
    }
}

std::unique_ptr<ValueBlock> ValueBlock::map(const ColumnOp& op) {
    return defaultMapImpl(op);
}

std::unique_ptr<ValueBlock> ValueBlock::defaultMapImpl(const ColumnOp& op) {
    auto extracted = extract();

    if (extracted.count == 0) {
        return std::make_unique<HeterogeneousBlock>();
    }

    std::vector<TypeTags> tags(extracted.count, TypeTags::Nothing);
    std::vector<Value> vals(extracted.count, Value{0u});

    op.processBatch(extracted.tags, extracted.vals, tags.data(), vals.data(), extracted.count);

    return std::make_unique<HeterogeneousBlock>(std::move(tags), std::move(vals));
}


TokenizedBlock ValueBlock::tokenize() {
    auto extracted = extract();
    std::vector<TypeTags> tokenTags;
    std::vector<Value> tokenVals;
    std::vector<size_t> idxs(extracted.count, 0);

    size_t unique_cnt = 0;
    auto tokenMap = ValueMapType<size_t>{0, value::ValueHash(), value::ValueEq()};
    for (size_t i = 0; i < extracted.count; ++i) {
        auto [it, inserted] = tokenMap.insert({extracted.tags[i], extracted.vals[i]}, unique_cnt);
        if (inserted) {
            unique_cnt++;
            auto [cpyTag, cpyVal] = value::copyValue(extracted.tags[i], extracted.vals[i]);
            tokenTags.push_back(cpyTag), tokenVals.push_back(cpyVal);
        }
        idxs[i] = it->second;
    }
    return {std::make_unique<HeterogeneousBlock>(std::move(tokenTags), std::move(tokenVals)), idxs};
}

TokenizedBlock MonoBlock::tokenize() {
    auto [tag, val] = value::copyValue(_tag, _val);
    std::vector<TypeTags> tokenTags{tag};
    std::vector<Value> tokenVals{val};

    return {std::make_unique<HeterogeneousBlock>(std::move(tokenTags), std::move(tokenVals)),
            std::vector<size_t>(_count, 0)};
}

std::unique_ptr<ValueBlock> HeterogeneousBlock::map(const ColumnOp& op) {
    auto outBlock = std::make_unique<HeterogeneousBlock>();

    size_t numElems = _vals.size();

    if (numElems > 0) {
        const TypeTags* inTags = _tags.data();
        const Value* inVals = _vals.data();

        outBlock->_tags.resize(numElems, TypeTags::Nothing);
        outBlock->_vals.resize(numElems, Value{0u});

        TypeTags* outTags = outBlock->_tags.data();
        Value* outVals = outBlock->_vals.data();

        op.processBatch(inTags, inVals, outTags, outVals, numElems);
    }

    return outBlock;
}

void HeterogeneousBlock::push_back(TypeTags t, Value v) {
    constexpr auto maxSizeT = std::numeric_limits<size_t>::max();

    ValueGuard guard(t, v);

    size_t cap = std::min<size_t>(_vals.capacity(), _tags.capacity());
    if (_vals.size() == cap) {
        size_t newCap = cap < maxSizeT / 2 ? (cap ? cap * 2 : 1) : maxSizeT;
        newCap = std::max<size_t>(newCap, cap + 1);

        _vals.reserve(newCap);
        _tags.reserve(newCap);
    }

    _vals.push_back(v);
    _tags.push_back(t);

    guard.reset();
}

}  // namespace mongo::sbe::value
