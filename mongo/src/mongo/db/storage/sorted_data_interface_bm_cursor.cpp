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


#include <benchmark/benchmark.h>
#include <cstddef>
#include <memory>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/record_id.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/key_string.h"
#include "mongo/db/storage/sorted_data_interface.h"
#include "mongo/db/storage/sorted_data_interface_test_harness.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/unittest/assert.h"


namespace mongo {
namespace {

using Cursor = SortedDataInterface::Cursor;
enum Direction { kBackward, kForward };
enum Uniqueness { kUnique, kNonUnique };
enum EndPosition { kWithEnd, kWithoutEnd };
const auto kRecordId = Cursor::KeyInclusion::kExclude;
const auto kRecordIdAndKey = Cursor::KeyInclusion::kInclude;

struct Fixture {
    Fixture(Uniqueness uniqueness, Direction direction, int nToInsert)
        : uniqueness(uniqueness),
          direction(direction),
          nToInsert(nToInsert),
          harness(newSortedDataInterfaceHarnessHelper()),
          sorted(harness->newSortedDataInterface(uniqueness == kUnique, /*partial*/ false)),
          opCtx(harness->newOperationContext()),
          cursor(sorted->newCursor(opCtx.get(), direction == kForward)),
          firstKey(makeKeyStringForSeek(sorted.get(),
                                        BSON("" << (direction == kForward ? 1 : nToInsert)),
                                        direction == kForward,
                                        true)) {

        WriteUnitOfWork uow(opCtx.get());
        for (int i = 0; i < nToInsert; i++) {
            BSONObj key = BSON("" << i);
            RecordId loc(42, i * 2);
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key, loc), true));
        }
        uow.commit();
        ASSERT_EQUALS(nToInsert, sorted->numEntries(opCtx.get()));
    }

    const Uniqueness uniqueness;
    const Direction direction;
    const int nToInsert;

    std::unique_ptr<SortedDataInterfaceHarnessHelper> harness;
    std::unique_ptr<SortedDataInterface> sorted;
    ServiceContext::UniqueOperationContext opCtx;
    std::unique_ptr<SortedDataInterface::Cursor> cursor;
    KeyString::Value firstKey;
    size_t itemsProcessed = 0;
};

void BM_Advance(benchmark::State& state,
                Direction direction,
                Cursor::KeyInclusion keyInclusion,
                Uniqueness uniqueness) {

    Fixture fix(uniqueness, direction, 100'000);

    for (auto _ : state) {
        fix.cursor->seek(fix.firstKey);
        for (int i = 1; i < fix.nToInsert; i++)
            fix.cursor->next(keyInclusion);
        fix.itemsProcessed += fix.nToInsert;
    }
    ASSERT(!fix.cursor->next());
    state.SetItemsProcessed(fix.itemsProcessed);
};

void BM_AdvanceWithEnd(benchmark::State& state, Direction direction, Uniqueness uniqueness) {

    Fixture fix(uniqueness, direction, 100'000);

    for (auto _ : state) {
        fix.cursor->seek(fix.firstKey);
        BSONObj lastKey = BSON("" << (direction == kForward ? fix.nToInsert : 1));
        fix.cursor->setEndPosition(lastKey, /*inclusive*/ true);
        for (int i = 1; i < fix.nToInsert; i++)
            fix.cursor->next(kRecordId);
        fix.itemsProcessed += fix.nToInsert;
    }
    ASSERT(!fix.cursor->next());
    state.SetItemsProcessed(fix.itemsProcessed);
};


BENCHMARK_CAPTURE(BM_Advance, AdvanceForwardLoc, kForward, kRecordId, kNonUnique);
BENCHMARK_CAPTURE(BM_Advance, AdvanceForwardKeyAndLoc, kForward, kRecordIdAndKey, kNonUnique);
BENCHMARK_CAPTURE(BM_Advance, AdvanceForwardLocUnique, kForward, kRecordId, kUnique);
BENCHMARK_CAPTURE(BM_Advance, AdvanceForwardKeyAndLocUnique, kForward, kRecordIdAndKey, kUnique);

BENCHMARK_CAPTURE(BM_Advance, AdvanceBackwardLoc, kBackward, kRecordId, kNonUnique);
BENCHMARK_CAPTURE(BM_Advance, AdvanceBackwardKeyAndLoc, kBackward, kRecordIdAndKey, kNonUnique);
BENCHMARK_CAPTURE(BM_Advance, AdvanceBackwardLocUnique, kBackward, kRecordId, kUnique);
BENCHMARK_CAPTURE(BM_Advance, AdvanceBackwardKeyAndLocUnique, kBackward, kRecordIdAndKey, kUnique);

BENCHMARK_CAPTURE(BM_AdvanceWithEnd, AdvanceForward, kForward, kNonUnique);
BENCHMARK_CAPTURE(BM_AdvanceWithEnd, AdvanceForwardUnique, kForward, kUnique);
BENCHMARK_CAPTURE(BM_AdvanceWithEnd, AdvanceBackward, kBackward, kNonUnique);
BENCHMARK_CAPTURE(BM_AdvanceWithEnd, AdvanceBackwardUnique, kBackward, kUnique);

}  // namespace
}  // namespace mongo
