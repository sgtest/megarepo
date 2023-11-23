/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#pragma once

#include <memory>

#include "mongo/db/concurrency/locker_impl.h"
#include "mongo/db/operation_context.h"

namespace mongo {
namespace shard_role_details {

/**
 * Interface for locking.  Caller DOES NOT own pointer.
 */
inline Locker* getLocker(OperationContext* opCtx) {
    return opCtx->lockState_DO_NOT_USE();
}

inline const Locker* getLocker(const OperationContext* opCtx) {
    return opCtx->lockState_DO_NOT_USE();
}

/**
 * Sets the locker for use by this OperationContext. Call during OperationContext initialization,
 * only.
 */
void setLocker(OperationContext* opCtx, std::unique_ptr<Locker> locker);

/**
 * Swaps the locker, releasing the old locker to the caller.
 * The Client lock is going to be acquired by this function.
 */
std::unique_ptr<Locker> swapLocker(OperationContext* opCtx, std::unique_ptr<Locker> newLocker);
std::unique_ptr<Locker> swapLocker(OperationContext* opCtx,
                                   std::unique_ptr<Locker> newLocker,
                                   WithLock lk);

/**
 * Dumps the contents of all locks to the log.
 */
void dumpLockManager();

}  // namespace shard_role_details
}  // namespace mongo
