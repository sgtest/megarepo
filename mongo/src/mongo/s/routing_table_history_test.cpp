/**
 *    Copyright (C) 2018-present MongoDB, Inc.
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

#include "mongo/platform/basic.h"

#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/service_context.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/unittest/death_test.h"
#include "mongo/unittest/unittest.h"

namespace mongo {
namespace {

const ShardId kThisShard("thisShard");
const NamespaceString kNss = NamespaceString::createNamespaceString_forTest("TestDB", "TestColl");

/**
 * Creates a new routing table from the input routing table by inserting the chunks specified by
 * newChunkBoundaryPoints.  newChunkBoundaryPoints specifies a contiguous array of keys indicating
 * chunk boundaries to be inserted. As an example, if you want to split the range [0, 2] into chunks
 * [0, 1] and [1, 2], newChunkBoundaryPoints should be [0, 1, 2].
 */
RoutingTableHistory splitChunk(const RoutingTableHistory& rt,
                               const std::vector<BSONObj>& newChunkBoundaryPoints) {

    invariant(newChunkBoundaryPoints.size() > 1);

    // Convert the boundary points into chunk range objects, e.g. {0, 1, 2} ->
    // {{ChunkRange{0, 1}, ChunkRange{1, 2}}
    std::vector<ChunkRange> newChunkRanges;
    for (size_t i = 0; i < newChunkBoundaryPoints.size() - 1; ++i) {
        newChunkRanges.emplace_back(newChunkBoundaryPoints[i], newChunkBoundaryPoints[i + 1]);
    }

    std::vector<ChunkType> newChunks;
    auto curVersion = rt.getVersion();

    for (const auto& range : newChunkRanges) {
        // Chunks must be inserted ordered by version
        curVersion.incMajor();
        newChunks.emplace_back(rt.getUUID(), range, curVersion, kThisShard);
    }

    return rt.makeUpdated(
        boost::none /* timeseriesFields */, boost::none /* reshardingFields */, true, newChunks);
}

/**
 * Gets a set of raw pointers to ChunkInfo objects in the specified range,
 */
std::set<ChunkInfo*> getChunksInRange(const RoutingTableHistory& rt,
                                      const BSONObj& min,
                                      const BSONObj& max) {
    std::set<ChunkInfo*> chunksFromSplit;

    rt.forEachOverlappingChunk(min, max, false, [&](auto& chunk) {
        chunksFromSplit.insert(chunk.get());
        return true;
    });

    return chunksFromSplit;
}

/**
 * Looks up a chunk that corresponds to or contains the range [min, max). There should only be one
 * such chunk in the input RoutingTableHistory object.
 */
ChunkInfo* getChunkToSplit(const RoutingTableHistory& rt, const BSONObj& min, const BSONObj& max) {
    std::shared_ptr<ChunkInfo> firstOverlappingChunk;

    rt.forEachOverlappingChunk(min, max, false, [&](auto& chunkInfo) {
        firstOverlappingChunk = chunkInfo;
        return false;  // only need first chunk
    });

    invariant(firstOverlappingChunk);
    return firstOverlappingChunk.get();
}

/**
 * Test fixture for tests that need to start with a fresh routing table with
 * only a single chunk in it, with bytes already written to that chunk object.
 */
class RoutingTableHistoryTest : public unittest::Test {
public:
    void setUp() override {
        const UUID uuid = UUID::gen();
        const OID epoch = OID::gen();
        const Timestamp timestamp(1);
        ChunkVersion version({epoch, timestamp}, {1, 0});

        auto initChunk =
            ChunkType{uuid,
                      ChunkRange{_shardKeyPattern.globalMin(), _shardKeyPattern.globalMax()},
                      version,
                      kThisShard};

        _rt.emplace(RoutingTableHistory::makeNew(kNss,
                                                 uuid,
                                                 _shardKeyPattern,
                                                 nullptr,
                                                 false,
                                                 epoch,
                                                 timestamp,
                                                 boost::none /* timeseriesFields */,
                                                 boost::none /* reshardingFields */,
                                                 true,
                                                 {initChunk}));
        ASSERT_EQ(_rt->numChunks(), 1ull);
    }

    const KeyPattern& getShardKeyPattern() const {
        return _shardKeyPattern;
    }

