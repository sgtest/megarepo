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

#include "mongo/db/storage/sorted_data_interface_test_harness.h"

#include <memory>

#include "mongo/db/storage/sorted_data_interface.h"
#include "mongo/unittest/unittest.h"

namespace mongo {
namespace {

// Insert multiple single-field keys and advance to each of them
// using a forward cursor by specifying their exact key. When
// advanceTo() is called on a duplicate key, the cursor is
// positioned at the first occurrence of that key in ascending
// order by RecordId.
TEST(SortedDataInterface, AdvanceTo) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key1, loc2), true /* allow duplicates */));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key1, loc3), true /* allow duplicates */));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key2, loc4), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc5), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, true, true)),
                  IndexKeyEntry(key1, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key1;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = -1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key1, loc1));

        seekPoint.keyPrefix = key2;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key2, loc4));

        seekPoint.keyPrefix = key3;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key3, loc5));

        seekPoint.keyPrefix = key4;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);
    }
}

// Insert multiple single-field keys and advance to each of them
// using a reverse cursor by specifying their exact key. When
// advanceTo() is called on a duplicate key, the cursor is
// positioned at the first occurrence of that key in descending
// order by RecordId (last occurrence in index order).
TEST(SortedDataInterface, AdvanceToReversed) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key2, loc2), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc3), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key3, loc4), true /* allow duplicates */));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key3, loc5), true /* allow duplicates */));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        bool isForward = false;
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(
            sorted->newCursor(opCtx.get(), isForward));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key3, isForward, true)),
                  IndexKeyEntry(key3, loc5));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key3;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = -1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key3, loc5));

        seekPoint.keyPrefix = key2;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key2, loc2));

        seekPoint.keyPrefix = key1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key1, loc1));

        seekPoint.keyPrefix = key0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  boost::none);
    }
}

// Insert two single-field keys, then seek a forward cursor to the larger one then seek behind
// the smaller one.  Ending position is on the smaller one since a seek describes where to go
// and should not be effected by current position.
TEST(SortedDataInterface, AdvanceToKeyBeforeCursorPosition) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key2, loc2), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(2, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, true, true)),
                  IndexKeyEntry(key1, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key0;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = -1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key1, loc1));

        seekPoint.firstExclusive = 0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key1, loc1));
    }
}

// Insert two single-field keys, then seek a reverse cursor to the smaller one then seek behind
// the larger one.  Ending position is on the larger one since a seek describes where to go
// and should not be effected by current position.
TEST(SortedDataInterface, AdvanceToKeyAfterCursorPositionReversed) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key2, loc2), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(2, sorted->numEntries(opCtx.get()));
    }

    {
        bool isForward = false;
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(
            sorted->newCursor(opCtx.get(), isForward));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key2, isForward, true)),
                  IndexKeyEntry(key2, loc2));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key3;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = -1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key2, loc2));

        seekPoint.firstExclusive = 0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key2, loc2));
    }
}

// Insert a single-field key and advance to EOF using a forward cursor
// by specifying that exact key. When seek() is called with the key
// where the cursor is positioned (and it is the first entry for that key),
// the cursor should remain at its current position. An exclusive seek will
// position the cursor on the next position, which may be EOF.
TEST(SortedDataInterface, AdvanceToKeyAtCursorPosition) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(1, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, true, true)),
                  IndexKeyEntry(key1, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key1;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = -1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key1, loc1));

        seekPoint.firstExclusive = 0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);
    }
}

// Insert a single-field key and advance to EOF using a reverse cursor
// by specifying that exact key. When seek() is called with the key
// where the cursor is positioned (and it is the first entry for that key),
// the cursor should remain at its current position. An exclusive seek will
// position the cursor on the next position, which may be EOF.
TEST(SortedDataInterface, AdvanceToKeyAtCursorPositionReversed) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(1, sorted->numEntries(opCtx.get()));
    }

    {
        bool isForward = false;
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(
            sorted->newCursor(opCtx.get(), isForward));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, isForward, true)),
                  IndexKeyEntry(key1, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key1;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = -1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key1, loc1));

        seekPoint.firstExclusive = 0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  boost::none);
    }
}

// Insert multiple single-field keys and advance to each of them using
// a forward cursor by specifying a key that comes immediately before.
// When advanceTo() is called in non-inclusive mode, the cursor is
// positioned at the key that comes after the one specified.
TEST(SortedDataInterface, AdvanceToExclusive) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key1, loc2), true /* allow duplicates */));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key1, loc3), true /* allow duplicates */));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key2, loc4), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc5), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, true, true)),
                  IndexKeyEntry(key1, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key1;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = 0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key2, loc4));

        seekPoint.keyPrefix = key2;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key3, loc5));

        seekPoint.keyPrefix = key3;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);

        seekPoint.keyPrefix = key4;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);
    }
}

