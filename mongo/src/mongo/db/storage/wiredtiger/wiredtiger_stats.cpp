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

#include "mongo/db/storage/wiredtiger/wiredtiger_stats.h"

#include <cstdint>
#include <utility>

#include <absl/container/node_hash_map.h>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <wiredtiger.h>

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_util.h"
#include "mongo/stdx/unordered_map.h"
#include "mongo/util/assert_util.h"

namespace mongo {
namespace {

enum class StatType { kData, kWait };

struct StatInfo {
    StringData name;
    StatType type;
};

const stdx::unordered_map<int, StatInfo> kWiredTigerStatCodeToStatInfo = {
    {WT_STAT_SESSION_BYTES_READ, {"bytesRead"_sd, StatType::kData}},
    {WT_STAT_SESSION_BYTES_WRITE, {"bytesWritten"_sd, StatType::kData}},
    {WT_STAT_SESSION_LOCK_DHANDLE_WAIT, {"handleLock"_sd, StatType::kWait}},
    {WT_STAT_SESSION_READ_TIME, {"timeReadingMicros"_sd, StatType::kData}},
    {WT_STAT_SESSION_WRITE_TIME, {"timeWritingMicros"_sd, StatType::kData}},
    {WT_STAT_SESSION_LOCK_SCHEMA_WAIT, {"schemaLock"_sd, StatType::kWait}},
    {WT_STAT_SESSION_CACHE_TIME, {"cache"_sd, StatType::kWait}}};

}  // namespace

WiredTigerStats::WiredTigerStats(WT_SESSION* session) {
    invariant(session);

    WT_CURSOR* c;
    uassert(ErrorCodes::CursorNotFound,
            "Unable to open statistics cursor",
            !session->open_cursor(session, "statistics:session", nullptr, "statistics=(fast)", &c));

    ScopeGuard guard{[c] {
        c->close(c);
    }};

    int32_t key;
    uint64_t value;
    while (c->next(c) == 0 && c->get_key(c, &key) == 0) {
        fassert(51035, c->get_value(c, nullptr, nullptr, &value) == 0);
        _stats[key] = WiredTigerUtil::castStatisticsValue<long long>(value);
    }
}

BSONObj WiredTigerStats::toBSON() const {
    boost::optional<BSONObjBuilder> dataSection;
    boost::optional<BSONObjBuilder> waitSection;

    for (auto&& [stat, value] : _stats) {
        if (value == 0) {
            continue;
        }

        auto it = kWiredTigerStatCodeToStatInfo.find(stat);
        if (it == kWiredTigerStatCodeToStatInfo.end()) {
            continue;
        }
        auto&& [name, type] = it->second;

        auto appendToSection = [name = name,
                                value = value](boost::optional<BSONObjBuilder>& section) {
            if (!section) {
                section.emplace();
            }
            section->append(name, value);
        };

        switch (type) {
            case StatType::kData:
                appendToSection(dataSection);
                break;
            case StatType::kWait:
                appendToSection(waitSection);
                break;
        }
    }

    BSONObjBuilder builder;
    if (dataSection) {
        builder.append("data", dataSection->obj());
    }
    if (waitSection) {
        builder.append("timeWaitingMicros", waitSection->obj());
    }

    return builder.obj();
}

std::unique_ptr<StorageStats> WiredTigerStats::clone() const {
    return std::make_unique<WiredTigerStats>(*this);
}

WiredTigerStats& WiredTigerStats::operator=(WiredTigerStats&& other) {
    _stats = std::move(other._stats);
    return *this;
}

WiredTigerStats& WiredTigerStats::operator+=(const WiredTigerStats& other) {
    for (auto&& [stat, value] : other._stats) {
        _stats[stat] += value;
    }
    return *this;
}

StorageStats& WiredTigerStats::operator+=(const StorageStats& other) {
    return *this += checked_cast<const WiredTigerStats&>(other);
}

WiredTigerStats& WiredTigerStats::operator-=(const WiredTigerStats& other) {
    for (auto const& otherStat : other._stats) {
        _stats[otherStat.first] -= otherStat.second;
    }
    return (*this);
}

StorageStats& WiredTigerStats::operator-=(const StorageStats& other) {
    *this -= checked_cast<const WiredTigerStats&>(other);
    return (*this);
}

}  // namespace mongo
