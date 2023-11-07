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

#include "mongo/db/s/migration_blocking_operation/migration_blocking_operation_coordinator.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {

MigrationBlockingOperationCoordinator::MigrationBlockingOperationCoordinator(
    ShardingDDLCoordinatorService* service, const BSONObj& initialState)
    : RecoverableShardingDDLCoordinator(
          service, "MigrationBlockingOperationCoordinator", initialState),
      _impl{std::make_unique<MigrationBlockingOperationCoordinatorImpl>()} {}


void MigrationBlockingOperationCoordinator::checkIfOptionsConflict(const BSONObj& stateDoc) const {}

StringData MigrationBlockingOperationCoordinator::serializePhase(const Phase& phase) const {
    return MigrationBlockingOperationCoordinatorPhase_serializer(phase);
}

ExecutorFuture<void> MigrationBlockingOperationCoordinator::_runImpl(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    const CancellationToken& token) noexcept {
    return _impl->run(executor, token).thenRunOn(**executor);
}

SemiFuture<void> MigrationBlockingOperationCoordinatorImpl::run(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    _completionPromise.emplaceValue();
    return _completionPromise.getFuture().semi();
}

}  // namespace mongo
