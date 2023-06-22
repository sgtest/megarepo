/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include <cstddef>
#include <utility>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/exec/sbe/values/value.h"

namespace mongo {
namespace sbe {
namespace bson {
template <bool View>
std::pair<value::TypeTags, value::Value> convertFrom(const char* be,
                                                     const char* end,
                                                     size_t fieldNameSize);

template <bool View>
std::pair<value::TypeTags, value::Value> convertFrom(const BSONElement& elem) {
    return convertFrom<View>(
        elem.rawdata(), elem.rawdata() + elem.size(), elem.fieldNameSize() - 1);
}

const char* advance(const char* be, size_t fieldNameSize);

inline auto fieldNameAndLength(const char* be) noexcept {
    return StringData{be + 1};
}

inline const char* fieldNameRaw(const char* be) noexcept {
    return be + 1;
}

template <class ArrayBuilder>
void convertToBsonObj(ArrayBuilder& builder, value::Array* arr);

template <class ObjBuilder>
void convertToBsonObj(ObjBuilder& builder, value::Object* obj);

template <class ObjBuilder>
void appendValueToBsonObj(ObjBuilder& builder,
                          StringData name,
                          value::TypeTags tag,
                          value::Value val);

template <class ArrayBuilder>
void convertToBsonObj(ArrayBuilder& builder, value::ArrayEnumerator arr);

}  // namespace bson
}  // namespace sbe
}  // namespace mongo
