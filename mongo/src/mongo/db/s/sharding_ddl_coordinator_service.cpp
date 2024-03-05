/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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


#include "mongo/db/s/sharding_ddl_coordinator_service.h"

#include <boost/smart_ptr.hpp>
#include <mutex>
#include <string>
#include <tuple>
#include <type_traits>
#include <utility>

#include <absl/container/node_hash_map.h>
#include <absl/meta/type_traits.h>
#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/client/dbclient_cursor.h"
#include "mongo/db/client.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/s/cleanup_structured_encryption_data_coordinator.h"
#include "mongo/db/s/collmod_coordinator.h"
#include "mongo/db/s/compact_structured_encryption_data_coordinator.h"
#include "mongo/db/s/convert_to_capped_coordinator.h"
#include "mongo/db/s/create_collection_coordinator.h"
#include "mongo/db/s/database_sharding_state.h"
#include "mongo/db/s/ddl_lock_manager.h"
#include "mongo/db/s/drop_collection_coordinator.h"
#include "mongo/db/s/drop_database_coordinator.h"
#include "mongo/db/s/forwardable_operation_metadata.h"
#include "mongo/db/s/migration_blocking_operation/migration_blocking_operation_coordinator.h"
#include "mongo/db/s/move_primary_coordinator.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/s/refine_collection_shard_key_coordinator.h"
#include "mongo/db/s/rename_collection_coordinator.h"
#include "mongo/db/s/reshard_collection_coordinator.h"
#include "mongo/db/s/set_allow_migrations_coordinator.h"
#include "mongo/db/s/sharding_ddl_coordinator.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/s/database_version.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding


