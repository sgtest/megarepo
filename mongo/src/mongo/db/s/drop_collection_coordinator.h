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

#pragma once

#include <boost/optional/optional.hpp>
#include <memory>
#include <string>

#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/drop_collection.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/ops/write_ops.h"
#include "mongo/db/s/drop_collection_coordinator_document_gen.h"
#include "mongo/db/s/sharding_ddl_coordinator.h"
#include "mongo/db/s/sharding_ddl_coordinator_gen.h"
#include "mongo/db/s/sharding_ddl_coordinator_service.h"
#include "mongo/executor/scoped_task_executor.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/cancellation.h"
#include "mongo/util/future.h"
#include "mongo/util/namespace_string_util.h"

namespace mongo {

class DropCollectionCoordinator final
    : public RecoverableShardingDDLCoordinator<DropCollectionCoordinatorDocument,
                                               DropCollectionCoordinatorPhaseEnum> {
public:
    using StateDoc = DropCollectionCoordinatorDocument;
    using Phase = DropCollectionCoordinatorPhaseEnum;

    DropCollectionCoordinator(ShardingDDLCoordinatorService* service, const BSONObj& initialState)
        : RecoverableShardingDDLCoordinator(service, "DropCollectionCoordinator", initialState),
          _critSecReason(BSON("command"
                              << "dropCollection"
                              << "ns"
                              << NamespaceStringUtil::serialize(
                                     originalNss(), SerializationContext::stateDefault()))) {}

    ~DropCollectionCoordinator() = default;

    void checkIfOptionsConflict(const BSONObj& doc) const final {}

    /**
     * Locally drops a collection, cleans its CollectionShardingRuntime metadata and refreshes the
     * catalog cache.
     * The oplog entry associated with the drop collection will be generated with the fromMigrate
     * flag.
     */
    static void dropCollectionLocally(OperationContext* opCtx,
                                      const NamespaceString& nss,
                                      bool fromMigrate);

private:
    const BSONObj _critSecReason;

    StringData serializePhase(const Phase& phase) const override {
        return DropCollectionCoordinatorPhase_serializer(phase);
    }

    bool _mustAlwaysMakeProgress() override {
        return _doc.getPhase() > Phase::kUnset;
    }

    ExecutorFuture<void> _runImpl(std::shared_ptr<executor::ScopedTaskExecutor> executor,
                                  const CancellationToken& token) noexcept override;

    void _checkPreconditionsAndSaveArgumentsOnDoc();

    void _freezeMigrations(std::shared_ptr<executor::ScopedTaskExecutor> executor);

    void _enterCriticalSection(std::shared_ptr<executor::ScopedTaskExecutor> executor,
                               const CancellationToken& token);

    void _commitDropCollection(std::shared_ptr<executor::ScopedTaskExecutor> executor);

    void _exitCriticalSection(std::shared_ptr<executor::ScopedTaskExecutor> executor,
                              const CancellationToken& token);
};

}  // namespace mongo