    const RoutingTableHistory& getInitialRoutingTable() const {
        return *_rt;
    }

private:
    boost::optional<RoutingTableHistory> _rt;

    KeyPattern _shardKeyPattern{BSON("a" << 1)};
};

/**
 * Test fixture for tests that need to start with three chunks in it, with the
 * same number of bytes written to every chunk object.
 */
class RoutingTableHistoryTestThreeInitialChunks : public RoutingTableHistoryTest {
public:
    void setUp() override {
        RoutingTableHistoryTest::setUp();
        _initialChunkBoundaryPoints = {getShardKeyPattern().globalMin(),
                                       BSON("a" << 10),
                                       BSON("a" << 20),
                                       getShardKeyPattern().globalMax()};
        _rt.emplace(splitChunk(RoutingTableHistoryTest::getInitialRoutingTable(),
                               _initialChunkBoundaryPoints));
        ASSERT_EQ(_rt->numChunks(), 3ull);
    }

    const RoutingTableHistory& getInitialRoutingTable() const {
        return *_rt;
    }

    std::vector<BSONObj> getInitialChunkBoundaryPoints() {
        return _initialChunkBoundaryPoints;
    }

private:
    boost::optional<RoutingTableHistory> _rt;

    std::vector<BSONObj> _initialChunkBoundaryPoints;
};

TEST_F(RoutingTableHistoryTest, TestSplits) {
    const UUID uuid = UUID::gen();
    const OID epoch = OID::gen();
    const Timestamp timestamp(1);
    ChunkVersion version({epoch, timestamp}, {1, 0});

    auto chunkAll =
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), getShardKeyPattern().globalMax()},
                  version,
                  kThisShard};

    auto rt = RoutingTableHistory::makeNew(kNss,
                                           uuid,
                                           getShardKeyPattern(),
                                           nullptr,
                                           false,
                                           epoch,
                                           timestamp,
                                           boost::none /* timeseriesFields */,
                                           boost::none /* reshardingFields */,
                                           true,
                                           {chunkAll});

    std::vector<ChunkType> chunks1 = {
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 0)},
                  ChunkVersion({epoch, timestamp}, {2, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 0), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard}};

    auto rt1 = rt.makeUpdated(
        boost::none /* timeseriesFields */, boost::none /* reshardingFields */, true, chunks1);
    auto v1 = ChunkVersion({epoch, timestamp}, {2, 2});
    ASSERT_EQ(v1, rt1.getVersion(kThisShard));

    std::vector<ChunkType> chunks2 = {
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 0), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << -1)},
                  ChunkVersion({epoch, timestamp}, {3, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << -1), BSON("a" << 0)},
                  ChunkVersion({epoch, timestamp}, {3, 2}),
                  kThisShard}};

    auto rt2 = rt1.makeUpdated(
        boost::none /* timeseriesFields */, boost::none /* reshardingFields */, true, chunks2);
    auto v2 = ChunkVersion({epoch, timestamp}, {3, 2});
    ASSERT_EQ(v2, rt2.getVersion(kThisShard));
}

TEST_F(RoutingTableHistoryTest, TestReplaceEmptyChunk) {
    const UUID uuid = UUID::gen();
    const OID epoch = OID::gen();
    const Timestamp timestamp(1);

    std::vector<ChunkType> initialChunks = {
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {1, 0}),
                  kThisShard}};

    auto rt = RoutingTableHistory::makeNew(kNss,
                                           uuid,
                                           getShardKeyPattern(),
                                           nullptr,
                                           false,
                                           epoch,
                                           timestamp,
                                           boost::none /* timeseriesFields */,
                                           boost::none /* reshardingFields */,
                                           true,
                                           initialChunks);
    ASSERT_EQ(rt.numChunks(), 1);

    std::vector<ChunkType> changedChunks = {
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 0)},
                  ChunkVersion({epoch, timestamp}, {2, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 0), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard}};

    auto rt1 = rt.makeUpdated(boost::none /* timeseriesFields */,
                              boost::none /* reshardingFields */,
                              true,
                              changedChunks);
    auto v1 = ChunkVersion({epoch, timestamp}, {2, 2});
    ASSERT_EQ(v1, rt1.getVersion(kThisShard));
    ASSERT_EQ(rt1.numChunks(), 2);

    std::shared_ptr<ChunkInfo> found;

    rt1.forEachChunk(
        [&](auto& chunkInfo) {
            if (chunkInfo->getShardIdAt(boost::none) == kThisShard) {
                found = chunkInfo;
                return false;
            }
            return true;
        },
        BSON("a" << 0));
    ASSERT(found);
}

