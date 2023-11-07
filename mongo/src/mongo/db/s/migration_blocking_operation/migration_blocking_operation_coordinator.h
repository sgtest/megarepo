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

#include "mongo/db/repl/primary_only_service.h"
#include "mongo/db/s/migration_blocking_operation/migration_blocking_operation_coordinator_gen.h"
#include "mongo/db/s/sharding_ddl_coordinator.h"

namespace mongo {

class MigrationBlockingOperationCoordinatorImpl;

class MigrationBlockingOperationCoordinator
    : public RecoverableShardingDDLCoordinator<MigrationBlockingOperationCoordinatorDocument,
                                               MigrationBlockingOperationCoordinatorPhaseEnum> {
public:
    using Phase = MigrationBlockingOperationCoordinatorPhaseEnum;

    MigrationBlockingOperationCoordinator(ShardingDDLCoordinatorService* service,
                                          const BSONObj& initialState);

    virtual void checkIfOptionsConflict(const BSONObj& stateDoc) const override;

private:
    virtual StringData serializePhase(const Phase& phase) const override;
    virtual ExecutorFuture<void> _runImpl(std::shared_ptr<executor::ScopedTaskExecutor> executor,
                                          const CancellationToken& token) noexcept override;

    std::unique_ptr<MigrationBlockingOperationCoordinatorImpl> _impl;
};

class MigrationBlockingOperationCoordinatorImpl {
public:
    SemiFuture<void> run(std::shared_ptr<executor::ScopedTaskExecutor> executor,
                         const CancellationToken& token);

private:
    SharedPromise<void> _completionPromise;
};

}  // namespace mongo
