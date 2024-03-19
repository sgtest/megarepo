/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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


#include <initializer_list>
#include <memory>
#include <string>
#include <type_traits>

#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/create_collection.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/s/auto_split_vector.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/s/shard_server_test_fixture.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/random.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTest


namespace mongo {
namespace {

const NamespaceString kNss = NamespaceString::createNamespaceString_forTest("autosplitDB", "coll");
const std::string kPattern = "_id";

/*
 * Call the autoSplitVector function of the test collection on a chunk with bounds [0, 100) and with
 * the specified `maxChunkSizeMB`.
 */
std::pair<std::vector<BSONObj>, bool> autoSplit(OperationContext* opCtx,
                                                int maxChunkSizeMB,
                                                boost::optional<int> limit = boost::none,
                                                bool forward = true) {
    return autoSplitVector(opCtx,
                           kNss,
                           BSON(kPattern << 1) /* shard key pattern */,
                           BSON(kPattern << 0) /* min */,
                           BSON(kPattern << 1000) /* max */,
                           maxChunkSizeMB * 1024 * 1024 /* max chunk size in bytes*/,
                           limit,
                           forward);
}

class AutoSplitVectorTest : public ShardServerTestFixture {
public:
    /*
     * Before each test case:
     * - Creates a sharded collection with shard key `_id`
     */
    void setUp() override {
        ShardServerTestFixture::setUp();

        auto opCtx = operationContext();

        {
            OperationShardingState::ScopedAllowImplicitCollectionCreate_UNSAFE
                unsafeCreateCollection(opCtx);
            uassertStatusOK(
                createCollection(operationContext(), kNss.dbName(), BSON("create" << kNss.coll())));
        }

        DBDirectClient client(opCtx);
        client.createIndex(kNss, BSON(kPattern << 1));
    }

    /*
     * Insert the specified number of documents in the test collection, with incremental shard key
     * `_id` starting from `_nextShardKey`.
     */
    void insertNDocsOf1MB(OperationContext* opCtx, int nDocs) {
        DBDirectClient client(opCtx);

        std::string s(1024 * 1024 - 24, 'a');  // To get a 1MB document
        for (int i = 0; i < nDocs; i++) {
            BSONObjBuilder builder;
            builder.append(kPattern, _nextShardKey++);
            builder.append("str", s);
            BSONObj obj = builder.obj();
            ASSERT(obj.objsize() == 1024 * 1024);  // 1 MB document
            client.insert(kNss, obj);
        }
    }