TEST_F(RoutingTableHistoryTest, TestUseLatestVersions) {
    const UUID uuid = UUID::gen();
    const OID epoch = OID::gen();
    const Timestamp timestamp(1);

    std::vector<ChunkType> initialChunks = {
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {1, 0}),
                  kThisShard}};

    auto rt = RoutingTableHistory::makeNew(kNss,
                                           uuid,
                                           getShardKeyPattern(),
                                           nullptr,
                                           false,
                                           epoch,
                                           timestamp,
                                           boost::none /* timeseriesFields */,
                                           boost::none /* reshardingFields */,
                                           true,
                                           initialChunks);
    ASSERT_EQ(rt.numChunks(), 1);

    std::vector<ChunkType> changedChunks = {
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {1, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 0)},
                  ChunkVersion({epoch, timestamp}, {2, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 0), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard}};

    auto rt1 = rt.makeUpdated(boost::none /* timeseriesFields */,
                              boost::none /* reshardingFields */,
                              true,
                              changedChunks);
    auto v1 = ChunkVersion({epoch, timestamp}, {2, 2});
    ASSERT_EQ(v1, rt1.getVersion(kThisShard));
    ASSERT_EQ(rt1.numChunks(), 2);
}

TEST_F(RoutingTableHistoryTest, TestOutOfOrderVersion) {
    const UUID uuid = UUID::gen();
    const OID epoch = OID::gen();
    const Timestamp timestamp(1);

    std::vector<ChunkType> initialChunks = {
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 0)},
                  ChunkVersion({epoch, timestamp}, {2, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 0), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard}};

    auto rt = RoutingTableHistory::makeNew(kNss,
                                           uuid,
                                           getShardKeyPattern(),
                                           nullptr,
                                           false,
                                           epoch,
                                           timestamp,
                                           boost::none /* timeseriesFields */,
                                           boost::none /* reshardingFields */,
                                           true,
                                           initialChunks);
    ASSERT_EQ(rt.numChunks(), 2);

    std::vector<ChunkType> changedChunks = {
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 0), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {3, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 0)},
                  ChunkVersion({epoch, timestamp}, {3, 1}),
                  kThisShard}};

    auto rt1 = rt.makeUpdated(boost::none /* timeseriesFields */,
                              boost::none /* reshardingFields */,
                              true,
                              changedChunks);
    auto v1 = ChunkVersion({epoch, timestamp}, {3, 1});
    ASSERT_EQ(v1, rt1.getVersion(kThisShard));
    ASSERT_EQ(rt1.numChunks(), 2);

    auto chunk1 = rt1.findIntersectingChunk(BSON("a" << 0));
    ASSERT_EQ(chunk1->getLastmod(), ChunkVersion({epoch, timestamp}, {3, 0}));
    ASSERT_EQ(chunk1->getMin().woCompare(BSON("a" << 0)), 0);
    ASSERT_EQ(chunk1->getMax().woCompare(getShardKeyPattern().globalMax()), 0);
}

TEST_F(RoutingTableHistoryTest, TestMergeChunks) {
    const UUID uuid = UUID::gen();
    const OID epoch = OID::gen();
    const Timestamp timestamp(1);

    std::vector<ChunkType> initialChunks = {
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 0), BSON("a" << 10)},
                  ChunkVersion({epoch, timestamp}, {2, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 0)},
                  ChunkVersion({epoch, timestamp}, {2, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 10), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard}};

    auto rt = RoutingTableHistory::makeNew(kNss,
                                           uuid,
                                           getShardKeyPattern(),
                                           nullptr,
                                           false,
                                           epoch,
                                           timestamp,
                                           boost::none /* timeseriesFields */,
                                           boost::none /* reshardingFields */,
                                           true,
                                           initialChunks);
    ASSERT_EQ(rt.numChunks(), 3);
    ASSERT_EQ(rt.getVersion(), ChunkVersion({epoch, timestamp}, {2, 2}));

    std::vector<ChunkType> changedChunks = {
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 10), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {3, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 10)},
                  ChunkVersion({epoch, timestamp}, {3, 1}),
                  kThisShard}};

    auto rt1 = rt.makeUpdated(boost::none /* timeseriesFields */,
                              boost::none /* reshardingFields */,
                              true,
                              changedChunks);
    auto v1 = ChunkVersion({epoch, timestamp}, {3, 1});
    ASSERT_EQ(v1, rt1.getVersion(kThisShard));
    ASSERT_EQ(rt1.numChunks(), 2);
}

