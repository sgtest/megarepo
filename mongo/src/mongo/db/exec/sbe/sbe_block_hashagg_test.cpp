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

#include <map>
#include <vector>

#include <boost/optional/optional.hpp>

#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/exec/sbe/expressions/compile_ctx.h"
#include "mongo/db/exec/sbe/sbe_block_test_helpers.h"
#include "mongo/db/exec/sbe/sbe_plan_stage_test.h"
#include "mongo/db/exec/sbe/stages/block_hashagg.h"
#include "mongo/db/exec/sbe/values/slot.h"
#include "mongo/db/exec/sbe/values/value.h"

namespace mongo::sbe {

namespace {
typedef std::map<int32_t, std::vector<int32_t>> TestResultType;
using TypedValue = std::pair<value::TypeTags, value::Value>;
}  // namespace

class BlockHashAggStageTest : public PlanStageTestFixture {
public:
    void setUp() override {
        PlanStageTestFixture::setUp();
        _globalLock = std::make_unique<Lock::GlobalLock>(operationContext(), MODE_IS);
    }

    void tearDown() override {
        _globalLock.reset();
        PlanStageTestFixture::tearDown();
    }

    static std::vector<TypedValue> unpackBlock(TypedValue blockPair, size_t expectedBlockSize) {
        auto [blockTag, blockVal] = blockPair;
        ASSERT_EQ(blockTag, value::TypeTags::valueBlock);
        auto deblocked = value::bitcastTo<value::ValueBlock*>(blockVal)->extract();

        ASSERT_EQ(deblocked.count(), expectedBlockSize);
        std::vector<TypedValue> res(deblocked.count());
        for (size_t i = 0; i < deblocked.count(); ++i) {
            res[i] = deblocked[i];
        }
        return res;
    }

    static std::vector<std::vector<TypedValue>> unpackArrayOfBlocks(value::Value arrayVal,
                                                                    size_t expectedBlockSize) {
        auto arr = value::getArrayView(arrayVal);
        std::vector<std::vector<TypedValue>> result;
        for (size_t i = 0; i < arr->size(); i++) {
            result.emplace_back(unpackBlock(arr->getAt(i), expectedBlockSize));
        }
        return result;
    }

    static TypedValue makeArray(std::vector<TypedValue> vals) {
        auto [arrTag, arrVal] = value::makeNewArray();
        value::ValueGuard guard(arrTag, arrVal);
        for (auto [t, v] : vals) {
            value::getArrayView(arrVal)->push_back(t, v);
        }
        guard.reset();
        return {arrTag, arrVal};
    }

    template <typename... BlockData>
    static TypedValue makeInputArray(int32_t id, std::vector<bool> bitset, BlockData... blockData) {
        auto [arrTag, arrVal] = value::makeNewArray();
        value::ValueGuard guard(arrTag, arrVal);
        auto arr = value::getArrayView(arrVal);

        // Append groupBy key.
        arr->push_back(makeInt32(id));
        // Append bitset block.
        auto bitsetBlock = makeBoolBlock(bitset);
        arr->push_back({sbe::value::TypeTags::valueBlock,
                        value::bitcastFrom<value::ValueBlock*>(bitsetBlock.release())});
        // Append data.
        (arr->push_back(makeHeterogeneousBlockTagVal(blockData)), ...);
        guard.reset();
        return {arrTag, arrVal};
    }

    // This helper takes an array of groupby results and compares to the expectedMap of group ID to
    // a list of accumulator results.
    static void assertResultMatchesMap(TypedValue result,
                                       TestResultType expectedMap,
                                       std::vector<size_t> expectedBlockSizes) {
        ASSERT_EQ(result.first, value::TypeTags::Array);
        auto resultArr = value::getArrayView(result.second);

        size_t ebsIndex = 0;
        for (auto [subArrTag, subArrVal] : resultArr->values()) {
            ASSERT_EQ(subArrTag, value::TypeTags::Array);

            // The first "row" in the vector stores the keys, and each subsequent row stores the
            // value of each accumulator. results[0][1] gives you the {tag, val} of the second key.
            // results[1][2] gives you the {tag, val} of the first accumlator for the third group.
            auto results = unpackArrayOfBlocks(subArrVal, expectedBlockSizes[ebsIndex++]);

            // Iterate over each key.
            for (size_t i = 0; i < results[0].size(); ++i) {
                ASSERT_EQ(results[0][i].first, value::TypeTags::NumberInt32);
                auto key = value::bitcastTo<int32_t>(results[0][i].second);

                auto expectedVals = expectedMap.at(key);
                ASSERT_EQ(results.size(), expectedVals.size() + 1);

                // Check the expected results for each accumulator.
                for (size_t j = 0; j < expectedVals.size(); j++) {
                    assertValuesEqual(results[j + 1][i].first,
                                      results[j + 1][i].second,
                                      value::TypeTags::NumberInt32,
                                      value::bitcastTo<int32_t>(expectedVals[j]));
                }

                // Delete from the expected map so we know we get the results exactly once.
                expectedMap.erase(key);
            }
        }
        ASSERT(expectedMap.empty());
    }

