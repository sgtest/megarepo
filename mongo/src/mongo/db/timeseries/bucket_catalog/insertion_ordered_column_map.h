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

#pragma once

#include <string>
#include <utility>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/util/bsoncolumnbuilder.h"

namespace mongo::timeseries::bucket_catalog {

/**
 * A map that stores keys to compressed column builders in insertion order, and fills in skips for
 * missing data fields.
 */
class InsertionOrderedColumnMap {
public:
    InsertionOrderedColumnMap() = default;

    /**
     * Inserts one measurement. Vector should contain every data field, including the time field,
     * but not meta field. Will account for skips:
     * - A new data field is added that wasn't in the map before - adds a number of skips equal to
     * the number of existing measurements in all builders prior to the insert into the builder of
     * the new data field.
     * - An existing data field is missing in this measurement - adds a skip to the builder of the
     * missing data field.
     */
    void insertOne(std::vector<BSONElement> oneMeasurementDataFields);

    // TODO(SERVER-84101): remove when tracking allocator is implemented.
    size_t getMemoryUsage() const {
        size_t buildersSize = (sizeof(BSONColumnBuilder) + sizeof(size_t)) * _builders.size();
        size_t insertionOrderAllocatedKeys = _insertionOrderSize;
        size_t insertionOrderUnAllocatedKeys =
            (_insertionOrder.capacity() - _insertionOrder.size()) * sizeof(std::string);
        size_t remainingMembersSize = 3 * sizeof(size_t);
        return buildersSize + insertionOrderAllocatedKeys + insertionOrderUnAllocatedKeys +
            remainingMembersSize;
    }

    /**
     * Sets internal state of builders to that of pre-existing compressed builders.
     * numMeasurements should be equal to the number of measurements in every data field in the
     * bucket.
     */
    void initBuilders(BSONObj bucketDataDocWithCompressedBuilders, size_t numMeasurements);

    BSONColumnBuilder& getBuilder(std::string key) {
        return _builders[key].second;
    }

    /**
     * Iterates over keys, in insertion order.
     */
    // TODO(SERVER-86187): Make these StringData
    boost::optional<std::string> begin();
    boost::optional<std::string> next();

private:
    /**
     * Inserts skips where needed to all builders. Must be called after inserting one measurement.
     * Cannot call this after multiple measurements have been inserted.
     */
    void _fillSkipsInMissingFields();

    friend class InsertionOrderedColumnMapTest;
    void _assertInternalStateIdentical_forTest();

    // Get current builder, checking invariants.
    std::string _getDirect();
    void _insertNewKey(const std::string& key,
                       const BSONElement& elem,
                       BSONColumnBuilder builder,
                       size_t numMeasurements = 1);

    using MeasurementCountAndBuilder = std::pair<size_t, BSONColumnBuilder>;
    StringMap<MeasurementCountAndBuilder> _builders;
    std::vector<std::string> _insertionOrder;  // keys, stored in insertion order
    size_t _insertionOrderSize{0};
    size_t _measurementCount{0};
    size_t _pos{0};
};

}  // namespace mongo::timeseries::bucket_catalog
