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

#include "mongo/db/s/migration_util.h"

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr.hpp>
#include <fmt/format.h>
#include <functional>
#include <mutex>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bson_field.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/client/dbclient_cursor.h"
#include "mongo/client/read_preference.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/commands.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/locker_api.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/ops/write_ops_gen.h"
#include "mongo/db/ops/write_ops_parsers.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/s/active_migrations_registry.h"
#include "mongo/db/s/collection_sharding_runtime.h"
#include "mongo/db/s/migration_coordinator.h"
#include "mongo/db/s/migration_destination_manager.h"
#include "mongo/db/s/shard_filtering_metadata_refresh.h"
#include "mongo/db/s/sharding_statistics.h"
#include "mongo/db/s/sharding_util.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/vector_clock_mutable.h"
#include "mongo/executor/network_interface_factory.h"
#include "mongo/executor/thread_pool_task_executor.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/mutex.h"
#include "mongo/rpc/get_status_from_command_result.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/client/shard.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/grid.h"
#include "mongo/s/request_types/ensure_chunk_version_is_greater_than_gen.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/concurrency/thread_name.h"
#include "mongo/util/concurrency/thread_pool.h"
#include "mongo/util/decorable.h"
#include "mongo/util/exit.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kShardingMigration

namespace mongo {
namespace migrationutil {
namespace {

using namespace fmt::literals;

MONGO_FAIL_POINT_DEFINE(hangBeforeFilteringMetadataRefresh);
MONGO_FAIL_POINT_DEFINE(hangInEnsureChunkVersionIsGreaterThanInterruptible);
MONGO_FAIL_POINT_DEFINE(hangInEnsureChunkVersionIsGreaterThanThenSimulateErrorUninterruptible);
MONGO_FAIL_POINT_DEFINE(hangInRefreshFilteringMetadataUntilSuccessInterruptible);
MONGO_FAIL_POINT_DEFINE(hangInRefreshFilteringMetadataUntilSuccessThenSimulateErrorUninterruptible);
MONGO_FAIL_POINT_DEFINE(hangInPersistMigrateCommitDecisionInterruptible);
MONGO_FAIL_POINT_DEFINE(hangInPersistMigrateCommitDecisionThenSimulateErrorUninterruptible);
MONGO_FAIL_POINT_DEFINE(hangInPersistMigrateAbortDecisionThenSimulateErrorUninterruptible);
MONGO_FAIL_POINT_DEFINE(hangInAdvanceTxnNumInterruptible);
MONGO_FAIL_POINT_DEFINE(hangInAdvanceTxnNumThenSimulateErrorUninterruptible);

const char kSourceShard[] = "source";
const char kDestinationShard[] = "destination";
const char kIsDonorShard[] = "isDonorShard";
const char kChunk[] = "chunk";
const char kCollection[] = "collection";
const char kSessionOplogEntriesMigrated[] = "sessionOplogEntriesMigrated";
const char ksessionOplogEntriesSkippedSoFarLowerBound[] =
    "sessionOplogEntriesSkippedSoFarLowerBound";
const char ksessionOplogEntriesToBeMigratedSoFar[] = "sessionOplogEntriesToBeMigratedSoFar";
const Backoff kExponentialBackoff(Seconds(10), Milliseconds::max());

const WriteConcernOptions kMajorityWriteConcern(WriteConcernOptions::kMajority,
                                                WriteConcernOptions::SyncMode::UNSET,
                                                WriteConcernOptions::kNoTimeout);

class MigrationUtilExecutor {
public:
    MigrationUtilExecutor()
        : _executor(std::make_shared<executor::ThreadPoolTaskExecutor>(
              _makePool(), executor::makeNetworkInterface("MigrationUtil-TaskExecutor"))) {}

    void shutDownAndJoin() {
        _executor->shutdown();
        _executor->join();
    }

    std::shared_ptr<executor::ThreadPoolTaskExecutor> getExecutor() {
        stdx::lock_guard<Latch> lg(_mutex);
        if (!_started) {
            _executor->startup();
            _started = true;
        }
        return _executor;
    }

private:
    std::unique_ptr<ThreadPool> _makePool() {
        ThreadPool::Options options;
        options.poolName = "MoveChunk";
        options.minThreads = 0;
        options.maxThreads = 16;
        return std::make_unique<ThreadPool>(std::move(options));
    }

    std::shared_ptr<executor::ThreadPoolTaskExecutor> _executor;

