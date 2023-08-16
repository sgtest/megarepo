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

#include "mongo/db/pipeline/memory_usage_tracker.h"

#include "mongo/unittest/assert.h"
#include "mongo/unittest/death_test.h"
#include "mongo/unittest/framework.h"

namespace mongo {
namespace {

class MemoryUsageTrackerTest : public unittest::Test {
public:
    static constexpr auto kDefaultMax = 1 * 1024;  // 1KB max.
    MemoryUsageTrackerTest()
        : _tracker(false /** allowDiskUse */, kDefaultMax), _funcTracker(_tracker["funcTracker"]) {}


    MemoryUsageTracker _tracker;
    MemoryUsageTracker::Impl _funcTracker;
};

TEST_F(MemoryUsageTrackerTest, SetFunctionUsageUpdatesGlobal) {
    _tracker.update(50LL);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 50LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 50LL);

    // 50 global + 50 _funcTracker.
    _funcTracker.set(50);
    ASSERT_EQ(_funcTracker.currentMemoryBytes(), 50LL);
    ASSERT_EQ(_funcTracker.maxMemoryBytes(), 50LL);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 100LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 100LL);

    // New tracker adds another 50, 150 total.
    _tracker.set("newTracker", 50);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 150LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);

    // Lower usage of function tracker is reflected in global.
    _tracker.set("newTracker", 0);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 100LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
}

TEST_F(MemoryUsageTrackerTest, UpdateUsageUpdatesGlobal) {
    _tracker.update(50LL);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 50LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 50LL);

    // Add another 50 to the global, 100 total.
    _tracker.update(50);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 100LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 100LL);

    // Function tracker adds another 50, 150 total.
    _funcTracker.update(50);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 150LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);

    // Lower usage of function tracker is reflected in global.
    _funcTracker.update(-25);
    ASSERT_EQ(_tracker.currentMemoryBytes(), 125LL);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
}

DEATH_TEST_F(MemoryUsageTrackerTest,
             UpdateFunctionUsageToNegativeIsDisallowed,
             "Underflow in memory tracking") {
    _funcTracker.set(50LL);
    _funcTracker.update(-100LL);
}

DEATH_TEST_F(MemoryUsageTrackerTest,
             UpdateMemUsageToNegativeIsDisallowed,
             "Underflow in memory tracking") {
    _tracker.update(50LL);
    _tracker.update(-100LL);
}

TEST_F(MemoryUsageTrackerTest, MemoryTokenUpdatesCurrentAndMax) {
    {
        MemoryToken token{50LL, &_tracker};
        ASSERT_EQ(_tracker.currentMemoryBytes(), 50LL);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 50LL);
        {
            MemoryToken funcToken{100LL, &_funcTracker};
            ASSERT_EQ(_funcTracker.currentMemoryBytes(), 100LL);
            ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

            ASSERT_EQ(_tracker.currentMemoryBytes(), 150LL);
            ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
        }
        ASSERT_EQ(_tracker.currentMemoryBytes(), 50LL);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
    }
    ASSERT_EQ(_funcTracker.currentMemoryBytes(), 0);
    ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

    ASSERT_EQ(_tracker.currentMemoryBytes(), 0);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
}

TEST_F(MemoryUsageTrackerTest, MemoryTokenCanBeMoved) {
    {
        MemoryToken token{50LL, &_tracker};
        ASSERT_EQ(_tracker.currentMemoryBytes(), 50LL);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 50LL);

        MemoryToken token2(std::move(token));
        ASSERT_EQ(_tracker.currentMemoryBytes(), 50LL);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 50LL);
    }
    ASSERT_EQ(_tracker.currentMemoryBytes(), 0);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 50LL);
}

TEST_F(MemoryUsageTrackerTest, MemoryTokenCanBeMoveAssigned) {
    {
        MemoryToken token{50LL, &_tracker};
        ASSERT_EQ(_tracker.currentMemoryBytes(), 50LL);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 50LL);
        {
            MemoryToken token2{100LL, &_funcTracker};
            ASSERT_EQ(_funcTracker.currentMemoryBytes(), 100LL);
            ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

            ASSERT_EQ(_tracker.currentMemoryBytes(), 150LL);
            ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);

            token = std::move(token2);
            ASSERT_EQ(_funcTracker.currentMemoryBytes(), 100LL);
            ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

            ASSERT_EQ(_tracker.currentMemoryBytes(), 100LL);
            ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
        }
        ASSERT_EQ(_funcTracker.currentMemoryBytes(), 100LL);
        ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

        ASSERT_EQ(_tracker.currentMemoryBytes(), 100LL);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
    }
    ASSERT_EQ(_funcTracker.currentMemoryBytes(), 0);
    ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

    ASSERT_EQ(_tracker.currentMemoryBytes(), 0);
    ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
}

TEST_F(MemoryUsageTrackerTest, MemoryTokenCanBeStoredInVector) {
    auto assertMemory = [this]() {
        ASSERT_EQ(_funcTracker.currentMemoryBytes(), 100LL);
        ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

        ASSERT_EQ(_tracker.currentMemoryBytes(), 150LL);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
    };

    auto assertZeroMemory = [this]() {
        ASSERT_EQ(_funcTracker.currentMemoryBytes(), 0);
        ASSERT_EQ(_funcTracker.maxMemoryBytes(), 100LL);

        ASSERT_EQ(_tracker.currentMemoryBytes(), 0);
        ASSERT_EQ(_tracker.maxMemoryBytes(), 150LL);
    };

    {
        std::vector<MemoryToken> tokens;
        // Use default constructor
        tokens.resize(10);
        {
            std::vector<MemoryToken> tokens2;
            tokens2.emplace_back(50LL, &_tracker);
            tokens2.emplace_back(100LL, &_funcTracker);
            assertMemory();

            // Force reallocation
            tokens2.reserve(2 * tokens2.capacity());
            assertMemory();

            tokens = std::move(tokens2);
            assertMemory();
        }
        assertMemory();

        tokens.clear();
        assertZeroMemory();
    }
    assertZeroMemory();
}

TEST_F(MemoryUsageTrackerTest, MemoryTokenWith) {
    static const std::vector<std::string> kLines = {"a", "bb", "ccc", "dddd"};

    int64_t total_size = 0;
    std::vector<MemoryTokenWith<std::string>> memory_tracked_vector;
    for (const auto& line : kLines) {
        memory_tracked_vector.emplace_back(MemoryToken{line.size(), &_tracker}, line);
        total_size += line.size();
        ASSERT_EQ(total_size, _tracker.currentMemoryBytes());
        ASSERT_EQ(total_size, _tracker.maxMemoryBytes());
    }

    int64_t max_size = total_size;
    while (!memory_tracked_vector.empty()) {
        total_size -= memory_tracked_vector.back().value().size();
        memory_tracked_vector.pop_back();
        ASSERT_EQ(total_size, _tracker.currentMemoryBytes());
        ASSERT_EQ(max_size, _tracker.maxMemoryBytes());
    }
}

}  // namespace
}  // namespace mongo