    /*
     * Get the number of documents inserted until now.
     */
    int getInsertedSize() {
        return _nextShardKey;
    }

private:
    int _nextShardKey = 0;
};

class AutoSplitVectorTest10MB : public AutoSplitVectorTest {
    /*
     * Before each test case:
     * - Creates a sharded collection with shard key `_id`
     * - Inserts `10` documents of ~1MB size (shard keys [0...9])
     */
    void setUp() override {
        AutoSplitVectorTest::setUp();

        auto opCtx = operationContext();

        insertNDocsOf1MB(opCtx, 10 /* nDocs */);

        DBDirectClient client(opCtx);
        ASSERT_EQUALS(10, client.count(kNss));
    }
};

// Throw exception upon calling autoSplitVector on dropped/unexisting collection
TEST_F(AutoSplitVectorTest, NoCollection) {
    ASSERT_THROWS_CODE(
        autoSplitVector(operationContext(),
                        NamespaceString::createNamespaceString_forTest("dummy", "collection"),
                        BSON(kPattern << 1) /* shard key pattern */,
                        BSON(kPattern << kMinBSONKey) /* min */,
                        BSON(kPattern << kMaxBSONKey) /* max */,
                        1 * 1024 * 1024 /* max chunk size in bytes*/),
        DBException,
        ErrorCodes::NamespaceNotFound);
}

TEST_F(AutoSplitVectorTest, EmptyCollection) {
    const auto [splitKey, continuation] =
        autoSplitVector(operationContext(),
                        kNss,
                        BSON(kPattern << 1) /* shard key pattern */,
                        BSON(kPattern << kMinBSONKey) /* min */,
                        BSON(kPattern << kMaxBSONKey) /* max */,
                        1 * 1024 * 1024 /* max chunk size in bytes*/);
    ASSERT_EQ(0, splitKey.size());
    ASSERT_FALSE(continuation);
}

TEST_F(AutoSplitVectorTest, EmptyCollectionBackwards) {
    const auto [splitKey, continuation] =
        autoSplitVector(operationContext(),
                        kNss,
                        BSON(kPattern << 1) /* shard key pattern */,
                        BSON(kPattern << kMinBSONKey) /* min */,
                        BSON(kPattern << kMaxBSONKey) /* max */,
                        1 * 1024 * 1024 /* max chunk size in bytes*/,
                        boost::none,
                        false);
    ASSERT_EQ(0, splitKey.size());
    ASSERT_FALSE(continuation);
}

TEST_F(AutoSplitVectorTest10MB, EmptyRange) {
    const auto [splitKey, continuation] =
        autoSplitVector(operationContext(),
                        kNss,
                        BSON(kPattern << 1) /* shard key pattern */,
                        BSON(kPattern << kMinBSONKey) /* min */,
                        BSON(kPattern << -10) /* max */,
                        1 * 1024 * 1024 /* max chunk size in bytes*/);
    ASSERT_EQ(0, splitKey.size());
    ASSERT_FALSE(continuation);
}


// No split points if estimated `data size < max chunk size`
TEST_F(AutoSplitVectorTest10MB, NoSplitIfDataLessThanMaxChunkSize) {
    auto [splitKeys, continuation] = autoSplit(operationContext(), 11 /* maxChunkSizeMB */);
    ASSERT_EQ(splitKeys.size(), 0);
    ASSERT_FALSE(continuation);
}

// Do not split in case of `chunk size == maxChunkSize`
TEST_F(AutoSplitVectorTest10MB, NoSplitIfDataEqualMaxChunkSize) {
    auto [splitKeys, continuation] = autoSplit(operationContext(), 10 /* maxChunkSizeMB */);
    ASSERT_EQ(splitKeys.size(), 0);
    ASSERT_FALSE(continuation);
}

// No split points if `chunk size > max chunk size` but threshold not reached
TEST_F(AutoSplitVectorTest10MB, NoSplitIfDataLessThanThreshold) {
    const auto surplus = 2;
    {
        // Increase collection size so that the auto splitter can actually be triggered. Use a
        // different range to don't interfere with the chunk getting splitted.
        insertNDocsOf1MB(operationContext(), surplus /* nDocs */);
    }
    auto [splitKeys, continuation] = autoSplit(operationContext(), 10 /* maxChunkSizeMB */);
    ASSERT_EQ(splitKeys.size(), 0);
    ASSERT_FALSE(continuation);
}

// One split point if `chunk size > max chunk size` and threshold reached
TEST_F(AutoSplitVectorTest10MB, SplitIfDataSlightlyMoreThanThreshold) {
    const auto surplus = 4;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);
    auto [splitKeys, continuation] = autoSplit(operationContext(), 10 /* maxChunkSizeMB */);
    ASSERT_EQ(splitKeys.size(), 1);
    ASSERT_EQ(6, splitKeys.front().getIntField(kPattern));
    ASSERT_FALSE(continuation);
}

TEST_F(AutoSplitVectorTest10MB, SplitIfDataSlightlyMoreThanThresholdBackwards) {
    const auto surplus = 4;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);
    auto [splitKeys, continuation] =
        autoSplit(operationContext(), 10 /* maxChunkSizeMB */, boost::none, false);
    ASSERT_EQ(splitKeys.size(), 1);
    ASSERT_EQ(7, splitKeys.front().getIntField(kPattern));
    ASSERT_FALSE(continuation);
}

// Split points if `data size > max chunk size * 2` and threshold reached
TEST_F(AutoSplitVectorTest10MB, SplitIfDataMoreThanThreshold) {
    const auto surplus = 14;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);
    auto [splitKeys, continuation] = autoSplit(operationContext(), 10 /* maxChunkSizeMB */);
    ASSERT_EQ(splitKeys.size(), 2);
    ASSERT_EQ(7, splitKeys.front().getIntField(kPattern));
    ASSERT_EQ(15, splitKeys.back().getIntField(kPattern));
    ASSERT_FALSE(continuation);
}

TEST_F(AutoSplitVectorTest10MB, SplitIfDataMoreThanThresholdBackwards) {
    const auto surplus = 14;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);
    auto [splitKeys, continuation] =
        autoSplit(operationContext(), 10 /* maxChunkSizeMB */, boost::none, false);
    ASSERT_EQ(splitKeys.size(), 2);
    ASSERT_EQ(16, splitKeys.front().getIntField(kPattern));
    ASSERT_EQ(8, splitKeys.back().getIntField(kPattern));
    ASSERT_FALSE(continuation);
}

// Split points are not recalculated if the right-most chunk is at least `80% maxChunkSize`
TEST_F(AutoSplitVectorTest10MB, NoRecalculateIfBigLastChunk) {
    const auto surplus = 8;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);
    auto [splitKeys, continuation] = autoSplit(operationContext(), 10 /* maxChunkSizeMB */);
    ASSERT_EQ(splitKeys.size(), 1);
    ASSERT_EQ(9, splitKeys.front().getIntField(kPattern));
    ASSERT_FALSE(continuation);
}

