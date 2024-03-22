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

#include <memory>

#include "mongo/db/record_id.h"
#include "mongo/db/storage/sorted_data_interface.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo {
namespace {

key_string::Value makeKeyString(key_string::Version version,
                                Ordering ordering,
                                BSONObj bsonKey,
                                RecordId& rid) {
    key_string::Builder builder(version, bsonKey, ordering);
    builder.appendRecordId(rid);
    return builder.getValueCopy();
}

TEST(SortedDataInterface, SortedDataKeyValueViewTest) {
    BSONObj key = BSON("a" << 1 << "b" << 2.0);
    const Ordering ALL_ASCENDING = Ordering::make(BSONObj());

    char ridBuf[12];
    memset(ridBuf, 0x55, 12);
    RecordId rid(ridBuf, 12);

    for (auto version : {key_string::Version::V0, key_string::Version::V1}) {
        auto keyString = makeKeyString(version, ALL_ASCENDING, key, rid);
        auto ksSize =
            getKeySize(keyString.getBuffer(), keyString.getSize(), ALL_ASCENDING, version);
        auto tb = keyString.getTypeBits();
        auto view = SortedDataKeyValueView(keyString.getBuffer(),
                                           ksSize,
                                           keyString.getBuffer() + ksSize, /* ridData */
                                           keyString.getSize() - ksSize,   /* ridSize */
                                           tb.getBuffer(),
                                           tb.getSize(),
                                           version,
                                           true);
        auto value = view.getValueCopy();
        auto bsonObj = key_string::toBson(value, ALL_ASCENDING);
        ASSERT_BSONOBJ_EQ(bsonObj, BSONObj::stripFieldNames(key));
        auto decodedRid = view.decodeRecordId(KeyFormat::String);
        ASSERT_EQ(rid, decodedRid);
    }
}

}  // namespace
}  // namespace mongo
