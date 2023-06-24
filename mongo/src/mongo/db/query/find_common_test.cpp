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

#include <string>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/query/find_common.h"
#include "mongo/stdx/type_traits.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace {

using namespace mongo;

TEST(BSONArrayResponseSizeTrackerTest, AddLargeNumberOfElements) {
    BSONObjBuilder bsonObjBuilder;
    {
        FindCommon::BSONArrayResponseSizeTracker sizeTracker;
        BSONArrayBuilder arrayBuilder{bsonObjBuilder.subarrayStart("a")};
        BSONObj emptyObject;
        while (sizeTracker.haveSpaceForNext(emptyObject)) {
            sizeTracker.add(emptyObject);
            arrayBuilder.append(emptyObject);
        }
    }
    // If the BSON object is successfully constructed, then space accounting was correct.
    bsonObjBuilder.obj();
}
TEST(BSONArrayResponseSizeTrackerTest, CanAddAtLeastOneDocument) {
    auto largeObject = BSON("a" << std::string(16 * 1024 * 1024, 'A'));
    BSONObj emptyObject;
    BSONObjBuilder bsonObjBuilder;
    {
        FindCommon::BSONArrayResponseSizeTracker sizeTracker;
        BSONArrayBuilder arrayBuilder{bsonObjBuilder.subarrayStart("a")};
        // Add an object that is larger than 16MB.
        ASSERT(sizeTracker.haveSpaceForNext(largeObject));
        sizeTracker.add(largeObject);
        arrayBuilder.append(largeObject);
        ASSERT(!sizeTracker.haveSpaceForNext(emptyObject));
    }
    // If the BSON object is successfully constructed, then space accounting was correct.
    bsonObjBuilder.obj();
}
}  // namespace