TEST_F(AutoSplitVectorTest10MB, NoRecalculateIfBigLastChunkBackwards) {
    const auto surplus = 8;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);
    auto [splitKeys, continuation] =
        autoSplit(operationContext(), 10 /* maxChunkSizeMB */, boost::none, false);
    ASSERT_EQ(splitKeys.size(), 1);
    ASSERT_EQ(8, splitKeys.front().getIntField(kPattern));
    ASSERT_FALSE(continuation);
}

// Test that the limit argument is honored and that split points are correctly repositioned
TEST_F(AutoSplitVectorTest10MB, LimitArgIsRespected) {
    const auto surplus = 4;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);

    // Maximum split keys returned (no limit)
    const auto numPossibleSplitKeys = [&]() {
        auto [splitKeys, continuation] = autoSplit(operationContext(), 2 /* maxChunkSizeMB */);
        return splitKeys.size();
    }();

    ASSERT_GT(numPossibleSplitKeys, 3);
    for (auto limit : {1, 2, 3}) {
        const auto [splitKeys, continuation] =
            autoSplit(operationContext(), 2 /* maxChunkSizeMB */, limit);
        ASSERT_EQ(splitKeys.size(), limit);
    }
}

TEST_F(AutoSplitVectorTest10MB, LimitArgIsRespectedBackwards) {
    const auto surplus = 4;
    insertNDocsOf1MB(operationContext(), surplus /* nDocs */);

    // Maximum split keys returned (no limit)
    const auto numPossibleSplitKeys = [&]() {
        auto [splitKeys, continuation] =
            autoSplit(operationContext(), 2 /* maxChunkSizeMB */, boost::none, false);
        return splitKeys.size();
    }();

    ASSERT_GT(numPossibleSplitKeys, 3);
    for (auto limit : {1, 2, 3}) {
        const auto [splitKeys, continuation] =
            autoSplit(operationContext(), 2 /* maxChunkSizeMB */, limit, false);
        ASSERT_EQ(splitKeys.size(), limit);
    }
}

class RepositionLastSplitPointsTest : public AutoSplitVectorTest {
public:
    /*
     * Tests that last split points are properly repositioned in case the surplus allows so or not
     * repositioned otherwise.
     */
    void checkRepositioning(int maxDocsPerChunk, int surplus, int nSplitPoints) {
        ASSERT(surplus >= 0 && surplus < maxDocsPerChunk);

        const auto maxDocsPerNewChunk =
            maxDocsPerChunk - ((maxDocsPerChunk - surplus) / (nSplitPoints + 1));
        bool mustReposition =
            surplus >= maxDocsPerChunk - maxDocsPerNewChunk && surplus < maxDocsPerChunk * 0.8;

        int toInsert = (maxDocsPerChunk * nSplitPoints) - getInsertedSize() + surplus;
        insertNDocsOf1MB(operationContext(), toInsert);

        int expectedChunkSize =
            mustReposition ? getInsertedSize() / (nSplitPoints + 1) : maxDocsPerChunk;
        auto [splitKeys, continuation] =
            autoSplit(operationContext(), maxDocsPerChunk /* maxChunkSizeMB */);

        int approximateNextMin = expectedChunkSize;
        for (const auto& splitKey : splitKeys) {
            int _id = splitKey.getIntField(kPattern);
            // Expect an approximate match due to integers rounding in the split points algorithm.
            ASSERT(_id >= approximateNextMin - 2 && _id <= approximateNextMin + 2) << BSON(
                "approximateNextMin"
                << approximateNextMin << "splitKeys" << splitKeys << "maxDocsPerChunk"
                << maxDocsPerChunk << "surplus" << surplus << "nSplitPoints" << nSplitPoints
                << "maxDocsPerNewChunk" << maxDocsPerNewChunk << "mustReposition" << mustReposition
                << "toInsert" << toInsert << "expectedChunkSize" << expectedChunkSize);
            approximateNextMin = _id + expectedChunkSize;
        }
    }
};


// Test that last split points are recalculated fairly (if the surplus allows so)
TEST_F(RepositionLastSplitPointsTest, RandomRepositioningTest) {
    PseudoRandom random(SecureRandom().nextInt64());
    // Avoid small sizes already checked in other test cases.
    // Random maxDocsPerChunk in interval: [10, 110).
    int maxDocsPerChunk = random.nextInt32(100) + 10;
    // Random surplus in interval: [0, maxDocsPerChunk).
    int surplus = random.nextInt32(maxDocsPerChunk);

    LOGV2(6000900,
          "RandomRepositioningTest parameters",
          "maxDocsPerChunk"_attr = maxDocsPerChunk,
          "surplus"_attr = surplus);

    for (int nSplitPointsToReposition = 1; nSplitPointsToReposition < 4;
         nSplitPointsToReposition++) {
        checkRepositioning(maxDocsPerChunk, surplus, nSplitPointsToReposition);
    }
}


}  // namespace
}  // namespace mongo