    // TODO SERVER-57253: get rid of _mutex and _started fields
    Mutex _mutex = MONGO_MAKE_LATCH("MigrationUtilExecutor::_mutex");
    bool _started = false;
};

const auto migrationUtilExecutorDecoration =
    ServiceContext::declareDecoration<MigrationUtilExecutor>();
const ServiceContext::ConstructorActionRegisterer migrationUtilExecutorRegisterer{
    "MigrationUtilExecutor",
    [](ServiceContext* service) {
        // TODO SERVER-57253: start migration util executor at decoration construction time
    },
    [](ServiceContext* service) {
        migrationUtilExecutorDecoration(service).shutDownAndJoin();
    }};


void refreshFilteringMetadataUntilSuccess(OperationContext* opCtx, const NamespaceString& nss) {
    hangBeforeFilteringMetadataRefresh.pauseWhileSet();

    sharding_util::retryIdempotentWorkAsPrimaryUntilSuccessOrStepdown(
        opCtx, "refreshFilteringMetadataUntilSuccess", [&nss](OperationContext* newOpCtx) {
            hangInRefreshFilteringMetadataUntilSuccessInterruptible.pauseWhileSet(newOpCtx);

            try {
                onCollectionPlacementVersionMismatch(newOpCtx, nss, boost::none);
            } catch (const ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
                // Can throw NamespaceNotFound if the collection/database was dropped
            }

            if (hangInRefreshFilteringMetadataUntilSuccessThenSimulateErrorUninterruptible
                    .shouldFail()) {
                hangInRefreshFilteringMetadataUntilSuccessThenSimulateErrorUninterruptible
                    .pauseWhileSet();
                uasserted(ErrorCodes::InternalError,
                          "simulate an error response for onCollectionPlacementVersionMismatch");
            }
        });
}

void ensureChunkVersionIsGreaterThan(OperationContext* opCtx,
                                     const NamespaceString& nss,
                                     const UUID& collUUID,
                                     const ChunkRange& range,
                                     const ChunkVersion& preMigrationChunkVersion) {
    ConfigsvrEnsureChunkVersionIsGreaterThan ensureChunkVersionIsGreaterThanRequest;
    ensureChunkVersionIsGreaterThanRequest.setDbName(DatabaseName::kAdmin);
    ensureChunkVersionIsGreaterThanRequest.setMinKey(range.getMin());
    ensureChunkVersionIsGreaterThanRequest.setMaxKey(range.getMax());
    ensureChunkVersionIsGreaterThanRequest.setVersion(preMigrationChunkVersion);
    ensureChunkVersionIsGreaterThanRequest.setNss(nss);
    ensureChunkVersionIsGreaterThanRequest.setCollectionUUID(collUUID);
    const auto ensureChunkVersionIsGreaterThanRequestBSON =
        ensureChunkVersionIsGreaterThanRequest.toBSON({});

    hangInEnsureChunkVersionIsGreaterThanInterruptible.pauseWhileSet(opCtx);

    const auto ensureChunkVersionIsGreaterThanResponse =
        Grid::get(opCtx)->shardRegistry()->getConfigShard()->runCommandWithFixedRetryAttempts(
            opCtx,
            ReadPreferenceSetting{ReadPreference::PrimaryOnly},
            DatabaseName::kAdmin,
            CommandHelpers::appendMajorityWriteConcern(ensureChunkVersionIsGreaterThanRequestBSON),
            Shard::RetryPolicy::kIdempotent);
    const auto ensureChunkVersionIsGreaterThanStatus =
        Shard::CommandResponse::getEffectiveStatus(ensureChunkVersionIsGreaterThanResponse);

    uassertStatusOK(ensureChunkVersionIsGreaterThanStatus);

    if (hangInEnsureChunkVersionIsGreaterThanThenSimulateErrorUninterruptible.shouldFail()) {
        hangInEnsureChunkVersionIsGreaterThanThenSimulateErrorUninterruptible.pauseWhileSet();
        uasserted(ErrorCodes::InternalError,
                  "simulate an error response for _configsvrEnsureChunkVersionIsGreaterThan");
    }
}

BSONObj getQueryFilterForRangeDeletionTask(const UUID& collectionUuid, const ChunkRange& range) {
    return BSON(RangeDeletionTask::kCollectionUuidFieldName
                << collectionUuid << RangeDeletionTask::kRangeFieldName + "." + ChunkRange::kMinKey
                << range.getMin() << RangeDeletionTask::kRangeFieldName + "." + ChunkRange::kMaxKey
                << range.getMax());
}


}  // namespace

std::shared_ptr<executor::ThreadPoolTaskExecutor> getMigrationUtilExecutor(
    ServiceContext* serviceContext) {
    return migrationUtilExecutorDecoration(serviceContext).getExecutor();
}

BSONObjBuilder _makeMigrationStatusDocumentCommon(const NamespaceString& nss,
                                                  const ShardId& fromShard,
                                                  const ShardId& toShard,
                                                  const bool& isDonorShard,
                                                  const BSONObj& min,
                                                  const BSONObj& max) {
    BSONObjBuilder builder;
    builder.append(kSourceShard, fromShard.toString());
    builder.append(kDestinationShard, toShard.toString());
    builder.append(kIsDonorShard, isDonorShard);
    builder.append(kChunk, BSON(ChunkType::min(min) << ChunkType::max(max)));
    builder.append(kCollection,
                   NamespaceStringUtil::serialize(nss, SerializationContext::stateDefault()));
    return builder;
}

BSONObj makeMigrationStatusDocumentSource(
    const NamespaceString& nss,
    const ShardId& fromShard,
    const ShardId& toShard,
    const bool& isDonorShard,
    const BSONObj& min,
    const BSONObj& max,
    boost::optional<long long> sessionOplogEntriesToBeMigratedSoFar,
    boost::optional<long long> sessionOplogEntriesSkippedSoFarLowerBound) {
    BSONObjBuilder builder =
        _makeMigrationStatusDocumentCommon(nss, fromShard, toShard, isDonorShard, min, max);
    if (sessionOplogEntriesToBeMigratedSoFar) {
        builder.append(ksessionOplogEntriesToBeMigratedSoFar,
                       sessionOplogEntriesToBeMigratedSoFar.value());
    }
    if (sessionOplogEntriesSkippedSoFarLowerBound) {
        builder.append(ksessionOplogEntriesSkippedSoFarLowerBound,
                       sessionOplogEntriesSkippedSoFarLowerBound.value());
    }
    return builder.obj();
}

BSONObj makeMigrationStatusDocumentDestination(
    const NamespaceString& nss,
    const ShardId& fromShard,
    const ShardId& toShard,
    const bool& isDonorShard,
    const BSONObj& min,
    const BSONObj& max,
    boost::optional<long long> sessionOplogEntriesMigrated) {
    BSONObjBuilder builder =
        _makeMigrationStatusDocumentCommon(nss, fromShard, toShard, isDonorShard, min, max);
    if (sessionOplogEntriesMigrated) {
        builder.append(kSessionOplogEntriesMigrated, sessionOplogEntriesMigrated.value());
    }
    return builder.obj();
}

ChunkRange extendOrTruncateBoundsForMetadata(const CollectionMetadata& metadata,
                                             const ChunkRange& range) {
    auto metadataShardKeyPattern = KeyPattern(metadata.getKeyPattern());

    // If the input range is shorter than the range in the ChunkManager inside
    // 'metadata', we must extend its bounds to get a correct comparison. If the input
    // range is longer than the range in the ChunkManager, we likewise must shorten it.
    // We make sure to match what's in the ChunkManager instead of the other way around,
    // since the ChunkManager only stores ranges and compares overlaps using a string version of the
    // key, rather than a BSONObj. This logic is necessary because the _metadata list can
    // contain ChunkManagers with different shard keys if the shard key has been refined.
    //
    // Note that it's safe to use BSONObj::nFields() (which returns the number of top level
    // fields in the BSONObj) to compare the two, since shard key refine operations can only add
    // top-level fields.
    //
    // Using extractFieldsUndotted to shorten the input range is correct because the ChunkRange and
    // the shard key pattern will both already store nested shard key fields as top-level dotted
    // fields, and extractFieldsUndotted uses the top-level fields verbatim rather than treating
    // dots as accessors for subfields.
    auto metadataShardKeyPatternBson = metadataShardKeyPattern.toBSON();
    auto numFieldsInMetadataShardKey = metadataShardKeyPatternBson.nFields();
    auto numFieldsInInputRangeShardKey = range.getMin().nFields();
    if (numFieldsInInputRangeShardKey < numFieldsInMetadataShardKey) {
        auto extendedRangeMin = metadataShardKeyPattern.extendRangeBound(
            range.getMin(), false /* makeUpperInclusive */);
        auto extendedRangeMax = metadataShardKeyPattern.extendRangeBound(
            range.getMax(), false /* makeUpperInclusive */);
        return ChunkRange(extendedRangeMin, extendedRangeMax);
    } else if (numFieldsInInputRangeShardKey > numFieldsInMetadataShardKey) {
        auto shortenedRangeMin = range.getMin().extractFieldsUndotted(metadataShardKeyPatternBson);
        auto shortenedRangeMax = range.getMax().extractFieldsUndotted(metadataShardKeyPatternBson);
        return ChunkRange(shortenedRangeMin, shortenedRangeMax);
    } else {
        return range;
    }
}

bool deletionTaskUuidMatchesFilteringMetadataUuid(
    OperationContext* opCtx,
    const boost::optional<mongo::CollectionMetadata>& optCollDescr,
    const RangeDeletionTask& deletionTask) {
    return optCollDescr && optCollDescr->isSharded() &&
        optCollDescr->uuidMatches(deletionTask.getCollectionUuid());
}

void persistMigrationCoordinatorLocally(OperationContext* opCtx,
                                        const MigrationCoordinatorDocument& migrationDoc) {
    PersistentTaskStore<MigrationCoordinatorDocument> store(
        NamespaceString::kMigrationCoordinatorsNamespace);
    try {
        store.add(opCtx, migrationDoc);
    } catch (const ExceptionFor<ErrorCodes::DuplicateKey>&) {
        // Convert a DuplicateKey error to an anonymous error.
        uasserted(
            31374,
            str::stream() << "While attempting to write migration information for migration "
                          << ", found document with the same migration id. Attempted migration: "
                          << migrationDoc.toBSON());
    }
}

void notifyChangeStreamsOnRecipientFirstChunk(OperationContext* opCtx,
                                              const NamespaceString& collNss,
                                              const ShardId& fromShardId,
                                              const ShardId& toShardId,
                                              boost::optional<UUID> collUUID) {

    const std::string dbgMessage = str::stream()
        << "Migrating chunk from shard " << fromShardId << " to shard " << toShardId
        << " with no chunks for this collection";

    // The message expected by change streams
    const auto o2Message =
        BSON("migrateChunkToNewShard"
             << NamespaceStringUtil::serialize(collNss, SerializationContext::stateDefault())
             << "fromShardId" << fromShardId << "toShardId" << toShardId);

    auto const serviceContext = opCtx->getClient()->getServiceContext();

    // TODO (SERVER-71444): Fix to be interruptible or document exception.
    UninterruptibleLockGuard noInterrupt(shard_role_details::getLocker(opCtx));  // NOLINT.
    AutoGetOplog oplogWrite(opCtx, OplogAccessMode::kWrite);
    writeConflictRetry(opCtx, "migrateChunkToNewShard", NamespaceString::kRsOplogNamespace, [&] {
        WriteUnitOfWork uow(opCtx);
        serviceContext->getOpObserver()->onInternalOpMessage(opCtx,
                                                             collNss,
                                                             *collUUID,
                                                             BSON("msg" << dbgMessage),
                                                             o2Message,
                                                             boost::none,
                                                             boost::none,
                                                             boost::none,
                                                             boost::none);
        uow.commit();
    });
}

void notifyChangeStreamsOnDonorLastChunk(OperationContext* opCtx,
                                         const NamespaceString& collNss,
                                         const ShardId& donorShardId,
                                         boost::optional<UUID> collUUID) {

    const std::string oMessage = str::stream()
        << "Migrate the last chunk for " << collNss.toStringForErrorMsg() << " off shard "
        << donorShardId;

    // The message expected by change streams
    const auto o2Message =
        BSON("migrateLastChunkFromShard"
             << NamespaceStringUtil::serialize(collNss, SerializationContext::stateDefault())
             << "shardId" << donorShardId);

    auto const serviceContext = opCtx->getClient()->getServiceContext();

    // TODO (SERVER-71444): Fix to be interruptible or document exception.
    UninterruptibleLockGuard noInterrupt(shard_role_details::getLocker(opCtx));  // NOLINT.
    AutoGetOplog oplogWrite(opCtx, OplogAccessMode::kWrite);
    writeConflictRetry(opCtx, "migrateLastChunkFromShard", NamespaceString::kRsOplogNamespace, [&] {
        WriteUnitOfWork uow(opCtx);
        serviceContext->getOpObserver()->onInternalOpMessage(opCtx,
                                                             collNss,
                                                             *collUUID,
                                                             BSON("msg" << oMessage),
                                                             o2Message,
                                                             boost::none,
                                                             boost::none,
                                                             boost::none,
                                                             boost::none);
        uow.commit();
    });
}

void persistCommitDecision(OperationContext* opCtx,
                           const MigrationCoordinatorDocument& migrationDoc) {
    invariant(migrationDoc.getDecision() &&
              *migrationDoc.getDecision() == DecisionEnum::kCommitted);

    hangInPersistMigrateCommitDecisionInterruptible.pauseWhileSet(opCtx);
    try {
        PersistentTaskStore<MigrationCoordinatorDocument> store(
            NamespaceString::kMigrationCoordinatorsNamespace);
        store.update(opCtx,
                     BSON(MigrationCoordinatorDocument::kIdFieldName << migrationDoc.getId()),
                     migrationDoc.toBSON());
        ShardingStatistics::get(opCtx).countDonorMoveChunkCommitted.addAndFetch(1);
    } catch (const ExceptionFor<ErrorCodes::NoMatchingDocument>&) {
        LOGV2_ERROR(6439800,
                    "No coordination doc found on disk for migration",
                    "migration"_attr = redact(migrationDoc.toBSON()));
    }

    if (hangInPersistMigrateCommitDecisionThenSimulateErrorUninterruptible.shouldFail()) {
        hangInPersistMigrateCommitDecisionThenSimulateErrorUninterruptible.pauseWhileSet(opCtx);
        uasserted(ErrorCodes::InternalError,
                  "simulate an error response when persisting migrate commit decision");
    }
}

void persistAbortDecision(OperationContext* opCtx,
                          const MigrationCoordinatorDocument& migrationDoc) {
    invariant(migrationDoc.getDecision() && *migrationDoc.getDecision() == DecisionEnum::kAborted);

    try {
        PersistentTaskStore<MigrationCoordinatorDocument> store(
            NamespaceString::kMigrationCoordinatorsNamespace);
        store.update(opCtx,
                     BSON(MigrationCoordinatorDocument::kIdFieldName << migrationDoc.getId()),
                     migrationDoc.toBSON());
        ShardingStatistics::get(opCtx).countDonorMoveChunkAborted.addAndFetch(1);
    } catch (const ExceptionFor<ErrorCodes::NoMatchingDocument>&) {
        LOGV2(6439801,
              "No coordination doc found on disk for migration",
              "migration"_attr = redact(migrationDoc.toBSON()));
    }

    if (hangInPersistMigrateAbortDecisionThenSimulateErrorUninterruptible.shouldFail()) {
        hangInPersistMigrateAbortDecisionThenSimulateErrorUninterruptible.pauseWhileSet(opCtx);
        uasserted(ErrorCodes::InternalError,
                  "simulate an error response when persisting migrate abort decision");
    }
}

void advanceTransactionOnRecipient(OperationContext* opCtx,
                                   const ShardId& recipientId,
                                   const LogicalSessionId& lsid,
                                   TxnNumber currentTxnNumber) {
    write_ops::UpdateCommandRequest updateOp(NamespaceString::kServerConfigurationNamespace);
    auto queryFilter = BSON("_id"
                            << "migrationCoordinatorStats");
    auto updateModification = write_ops::UpdateModification(
        write_ops::UpdateModification::parseFromClassicUpdate(BSON("$inc" << BSON("count" << 1))));

    write_ops::UpdateOpEntry updateEntry(queryFilter, updateModification);
    updateEntry.setMulti(false);
    updateEntry.setUpsert(true);
    updateOp.setUpdates({updateEntry});

    auto passthroughFields = BSON(WriteConcernOptions::kWriteConcernField
                                  << WriteConcernOptions::Majority << "lsid" << lsid.toBSON()
                                  << "txnNumber" << currentTxnNumber + 1);

    hangInAdvanceTxnNumInterruptible.pauseWhileSet(opCtx);
    sharding_util::invokeCommandOnShardWithIdempotentRetryPolicy(
        opCtx,
        recipientId,
        NamespaceString::kServerConfigurationNamespace.dbName(),
        updateOp.toBSON(passthroughFields));

    if (hangInAdvanceTxnNumThenSimulateErrorUninterruptible.shouldFail()) {
        hangInAdvanceTxnNumThenSimulateErrorUninterruptible.pauseWhileSet(opCtx);
        uasserted(ErrorCodes::InternalError,
                  "simulate an error response when initiating range deletion locally");
    }
}

void resumeMigrationCoordinationsOnStepUp(OperationContext* opCtx) {
    LOGV2_DEBUG(4798510, 2, "Starting migration coordinator step-up recovery");

    unsigned long long unfinishedMigrationsCount = 0;

    PersistentTaskStore<MigrationCoordinatorDocument> store(
        NamespaceString::kMigrationCoordinatorsNamespace);
    store.forEach(opCtx,
                  BSONObj{},
                  [&opCtx, &unfinishedMigrationsCount](const MigrationCoordinatorDocument& doc) {
                      unfinishedMigrationsCount++;
                      LOGV2_DEBUG(4798511,
                                  3,
                                  "Found unfinished migration on step-up",
                                  "migrationCoordinatorDoc"_attr = redact(doc.toBSON()),
                                  "unfinishedMigrationsCount"_attr = unfinishedMigrationsCount);

                      const auto& nss = doc.getNss();

                      {
                          AutoGetCollection autoColl(opCtx, nss, MODE_IX);
                          CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                              opCtx, nss)
                              ->clearFilteringMetadata(opCtx);
                      }

                      asyncRecoverMigrationUntilSuccessOrStepDown(opCtx, nss);

                      return true;
                  });

