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

#include "query_settings_hash.h"

#include <fmt/format.h>

#include "mongo/db/query/query_settings/query_settings_gen.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

namespace mongo::query_settings {

TEST(QuerySettingsHashTest, QuerySettingsHashIncludesRejection) {
    // Change reject in query settings, verify that the hash differs.
    QuerySettings settings;
    auto hashA = mongo::query_settings::hash(settings);

    settings.setReject(false);
    auto hashB = mongo::query_settings::hash(settings);

    settings.setReject(true);
    auto hashC = mongo::query_settings::hash(settings);

    ASSERT_EQ(hashA, hashB);
    ASSERT_NE(hashA, hashC);
}

TEST(QuerySettingsHashTest, QuerySettingsHashStability) {
    // Verify that the hash resulting from setting each query setting matches a "golden" value,
    // guarding stability of this hash.
    // Variation between versions is fine ()
    QuerySettings settings;
    settings.setQueryFramework(mongo::QueryFrameworkControlEnum::kForceClassicEngine);
    NamespaceSpec ns;
    ns.setDb(
        DatabaseNameUtil::deserialize(boost::none, "testDB", SerializationContext::stateDefault()));
    ns.setColl(StringData("testColl"));
    settings.setIndexHints({{IndexHintSpec(ns, {IndexHint("a_1")})}});
    settings.setReject(true);
    auto observedHash = mongo::query_settings::hash(settings);

    static const size_t expectedHash = 0xf9c438f33d810af8;

    ASSERT_EQ(observedHash, expectedHash)
        << fmt::format("{:#016x} != {:#016x}", observedHash, expectedHash);
}

}  // namespace mongo::query_settings