// Insert multiple single-field keys and advance to each of them using
// a reverse cursor by specifying a key that comes immediately after.
// When advanceTo() is called in non-inclusive mode, the cursor is
// positioned at the key that comes before the one specified.
TEST(SortedDataInterface, AdvanceToExclusiveReversed) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key2, loc2), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc3), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key3, loc4), true /* allow duplicates */));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), key3, loc5), true /* allow duplicates */));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        bool isForward = false;
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(
            sorted->newCursor(opCtx.get(), isForward));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key3, isForward, true)),
                  IndexKeyEntry(key3, loc5));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = key3;
        seekPoint.prefixLen = 1;
        seekPoint.firstExclusive = 0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key2, loc2));

        seekPoint.keyPrefix = key2;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key1, loc1));

        seekPoint.keyPrefix = key1;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  boost::none);

        seekPoint.keyPrefix = key0;
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  boost::none);
    }
}

// Insert multiple, non-consecutive, single-field keys and advance to
// each of them using a forward cursor by specifying a key between their
// exact key and the current position of the cursor.
TEST(SortedDataInterface, AdvanceToIndirect) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    BSONObj unusedKey = key6;  // larger than any inserted key

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc2), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key5, loc3), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(3, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, true, true)),
                  IndexKeyEntry(key1, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.prefixLen = 0;
        BSONElement suffix0;
        seekPoint.keySuffix = {&suffix0};
        seekPoint.firstExclusive = -1;

        suffix0 = key2.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key3, loc2));

        suffix0 = key4.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key5, loc3));
    }
}

// Insert multiple, non-consecutive, single-field keys and advance to
// each of them using a reverse cursor by specifying a key between their
// exact key and the current position of the cursor.
TEST(SortedDataInterface, AdvanceToIndirectReversed) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    BSONObj unusedKey = key0;  // smaller than any inserted key

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc2), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key5, loc3), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(3, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(
            sorted->newCursor(opCtx.get(), false));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key5, false, true)),
                  IndexKeyEntry(key5, loc3));

        IndexSeekPoint seekPoint;
        seekPoint.prefixLen = 0;
        BSONElement suffix0;
        seekPoint.keySuffix = {&suffix0};
        seekPoint.firstExclusive = -1;

        suffix0 = key4.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key3, loc2));

        suffix0 = key2.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key1, loc1));
    }
}

// Insert multiple, non-consecutive, single-field keys and advance to
// each of them using a forward cursor by specifying a key between their
// exact key and the current position of the cursor. When advanceTo()
// is called in non-inclusive mode, the cursor is positioned at the key
// that comes after the one specified.
TEST(SortedDataInterface, AdvanceToIndirectExclusive) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    BSONObj unusedKey = key6;  // larger than any inserted key

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc2), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key5, loc3), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(3, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, true, true)),
                  IndexKeyEntry(key1, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.prefixLen = 0;
        BSONElement suffix0;
        seekPoint.keySuffix = {&suffix0};
        seekPoint.firstExclusive = 0;

        suffix0 = key2.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key3, loc2));

        suffix0 = key4.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key5, loc3));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key1, true, true)),
                  IndexKeyEntry(key1, loc1));

        suffix0 = key3.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(key5, loc3));
    }
}

// Insert multiple, non-consecutive, single-field keys and advance to
// each of them using a reverse cursor by specifying a key between their
// exact key and the current position of the cursor. When advanceTo()
// is called in non-inclusive mode, the cursor is positioned at the key
// that comes before the one specified.
TEST(SortedDataInterface, AdvanceToIndirectExclusiveReversed) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    BSONObj unusedKey = key0;  // smaller than any inserted key

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc2), true));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key5, loc3), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(3, sorted->numEntries(opCtx.get()));
    }

    {
        bool isForward = false;
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(
            sorted->newCursor(opCtx.get(), isForward));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key5, isForward, true)),
                  IndexKeyEntry(key5, loc3));

        IndexSeekPoint seekPoint;
        seekPoint.prefixLen = 0;
        BSONElement suffix0;
        seekPoint.keySuffix = {&suffix0};
        seekPoint.firstExclusive = 0;

        suffix0 = key4.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key3, loc2));

        suffix0 = key2.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key1, loc1));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), key5, isForward, true)),
                  IndexKeyEntry(key5, loc3));

        suffix0 = key3.firstElement();
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), isForward)),
                  IndexKeyEntry(key1, loc1));
    }
}

// Insert multiple two-field keys and advance to each of them using a forward cursor by specifying
// their exact key. When advanceTo() is called on a duplicate key, the cursor is positioned at the
// first occurrence of that key in ascending order by RecordId.
TEST(SortedDataInterface, AdvanceToCompoundWithPrefixAndSuffixInclusive) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1a, loc1), true));
            ASSERT_OK(sorted->insert(opCtx.get(),
                                     makeKeyString(sorted.get(), compoundKey1a, loc2),
                                     true /* allow duplicates */));
            ASSERT_OK(sorted->insert(opCtx.get(),
                                     makeKeyString(sorted.get(), compoundKey1a, loc3),
                                     true /* allow duplicates */));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey2b, loc4), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey3b, loc5), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), compoundKey1a, true, true)),
                  IndexKeyEntry(compoundKey1a, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = compoundKey1a;
        seekPoint.prefixLen = 1;  // Get first field from the prefix
        std::vector<BSONElement> suffix;
        compoundKey1a.elems(suffix);
        seekPoint.keySuffix = {&suffix[0], &suffix[1]};
        seekPoint.firstExclusive = -1;  // Get second field from the suffix, no exclusive fields

        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey1a, loc1));

        seekPoint.keyPrefix = compoundKey2b;
        suffix.clear();
        compoundKey2b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey2b, loc4));


        seekPoint.keyPrefix = compoundKey3b;
        suffix.clear();
        compoundKey3b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey3b, loc5));

        seekPoint.keyPrefix = compoundKey3c;
        suffix.clear();
        compoundKey3c.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);
    }
}