    ShardingStatistics::get(opCtx).unfinishedMigrationFromPreviousPrimary.store(
        unfinishedMigrationsCount);

    LOGV2_DEBUG(4798513,
                2,
                "Finished migration coordinator step-up recovery",
                "unfinishedMigrationsCount"_attr = unfinishedMigrationsCount);
}

void recoverMigrationCoordinations(OperationContext* opCtx,
                                   NamespaceString nss,
                                   CancellationToken cancellationToken) {
    LOGV2_DEBUG(4798501, 2, "Starting migration recovery", logAttrs(nss));

    unsigned migrationRecoveryCount = 0;

    PersistentTaskStore<MigrationCoordinatorDocument> store(
        NamespaceString::kMigrationCoordinatorsNamespace);
    store.forEach(
        opCtx,
        BSON(MigrationCoordinatorDocument::kNssFieldName
             << NamespaceStringUtil::serialize(nss, SerializationContext::stateDefault())),
        [&opCtx, &nss, &migrationRecoveryCount, &cancellationToken](
            const MigrationCoordinatorDocument& doc) {
            LOGV2_DEBUG(4798502,
                        2,
                        "Recovering migration",
                        "migrationCoordinatorDocument"_attr = redact(doc.toBSON()));

            // Ensure there is only one migrationCoordinator document to be recovered for this
            // namespace.
            invariant(++migrationRecoveryCount == 1,
                      str::stream() << "Found more then one migration to recover for namespace '"
                                    << nss.toStringForErrorMsg() << "'");

            // Create a MigrationCoordinator to complete the coordination.
            MigrationCoordinator coordinator(doc);

            if (doc.getDecision()) {
                // The decision is already known.
                coordinator.setShardKeyPattern(
                    rangedeletionutil::getShardKeyPatternFromRangeDeletionTask(opCtx, doc.getId()));
                coordinator.completeMigration(opCtx);
                return true;
            }

            // The decision is not known. Recover the decision from the config server.

            ensureChunkVersionIsGreaterThan(opCtx,
                                            doc.getNss(),
                                            doc.getCollectionUuid(),
                                            doc.getRange(),
                                            doc.getPreMigrationChunkVersion());

            hangInRefreshFilteringMetadataUntilSuccessInterruptible.pauseWhileSet(opCtx);

            auto currentMetadata = forceGetCurrentMetadata(opCtx, doc.getNss());

            if (hangInRefreshFilteringMetadataUntilSuccessThenSimulateErrorUninterruptible
                    .shouldFail()) {
                hangInRefreshFilteringMetadataUntilSuccessThenSimulateErrorUninterruptible
                    .pauseWhileSet();
                uasserted(ErrorCodes::InternalError,
                          "simulate an error response for forceGetCurrentMetadata");
            }

            auto setFilteringMetadata = [&opCtx, &currentMetadata, &doc, &cancellationToken]() {
                AutoGetDb autoDb(opCtx, doc.getNss().dbName(), MODE_IX);
                Lock::CollectionLock collLock(opCtx, doc.getNss(), MODE_IX);
                auto scopedCsr =
                    CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
                        opCtx, doc.getNss());

                auto optMetadata = scopedCsr->getCurrentMetadataIfKnown();
                invariant(!optMetadata);

                if (!cancellationToken.isCanceled()) {
                    scopedCsr->setFilteringMetadata(opCtx, std::move(currentMetadata));
                }
            };

            if (!currentMetadata.isSharded() ||
                !currentMetadata.uuidMatches(doc.getCollectionUuid())) {
                if (!currentMetadata.isSharded()) {
                    LOGV2(4798503,
                          "During migration recovery the collection was discovered to have been "
                          "dropped."
                          "Deleting the range deletion tasks on the donor and the recipient "
                          "as well as the migration coordinator document on this node",
                          "migrationCoordinatorDocument"_attr = redact(doc.toBSON()));
                } else {
                    // UUID don't match
                    LOGV2(4798504,
                          "During migration recovery the collection was discovered to have been "
                          "dropped and recreated. Collection has a UUID that "
                          "does not match the one in the migration coordinator "
                          "document. Deleting the range deletion tasks on the donor and "
                          "recipient as well as the migration coordinator document on this node",
                          "migrationCoordinatorDocument"_attr = redact(doc.toBSON()),
                          "refreshedMetadataUUID"_attr =
                              currentMetadata.getChunkManager()->getUUID(),
                          "coordinatorDocumentUUID"_attr = doc.getCollectionUuid());
                }

                // TODO SERVER-77472: remove this once we are sure all operations persist the config
                // time after a collection drop. Since the collection has been dropped, persist
                // config time inclusive of the drop collection event before deleting leftover
                // migration metadata. This will ensure that in case of stepdown the new primary
                // won't read stale data from config server and think that the sharded collection
                // still exists.
                VectorClockMutable::get(opCtx)->waitForDurableConfigTime().get(opCtx);

                rangedeletionutil::deleteRangeDeletionTaskOnRecipient(opCtx,
                                                                      doc.getRecipientShardId(),
                                                                      doc.getCollectionUuid(),
                                                                      doc.getRange(),
                                                                      doc.getId());
                rangedeletionutil::deleteRangeDeletionTaskLocally(
                    opCtx, doc.getCollectionUuid(), doc.getRange());
                coordinator.forgetMigration(opCtx);
                setFilteringMetadata();
                return true;
            }

            // Note this should only extend the range boundaries (if there has been a shard key
            // refine since the migration began) and never truncate them.
            auto chunkRangeToCompareToMetadata =
                extendOrTruncateBoundsForMetadata(currentMetadata, doc.getRange());
            if (currentMetadata.keyBelongsToMe(chunkRangeToCompareToMetadata.getMin())) {
                coordinator.setMigrationDecision(DecisionEnum::kAborted);
            } else {
                coordinator.setMigrationDecision(DecisionEnum::kCommitted);
                if (!currentMetadata.getChunkManager()->getVersion(doc.getDonorShardId()).isSet()) {
                    notifyChangeStreamsOnDonorLastChunk(
                        opCtx, doc.getNss(), doc.getDonorShardId(), doc.getCollectionUuid());
                }
            }

            coordinator.setShardKeyPattern(KeyPattern(currentMetadata.getKeyPattern()));
            coordinator.completeMigration(opCtx);
            setFilteringMetadata();
            return true;
        });
}

