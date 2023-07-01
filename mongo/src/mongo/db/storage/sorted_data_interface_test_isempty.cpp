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

#include <boost/move/utility_core.hpp>
#include <memory>

#include "mongo/base/string_data.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/record_id.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/sorted_data_interface.h"
#include "mongo/db/storage/sorted_data_interface_test_harness.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo {
namespace {

// Verify that isEmpty() returns true when the index is empty,
// returns false when a key is inserted, and returns true again
// when that is unindex.
TEST(SortedDataInterface, IsEmpty) {
    const auto harnessHelper(newSortedDataInterfaceHarnessHelper());
    const std::unique_ptr<SortedDataInterface> sorted(
        harnessHelper->newSortedDataInterface(/*unique=*/true, /*partial=*/false));

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        Lock::GlobalLock globalLock(opCtx.get(), MODE_X);
        {
            WriteUnitOfWork uow(opCtx.get());
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), false));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key2, loc2), false));
            ASSERT_OK(sorted->insert(opCtx.get(), makeKeyString(sorted.get(), key3, loc3), false));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(!sorted->isEmpty(opCtx.get()));
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        Lock::GlobalLock globalLock(opCtx.get(), MODE_X);
        {
            WriteUnitOfWork uow(opCtx.get());
            sorted->unindex(opCtx.get(), makeKeyString(sorted.get(), key1, loc1), false);
            sorted->unindex(opCtx.get(), makeKeyString(sorted.get(), key2, loc2), false);
            sorted->unindex(opCtx.get(), makeKeyString(sorted.get(), key3, loc3), false);
            ASSERT(sorted->isEmpty(opCtx.get()));
            uow.commit();
        }
    }

    {
        const ServiceContext::UniqueOperationContext opCtx(harnessHelper->newOperationContext());
        ASSERT(sorted->isEmpty(opCtx.get()));
    }
}

}  // namespace
}  // namespace mongo