// Insert multiple two-field keys and advance to each of them using a forward cursor by specifying a
// key that comes before. When advanceTo() is called in non-inclusive mode, the cursor is positioned
// at the key that comes after the one specified. When dealing with prefixes, that means that any
// keys that match on the prefix are skipped.
TEST(SortedDataInterface, AdvanceToCompoundWithPrefixExclusive) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1a, loc1), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1b, loc2), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1c, loc3), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey2b, loc4), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey3b, loc5), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), compoundKey1a, true, true)),
                  IndexKeyEntry(compoundKey1a, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = compoundKey1a;
        seekPoint.prefixLen = 1;  // Get first field from prefix
        std::vector<BSONElement> suffix;
        compoundKey1a.elems(suffix);
        seekPoint.keySuffix = {&suffix[0], &suffix[1]};
        seekPoint.firstExclusive = 0;  // Ignore the suffix, make prefix exclusive

        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey2b, loc4));

        seekPoint.keyPrefix = compoundKey2b;
        suffix.clear();
        compoundKey2b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey3b, loc5));


        seekPoint.keyPrefix = compoundKey3b;
        suffix.clear();
        compoundKey3b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);

        seekPoint.keyPrefix = compoundKey3c;
        suffix.clear();
        compoundKey3c.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);
    }
}

// Insert multiple two-field keys and advance to each of them using a forward cursor by specifying a
// key that comes before. When advanceTo() is called in non-inclusive mode, the cursor is positioned
// at the key that comes after the one specified.
TEST(SortedDataInterface, AdvanceToCompoundWithPrefixAndSuffixExclusive) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1a, loc1), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1b, loc2), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1c, loc3), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey2b, loc4), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey3b, loc5), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), compoundKey1a, true, true)),
                  IndexKeyEntry(compoundKey1a, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = compoundKey1a;
        seekPoint.prefixLen = 1;  // Get first field from the prefix
        std::vector<BSONElement> suffix;
        compoundKey1a.elems(suffix);
        seekPoint.keySuffix = {&suffix[0], &suffix[1]};
        seekPoint.firstExclusive = 1;  // Get second field from suffix, make it exclusive

        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey1b, loc2));

        seekPoint.keyPrefix = compoundKey2b;
        suffix.clear();
        compoundKey2b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey3b, loc5));


        seekPoint.keyPrefix = compoundKey3b;
        suffix.clear();
        compoundKey3b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);

        seekPoint.keyPrefix = compoundKey3c;
        suffix.clear();
        compoundKey3c.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);
    }
}

// Insert multiple two-field keys and advance to each of them using a forward cursor by specifying a
// key that comes before. When advanceTo() is called in non-inclusive mode, the cursor is positioned
// at the key that comes after the one specified.
TEST(SortedDataInterface, AdvanceToCompoundWithSuffixExclusive) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/false, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1a, loc1), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1b, loc2), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey1c, loc3), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey2b, loc4), true));
            ASSERT_OK(sorted->insert(
                opCtx.get(), makeKeyString(sorted.get(), compoundKey3b, loc5), true));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT_EQUALS(5, sorted->numEntries(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        const std::unique_ptr<SortedDataInterface::Cursor> cursor(sorted->newCursor(opCtx.get()));

        ASSERT_EQ(cursor->seek(makeKeyStringForSeek(sorted.get(), compoundKey1a, true, true)),
                  IndexKeyEntry(compoundKey1a, loc1));

        IndexSeekPoint seekPoint;
        seekPoint.keyPrefix = compoundKey1a;
        seekPoint.prefixLen = 0;  // Ignore the prefix
        std::vector<BSONElement> suffix;
        compoundKey1a.elems(suffix);
        seekPoint.keySuffix = {&suffix[0], &suffix[1]};
        seekPoint.firstExclusive = 1;  // Get both fields from the suffix, make the second exclusive

        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey1b, loc2));

        seekPoint.keyPrefix = compoundKey2b;
        suffix.clear();
        compoundKey2b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  IndexKeyEntry(compoundKey3b, loc5));


        seekPoint.keyPrefix = compoundKey3b;
        suffix.clear();
        compoundKey3b.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);

        seekPoint.keyPrefix = compoundKey3c;
        suffix.clear();
        compoundKey3c.elems(suffix);
        ASSERT_EQ(cursor->seek(IndexEntryComparison::makeKeyStringFromSeekPointForSeek(
                      seekPoint, sorted->getKeyStringVersion(), sorted->getOrdering(), true)),
                  boost::none);
    }
}
}  // namespace
}  // namespace mongo