ExecutorFuture<void> launchReleaseCriticalSectionOnRecipientFuture(
    OperationContext* opCtx,
    const ShardId& recipientShardId,
    const NamespaceString& nss,
    const MigrationSessionId& sessionId) {
    const auto serviceContext = opCtx->getServiceContext();
    auto executor = Grid::get(opCtx)->getExecutorPool()->getFixedExecutor();

    return ExecutorFuture<void>(executor).then([=] {
        ThreadClient tc("releaseRecipientCritSec",
                        serviceContext->getService(ClusterRole::ShardServer));
        auto uniqueOpCtx = tc->makeOperationContext();
        auto opCtx = uniqueOpCtx.get();

        const auto recipientShard =
            uassertStatusOK(Grid::get(opCtx)->shardRegistry()->getShard(opCtx, recipientShardId));

        BSONObjBuilder builder;
        builder.append("_recvChunkReleaseCritSec",
                       NamespaceStringUtil::serialize(nss, SerializationContext::stateDefault()));
        sessionId.append(&builder);
        const auto commandObj = CommandHelpers::appendMajorityWriteConcern(builder.obj());

        sharding_util::retryIdempotentWorkAsPrimaryUntilSuccessOrStepdown(
            opCtx,
            "release migration critical section on recipient",
            [&](OperationContext* newOpCtx) {
                try {
                    const auto response = recipientShard->runCommandWithFixedRetryAttempts(
                        newOpCtx,
                        ReadPreferenceSetting{ReadPreference::PrimaryOnly},
                        DatabaseName::kAdmin,
                        commandObj,
                        Shard::RetryPolicy::kIdempotent);

                    uassertStatusOK(Shard::CommandResponse::getEffectiveStatus(response));
                } catch (const ExceptionFor<ErrorCodes::ShardNotFound>& exShardNotFound) {
                    LOGV2(5899106,
                          "Failed to release critical section on recipient",
                          "shardId"_attr = recipientShardId,
                          "sessionId"_attr = sessionId,
                          "error"_attr = exShardNotFound);
                }
            },
            Backoff(Seconds(1), Milliseconds::max()));
    });
}

