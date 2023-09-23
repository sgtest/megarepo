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

#include <string>
#include <utility>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/matcher/schema/json_pointer.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/uuid.h"

namespace mongo {

/**
 * A JSON Pointer to the key id or an array of UUIDs identifying a set of keys.
 */
class EncryptSchemaKeyId {
    friend class EncryptionInfo;
    friend class EncryptionMetadata;

public:
    enum class Type {
        kUUIDs,
        kJSONPointer,
    };

    static EncryptSchemaKeyId parseFromBSON(const BSONElement& element);

    EncryptSchemaKeyId(std::string key) : _pointer(key), _type(Type::kJSONPointer) {}

    EncryptSchemaKeyId(std::vector<UUID> keys) : _uuids(std::move(keys)), _type(Type::kUUIDs) {}

    void serializeToBSON(StringData fieldName, BSONObjBuilder* builder) const;

    Type type() const {
        return _type;
    }

    /**
     * Callers must check that the result of type() is kUUIDs first.
     */
    const std::vector<UUID>& uuids() const {
        invariant(_type == Type::kUUIDs);
        return _uuids;
    }

    /**
     * Callers must check that the result of type() is kJSONPointer first.
     */
    const JSONPointer& jsonPointer() const {
        invariant(_type == Type::kJSONPointer);
        return _pointer;
    }

    bool operator==(const EncryptSchemaKeyId& other) const {
        if (_type != other.type()) {
            return false;
        }

        return _type == Type::kUUIDs ? _uuids == other.uuids() : _pointer == other.jsonPointer();
    }

    bool operator!=(const EncryptSchemaKeyId& other) const {
        return !(*this == other);
    }

    /**
     * IDL requires overload of all comparison operators, however for this class the only viable
     * comparison is equality. These should be removed once SERVER-39677 is implemented.
     */
    bool operator>(const EncryptSchemaKeyId& other) const {
        MONGO_UNREACHABLE;
    }

    bool operator<(const EncryptSchemaKeyId& other) const {
        MONGO_UNREACHABLE;
    }

private:
    // The default constructor is required to exist by IDL, but is private because it does not
    // construct a valid EncryptSchemaKeyId and should not be called.
    EncryptSchemaKeyId() = default;

    JSONPointer _pointer;
    std::vector<UUID> _uuids;

    Type _type;
};
}  // namespace mongo