    template <typename... BlockData>
    static TypedValue makeInputArray(std::vector<TypedValue> id,
                                     std::vector<bool> bitset,
                                     BlockData... blockData) {
        auto [arrTag, arrVal] = value::makeNewArray();
        value::Array* arr = value::getArrayView(arrVal);

        // Append groupby keys.
        arr->push_back(makeHeterogeneousBlockTagVal(id));
        // Append corresponding bitset.
        auto bitsetBlock = makeBoolBlock(bitset);
        arr->push_back({sbe::value::TypeTags::valueBlock,
                        value::bitcastFrom<value::ValueBlock*>(bitsetBlock.release())});
        // Append data.
        (arr->push_back(makeHeterogeneousBlockTagVal(blockData)), ...);
        return {arrTag, arrVal};
    }

    // Given the data input, the number of slots the stage requires, accumulators used, and
    // expected output, runs the BlockHashAgg stage and asserts that we get correct results.
    void runBlockHashAggTest(TypedValue inputData,
                             size_t numScanSlots,
                             std::vector<std::pair<std::string, std::string>> accNames,
                             TestResultType expected,
                             std::vector<size_t> expectedBlockSizes) {
        auto makeFn = [&](value::SlotVector scanSlots, std::unique_ptr<PlanStage> scanStage) {
            auto idSlot = scanSlots[0];
            auto bitsetInSlot = scanSlots[1];
            value::SlotVector outputSlots{idSlot};

            value::SlotVector dataInSlots;
            value::SlotVector accDataSlots;

            auto accumulatorBitset = generateSlotId();
            auto internalSlot = generateSlotId();
            BlockHashAggStage::BlockAndRowAggs aggs;
            size_t scanSlotIdx = 2;
            for (const auto& [blockAcc, rowAcc] : accNames) {
                auto outputSlot = generateSlotId();
                std::unique_ptr<sbe::EExpression> blockAccFunc;
                if (blockAcc == "valueBlockCount") {
                    // valueBlockCount is the exception - it takes just the bitset.
                    blockAccFunc =
                        stage_builder::makeFunction(blockAcc, makeE<EVariable>(accumulatorBitset));
                } else {
                    dataInSlots.push_back(scanSlots[scanSlotIdx]);
                    auto dataSlot = generateSlotId();
                    accDataSlots.push_back(dataSlot);
                    blockAccFunc = stage_builder::makeFunction(
                        blockAcc, makeE<EVariable>(accumulatorBitset), makeE<EVariable>(dataSlot));

                    scanSlotIdx++;
                }
                aggs.emplace(
                    outputSlot,
                    BlockHashAggStage::BlockRowAccumulators{
                        std::move(blockAccFunc),
                        stage_builder::makeFunction(rowAcc, makeE<EVariable>(internalSlot))});
                outputSlots.push_back(outputSlot);
            }

            auto outStage = makeS<BlockHashAggStage>(std::move(scanStage),
                                                     idSlot,
                                                     bitsetInSlot,
                                                     dataInSlots,
                                                     internalSlot,
                                                     accumulatorBitset,
                                                     accDataSlots,
                                                     std::move(aggs),
                                                     kEmptyPlanNodeId,
                                                     true);
            return std::make_pair(outputSlots, std::move(outStage));
        };

        auto result = runTestMulti(numScanSlots, inputData.first, inputData.second, makeFn);
        value::ValueGuard resultGuard{result};
        assertResultMatchesMap(result, expected, expectedBlockSizes);
    }

private:
    std::unique_ptr<Lock::GlobalLock> _globalLock;
};

TEST_F(BlockHashAggStageTest, NoData) {
    auto [inputTag, inputVal] = makeArray({});
    // We should have an empty block with no data.
    TestResultType expected = {};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockMin", "min"}}, expected, {});
}

TEST_F(BlockHashAggStageTest, AllDataFiltered) {
    // All data has "false" for bitset.
    auto [inputTag, inputVal] =
        makeArray({makeInputArray(0, {false, false, false}, makeInt32s({50, 20, 30}))});
    // We should have an empty block with no data.
    TestResultType expected = {};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockMin", "min"}}, expected, {});
}

TEST_F(BlockHashAggStageTest, SingleAccumulatorMin) {
    // Each entry is ID followed by bitset followed by a block of data. For example
    // [groupid, [block bitset values], [block data values]]
    auto [inputTag, inputVal] =
        makeArray({makeInputArray(0, {true, true, false}, makeInt32s({50, 20, 30})),
                   makeInputArray(2, {false, true, true}, makeInt32s({40, 30, 60})),
                   makeInputArray(1, {true, true, true}, makeInt32s({70, 80, 10})),
                   makeInputArray(2, {false, false, false}, makeInt32s({10, 20, 30})),
                   makeInputArray(2, {true, false, true}, makeInt32s({30, 40, 50}))});
    /*
     * 0 -> min(50, 20) = 20
     * 1 -> min(70, 80, 10) = 10
     * 2 -> min(30, 60, 30, 50) = 30
     */
    TestResultType expected = {{0, {20}}, {1, {10}}, {2, {30}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockMin", "min"}}, expected, {3});
}

TEST_F(BlockHashAggStageTest, Count1) {
    // Each entry is ID followed by a bitset.
    auto [inputTag, inputVal] = makeArray({makeInputArray(0, {true, true, true}),
                                           makeInputArray(0, {true, false, true}),
                                           makeInputArray(1, {true, false, true}),
                                           makeInputArray(1, {true, true, false})});
    TestResultType expected = {{0, {5}}, {1, {4}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockCount", "sum"}}, expected, {2});
}

TEST_F(BlockHashAggStageTest, Sum1) {
    // Each entry is ID followed by bitset followed by a block of data.
    auto [inputTag, inputVal] =
        makeArray({makeInputArray(0, {true, true, false}, makeInt32s({1, 2, 3})),
                   makeInputArray(2, {false, true, true}, makeInt32s({4, 5, 6})),
                   makeInputArray(1, {true, true, true}, makeInt32s({7, 8, 9})),
                   makeInputArray(2, {false, false, false}, makeInt32s({10, 11, 12})),
                   makeInputArray(2, {true, false, true}, makeInt32s({13, 14, 15}))});
    /*
     * 0 -> 1+2 = 3
     * 1 -> 7+8+9 = 24
     * 2 -> 5+6+13+15 = 39
     */
    TestResultType expected = {{0, {3}}, {1, {24}}, {2, {39}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockSum", "sum"}}, expected, {3});
}

TEST_F(BlockHashAggStageTest, MultipleAccumulators) {
    // Each entry is ID followed by bitset followed by block A and block B.
    auto [inputTag, inputVal] = makeArray(
        {makeInputArray(
             100, {true, true, false}, makeInt32s({200, 100, 150}), makeInt32s({2, 4, 7})),
         makeInputArray(
             100, {false, true, true}, makeInt32s({50, 90, 60}), makeInt32s({-100, 20, 3})),
         makeInputArray(
             50, {true, true, true}, makeInt32s({200, 100, 150}), makeInt32s({-150, 150, 20})),
         makeInputArray(
             25, {true, false, false}, makeInt32s({20, 75, 10}), makeInt32s({0, 20, -20})),
         makeInputArray(
             50, {true, false, true}, makeInt32s({75, 75, 75}), makeInt32s({-2, 5, 8}))});
    /*
     * 25  -> min(20) = 20, count=1, min(0) = 0
     * 50  -> min(200, 100, 150, 75, 75) = 75, count = 5, min(-150, 150, 20, -2, 8) = -150
     * 100 -> min(200, 100, 90, 60) = 60, count = 4, min(2, 4, 20, 3) = 2
     */
    TestResultType expected = {{25, {20, 1, 0}}, {50, {75, 5, -150}}, {100, {60, 4, 2}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal),
        4,
        {{"valueBlockMin", "min"}, {"valueBlockCount", "sum"}, {"valueBlockMin", "min"}},
        expected,
        {3});
}

// --- Tests with block groupby key inputs ---

TEST_F(BlockHashAggStageTest, SumBlockGroupByKey1) {
    // Each entry is ID followed by bitset followed by a block of data.
    auto [inputTag, inputVal] = makeArray(
        {makeInputArray(makeInt32s({0, 0, 0}), {true, true, false}, makeInt32s({1, 2, 3})),
         makeInputArray(makeInt32s({2, 2, 2}), {false, true, true}, makeInt32s({4, 5, 6})),
         makeInputArray(makeInt32s({1, 1, 1}), {true, true, true}, makeInt32s({7, 8, 9})),
         makeInputArray(makeInt32s({2, 2, 2}), {false, false, false}, makeInt32s({10, 11, 12})),
         makeInputArray(makeInt32s({2, 2, 2}), {true, false, true}, makeInt32s({13, 14, 15}))});

    /*
     * 0 -> 1+2 = 3
     * 1 -> 7+8+9 = 24
     * 2 -> 5+6+13+15 = 39
     */
    TestResultType expected = {{0, {3}}, {1, {24}}, {2, {39}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockSum", "sum"}}, expected, {3});
}

// Similar to the test above, but we change the groupby keys so they are different within each
// block.
TEST_F(BlockHashAggStageTest, SumDifferentBlockGroupByKeys2) {
    // Each entry is ID followed by bitset followed by a block of data.
    auto [inputTag, inputVal] = makeArray(
        {makeInputArray(makeInt32s({1, 2, 3}), {true, true, false}, makeInt32s({1, 2, 3})),
         makeInputArray(makeInt32s({2, 2, 2}), {false, true, true}, makeInt32s({4, 5, 6})),
         makeInputArray(makeInt32s({3, 2, 1}), {true, true, true}, makeInt32s({7, 8, 9})),
         makeInputArray(makeInt32s({2, 3, 4}), {false, true, true}, makeInt32s({10, 11, 12})),
         makeInputArray(makeInt32s({2, 3, 4}), {false, false, false}, makeInt32s({0, 5, 4})),
         makeInputArray(makeInt32s({1, 1, 2}), {true, true, true}, makeInt32s({13, 14, 15}))});

    /*
     * 1 -> 1+9+13+14  = 37
     * 2 -> 2+5+6+8+15 = 36
     * 3 -> 7+11       = 18
     * 4 -> 12         = 12
     */
    TestResultType expected = {{1, {37}}, {2, {36}}, {3, {18}}, {4, {12}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockSum", "sum"}}, expected, {4});
}

// Similar test as above but the "2" key appears in every block but is always false, so we make
// sure it's missing.
TEST_F(BlockHashAggStageTest, SumDifferentBlockGroupByKeysMissingKey) {
    // Each entry is ID followed by bitset followed by a block of data.
    // Mix blocks with a high number of unique keys and blocks with a low number of unique keys.
    auto [inputTag, inputVal] = makeArray(
        {makeInputArray(makeInt32s({1, 2, 3, 5, 6, 7}),
                        {true, false, false, true, true, true},
                        makeInt32s({1, 2, 3, 4, 5, 6})),
         makeInputArray(makeInt32s({2, 2, 2}), {false, false, false}, makeInt32s({4, 5, 6})),
         makeInputArray(makeInt32s({3, 2, 1, 7, 6, 5}),
                        {true, false, true, false, true, true},
                        makeInt32s({7, 8, 9, 1, 2, 3})),
         makeInputArray(makeInt32s({2, 3, 4, 6, 7, 5}),
                        {false, true, true, true, true, false},
                        makeInt32s({10, 11, 12, 15, 15, 15})),
         makeInputArray(makeInt32s({2, 3, 4}), {false, false, false}, makeInt32s({0, 5, 4})),
         makeInputArray(makeInt32s({1, 1, 2}), {true, true, false}, makeInt32s({13, 14, 15}))});

    /*
     * 1 -> 1+9+13+14  = 37
     * 2 -> missing
     * 3 -> 7+11       = 18
     * 4 -> 12         = 12
     * 5 -> 4+3        = 7
     * 6 -> 5+2+15     = 22
     * 7 -> 6+15       = 21
     */
    TestResultType expected = {{1, {37}}, {3, {18}}, {4, {12}}, {5, {7}}, {6, {22}}, {7, {21}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockSum", "sum"}}, expected, {6});
}

TEST_F(BlockHashAggStageTest, MultipleAccumulatorsDifferentBlockGroupByKeys) {
    // Each entry is ID followed by bitset followed by block A and block B.
    auto [inputTag, inputVal] = makeArray({makeInputArray(makeInt32s({25, 50, 100}),
                                                          {true, true, false},
                                                          makeInt32s({200, 100, 150}),
                                                          makeInt32s({2, 4, 7})),
                                           makeInputArray(makeInt32s({50, 50, 50}),
                                                          {false, true, true},
                                                          makeInt32s({50, 90, 60}),
                                                          makeInt32s({-100, 20, 3})),
                                           makeInputArray(makeInt32s({25, 25, 100}),
                                                          {true, true, true},
                                                          makeInt32s({200, 100, 150}),
                                                          makeInt32s({-150, 150, 2})),
                                           makeInputArray(makeInt32s({100, 50, 25}),
                                                          {true, false, false},
                                                          makeInt32s({20, 75, 10}),
                                                          makeInt32s({0, 20, -20})),
                                           makeInputArray(makeInt32s({100, 25, 50}),
                                                          {true, false, true},
                                                          makeInt32s({75, 75, 75}),
                                                          makeInt32s({-2, 5, 8}))});

    /*
     * 25  -> min(200, 200, 100) = 100, count = 3, min(2, -150, 150) = -150
     * 50  -> min(100, 90, 60, 75) = 60, count = 4, min(4, 20, 3, 8) = 3
     * 100 -> min(150, 20, 75) = 20, count = 3, min(20, 0, -2) = -2
     */
    TestResultType expected = {{25, {100, 3, -150}}, {50, {60, 4, 3}}, {100, {20, 3, -2}}};
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal),
        4,
        {{"valueBlockMin", "min"}, {"valueBlockCount", "sum"}, {"valueBlockMin", "min"}},
        expected,
        {3});
}

TEST_F(BlockHashAggStageTest, BlockOutSizeTest) {
    TestResultType expected;
    auto addToExpected = [&expected](int32_t id, bool exists, int32_t data) {
        if (exists) {
            expected.emplace(id, std::vector<int32_t>{0});
            expected[id][0] += data;
        }
    };

    std::vector<TypedValue> vals;
    // Create kBlockOutSize * 3 + 1 group ids, so that the output is 3 blocks of size
    // kBlockOutSize, and 1 block of size 1.
    for (size_t id = 0; id < BlockHashAggStage::kBlockOutSize * 3 + 1; ++id) {
        std::vector<int32_t> ids;
        std::vector<bool> bitmap;
        std::vector<int32_t> data;

        for (size_t i = 0; i < 6; i++) {
            // Every third entry will be false.
            bool exists = i % 3 != 0;
            int32_t dataPoint = i + id * 5;

            // Add to our expected result map, and to our input data.
            addToExpected(id, exists, dataPoint);
            ids.push_back(id);
            bitmap.push_back(exists);
            data.push_back(dataPoint);
        }

        auto input = makeInputArray(makeInt32s(ids), bitmap, makeInt32s(data));
        vals.push_back(input);
    }

    auto [inputTag, inputVal] = makeArray(vals);
    runBlockHashAggTest(std::make_pair(inputTag, inputVal),
                        3,
                        {{"valueBlockSum", "sum"}},
                        expected,
                        {BlockHashAggStage::kBlockOutSize,
                         BlockHashAggStage::kBlockOutSize,
                         BlockHashAggStage::kBlockOutSize,
                         1});
}

TEST_F(BlockHashAggStageTest, MultipleAccumulatorsDifferentPartitionSizes) {
    // Test blocks of partition size three below the limit to three above the limit. For more
    // interesting data we'll use blocks that are two times this size, so there will be two entries
    // per key. For example for partitionSize=3 we would have [1,2,3,1,2,3].
    const size_t lowPartitionSize = BlockHashAggStage::kMaxNumPartitionsForTokenizedPath - 3;
    const size_t highPartitionSize = BlockHashAggStage::kMaxNumPartitionsForTokenizedPath + 3;

    TestResultType expected;
    auto addToExpected = [&expected](int32_t id, bool exists, int32_t data) {
        if (exists) {
            expected.emplace(id, std::vector<int32_t>{0});
            expected[id][0] += data;
        }
    };

    std::vector<std::pair<value::TypeTags, value::Value>> vals;
    size_t i = 0;
    for (size_t partitionSize = lowPartitionSize; partitionSize <= highPartitionSize;
         partitionSize++) {
        std::vector<int32_t> ids;
        std::vector<bool> bitmap;
        std::vector<int32_t> data;
        for (size_t dupRound = 0; dupRound < 2; dupRound++) {
            for (size_t blockIndex = 0; blockIndex < partitionSize; blockIndex++) {
                int32_t id = blockIndex;
                // Every third entry will be false.
                bool exists = i % 3 != 0;
                int32_t dataPoint = partitionSize * 2 + dupRound * 3 + blockIndex * 5;

                // Add to our expected result map, and to our input data.
                addToExpected(id, exists, dataPoint);
                ids.push_back(id);
                bitmap.push_back(exists);
                data.push_back(dataPoint);
                i++;
            }
        }
        auto input = makeInputArray(makeInt32s(ids), bitmap, makeInt32s(data));
        vals.push_back(input);
    }

    auto [inputTag, inputVal] = makeArray(vals);
    runBlockHashAggTest(
        std::make_pair(inputTag, inputVal), 3, {{"valueBlockSum", "sum"}}, expected, {8});
}
}  // namespace mongo::sbe