void persistMigrationRecipientRecoveryDocument(
    OperationContext* opCtx, const MigrationRecipientRecoveryDocument& migrationRecipientDoc) {
    PersistentTaskStore<MigrationRecipientRecoveryDocument> store(
        NamespaceString::kMigrationRecipientsNamespace);
    try {
        store.add(
            opCtx, migrationRecipientDoc, WriteConcerns::kMajorityWriteConcernShardingTimeout);
    } catch (const ExceptionFor<ErrorCodes::DuplicateKey>&) {
        // Convert a DuplicateKey error to an anonymous error.
        uasserted(6064502,
                  str::stream()
                      << "While attempting to write migration recipient information for migration "
                      << ", found document with the same migration id. Attempted migration: "
                      << migrationRecipientDoc.toBSON());
    }
}

void deleteMigrationRecipientRecoveryDocument(OperationContext* opCtx, const UUID& migrationId) {
    // Before deleting the migration recipient recovery document, ensure that in the case of a
    // crash, the node will start-up from a configTime that is inclusive of the migration that was
    // committed during the critical section.
    VectorClockMutable::get(opCtx)->waitForDurableConfigTime().get(opCtx);

    PersistentTaskStore<MigrationRecipientRecoveryDocument> store(
        NamespaceString::kMigrationRecipientsNamespace);
    store.remove(opCtx,
                 BSON(MigrationRecipientRecoveryDocument::kIdFieldName << migrationId),
                 ShardingCatalogClient::kMajorityWriteConcern);
}