TEST_F(RoutingTableHistoryTest, TestMergeChunksOrdering) {
    const UUID uuid = UUID::gen();
    const OID epoch = OID::gen();
    const Timestamp timestamp(1);

    std::vector<ChunkType> initialChunks = {
        ChunkType{uuid,
                  ChunkRange{BSON("a" << -10), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << -500)},
                  ChunkVersion({epoch, timestamp}, {2, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << -500), BSON("a" << -10)},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard}};

    auto rt = RoutingTableHistory::makeNew(kNss,
                                           uuid,
                                           getShardKeyPattern(),
                                           nullptr,
                                           false,
                                           epoch,
                                           timestamp,
                                           boost::none /* timeseriesFields */,
                                           boost::none /* reshardingFields */,
                                           true,
                                           initialChunks);
    ASSERT_EQ(rt.numChunks(), 3);
    ASSERT_EQ(rt.getVersion(), ChunkVersion({epoch, timestamp}, {2, 2}));

    std::vector<ChunkType> changedChunks = {
        ChunkType{uuid,
                  ChunkRange{BSON("a" << -500), BSON("a" << -10)},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << -10)},
                  ChunkVersion({epoch, timestamp}, {3, 1}),
                  kThisShard}};

    auto rt1 = rt.makeUpdated(boost::none /* timeseriesFields */,
                              boost::none /* reshardingFields */,
                              true,
                              changedChunks);
    auto v1 = ChunkVersion({epoch, timestamp}, {3, 1});
    ASSERT_EQ(v1, rt1.getVersion(kThisShard));
    ASSERT_EQ(rt1.numChunks(), 2);

    auto chunk1 = rt1.findIntersectingChunk(BSON("a" << -500));
    ASSERT_EQ(chunk1->getLastmod(), ChunkVersion({epoch, timestamp}, {3, 1}));
    ASSERT_EQ(chunk1->getMin().woCompare(getShardKeyPattern().globalMin()), 0);
    ASSERT_EQ(chunk1->getMax().woCompare(BSON("a" << -10)), 0);
}

TEST_F(RoutingTableHistoryTest, TestFlatten) {
    const UUID uuid = UUID::gen();
    const OID epoch = OID::gen();
    const Timestamp timestamp(1);

    std::vector<ChunkType> initialChunks = {
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 10)},
                  ChunkVersion({epoch, timestamp}, {2, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 10), BSON("a" << 20)},
                  ChunkVersion({epoch, timestamp}, {2, 1}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 20), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {2, 2}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {3, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{getShardKeyPattern().globalMin(), BSON("a" << 10)},
                  ChunkVersion({epoch, timestamp}, {4, 0}),
                  kThisShard},
        ChunkType{uuid,
                  ChunkRange{BSON("a" << 10), getShardKeyPattern().globalMax()},
                  ChunkVersion({epoch, timestamp}, {4, 1}),
                  kThisShard},
    };

    auto rt = RoutingTableHistory::makeNew(kNss,
                                           UUID::gen(),
                                           getShardKeyPattern(),
                                           nullptr,
                                           false,
                                           epoch,
                                           timestamp,
                                           boost::none /* timeseriesFields */,
                                           boost::none /* reshardingFields */,
                                           true,
                                           initialChunks);
    ASSERT_EQ(rt.numChunks(), 2);
    ASSERT_EQ(rt.getVersion(), ChunkVersion({epoch, timestamp}, {4, 1}));

    auto chunk1 = rt.findIntersectingChunk(BSON("a" << 0));
    ASSERT_EQ(chunk1->getLastmod(), ChunkVersion({epoch, timestamp}, {4, 0}));
    ASSERT_EQ(chunk1->getMin().woCompare(getShardKeyPattern().globalMin()), 0);
    ASSERT_EQ(chunk1->getMax().woCompare(BSON("a" << 10)), 0);
}

}  // namespace
}  // namespace mongo
