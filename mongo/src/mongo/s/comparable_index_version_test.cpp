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

#include <memory>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/timestamp.h"
#include "mongo/s/sharding_index_catalog_cache.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo {
namespace {

TEST(ComparableIndexVersionTest, NoIndexesVersionsAreEqual) {
    const auto version1 = ComparableIndexVersion::makeComparableIndexVersion(boost::none);
    const auto version2 = ComparableIndexVersion::makeComparableIndexVersion(boost::none);
    ASSERT(version1 == version2);
}

TEST(ComparableIndexVersionTest, SameTimestampVersionsAreEqual) {
    const auto timestamp = Timestamp(5, 4);
    const auto version1 = ComparableIndexVersion::makeComparableIndexVersion(timestamp);
    const auto version2 = ComparableIndexVersion::makeComparableIndexVersion(timestamp);
    ASSERT(version1 == version2);
}

TEST(ComparableIndexVersionTest, VersionsEqualAfterCopy) {
    const Timestamp timestamp(5, 4);
    const auto version1 = ComparableIndexVersion::makeComparableIndexVersion(timestamp);
    const auto version2 = version1;
    ASSERT(version1 == version2);
}

TEST(ComparableIndexVersionTest, HigherTimestampIsGreater) {
    const auto version1 = ComparableIndexVersion::makeComparableIndexVersion(Timestamp(1, 0));
    const auto version2 = ComparableIndexVersion::makeComparableIndexVersion(Timestamp(1, 1));
    const auto version3 = ComparableIndexVersion::makeComparableIndexVersion(Timestamp(2, 0));
    ASSERT(version2 != version1);
    ASSERT(version2 > version1);
    ASSERT_FALSE(version2 < version1);
    ASSERT(version3 != version2);
    ASSERT(version3 > version2);
    ASSERT_FALSE(version3 < version2);
}

TEST(ComparableIndexVersionTest, LowerTimestampIsLess) {
    const auto version1 = ComparableIndexVersion::makeComparableIndexVersion(Timestamp(1, 0));
    const auto version2 = ComparableIndexVersion::makeComparableIndexVersion(Timestamp(1, 1));
    const auto version3 = ComparableIndexVersion::makeComparableIndexVersion(Timestamp(2, 0));
    ASSERT(version1 != version2);
    ASSERT(version1 < version2);
    ASSERT_FALSE(version1 > version2);
    ASSERT(version3 != version2);
    ASSERT(version2 < version3);
    ASSERT_FALSE(version2 > version3);
}

TEST(ComparableIndexVersionTest, DefaultConstructedVersionsAreEqual) {
    const ComparableIndexVersion defaultVersion1{}, defaultVersion2{};
    ASSERT(defaultVersion1 == defaultVersion2);
    ASSERT_FALSE(defaultVersion1 < defaultVersion2);
    ASSERT_FALSE(defaultVersion1 > defaultVersion2);
}

TEST(ComparableIndexVersionTest, DefaultConstructedVersionIsLessThanNoIndexesVersion) {
    const ComparableIndexVersion defaultVersion{};
    const auto withIndexesVersion = ComparableIndexVersion::makeComparableIndexVersion(boost::none);
    ASSERT(defaultVersion != withIndexesVersion);
    ASSERT(defaultVersion < withIndexesVersion);
    ASSERT_FALSE(defaultVersion > withIndexesVersion);
}

TEST(ComparableIndexVersionTest, DefaultConstructedVersionIsLessThanWithTimestampVersion) {
    const ComparableIndexVersion defaultVersion{};
    const auto noIndexesVersion =
        ComparableIndexVersion::makeComparableIndexVersion(Timestamp(5, 4));
    ASSERT(defaultVersion != noIndexesVersion);
    ASSERT(defaultVersion < noIndexesVersion);
    ASSERT_FALSE(defaultVersion > noIndexesVersion);
}

TEST(ComparableIndexVersionTest, NoIndexesGreaterThanDefault) {
    const auto noIndexesVersion = ComparableIndexVersion::makeComparableIndexVersion(boost::none);
    const ComparableIndexVersion defaultVersion{};
    ASSERT(noIndexesVersion != defaultVersion);
    ASSERT(noIndexesVersion > defaultVersion);
}

TEST(ComparableIndexVersionTest, NoIndexesAndWithTimestampUseDisambiguatingSequenceNumber) {
    const auto firstNoIndexesVersion =
        ComparableIndexVersion::makeComparableIndexVersion(boost::none);
    const auto firstWithTimestampVersion =
        ComparableIndexVersion::makeComparableIndexVersion(Timestamp(1, 0));
    const auto secondNoIndexesVersion =
        ComparableIndexVersion::makeComparableIndexVersion(boost::none);
    const auto secondWithTimestampVersion =
        ComparableIndexVersion::makeComparableIndexVersion(Timestamp(1, 1));

    ASSERT(firstNoIndexesVersion != firstWithTimestampVersion);
    ASSERT(firstWithTimestampVersion > firstNoIndexesVersion);
    ASSERT(firstNoIndexesVersion < firstWithTimestampVersion);

    ASSERT(secondNoIndexesVersion == firstNoIndexesVersion);
    ASSERT(secondNoIndexesVersion != firstWithTimestampVersion);
    ASSERT(secondNoIndexesVersion > firstWithTimestampVersion);

    ASSERT(secondNoIndexesVersion != secondWithTimestampVersion);
    ASSERT(secondWithTimestampVersion > secondNoIndexesVersion);
    ASSERT(secondNoIndexesVersion < secondWithTimestampVersion);
}

TEST(ComparableIndexVersionTest, CompareForcedRefreshVersionVersusValidCollectionIndexes) {
    const Timestamp indexVersionTimestamp = Timestamp(100, 0);
    const ComparableIndexVersion defaultVersionBeforeForce;
    const auto versionBeforeForce =
        ComparableIndexVersion::makeComparableIndexVersion(indexVersionTimestamp);
    const auto forcedRefreshVersion =
        ComparableIndexVersion::makeComparableIndexVersionForForcedRefresh();
    const auto versionAfterForce =
        ComparableIndexVersion::makeComparableIndexVersion(indexVersionTimestamp);
    const ComparableIndexVersion defaultVersionAfterForce;

    ASSERT(defaultVersionBeforeForce != forcedRefreshVersion);
    ASSERT(defaultVersionBeforeForce < forcedRefreshVersion);

    ASSERT(versionBeforeForce != forcedRefreshVersion);
    ASSERT(versionBeforeForce < forcedRefreshVersion);

    ASSERT(versionAfterForce != forcedRefreshVersion);
    ASSERT(versionAfterForce > forcedRefreshVersion);

    ASSERT(defaultVersionAfterForce != forcedRefreshVersion);
    ASSERT(defaultVersionAfterForce < forcedRefreshVersion);
}

TEST(ComparableIndexVersionTest, CompareTwoForcedRefreshVersions) {
    const auto forcedRefreshVersion1 =
        ComparableIndexVersion::makeComparableIndexVersionForForcedRefresh();
    ASSERT(forcedRefreshVersion1 == forcedRefreshVersion1);
    ASSERT_FALSE(forcedRefreshVersion1 < forcedRefreshVersion1);
    ASSERT_FALSE(forcedRefreshVersion1 > forcedRefreshVersion1);

    const auto forcedRefreshVersion2 =
        ComparableIndexVersion::makeComparableIndexVersionForForcedRefresh();
    ASSERT_FALSE(forcedRefreshVersion1 == forcedRefreshVersion2);
    ASSERT(forcedRefreshVersion1 < forcedRefreshVersion2);
    ASSERT_FALSE(forcedRefreshVersion1 > forcedRefreshVersion2);
}

}  // namespace
}  // namespace mongo
