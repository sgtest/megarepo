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

#include "mongo/util/debug_util.h"
#include <cstdint>
#include <utility>

#include <absl/container/node_hash_map.h>
#include <boost/optional/optional.hpp>
#include <wiredtiger.h>

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_util.h"
#include "mongo/util/assert_util.h"

namespace mongo {

namespace {

uint64_t getNextStat(WT_CURSOR* c, int32_t key_id) {

    int32_t key;
    invariant(c->next(c) == 0);
    if (kDebugBuild) {
        invariant(c->get_key(c, &key) == 0);
        invariant(key == key_id);
    }

    uint64_t value;
    fassert(51035, c->get_value(c, nullptr, nullptr, &value) == 0);

    return WiredTigerUtil::castStatisticsValue<long long>(value);
}

void appendIfNonZero(StringData fieldName, int64_t value, BSONObjBuilder* builder) {
    if (value != 0) {
        builder->append(fieldName, value);
    }
}

}  // namespace

WiredTigerStats::WiredTigerStats(WT_SESSION* session) {
    invariant(session);

    // TODO SERVER-88233: stats cursors are expensive to fetch and consider caching them
    WT_CURSOR* c;
    uassert(ErrorCodes::CursorNotFound,
            "Unable to open statistics cursor",
            !session->open_cursor(session, "statistics:session", nullptr, "statistics=(fast)", &c));

    ScopeGuard guard{[c] {
        c->close(c);
    }};


    // Get all the stats
    // Take advantage of the fact that WT returns in fixed order to elide some function calls.
    bytes_read = getNextStat(c, WT_STAT_SESSION_BYTES_READ);
    bytes_write = getNextStat(c, WT_STAT_SESSION_BYTES_WRITE);
    lock_dhandle_wait = getNextStat(c, WT_STAT_SESSION_LOCK_DHANDLE_WAIT);
    txn_bytes_dirty = getNextStat(c, WT_STAT_SESSION_TXN_BYTES_DIRTY);
    read_time = getNextStat(c, WT_STAT_SESSION_READ_TIME);
    write_time = getNextStat(c, WT_STAT_SESSION_WRITE_TIME);
    lock_schema_wait = getNextStat(c, WT_STAT_SESSION_LOCK_SCHEMA_WAIT);
    cache_time = getNextStat(c, WT_STAT_SESSION_CACHE_TIME);


    // Assert we have reached the end of the list of stats. If this assert triggers, it means WT
    // added a new one.
    dassert(c->next(c) != 0);
}

BSONObj WiredTigerStats::toBSON() const {

    BSONObjBuilder builder;

    // Only output metrics for non-zero values
    if (bytes_read != 0 || bytes_write != 0 || read_time != 0 || write_time != 0 ||
        txn_bytes_dirty != 0) {
        BSONObjBuilder dataSection(builder.subobjStart("data"));
        appendIfNonZero("bytesRead", bytes_read, &dataSection);
        appendIfNonZero("bytesWritten", bytes_write, &dataSection);
        appendIfNonZero("timeReadingMicros", read_time, &dataSection);
        appendIfNonZero("timeWritingMicros", write_time, &dataSection);
        appendIfNonZero("txnBytesDirty", txn_bytes_dirty, &dataSection);
    }

    if (lock_dhandle_wait != 0 || lock_schema_wait != 0 || cache_time != 0) {
        BSONObjBuilder waitingSection(builder.subobjStart("timeWaitingMicros"));
        appendIfNonZero("handleLock", lock_dhandle_wait, &waitingSection);
        appendIfNonZero("schemaLock", lock_schema_wait, &waitingSection);
        appendIfNonZero("cache", cache_time, &waitingSection);
    }

    return builder.obj();
}

std::unique_ptr<StorageStats> WiredTigerStats::clone() const {
    return std::make_unique<WiredTigerStats>(*this);
}

WiredTigerStats& WiredTigerStats::operator=(WiredTigerStats&& other) {
    bytes_read = other.bytes_read;
    bytes_write = other.bytes_write;
    lock_dhandle_wait = other.lock_dhandle_wait;
    txn_bytes_dirty = other.txn_bytes_dirty;
    read_time = other.read_time;
    write_time = other.write_time;
    lock_schema_wait = other.lock_schema_wait;
    cache_time = other.cache_time;

    return *this;
}

WiredTigerStats& WiredTigerStats::operator+=(const WiredTigerStats& other) {
    bytes_read += other.bytes_read;
    bytes_write += other.bytes_write;
    lock_dhandle_wait += other.lock_dhandle_wait;
    txn_bytes_dirty += other.txn_bytes_dirty;
    read_time += other.read_time;
    write_time += other.write_time;
    lock_schema_wait += other.lock_schema_wait;
    cache_time += other.cache_time;

    return *this;
}

StorageStats& WiredTigerStats::operator+=(const StorageStats& other) {
    return *this += checked_cast<const WiredTigerStats&>(other);
}

WiredTigerStats& WiredTigerStats::operator-=(const WiredTigerStats& other) {
    bytes_read -= other.bytes_read;
    bytes_write -= other.bytes_write;
    lock_dhandle_wait -= other.lock_dhandle_wait;
    txn_bytes_dirty -= other.txn_bytes_dirty;
    read_time -= other.read_time;
    write_time -= other.write_time;
    lock_schema_wait -= other.lock_schema_wait;
    cache_time -= other.cache_time;

    return (*this);
}

StorageStats& WiredTigerStats::operator-=(const StorageStats& other) {
    *this -= checked_cast<const WiredTigerStats&>(other);
    return (*this);
}

}  // namespace mongo
