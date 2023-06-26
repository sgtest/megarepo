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

#include "mongo/platform/basic.h"

#include <memory>

#include "mongo/base/init.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/json.h"
#include "mongo/db/storage/sorted_data_interface_test_harness.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_column_store.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_record_store.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_recovery_unit.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_session_cache.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_util.h"
#include "mongo/unittest/temp_dir.h"
#include "mongo/unittest/unittest.h"

namespace mongo {
namespace {

using std::string;

TEST(WiredTigerColumnStoreTest, MakeKey) {
    std::string out = WiredTigerColumnStore::makeKey_ForTest("a.b", 66 /* RowId */);

    //                     a  .  b  \0
    //                                 < Big Endian encoding of the number 27 in uint 64>
    const auto expected = "61 2e 62 00 00 00 00 00 00 00 00 42";
    ASSERT_EQ(expected, hexdump(out.data(), out.size()));
}

TEST(WiredTigerColumnStoreTest, MakeKeyRIDColumn) {
    std::string out = WiredTigerColumnStore::makeKey_ForTest("\xFF", 256 /* RowId */);

    // For the special path 0xff, we do not encode a NUL terminator.

    //                   0xff
    //                        < Big Endian encoding of the number 256 in uint 64>
    const auto expected = "ff 00 00 00 00 00 00 01 00";

    ASSERT_EQ(expected, hexdump(out.data(), out.size()));
}
// TODO: SERVER-64257 Add tests for WT config string.
}  // namespace
}  // namespace mongo