void resumeMigrationRecipientsOnStepUp(OperationContext* opCtx) {
    LOGV2_DEBUG(6064504, 2, "Starting migration recipient step-up recovery");

    unsigned long long ongoingMigrationRecipientsCount = 0;

    PersistentTaskStore<MigrationRecipientRecoveryDocument> store(
        NamespaceString::kMigrationRecipientsNamespace);

    store.forEach(
        opCtx,
        BSONObj{},
        [&opCtx, &ongoingMigrationRecipientsCount](const MigrationRecipientRecoveryDocument& doc) {
            invariant(ongoingMigrationRecipientsCount == 0,
                      str::stream()
                          << "Upon step-up a second migration recipient recovery document was found"
                          << redact(doc.toBSON()));
            ongoingMigrationRecipientsCount++;
            LOGV2_DEBUG(5899102,
                        3,
                        "Found ongoing migration recipient critical section on step-up",
                        "migrationRecipientCoordinatorDoc"_attr = redact(doc.toBSON()));

            const auto& nss = doc.getNss();

            // Register this receiveChunk on the ActiveMigrationsRegistry before completing step-up
            // to prevent a new migration from starting while a receiveChunk was ongoing. Wait for
            // any migrations that began in a previous term to complete if there are any.
            auto scopedReceiveChunk(
                uassertStatusOK(ActiveMigrationsRegistry::get(opCtx).registerReceiveChunk(
                    opCtx,
                    nss,
                    doc.getRange(),
                    doc.getDonorShardIdForLoggingPurposesOnly(),
                    true /* waitForCompletionOfConflictingOps */)));

            const auto mdm = MigrationDestinationManager::get(opCtx);
            uassertStatusOK(
                mdm->restoreRecoveredMigrationState(opCtx, std::move(scopedReceiveChunk), doc));

            return true;
        });

    LOGV2_DEBUG(6064505,
                2,
                "Finished migration recipient step-up recovery",
                "ongoingRecipientCritSecCount"_attr = ongoingMigrationRecipientsCount);
}

