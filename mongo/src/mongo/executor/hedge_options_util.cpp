/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include "mongo/executor/hedge_options_util.h"

#include <algorithm>
#include <array>
#include <string>

#include <boost/optional/optional.hpp>

#include "mongo/client/hedging_mode_gen.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/s/mongos_server_parameters.h"
#include "mongo/s/mongos_server_parameters_gen.h"
#include "mongo/util/ctype.h"
#include "mongo/util/sort.h"

namespace mongo {
MONGO_FAIL_POINT_DEFINE(hedgedReadsSendRequestsToTargetHostsInAlphabeticalOrder);
namespace {
// Only do hedging for commands that cannot trigger writes.
constexpr std::array hedgeCommands{
    "collStats"_sd,
    "count"_sd,
    "dataSize"_sd,
    "dbStats"_sd,
    "distinct"_sd,
    "filemd5"_sd,
    "find"_sd,
    "listCollections"_sd,
    "listIndexes"_sd,
    "planCacheListFilters"_sd,
};

static_assert(constexprIsSorted(hedgeCommands.begin(), hedgeCommands.end()));

bool commandCanHedge(StringData command) {
    return std::binary_search(hedgeCommands.begin(), hedgeCommands.end(), command);
}

bool commandShouldHedge(StringData command, const ReadPreferenceSetting& readPref) {
    if (gReadHedgingMode.load() != ReadHedgingMode::kOn) {
        return false;  // Hedging is globally disabled.
    }
    auto&& mode = readPref.hedgingMode;
    if (!mode || !mode->getEnabled()) {
        return false;  // The read preference didn't enable hedging.
    }
    return commandCanHedge(command);
}

template <typename IA, typename IB, typename F>
int compareTransformed(IA a1, IA a2, IB b1, IB b2, F&& f) {
    for (;; ++a1, ++b1)
        if (a1 == a2)
            return b1 == b2 ? 0 : -1;
        else if (b1 == b2)
            return 1;
        else if (int r = f(*a1) - f(*b1))
            return r;
}
}  // namespace

bool compareByLowerHostThenPort(const HostAndPort& a, const HostAndPort& b) {
    const auto& ah = a.host();
    const auto& bh = b.host();
    if (int r = compareTransformed(
            ah.begin(), ah.end(), bh.begin(), bh.end(), [](auto&& c) { return ctype::toLower(c); }))
        return r < 0;
    return a.port() < b.port();
}

HedgeOptions getHedgeOptions(StringData command, const ReadPreferenceSetting& readPref) {
    bool shouldHedge = commandShouldHedge(command, readPref);
    size_t hedgeCount = shouldHedge ? 1 : 0;
    int maxTimeMSForHedgedReads = shouldHedge ? gMaxTimeMSForHedgedReads.load() : 0;
    return {shouldHedge, hedgeCount, maxTimeMSForHedgedReads};
}
}  // namespace mongo
