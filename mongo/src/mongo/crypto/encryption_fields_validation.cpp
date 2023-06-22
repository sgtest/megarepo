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

#include "encryption_fields_validation.h"

#include <cmath>
#include <limits>
#include <utility>
#include <variant>
#include <vector>

#include <absl/container/node_hash_map.h>
#include <boost/container/small_vector.hpp>
#include <boost/cstdint.hpp>
// IWYU pragma: no_include "boost/intrusive/detail/iterator.hpp"
#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <fmt/format.h>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/crypto/encryption_fields_gen.h"
#include "mongo/crypto/encryption_fields_util.h"
#include "mongo/db/field_ref.h"
#include "mongo/db/namespace_string.h"
#include "mongo/stdx/unordered_set.h"
#include "mongo/stdx/variant.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"
#include "mongo/util/uuid.h"

namespace mongo {

Value coerceValueToRangeIndexTypes(Value val, BSONType fieldType) {
    BSONType valType = val.getType();

    if (valType == fieldType)
        return val;

    if (valType == Date || fieldType == Date) {
        uassert(6720002,
                "If the value type is a date, the type of the index must also be date (and vice "
                "versa). ",
                valType == fieldType);
        return val;
    }

    uassert(6742000,
            str::stream() << "type" << valType
                          << " type isn't supported for the range encrypted index. ",
            isNumericBSONType(valType));

    // If we get to this point, we've already established that valType and fieldType are NOT the
    // same type, so if either of them is a double or a decimal we can't coerce.
    if (valType == NumberDecimal || valType == NumberDouble || fieldType == NumberDecimal ||
        fieldType == NumberDouble) {
        uasserted(
            6742002,
            str::stream() << "If the value type and the field type are not the same type and one "
                             "or both of them is a double or a decimal, coercion of the value to "
                             "field type is not supported, due to possible loss of precision.");
    }

    switch (fieldType) {
        case NumberInt:
            return Value(val.coerceToInt());
        case NumberLong:
            return Value(val.coerceToLong());
        default:
            MONGO_UNREACHABLE;
    }
}


void validateRangeIndex(BSONType fieldType, QueryTypeConfig& query) {
    uassert(6775201,
            str::stream() << "Type '" << typeName(fieldType)
                          << "' is not a supported range indexed type",
            isFLE2RangeIndexedSupportedType(fieldType));

    uassert(6775202,
            "The field 'sparsity' is missing but required for range index",
            query.getSparsity().has_value());
    uassert(6775214,
            "The field 'sparsity' must be between 1 and 4",
            query.getSparsity().value() >= 1 && query.getSparsity().value() <= 4);


    switch (fieldType) {
        case NumberDouble:
        case NumberDecimal: {
            if (!((query.getMin().has_value() == query.getMax().has_value()) &&
                  (query.getMin().has_value() == query.getPrecision().has_value()))) {
                uasserted(6967100,
                          str::stream() << "Precision, min, and max must all be specified "
                                        << "together for floating point fields");
            }

            if (!query.getMin().has_value()) {
                if (fieldType == NumberDouble) {
                    query.setMin(mongo::Value(std::numeric_limits<double>::lowest()));
                    query.setMax(mongo::Value(std::numeric_limits<double>::max()));
                } else {
                    query.setMin(mongo::Value(Decimal128::kLargestNegative));
                    query.setMax(mongo::Value(Decimal128::kLargestPositive));
                }
            }

            if (query.getPrecision().has_value()) {
                uint32_t precision = query.getPrecision().value();
                if (fieldType == NumberDouble) {
                    uassert(
                        6966805,
                        "The number of decimal digits for minimum value must be less then or equal "
                        "to precision",
                        validateDoublePrecisionRange(query.getMin()->coerceToDouble(), precision));
                    uassert(
                        6966806,
                        "The number of decimal digits for maximum value must be less then or equal "
                        "to precision",
                        validateDoublePrecisionRange(query.getMax()->coerceToDouble(), precision));

                } else {
                    auto minDecimal = query.getMin()->coerceToDecimal();
                    uassert(6966807,
                            "The number of decimal digits for minimum value must be less then or "
                            "equal to precision",
                            validateDecimal128PrecisionRange(minDecimal, precision));
                    auto maxDecimal = query.getMax()->coerceToDecimal();
                    uassert(6966808,
                            "The number of decimal digits for maximum value must be less then or "
                            "equal to precision",
                            validateDecimal128PrecisionRange(maxDecimal, precision));
                }
            }
        }
            // We want to perform the same validation after sanitizing floating
            // point parameters, so we call FMT_FALLTHROUGH here.

            FMT_FALLTHROUGH;
        case NumberInt:
        case NumberLong:
        case Date: {
            uassert(6775203,
                    "The field 'min' is missing but required for range index",
                    query.getMin().has_value());
            uassert(6775204,
                    "The field 'max' is missing but required for range index",
                    query.getMax().has_value());

            auto indexMin = query.getMin().value();
            auto indexMax = query.getMax().value();

            uassert(7018200,
                    "Min should have the same type as the field.",
                    fieldType == indexMin.getType());
            uassert(7018201,
                    "Max should have the same type as the field.",
                    fieldType == indexMax.getType());

            uassert(6720005,
                    "Min must be less than max.",
                    Value::compare(indexMin, indexMax, nullptr) < 0);
        }

        break;
        default:
            uasserted(7018202, "Range index only supports numeric types and the Date type.");
    }
}

void validateEncryptedField(const EncryptedField* field) {
    if (field->getQueries().has_value()) {
        auto encryptedIndex =
            stdx::visit(OverloadedVisitor{
                            [](QueryTypeConfig config) { return config; },
                            [](std::vector<QueryTypeConfig> configs) {
                                // TODO SERVER-67421 - remove restriction that only one query type
                                // can be specified per field
                                uassert(6338404,
                                        "Exactly one query type should be specified per field",
                                        configs.size() == 1);
                                return configs[0];
                            },
                        },
                        field->getQueries().value());

        uassert(6412601,
                "Bson type needs to be specified for an indexed field",
                field->getBsonType().has_value());
        auto fieldType = typeFromName(field->getBsonType().value());

        switch (encryptedIndex.getQueryType()) {
            case QueryTypeEnum::Equality:
                uassert(6338405,
                        str::stream() << "Type '" << typeName(fieldType)
                                      << "' is not a supported equality indexed type",
                        isFLE2EqualityIndexedSupportedType(fieldType));
                uassert(6775205,
                        "The field 'sparsity' is not allowed for equality index but is present",
                        !encryptedIndex.getSparsity().has_value());
                uassert(6775206,
                        "The field 'min' is not allowed for equality index but is present",
                        !encryptedIndex.getMin().has_value());
                uassert(6775207,
                        "The field 'max' is not allowed for equality index but is present",
                        !encryptedIndex.getMax().has_value());
                break;
            case QueryTypeEnum::RangePreview: {
                validateRangeIndex(fieldType, encryptedIndex);
                break;
            }
        }
    } else {
        if (field->getBsonType().has_value()) {
            BSONType type = typeFromName(field->getBsonType().value());

            uassert(6338406,
                    str::stream() << "Type '" << typeName(type)
                                  << "' is not a supported unindexed type",
                    isFLE2UnindexedSupportedType(type));
        }
    }
}

void validateEncryptedFieldConfig(const EncryptedFieldConfig* config) {
    stdx::unordered_set<UUID, UUID::Hash> keys(config->getFields().size());
    std::vector<FieldRef> fieldPaths;
    fieldPaths.reserve(config->getFields().size());

    if (config->getEscCollection()) {
        uassert(
            7406900,
            "Encrypted State Collection name should follow enxcol_.<collection>.esc naming pattern",
            NamespaceString("", config->getEscCollection().get()).isFLE2StateCollection());
    }
    if (config->getEcocCollection()) {
        uassert(7406902,
                "Encrypted Compaction Collection name should follow enxcol_.<collection>.ecoc "
                "naming pattern",
                NamespaceString("", config->getEcocCollection().get()).isFLE2StateCollection());
    }
    for (const auto& field : config->getFields()) {
        UUID keyId = field.getKeyId();

        // Duplicate key ids are bad, it breaks the design
        uassert(6338401, "Duplicate key ids are not allowed", keys.count(keyId) == 0);
        keys.insert(keyId);

        uassert(6316402, "Encrypted field must have a non-empty path", !field.getPath().empty());
        FieldRef newPath(field.getPath());
        uassert(6316403, "Cannot encrypt _id or its subfields", newPath.getPart(0) != "_id");

        for (const auto& path : fieldPaths) {
            uassert(6338402, "Duplicate paths are not allowed", newPath != path);
            // Cannot have indexes on "a" and "a.b"
            uassert(6338403,
                    str::stream() << "Conflicting index paths found as one is a prefix of another '"
                                  << newPath.dottedField() << "' and '" << path.dottedField()
                                  << "'",
                    !path.fullyOverlapsWith(newPath));
        }

        fieldPaths.push_back(std::move(newPath));
    }
}

bool validateDoublePrecisionRange(double d, uint32_t precision) {
    double maybe_integer = d * pow(10.0, precision);
    double floor_integer = floor(maybe_integer);

    // We want to prevent users from making mistakes by specifing extra precision in the bounds.
    // Since floating point is inaccurate, we need to account for this when testing for equality by
    // considering the values almost equal to likely mean the bounds are within the precision range.
    auto e = std::numeric_limits<double>::epsilon();
    return fabs(maybe_integer - floor_integer) <= (e * floor_integer);
}

bool validateDecimal128PrecisionRange(Decimal128& dec, uint32_t precision) {
    Decimal128 maybe_integer = dec.scale(precision);
    Decimal128 floor_integer = maybe_integer.round();

    return maybe_integer == floor_integer;
}

}  // namespace mongo