void drainMigrationsPendingRecovery(OperationContext* opCtx) {
    PersistentTaskStore<MigrationCoordinatorDocument> store(
        NamespaceString::kMigrationCoordinatorsNamespace);

    while (store.count(opCtx)) {
        store.forEach(opCtx, BSONObj(), [opCtx](const MigrationCoordinatorDocument& doc) {
            try {
                onCollectionPlacementVersionMismatch(opCtx, doc.getNss(), boost::none);
            } catch (DBException& ex) {
                ex.addContext(str::stream() << "Failed to recover pending migration for document "
                                            << doc.toBSON());
                throw;
            }
            return true;
        });
    }
}

void asyncRecoverMigrationUntilSuccessOrStepDown(OperationContext* opCtx,
                                                 const NamespaceString& nss) noexcept {
    ExecutorFuture<void>{Grid::get(opCtx)->getExecutorPool()->getFixedExecutor()}
        .then([svcCtx{opCtx->getServiceContext()}, nss] {
            ThreadClient tc{"MigrationRecovery", svcCtx->getService(ClusterRole::ShardServer)};
            auto uniqueOpCtx{tc->makeOperationContext()};
            auto opCtx{uniqueOpCtx.get()};

            try {
                refreshFilteringMetadataUntilSuccess(opCtx, nss);
            } catch (const DBException& ex) {
                // This is expected in the event of a stepdown.
                LOGV2(6316100,
                      "Interrupted deferred migration recovery",
                      logAttrs(nss),
                      "error"_attr = redact(ex));
            }
        })
        .getAsync([](auto) {});
}

}  // namespace migrationutil
}  // namespace mongo
