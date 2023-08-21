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

#include "mongo/db/query/optimizer/syntax/expr.h"

#include <absl/container/flat_hash_map.h>

#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/storage/key_string.h"
#include "mongo/platform/decimal128.h"

namespace mongo::optimizer {

using namespace sbe::value;

Constant::Constant(TypeTags tag, sbe::value::Value val) : _tag(tag), _val(val) {}

Constant::Constant(const Constant& other) {
    auto [tag, val] = copyValue(other._tag, other._val);
    _tag = tag;
    _val = val;
}

Constant::Constant(Constant&& other) noexcept {
    _tag = other._tag;
    _val = other._val;

    other._tag = TypeTags::Nothing;
    other._val = 0;
}

ABT Constant::createFromCopy(const sbe::value::TypeTags tag, const sbe::value::Value val) {
    auto copy = sbe::value::copyValue(tag, val);
    return make<Constant>(copy.first, copy.second);
}

ABT Constant::str(StringData str) {
    // Views are non-owning so we have to make a copy.
    auto [tag, val] = makeNewString(str);
    return make<Constant>(tag, val);
}

ABT Constant::int32(int32_t valueInt32) {
    return make<Constant>(TypeTags::NumberInt32, bitcastFrom<int32_t>(valueInt32));
}

ABT Constant::int64(int64_t valueInt64) {
    return make<Constant>(TypeTags::NumberInt64, bitcastFrom<int64_t>(valueInt64));
}

ABT Constant::fromDouble(double value) {
    return make<Constant>(TypeTags::NumberDouble, bitcastFrom<double>(value));
}

ABT Constant::fromDecimal(const Decimal128& value) {
    auto [tag, val] = makeCopyDecimal(value);
    return make<Constant>(tag, val);
}

ABT Constant::timestamp(const Timestamp& t) {
    return make<Constant>(TypeTags::Timestamp, bitcastFrom<uint64_t>(t.asULL()));
}

ABT Constant::date(const Date_t& d) {
    return make<Constant>(TypeTags::Date, bitcastFrom<int64_t>(d.toMillisSinceEpoch()));
}

ABT Constant::emptyObject() {
    auto [tag, val] = makeNewObject();
    return make<Constant>(tag, val);
}

ABT Constant::emptyArray() {
    return array();
}

ABT Constant::nothing() {
    return make<Constant>(TypeTags::Nothing, 0);
}

ABT Constant::null() {
    return make<Constant>(TypeTags::Null, 0);
}

ABT Constant::boolean(bool b) {
    return make<Constant>(TypeTags::Boolean, bitcastFrom<bool>(b));
}

ABT Constant::minKey() {
    return make<Constant>(TypeTags::MinKey, 0);
}

ABT Constant::maxKey() {
    return make<Constant>(TypeTags::MaxKey, 0);
}

Constant::~Constant() {
    releaseValue(_tag, _val);
}

bool Constant::operator==(const Constant& other) const {
    // Handle the cases when only one of the compared values is Nothing; in this scenario,
    // compareValue returns Nothing instead of the answer we want.
    if (_tag == sbe::value::TypeTags::Nothing || other._tag == sbe::value::TypeTags::Nothing) {
        return _tag == other._tag;
    }
    const auto [compareTag, compareVal] = compareValue(_tag, _val, other._tag, other._val);
    uassert(7086702, "Invalid comparison result", compareTag == sbe::value::TypeTags::NumberInt32);
    return sbe::value::bitcastTo<int32_t>(compareVal) == 0;
}

bool Constant::isString() const {
    return sbe::value::isString(_tag);
}

StringData Constant::getString() const {
    return getStringView(_tag, _val);
}

bool Constant::isValueInt64() const {
    return _tag == TypeTags::NumberInt64;
}

int64_t Constant::getValueInt64() const {
    uassert(6624057, "Constant value type is not int64_t", isValueInt64());
    return bitcastTo<int64_t>(_val);
}

bool Constant::isValueInt32() const {
    return _tag == TypeTags::NumberInt32;
}

int32_t Constant::getValueInt32() const {
    uassert(6624354, "Constant value type is not int32_t", isValueInt32());
    return bitcastTo<int32_t>(_val);
}

bool Constant::isValueDouble() const {
    return _tag == TypeTags::NumberDouble;
}

double Constant::getValueDouble() const {
    uassert(673180, "Constant value type is not double", isValueDouble());
    return bitcastTo<double>(_val);
}

bool Constant::isValueDecimal() const {
    return _tag == TypeTags::NumberDecimal;
}

Decimal128 Constant::getValueDecimal() const {
    uassert(673181, "Constant value type is not Decimal128", isValueDecimal());
    return bitcastTo<Decimal128>(_val);
}

bool Constant::isValueBool() const {
    return _tag == TypeTags::Boolean;
}

bool Constant::getValueBool() const {
    uassert(6624356, "Constant value type is not bool", isValueBool());
    return bitcastTo<bool>(_val);
}

}  // namespace mongo::optimizer