namespace mongo {
namespace {

MONGO_FAIL_POINT_DEFINE(pauseShardingDDLCoordinatorServiceOnRecovery);

std::shared_ptr<ShardingDDLCoordinator> constructShardingDDLCoordinatorInstance(
    ShardingDDLCoordinatorService* service, BSONObj initialState) {
    const auto op = extractShardingDDLCoordinatorMetadata(initialState);
    LOGV2(
        5390510, "Constructing new sharding DDL coordinator", "coordinatorDoc"_attr = op.toBSON());
    switch (op.getId().getOperationType()) {
        case DDLCoordinatorTypeEnum::kMovePrimary:
            return std::make_shared<MovePrimaryCoordinator>(service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kDropDatabase:
            return std::make_shared<DropDatabaseCoordinator>(service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kDropCollection:
            return std::make_shared<DropCollectionCoordinator>(service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kRenameCollection:
            return std::make_shared<RenameCollectionCoordinator>(service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kCreateCollection:
            return std::make_shared<CreateCollectionCoordinator>(service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kRefineCollectionShardKeyPre71Compatible:
            return std::make_shared<RefineCollectionShardKeyCoordinatorPre71Compatible>(
                service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kCreateCollectionPre80Compatible:
            return std::make_shared<CreateCollectionCoordinatorLegacy>(service,
                                                                       std::move(initialState));
        case DDLCoordinatorTypeEnum::kRefineCollectionShardKey:
            return std::make_shared<RefineCollectionShardKeyCoordinator>(service,
                                                                         std::move(initialState));
        case DDLCoordinatorTypeEnum::kSetAllowMigrations:
            return std::make_shared<SetAllowMigrationsCoordinator>(service,
                                                                   std::move(initialState));
        case DDLCoordinatorTypeEnum::kCollMod:
            return std::make_shared<CollModCoordinator>(service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kReshardCollection:
            return std::make_shared<ReshardCollectionCoordinator>(service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kCompactStructuredEncryptionData:
            return std::make_shared<CompactStructuredEncryptionDataCoordinator>(
                service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kCleanupStructuredEncryptionData:
            return std::make_shared<CleanupStructuredEncryptionDataCoordinator>(
                service, std::move(initialState));
        case DDLCoordinatorTypeEnum::kMigrationBlockingOperation:
            return std::make_shared<MigrationBlockingOperationCoordinator>(service,
                                                                           std::move(initialState));
        case DDLCoordinatorTypeEnum::kConvertToCapped:
            return std::make_shared<ConvertToCappedCoordinator>(service, std::move(initialState));
        default:
            uasserted(ErrorCodes::BadValue,
                      str::stream()
                          << "Encountered unknown Sharding DDL operation type: "
                          << DDLCoordinatorType_serializer(op.getId().getOperationType()));
    }
}


}  // namespace

ShardingDDLCoordinatorService* ShardingDDLCoordinatorService::getService(OperationContext* opCtx) {
    auto registry = repl::PrimaryOnlyServiceRegistry::get(opCtx->getServiceContext());
    auto service = registry->lookupServiceByName(kServiceName);
    return checked_cast<ShardingDDLCoordinatorService*>(std::move(service));
}

std::shared_ptr<ShardingDDLCoordinatorService::Instance>
ShardingDDLCoordinatorService::constructInstance(BSONObj initialState) {
    auto coord = constructShardingDDLCoordinatorInstance(this, std::move(initialState));

    {
        stdx::lock_guard lg(_mutex);
        const auto it = _numActiveCoordinatorsPerType.find(coord->operationType());
        if (it != _numActiveCoordinatorsPerType.end()) {
            it->second++;
        } else {
            _numActiveCoordinatorsPerType.emplace(coord->operationType(), 1);
        }
    }

    pauseShardingDDLCoordinatorServiceOnRecovery.pauseWhileSet();

    coord->getConstructionCompletionFuture()
        .thenRunOn(getInstanceCleanupExecutor())
        .getAsync([this](auto status) {
            AllowOpCtxWhenServiceRebuildingBlock allowOpCtxBlock(Client::getCurrent());
            auto opCtx = cc().makeOperationContext();
            stdx::lock_guard lg(_mutex);
            if (_state != State::kRecovering) {
                return;
            }
            invariant(_numCoordinatorsToWait > 0);
            if (--_numCoordinatorsToWait == 0) {
                _transitionToRecovered(lg, opCtx.get());
            }
        });

    coord->getCompletionFuture()
        .thenRunOn(getInstanceCleanupExecutor())
        .getAsync([this, coordinatorType = coord->operationType()](auto status) {
            stdx::lock_guard lg(_mutex);
            const auto it = _numActiveCoordinatorsPerType.find(coordinatorType);
            invariant(it != _numActiveCoordinatorsPerType.end());
            it->second--;
            _recoveredOrCoordinatorCompletedCV.notify_all();
        });

    return coord;
}

std::shared_ptr<ShardingDDLCoordinatorExternalState>
ShardingDDLCoordinatorService::createExternalState() const {
    return _externalStateFactory->create();
}

void ShardingDDLCoordinatorService::waitForCoordinatorsOfGivenTypeToComplete(
    OperationContext* opCtx, DDLCoordinatorTypeEnum type) const {
    stdx::unique_lock lk(_mutex);
    opCtx->waitForConditionOrInterrupt(_recoveredOrCoordinatorCompletedCV, lk, [this, type]() {
        const auto it = _numActiveCoordinatorsPerType.find(type);
        return _state == State::kRecovered &&
            (it == _numActiveCoordinatorsPerType.end() || it->second == 0);
    });
}

void ShardingDDLCoordinatorService::waitForOngoingCoordinatorsToFinish(
    OperationContext* opCtx, std::function<bool(const ShardingDDLCoordinator&)> pred) {
    std::vector<SharedSemiFuture<void>> futuresToWait;

    const auto instances = getAllInstances(opCtx);
    for (const auto& instance : instances) {
        auto typedInstance = checked_pointer_cast<ShardingDDLCoordinator>(instance);
        if (pred(*typedInstance)) {
            futuresToWait.emplace_back(typedInstance->getCompletionFuture());
        }
    }

    for (auto&& future : futuresToWait) {
        future.wait(opCtx);
    }
}

void ShardingDDLCoordinatorService::_onServiceTermination() {
    stdx::lock_guard lg(_mutex);
    _state = State::kPaused;
    _numCoordinatorsToWait = 0;
    DDLLockManager::get(cc().getServiceContext())->setState(DDLLockManager::State::kPaused);
}

size_t ShardingDDLCoordinatorService::_countCoordinatorDocs(OperationContext* opCtx) {
    constexpr auto kNumCoordLabel = "numCoordinators"_sd;
    static const auto countStage = BSON("$count" << kNumCoordLabel);

    AggregateCommandRequest aggRequest{getStateDocumentsNS(), {countStage}};

    DBDirectClient client(opCtx);
    auto cursor = uassertStatusOKWithContext(
        DBClientCursor::fromAggregationRequest(
            &client, std::move(aggRequest), false /* secondaryOk */, true /* useExhaust */),
        "Failed to establish a cursor for aggregation");

    if (!cursor->more()) {
        return 0;
    }

    auto res = cursor->nextSafe();
    auto numCoordField = res.getField(kNumCoordLabel);
    invariant(numCoordField);
    return numCoordField.numberLong();
}

void ShardingDDLCoordinatorService::waitForRecoveryCompletion(OperationContext* opCtx) const {
    stdx::unique_lock lk(_mutex);
    opCtx->waitForConditionOrInterrupt(
        _recoveredOrCoordinatorCompletedCV, lk, [this]() { return _state == State::kRecovered; });
}

ExecutorFuture<void> ShardingDDLCoordinatorService::_rebuildService(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    return ExecutorFuture<void>(**executor)
        .then([this] {
            AllowOpCtxWhenServiceRebuildingBlock allowOpCtxBlock(Client::getCurrent());
            auto opCtx = cc().makeOperationContext();
            const auto numCoordinators = _countCoordinatorDocs(opCtx.get());
            if (numCoordinators > 0) {
                LOGV2(5622500,
                      "Found Sharding DDL Coordinators to rebuild",
                      "numCoordinators"_attr = numCoordinators);
            }
            if (numCoordinators > 0) {
                stdx::lock_guard lg(_mutex);
                _state = State::kRecovering;
                _numCoordinatorsToWait = numCoordinators;
            } else {
                pauseShardingDDLCoordinatorServiceOnRecovery.pauseWhileSet();
                stdx::lock_guard lg(_mutex);
                _transitionToRecovered(lg, opCtx.get());
            }
        })
        .onError([this](const Status& status) {
            LOGV2_ERROR(5469630,
                        "Failed to rebuild Sharding DDL coordinator service",
                        "error"_attr = status);
            return status;
        });
}

std::shared_ptr<ShardingDDLCoordinatorService::Instance>
ShardingDDLCoordinatorService::getOrCreateInstance(OperationContext* opCtx,
                                                   BSONObj coorDoc,
                                                   bool checkOptions) {

    // Wait for all coordinators to be recovered before to allow the creation of new ones.
    waitForRecoveryCompletion(opCtx);

    auto coorMetadata = extractShardingDDLCoordinatorMetadata(coorDoc);
    const auto& nss = coorMetadata.getId().getNss();

    if (!nss.isConfigDB() && !nss.isAdminDB()) {
        // Check that the operation context has a database version for this namespace
        const auto clientDbVersion = OperationShardingState::get(opCtx).getDbVersion(nss.dbName());
        uassert(ErrorCodes::IllegalOperation,
                "Request sent without attaching database version",
                clientDbVersion);
        {
            Lock::DBLock dbLock(opCtx, nss.dbName(), MODE_IS);
            const auto scopedDss =
                DatabaseShardingState::assertDbLockedAndAcquireShared(opCtx, nss.dbName());
            scopedDss->assertIsPrimaryShardForDb(opCtx);
        }
        coorMetadata.setDatabaseVersion(clientDbVersion);
    }

    coorMetadata.setForwardableOpMetadata(boost::optional<ForwardableOperationMetadata>(opCtx));
    const auto patchedCoorDoc = coorDoc.addFields(coorMetadata.toBSON());

    auto [coordinator, created] = [&] {
        try {
            auto [coordinator, created] =
                PrimaryOnlyService::getOrCreateInstance(opCtx, patchedCoorDoc, checkOptions);
            return std::make_pair(
                checked_pointer_cast<ShardingDDLCoordinator>(std::move(coordinator)),
                std::move(created));
        } catch (const DBException& ex) {
            LOGV2_ERROR(5390512,
                        "Failed to create instance of sharding DDL coordinator",
                        "coordinatorId"_attr = coorMetadata.getId(),
                        "reason"_attr = redact(ex));
            throw;
        }
    }();

    return coordinator;
}


std::shared_ptr<executor::TaskExecutor> ShardingDDLCoordinatorService::getInstanceCleanupExecutor()
    const {
    return PrimaryOnlyService::getInstanceCleanupExecutor();
}

void ShardingDDLCoordinatorService::_transitionToRecovered(WithLock lk, OperationContext* opCtx) {
    _state = State::kRecovered;
    DDLLockManager::get(opCtx)->setState(DDLLockManager::State::kPrimaryAndRecovered);
    _recoveredOrCoordinatorCompletedCV.notify_all();
}

}  // namespace mongo
