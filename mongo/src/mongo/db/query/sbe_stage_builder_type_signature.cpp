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

#include "mongo/db/query/sbe_stage_builder_type_signature.h"

namespace mongo::stage_builder {

TypeSignature getTypeSignature(sbe::value::TypeTags type) {
    uint8_t tagIndex = static_cast<uint8_t>(type);
    return TypeSignature{1LL << tagIndex};
}

// This constant signature holds all the types that have a BSON counterpart and can
// represent a value stored in the database, excluding all the TypeTags that describe
// internal types like SortSpec, TimeZoneDB, etc...
TypeSignature TypeSignature::kAnyBSONType = getTypeSignature(sbe::value::TypeTags::Nothing,
                                                             sbe::value::TypeTags::NumberInt32,
                                                             sbe::value::TypeTags::NumberInt64,
                                                             sbe::value::TypeTags::NumberDouble,
                                                             sbe::value::TypeTags::NumberDecimal,
                                                             sbe::value::TypeTags::Date,
                                                             sbe::value::TypeTags::Timestamp,
                                                             sbe::value::TypeTags::Boolean,
                                                             sbe::value::TypeTags::Null,
                                                             sbe::value::TypeTags::StringSmall,
                                                             sbe::value::TypeTags::StringBig,
                                                             sbe::value::TypeTags::Array,
                                                             sbe::value::TypeTags::ArraySet,
                                                             sbe::value::TypeTags::ArrayMultiSet,
                                                             sbe::value::TypeTags::Object,
                                                             sbe::value::TypeTags::ObjectId,
                                                             sbe::value::TypeTags::MinKey,
                                                             sbe::value::TypeTags::MaxKey,
                                                             sbe::value::TypeTags::bsonObject,
                                                             sbe::value::TypeTags::bsonArray,
                                                             sbe::value::TypeTags::bsonString,
                                                             sbe::value::TypeTags::bsonSymbol,
                                                             sbe::value::TypeTags::bsonObjectId,
                                                             sbe::value::TypeTags::bsonBinData,
                                                             sbe::value::TypeTags::bsonUndefined,
                                                             sbe::value::TypeTags::bsonRegex,
                                                             sbe::value::TypeTags::bsonJavascript,
                                                             sbe::value::TypeTags::bsonDBPointer,
                                                             sbe::value::TypeTags::bsonCodeWScope);
TypeSignature TypeSignature::kAnyScalarType = TypeSignature{~0}.exclude(
    getTypeSignature(sbe::value::TypeTags::cellBlock, sbe::value::TypeTags::valueBlock));
TypeSignature TypeSignature::kArrayType = getTypeSignature(sbe::value::TypeTags::Array,
                                                           sbe::value::TypeTags::ArraySet,
                                                           sbe::value::TypeTags::ArrayMultiSet,
                                                           sbe::value::TypeTags::bsonArray);
TypeSignature TypeSignature::kBlockType = getTypeSignature(sbe::value::TypeTags::valueBlock);
TypeSignature TypeSignature::kBooleanType = getTypeSignature(sbe::value::TypeTags::Boolean);
TypeSignature TypeSignature::kCellType = getTypeSignature(sbe::value::TypeTags::cellBlock);
TypeSignature TypeSignature::kDateTimeType =
    getTypeSignature(sbe::value::TypeTags::Date, sbe::value::TypeTags::Timestamp);
TypeSignature TypeSignature::kNothingType = getTypeSignature(sbe::value::TypeTags::Nothing);
TypeSignature TypeSignature::kNumericType = getTypeSignature(sbe::value::TypeTags::NumberInt32,
                                                             sbe::value::TypeTags::NumberInt64,
                                                             sbe::value::TypeTags::NumberDecimal,
                                                             sbe::value::TypeTags::NumberDouble);
TypeSignature TypeSignature::kObjectType =
    getTypeSignature(sbe::value::TypeTags::Object, sbe::value::TypeTags::bsonObject);
TypeSignature TypeSignature::kStringType = getTypeSignature(sbe::value::TypeTags::StringSmall,
                                                            sbe::value::TypeTags::StringBig,
                                                            sbe::value::TypeTags::bsonString);

// Return the set of SBE types encoded in the provided signature.
std::vector<sbe::value::TypeTags> getBSONTypesFromSignature(TypeSignature signature) {
    signature = signature.intersect(TypeSignature::kAnyBSONType);
    std::vector<sbe::value::TypeTags> tags;
    for (size_t i = 0; i < sizeof(size_t) * 8; i++) {
        auto tag = static_cast<sbe::value::TypeTags>(i);
        if (getTypeSignature(tag).isSubset(signature)) {
            tags.push_back(tag);
        }
    }
    return tags;
}

}  // namespace mongo::stage_builder
