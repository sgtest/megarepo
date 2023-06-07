/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/logv2/log.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage


namespace mongo {
namespace {
// SnapshotIds need to be globally unique, as they are used in a WorkingSetMember to
// determine if documents changed, but a different recovery unit may be used across a getMore,
// so there is a chance the snapshot ID will be reused.
AtomicWord<unsigned long long> nextSnapshotId{1};
MONGO_FAIL_POINT_DEFINE(widenWUOWChangesWindow);

SnapshotId getNextSnapshotId() {
    return SnapshotId(nextSnapshotId.fetchAndAdd(1));
}
}  // namespace

RecoveryUnit::RecoveryUnit() : _snapshot(getNextSnapshotId()) {}

RecoveryUnit::Snapshot& RecoveryUnit::getSnapshot() {
    return _snapshot.get();
}

void RecoveryUnit::assignNextSnapshot() {
    // The current snapshot's destructor will be called first, followed by the constructors for the
    // next snapshot.
    _snapshot.emplace(getNextSnapshotId());
}

void RecoveryUnit::registerPreCommitHook(std::function<void(OperationContext*)> callback) {
    _preCommitHooks.push_back(std::move(callback));
}

void RecoveryUnit::runPreCommitHooks(OperationContext* opCtx) {
    ON_BLOCK_EXIT([&] { _preCommitHooks.clear(); });
    for (auto& hook : _preCommitHooks) {
        hook(opCtx);
    }
}

void RecoveryUnit::registerChange(std::unique_ptr<Change> change) {
    validateInUnitOfWork();
    _changes.push_back(std::move(change));
}

void RecoveryUnit::registerChangeForCatalogVisibility(std::unique_ptr<Change> change) {
    validateInUnitOfWork();
    invariant(!_changeForCatalogVisibility);
    _changeForCatalogVisibility = std::move(change);
}

bool RecoveryUnit::hasRegisteredChangeForCatalogVisibility() {
    validateInUnitOfWork();
    return _changeForCatalogVisibility.get();
}

void RecoveryUnit::commitRegisteredChanges(boost::optional<Timestamp> commitTimestamp) {
    // Getting to this method implies `runPreCommitHooks` completed successfully, resulting in
    // having its contents cleared.
    invariant(_preCommitHooks.empty());
    if (MONGO_unlikely(widenWUOWChangesWindow.shouldFail())) {
        sleepmillis(1000);
    }
    _executeCommitHandlers(commitTimestamp);
}

void RecoveryUnit::beginUnitOfWork(bool readOnly) {
    _readOnly = readOnly;
    if (!_readOnly) {
        doBeginUnitOfWork();
    }
}

void RecoveryUnit::commitUnitOfWork() {
    invariant(!_readOnly);
    doCommitUnitOfWork();
    assignNextSnapshot();
}

void RecoveryUnit::abortUnitOfWork() {
    invariant(!_readOnly);
    doAbortUnitOfWork();
    assignNextSnapshot();
}

void RecoveryUnit::endReadOnlyUnitOfWork() {
    _readOnly = false;
}

void RecoveryUnit::abandonSnapshot() {
    doAbandonSnapshot();
    assignNextSnapshot();
}

void RecoveryUnit::setOperationContext(OperationContext* opCtx) {
    _opCtx = opCtx;
}

void RecoveryUnit::_executeCommitHandlers(boost::optional<Timestamp> commitTimestamp) {
    invariant(_opCtx);
    for (auto& change : _changes) {
        try {
            // Log at higher level because commits occur far more frequently than rollbacks.
            LOGV2_DEBUG(22244,
                        3,
                        "Custom commit",
                        "changeName"_attr = redact(demangleName(typeid(*change))));
            change->commit(_opCtx, commitTimestamp);
        } catch (...) {
            std::terminate();
        }
    }
    try {
        if (_changeForCatalogVisibility) {
            // Log at higher level because commits occur far more frequently than rollbacks.
            LOGV2_DEBUG(5255701,
                        2,
                        "Custom commit",
                        "changeName"_attr =
                            redact(demangleName(typeid(*_changeForCatalogVisibility))));
            _changeForCatalogVisibility->commit(_opCtx, commitTimestamp);
        }
    } catch (...) {
        std::terminate();
    }
    _changes.clear();
    _changeForCatalogVisibility.reset();
}

void RecoveryUnit::abortRegisteredChanges() {
    _preCommitHooks.clear();
    if (MONGO_unlikely(widenWUOWChangesWindow.shouldFail())) {
        sleepmillis(1000);
    }
    _executeRollbackHandlers();
}
void RecoveryUnit::_executeRollbackHandlers() {
    // Make sure we have an OperationContext when executing rollback handlers. Unless we have no
    // handlers to run, which might be the case in unit tests.
    invariant(_opCtx || (_changes.empty() && !_changeForCatalogVisibility));
    try {
        if (_changeForCatalogVisibility) {
            LOGV2_DEBUG(5255702,
                        2,
                        "Custom rollback",
                        "changeName"_attr =
                            redact(demangleName(typeid(*_changeForCatalogVisibility))));
            _changeForCatalogVisibility->rollback(_opCtx);
        }
        for (Changes::const_reverse_iterator it = _changes.rbegin(), end = _changes.rend();
             it != end;
             ++it) {
            Change* change = it->get();
            LOGV2_DEBUG(22245,
                        2,
                        "Custom rollback",
                        "changeName"_attr = redact(demangleName(typeid(*change))));
            change->rollback(_opCtx);
        }
        _changeForCatalogVisibility.reset();
        _changes.clear();
    } catch (...) {
        std::terminate();
    }
}

void RecoveryUnit::_setState(State newState) {
    _state = newState;
}

void RecoveryUnit::validateInUnitOfWork() const {
    invariant(_inUnitOfWork() || _readOnly,
              fmt::format("state: {}, readOnly: {}", toString(_getState()), _readOnly));
}

}  // namespace mongo
