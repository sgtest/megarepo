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


#include <fmt/format.h>
#include <string>

#include <wiredtiger.h>

#include "mongo/db/repl/repl_settings.h"
#include "mongo/db/repl_set_member_in_standalone_mode.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_begin_transaction_block.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_compiled_configuration.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_util.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage


namespace mongo {
using namespace fmt::literals;

static CompiledConfiguration compiledBeginTransaction(
    "WT_SESSION.begin_transaction",
    "ignore_prepare=%s,roundup_timestamps=(prepared=%d,read=%d),no_timestamp=%d");

WiredTigerBeginTxnBlock::WiredTigerBeginTxnBlock(
    WiredTigerSession* session,
    PrepareConflictBehavior prepareConflictBehavior,
    RoundUpPreparedTimestamps roundUpPreparedTimestamps,
    RoundUpReadTimestamp roundUpReadTimestamp,
    RecoveryUnit::UntimestampedWriteAssertionLevel allowUntimestampedWrite)
    : _session(session) {
    invariant(!_rollback);
    _wt_session = _session->getSession();

    const char* compiled_config = nullptr;
    // Only create a bound configuration string if we have non-default options.
    if (prepareConflictBehavior == PrepareConflictBehavior::kIgnoreConflicts ||
        prepareConflictBehavior == PrepareConflictBehavior::kIgnoreConflictsAllowWrites ||
        roundUpPreparedTimestamps == RoundUpPreparedTimestamps::kRound ||
        roundUpReadTimestamp == RoundUpReadTimestamp::kRound ||
        allowUntimestampedWrite != RecoveryUnit::UntimestampedWriteAssertionLevel::kEnforce ||
        MONGO_unlikely(gAllowUnsafeUntimestampedWrites &&
                       getReplSetMemberInStandaloneMode(getGlobalServiceContext()) &&
                       !repl::ReplSettings::shouldRecoverFromOplogAsStandalone())) {
        const char* ignore_prepare;
        if (prepareConflictBehavior == PrepareConflictBehavior::kIgnoreConflicts) {
            ignore_prepare = "true";
        } else if (prepareConflictBehavior ==
                   PrepareConflictBehavior::kIgnoreConflictsAllowWrites) {
            ignore_prepare = "force";
        } else {
            ignore_prepare = "false";
        }

        bool roundup_prepared = false;
        if (roundUpPreparedTimestamps == RoundUpPreparedTimestamps::kRound) {
            roundup_prepared = true;
        }

        bool roundup_read = false;
        if (roundUpReadTimestamp == RoundUpReadTimestamp::kRound) {
            roundup_read = true;
        }

        bool no_timestamp = false;
        if (allowUntimestampedWrite != RecoveryUnit::UntimestampedWriteAssertionLevel::kEnforce) {
            no_timestamp = true;
        } else if (MONGO_unlikely(gAllowUnsafeUntimestampedWrites &&
                                  getReplSetMemberInStandaloneMode(getGlobalServiceContext()) &&
                                  !repl::ReplSettings::shouldRecoverFromOplogAsStandalone())) {
            // We can safely ignore setting this configuration option when recovering from the
            // oplog as standalone because:
            // 1. Replaying oplog entries write with a timestamp.
            // 2. The instance is put in read-only mode after oplog application has finished.
            no_timestamp = true;
        }

        compiled_config = compiledBeginTransaction.getConfig(_session);
        invariantWTOK(_wt_session->bind_configuration(_wt_session,
                                                      compiled_config,
                                                      ignore_prepare,
                                                      (int)roundup_prepared,
                                                      (int)roundup_read,
                                                      (int)no_timestamp),
                      _wt_session);
    }
    invariantWTOK(_wt_session->begin_transaction(_wt_session, compiled_config), _wt_session);
    _rollback = true;
}

WiredTigerBeginTxnBlock::WiredTigerBeginTxnBlock(WiredTigerSession* session, const char* config)
    : _session(session) {
    invariant(!_rollback);
    _wt_session = _session->getSession();
    invariantWTOK(_wt_session->begin_transaction(_wt_session, config), _wt_session);
    _rollback = true;
}

WiredTigerBeginTxnBlock::~WiredTigerBeginTxnBlock() {
    if (_rollback) {
        invariant(_wt_session->rollback_transaction(_wt_session, nullptr) == 0);
    }
}

Status WiredTigerBeginTxnBlock::setReadSnapshot(Timestamp readTimestamp) {
    invariant(_rollback);
    return wtRCToStatus(_wt_session->timestamp_transaction_uint(
                            _wt_session, WT_TS_TXN_TYPE_READ, readTimestamp.asULL()),
                        _wt_session);
}

void WiredTigerBeginTxnBlock::done() {
    invariant(_rollback);
    _rollback = false;
}

}  // namespace mongo
