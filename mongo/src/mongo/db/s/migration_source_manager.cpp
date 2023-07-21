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

#include "mongo/db/s/migration_source_manager.h"

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <mutex>
#include <string>
#include <tuple>
#include <type_traits>
#include <vector>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/read_preference.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/database_name.h"
#include "mongo/db/keypattern.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/persistent_task_store.h"
#include "mongo/db/read_concern.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/repl/read_concern_level.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/s/auto_split_vector.h"
#include "mongo/db/s/chunk_operation_precondition_checks.h"
#include "mongo/db/s/commit_chunk_migration_gen.h"
#include "mongo/db/s/migration_chunk_cloner_source.h"
#include "mongo/db/s/migration_coordinator.h"
#include "mongo/db/s/migration_coordinator_document_gen.h"
#include "mongo/db/s/migration_util.h"
#include "mongo/db/s/shard_filtering_metadata_refresh.h"
#include "mongo/db/s/shard_metadata_util.h"
#include "mongo/db/s/sharding_logging.h"
#include "mongo/db/s/sharding_runtime_d_params_gen.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/s/sharding_statistics.h"
#include "mongo/db/s/type_shard_collection.h"
#include "mongo/db/s/type_shard_collection_gen.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/timeseries/bucket_catalog/bucket_catalog.h"
#include "mongo/db/write_concern.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/catalog_cache_loader.h"
#include "mongo/s/chunk.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/client/shard.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/grid.h"
#include "mongo/s/shard_key_pattern.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kShardingMigration

namespace mongo {
namespace {

const auto msmForCsr = CollectionShardingRuntime::declareDecoration<MigrationSourceManager*>();

// Wait at most this much time for the recipient to catch up sufficiently so critical section can be
// entered
const Hours kMaxWaitToEnterCriticalSectionTimeout(6);
const char kWriteConcernField[] = "writeConcern";
const WriteConcernOptions kMajorityWriteConcern(WriteConcernOptions::kMajority,
                                                WriteConcernOptions::SyncMode::UNSET,
                                                WriteConcernOptions::kWriteConcernTimeoutMigration);

std::string kEmptyErrMsgForMoveTimingHelper;

/*
 * Calculates the max or min bound perform split+move in case the chunk in question is splittable.
 * If the chunk is not splittable, returns the bound of the existing chunk for the max or min.Finds
 * a max bound if needMaxBound is true and a min bound if forward is false.
 */
BSONObj computeOtherBound(OperationContext* opCtx,
                          const NamespaceString& nss,
                          const BSONObj& min,
                          const BSONObj& max,
                          const ShardKeyPattern& skPattern,
                          const long long maxChunkSizeBytes,
                          bool needMaxBound) {
    auto [splitKeys, _] = autoSplitVector(
        opCtx, nss, skPattern.toBSON(), min, max, maxChunkSizeBytes, 1, needMaxBound);
    if (splitKeys.size()) {
        return std::move(splitKeys.front());
    }

    return needMaxBound ? max : min;
}

/**
 * If `max` is the max bound of some chunk, returns that chunk. Otherwise, returns the chunk that
 * contains the key `max`.
 */
Chunk getChunkForMaxBound(const ChunkManager& cm, const BSONObj& max) {
    boost::optional<Chunk> chunkWithMaxBound;
    cm.forEachChunk([&](const auto& chunk) {
        if (chunk.getMax().woCompare(max) == 0) {
            chunkWithMaxBound.emplace(chunk);
            return false;
        }
        return true;
    });
    if (chunkWithMaxBound) {
        return *chunkWithMaxBound;
    }
    return cm.findIntersectingChunkWithSimpleCollation(max);
}

MONGO_FAIL_POINT_DEFINE(moveChunkHangAtStep1);
MONGO_FAIL_POINT_DEFINE(moveChunkHangAtStep2);
MONGO_FAIL_POINT_DEFINE(moveChunkHangAtStep3);
MONGO_FAIL_POINT_DEFINE(moveChunkHangAtStep4);
MONGO_FAIL_POINT_DEFINE(moveChunkHangAtStep5);
MONGO_FAIL_POINT_DEFINE(moveChunkHangAtStep6);

MONGO_FAIL_POINT_DEFINE(failMigrationCommit);
MONGO_FAIL_POINT_DEFINE(hangBeforeLeavingCriticalSection);
MONGO_FAIL_POINT_DEFINE(migrationCommitNetworkError);
MONGO_FAIL_POINT_DEFINE(hangBeforePostMigrationCommitRefresh);

}  // namespace

MigrationSourceManager* MigrationSourceManager::get(const CollectionShardingRuntime& csr) {
    return msmForCsr(csr);
}

std::shared_ptr<MigrationChunkClonerSource> MigrationSourceManager::getCurrentCloner(
    const CollectionShardingRuntime& csr) {
    auto msm = get(csr);
    if (!msm)
        return nullptr;
    return msm->_cloneDriver;
}

// static
bool MigrationSourceManager::isMigrating(OperationContext* opCtx,
                                         NamespaceString const& nss,
                                         BSONObj const& docToDelete) {
    const auto scopedCsr =
        CollectionShardingRuntime::assertCollectionLockedAndAcquireShared(opCtx, nss);
    auto cloner = MigrationSourceManager::getCurrentCloner(*scopedCsr);

    return cloner && cloner->isDocumentInMigratingChunk(docToDelete);
}

MigrationSourceManager::MigrationSourceManager(OperationContext* opCtx,
                                               ShardsvrMoveRange&& request,
                                               WriteConcernOptions&& writeConcern,
                                               ConnectionString donorConnStr,
                                               HostAndPort recipientHost)
    : _opCtx(opCtx),
      _args(request),
      _writeConcern(writeConcern),
      _donorConnStr(std::move(donorConnStr)),
      _recipientHost(std::move(recipientHost)),
      _stats(ShardingStatistics::get(_opCtx)),
      _critSecReason(BSON("command"
                          << "moveChunk"
                          << "fromShard" << _args.getFromShard() << "toShard"
                          << _args.getToShard())),
      _moveTimingHelper(_opCtx,
                        "from",
                        NamespaceStringUtil::serialize(_args.getCommandParameter()),
                        _args.getMin(),
                        _args.getMax(),
                        6,  // Total number of steps
                        &kEmptyErrMsgForMoveTimingHelper,
                        _args.getToShard(),
                        _args.getFromShard()) {
    invariant(!_opCtx->lockState()->isLocked());

    LOGV2(22016,
          "Starting chunk migration donation {requestParameters} with expected collection epoch "
          "{collectionEpoch}",
          "Starting chunk migration donation",
          "requestParameters"_attr = redact(_args.toBSON({})),
          "collectionEpoch"_attr = _args.getEpoch());

    _moveTimingHelper.done(1);
    moveChunkHangAtStep1.pauseWhileSet();

    // Make sure the latest placement version is recovered as of the time of the invocation of the
    // command.
    onCollectionPlacementVersionMismatch(_opCtx, nss(), boost::none);

    const auto shardId = ShardingState::get(opCtx)->shardId();

    // Complete any unfinished migration pending recovery
    {
        migrationutil::drainMigrationsPendingRecovery(opCtx);

        // Since the moveChunk command is holding the ActiveMigrationRegistry and we just drained
        // all migrations pending recovery, now there cannot be any document in
        // config.migrationCoordinators.
        PersistentTaskStore<MigrationCoordinatorDocument> store(
            NamespaceString::kMigrationCoordinatorsNamespace);
        invariant(store.count(opCtx) == 0);
    }

    // Snapshot the committed metadata from the time the migration starts
    const auto [collectionMetadata, collectionIndexInfo, collectionUUID] = [&] {
        // TODO (SERVER-71444): Fix to be interruptible or document exception.
        UninterruptibleLockGuard noInterrupt(_opCtx->lockState());  // NOLINT.
        AutoGetCollection autoColl(_opCtx, nss(), MODE_IS);
        auto scopedCsr =
            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(opCtx, nss());

        const auto [metadata, indexInfo] =
            checkCollectionIdentity(_opCtx, nss(), _args.getEpoch(), boost::none);

        UUID collectionUUID = autoColl.getCollection()->uuid();

        // Atomically (still under the CSR lock held above) check whether migrations are allowed and
        // register the MigrationSourceManager on the CSR. This ensures that interruption due to the
        // change of allowMigrations to false will properly serialise and not allow any new MSMs to
        // be running after the change.
        uassert(ErrorCodes::ConflictingOperationInProgress,
                "Collection is undergoing changes so moveChunk is not allowed.",
                metadata.allowMigrations());

        _scopedRegisterer.emplace(this, *scopedCsr);

        return std::make_tuple(
            std::move(metadata), std::move(indexInfo), std::move(collectionUUID));
    }();

    // Compute the max or min bound in case only one is set (moveRange)
    if (!_args.getMax().has_value()) {
        const auto& min = *_args.getMin();

        const auto cm = collectionMetadata.getChunkManager();
        const auto owningChunk = cm->findIntersectingChunkWithSimpleCollation(min);
        const auto max = computeOtherBound(_opCtx,
                                           nss(),
                                           min,
                                           owningChunk.getMax(),
                                           cm->getShardKeyPattern(),
                                           _args.getMaxChunkSizeBytes(),
                                           true /* needMaxBound */);
        _args.getMoveRangeRequestBase().setMax(max);
        _moveTimingHelper.setMax(max);
    } else if (!_args.getMin().has_value()) {
        const auto& max = *_args.getMax();

        const auto cm = collectionMetadata.getChunkManager();
        const auto owningChunk = getChunkForMaxBound(*cm, max);
        const auto min = computeOtherBound(_opCtx,
                                           nss(),
                                           owningChunk.getMin(),
                                           max,
                                           cm->getShardKeyPattern(),
                                           _args.getMaxChunkSizeBytes(),
                                           false /* needMaxBound */);

        _args.getMoveRangeRequestBase().setMin(min);
        _moveTimingHelper.setMin(min);
    }

    checkShardKeyPattern(_opCtx,
                         nss(),
                         collectionMetadata,
                         collectionIndexInfo,
                         ChunkRange(*_args.getMin(), *_args.getMax()));
    checkRangeWithinChunk(_opCtx,
                          nss(),
                          collectionMetadata,
                          collectionIndexInfo,
                          ChunkRange(*_args.getMin(), *_args.getMax()));

    _collectionEpoch = _args.getEpoch();
    _collectionUUID = collectionUUID;

    _chunkVersion = collectionMetadata.getChunkManager()
                        ->findIntersectingChunkWithSimpleCollation(*_args.getMin())
                        .getLastmod();

    _moveTimingHelper.done(2);
    moveChunkHangAtStep2.pauseWhileSet();
}

MigrationSourceManager::~MigrationSourceManager() {
    invariant(!_cloneDriver);
    _stats.totalDonorMoveChunkTimeMillis.addAndFetch(_entireOpTimer.millis());

    _completion.emplaceValue();
}

void MigrationSourceManager::startClone() {
    invariant(!_opCtx->lockState()->isLocked());
    invariant(_state == kCreated);
    ScopeGuard scopedGuard([&] { _cleanupOnError(); });
    _stats.countDonorMoveChunkStarted.addAndFetch(1);

    uassertStatusOK(ShardingLogging::get(_opCtx)->logChangeChecked(
        _opCtx,
        "moveChunk.start",
        NamespaceStringUtil::serialize(nss()),
        BSON("min" << *_args.getMin() << "max" << *_args.getMax() << "from" << _args.getFromShard()
                   << "to" << _args.getToShard()),
        ShardingCatalogClient::kMajorityWriteConcern));

    _cloneAndCommitTimer.reset();

    auto replCoord = repl::ReplicationCoordinator::get(_opCtx);
    auto replEnabled = replCoord->getSettings().isReplSet();

    {
        const auto metadata = _getCurrentMetadataAndCheckEpoch();

        AutoGetCollection autoColl(_opCtx,
                                   nss(),
                                   replEnabled ? MODE_IX : MODE_X,
                                   AutoGetCollection::Options{}.deadline(
                                       _opCtx->getServiceContext()->getPreciseClockSource()->now() +
                                       Milliseconds(migrationLockAcquisitionMaxWaitMS.load())));

        auto scopedCsr =
            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(_opCtx, nss());

        // Having the metadata manager registered on the collection sharding state is what indicates
        // that a chunk on that collection is being migrated to the OpObservers. With an active
        // migration, write operations require the cloner to be present in order to track changes to
        // the chunk which needs to be transmitted to the recipient.
        _cloneDriver = std::make_shared<MigrationChunkClonerSource>(
            _opCtx, _args, _writeConcern, metadata.getKeyPattern(), _donorConnStr, _recipientHost);

        _coordinator.emplace(_cloneDriver->getSessionId(),
                             _args.getFromShard(),
                             _args.getToShard(),
                             nss(),
                             *_collectionUUID,
                             ChunkRange(*_args.getMin(), *_args.getMax()),
                             *_chunkVersion,
                             KeyPattern(metadata.getKeyPattern()),
                             _args.getWaitForDelete());

        _state = kCloning;
    }

    if (replEnabled) {
        auto const readConcernArgs = repl::ReadConcernArgs(
            replCoord->getMyLastAppliedOpTime(), repl::ReadConcernLevel::kLocalReadConcern);
        uassertStatusOK(waitForReadConcern(_opCtx, readConcernArgs, DatabaseName(), false));

        setPrepareConflictBehaviorForReadConcern(
            _opCtx, readConcernArgs, PrepareConflictBehavior::kEnforce);
    }

    _coordinator->startMigration(_opCtx);

    uassertStatusOK(_cloneDriver->startClone(_opCtx,
                                             _coordinator->getMigrationId(),
                                             _coordinator->getLsid(),
                                             _coordinator->getTxnNumber()));

    _moveTimingHelper.done(3);
    moveChunkHangAtStep3.pauseWhileSet();
    scopedGuard.dismiss();
}

void MigrationSourceManager::awaitToCatchUp() {
    invariant(!_opCtx->lockState()->isLocked());
    invariant(_state == kCloning);
    ScopeGuard scopedGuard([&] { _cleanupOnError(); });
    _stats.totalDonorChunkCloneTimeMillis.addAndFetch(_cloneAndCommitTimer.millis());
    _cloneAndCommitTimer.reset();

    // Block until the cloner deems it appropriate to enter the critical section.
    uassertStatusOK(_cloneDriver->awaitUntilCriticalSectionIsAppropriate(
        _opCtx, kMaxWaitToEnterCriticalSectionTimeout));

    _state = kCloneCaughtUp;
    _moveTimingHelper.done(4);
    moveChunkHangAtStep4.pauseWhileSet(_opCtx);
    scopedGuard.dismiss();
}

void MigrationSourceManager::enterCriticalSection() {
    invariant(!_opCtx->lockState()->isLocked());
    invariant(_state == kCloneCaughtUp);
    ScopeGuard scopedGuard([&] { _cleanupOnError(); });
    _stats.totalDonorChunkCloneTimeMillis.addAndFetch(_cloneAndCommitTimer.millis());
    _cloneAndCommitTimer.reset();

    const auto& metadata = _getCurrentMetadataAndCheckEpoch();

    // Check that there are no chunks on the recepient shard. Write an oplog event for change
    // streams if this is the first migration to the recipient.
    if (!metadata.getChunkManager()->getVersion(_args.getToShard()).isSet()) {
        migrationutil::notifyChangeStreamsOnRecipientFirstChunk(
            _opCtx, nss(), _args.getFromShard(), _args.getToShard(), _collectionUUID);

        // Wait for the above 'migrateChunkToNewShard' oplog message to be majority acknowledged.
        WriteConcernResult ignoreResult;
        auto latestOpTime = repl::ReplClientInfo::forClient(_opCtx->getClient()).getLastOp();
        uassertStatusOK(waitForWriteConcern(
            _opCtx, latestOpTime, WriteConcerns::kMajorityWriteConcernNoTimeout, &ignoreResult));
    }

    LOGV2_DEBUG_OPTIONS(4817402,
                        2,
                        {logv2::LogComponent::kShardMigrationPerf},
                        "Starting critical section",
                        "migrationId"_attr = _coordinator->getMigrationId());

    _critSec.emplace(_opCtx, nss(), _critSecReason);

    _state = kCriticalSection;

    // Persist a signal to secondaries that we've entered the critical section. This is will cause
    // secondaries to refresh their routing table when next accessed, which will block behind the
    // critical section. This ensures causal consistency by preventing a stale mongos with a cluster
    // time inclusive of the migration config commit update from accessing secondary data.
    // Note: this write must occur after the critSec flag is set, to ensure the secondary refresh
    // will stall behind the flag.
    uassertStatusOKWithContext(
        shardmetadatautil::updateShardCollectionsEntry(
            _opCtx,
            BSON(ShardCollectionType::kNssFieldName << NamespaceStringUtil::serialize(nss())),
            BSON("$inc" << BSON(ShardCollectionType::kEnterCriticalSectionCounterFieldName << 1)),
            false /*upsert*/),
        "Persist critical section signal for secondaries");

    LOGV2(22017,
          "Migration successfully entered critical section",
          "migrationId"_attr = _coordinator->getMigrationId());

    scopedGuard.dismiss();
}

void MigrationSourceManager::commitChunkOnRecipient() {
    invariant(!_opCtx->lockState()->isLocked());
    invariant(_state == kCriticalSection);
    ScopeGuard scopedGuard([&] {
        _cleanupOnError();
        migrationutil::asyncRecoverMigrationUntilSuccessOrStepDown(_opCtx,
                                                                   _args.getCommandParameter());
    });

    // Tell the recipient shard to fetch the latest changes.
    auto commitCloneStatus = _cloneDriver->commitClone(_opCtx);

    if (MONGO_unlikely(failMigrationCommit.shouldFail()) && commitCloneStatus.isOK()) {
        commitCloneStatus = {ErrorCodes::InternalError,
                             "Failing _recvChunkCommit due to failpoint."};
    }

    uassertStatusOKWithContext(commitCloneStatus, "commit clone failed");
    _recipientCloneCounts = commitCloneStatus.getValue()["counts"].Obj().getOwned();

    _state = kCloneCompleted;
    _moveTimingHelper.done(5);
    moveChunkHangAtStep5.pauseWhileSet();
    scopedGuard.dismiss();
}

void MigrationSourceManager::commitChunkMetadataOnConfig() {
    invariant(!_opCtx->lockState()->isLocked());
    invariant(_state == kCloneCompleted);

    ScopeGuard scopedGuard([&] {
        _cleanupOnError();
        migrationutil::asyncRecoverMigrationUntilSuccessOrStepDown(_opCtx, nss());
    });

    // If we have chunks left on the FROM shard, bump the version of one of them as well. This will
    // change the local collection major version, which indicates to other processes that the chunk
    // metadata has changed and they should refresh.
    BSONObjBuilder builder;

    {
        const auto metadata = _getCurrentMetadataAndCheckEpoch();

        auto migratedChunk = MigratedChunkType(*_chunkVersion, *_args.getMin(), *_args.getMax());

        CommitChunkMigrationRequest request(nss(),
                                            _args.getFromShard(),
                                            _args.getToShard(),
                                            migratedChunk,
                                            metadata.getCollPlacementVersion());

        request.serialize({}, &builder);
        builder.append(kWriteConcernField, kMajorityWriteConcern.toBSON());
    }

    // Read operations must begin to wait on the critical section just before we send the commit
    // operation to the config server
    _critSec->enterCommitPhase();

    _state = kCommittingOnConfig;

    Timer t;

    auto commitChunkMigrationResponse =
        Grid::get(_opCtx)->shardRegistry()->getConfigShard()->runCommandWithFixedRetryAttempts(
            _opCtx,
            ReadPreferenceSetting{ReadPreference::PrimaryOnly},
            "admin",
            builder.obj(),
            Shard::RetryPolicy::kIdempotent);

    if (MONGO_unlikely(migrationCommitNetworkError.shouldFail())) {
        commitChunkMigrationResponse = Status(
            ErrorCodes::InternalError, "Failpoint 'migrationCommitNetworkError' generated error");
    }

    Status migrationCommitStatus =
        Shard::CommandResponse::getEffectiveStatus(commitChunkMigrationResponse);

    if (!migrationCommitStatus.isOK()) {
        {
            // TODO (SERVER-71444): Fix to be interruptible or document exception.
            UninterruptibleLockGuard noInterrupt(_opCtx->lockState());  // NOLINT.
            AutoGetCollection autoColl(_opCtx, nss(), MODE_IX);
            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(_opCtx, nss())
                ->clearFilteringMetadata(_opCtx);
        }
        scopedGuard.dismiss();
        _cleanup(false);
        migrationutil::asyncRecoverMigrationUntilSuccessOrStepDown(_opCtx, nss());
        uassertStatusOK(migrationCommitStatus);
    }

    // Asynchronously tell the recipient to release its critical section
    _coordinator->launchReleaseRecipientCriticalSection(_opCtx);

    hangBeforePostMigrationCommitRefresh.pauseWhileSet();

    try {
        LOGV2_DEBUG_OPTIONS(4817404,
                            2,
                            {logv2::LogComponent::kShardMigrationPerf},
                            "Starting post-migration commit refresh on the shard",
                            "migrationId"_attr = _coordinator->getMigrationId());

        forceShardFilteringMetadataRefresh(_opCtx, nss());

        LOGV2_DEBUG_OPTIONS(4817405,
                            2,
                            {logv2::LogComponent::kShardMigrationPerf},
                            "Finished post-migration commit refresh on the shard",
                            "migrationId"_attr = _coordinator->getMigrationId());
    } catch (const DBException& ex) {
        LOGV2_DEBUG_OPTIONS(4817410,
                            2,
                            {logv2::LogComponent::kShardMigrationPerf},
                            "Finished post-migration commit refresh on the shard with error",
                            "migrationId"_attr = _coordinator->getMigrationId(),
                            "error"_attr = redact(ex));
        {
            // TODO (SERVER-71444): Fix to be interruptible or document exception.
            UninterruptibleLockGuard noInterrupt(_opCtx->lockState());  // NOLINT.
            AutoGetCollection autoColl(_opCtx, nss(), MODE_IX);
            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(_opCtx, nss())
                ->clearFilteringMetadata(_opCtx);
        }
        scopedGuard.dismiss();
        _cleanup(false);
        // Best-effort recover of the chunk version.
        onCollectionPlacementVersionMismatchNoExcept(_opCtx, nss(), boost::none).ignore();
        throw;
    }

    // Migration succeeded

    const auto refreshedMetadata = _getCurrentMetadataAndCheckEpoch();
    // Check if there are no chunks left on donor shard. Write an oplog event for change streams if
    // the last chunk migrated off the donor.
    if (!refreshedMetadata.getChunkManager()->getVersion(_args.getFromShard()).isSet()) {
        migrationutil::notifyChangeStreamsOnDonorLastChunk(
            _opCtx, nss(), _args.getFromShard(), _collectionUUID);
    }


    LOGV2(22018,
          "Migration succeeded and updated collection placement version to "
          "{updatedCollectionPlacementVersion}",
          "Migration succeeded and updated collection placement version",
          "updatedCollectionPlacementVersion"_attr = refreshedMetadata.getCollPlacementVersion(),
          "migrationId"_attr = _coordinator->getMigrationId());

    // If the migration has succeeded, clear the BucketCatalog so that the buckets that got migrated
    // out are no longer updatable.
    if (nss().isTimeseriesBucketsCollection()) {
        auto& bucketCatalog = timeseries::bucket_catalog::BucketCatalog::get(_opCtx);
        clear(bucketCatalog, nss().getTimeseriesViewNamespace());
    }

    _coordinator->setMigrationDecision(DecisionEnum::kCommitted);

    hangBeforeLeavingCriticalSection.pauseWhileSet();

    scopedGuard.dismiss();

    _stats.totalCriticalSectionCommitTimeMillis.addAndFetch(t.millis());

    LOGV2(6107801,
          "Exiting commit critical section",
          "migrationId"_attr = _coordinator->getMigrationId(),
          "durationMillis"_attr = t.millis());

    // Exit the critical section and ensure that all the necessary state is fully persisted before
    // scheduling orphan cleanup.
    _cleanup(true);

    ShardingLogging::get(_opCtx)->logChange(
        _opCtx,
        "moveChunk.commit",
        NamespaceStringUtil::serialize(nss()),
        BSON("min" << *_args.getMin() << "max" << *_args.getMax() << "from" << _args.getFromShard()
                   << "to" << _args.getToShard() << "counts" << *_recipientCloneCounts),
        ShardingCatalogClient::kMajorityWriteConcern);

    const ChunkRange range(*_args.getMin(), *_args.getMax());

    std::string orphanedRangeCleanUpErrMsg = str::stream()
        << "Moved chunks successfully but failed to clean up " << nss().toStringForErrorMsg()
        << " range " << redact(range.toString()) << " due to: ";

    if (_args.getWaitForDelete()) {
        LOGV2(22019,
              "Waiting for migration cleanup after chunk commit for the namespace {namespace} "
              "and range {range}",
              "Waiting for migration cleanup after chunk commit",
              logAttrs(nss()),
              "range"_attr = redact(range.toString()),
              "migrationId"_attr = _coordinator->getMigrationId());

        Status deleteStatus = _cleanupCompleteFuture
            ? _cleanupCompleteFuture->getNoThrow(_opCtx)
            : Status(ErrorCodes::Error(5089002),
                     "Not honouring the 'waitForDelete' request because migration coordinator "
                     "cleanup didn't succeed");
        if (!deleteStatus.isOK()) {
            uasserted(ErrorCodes::OrphanedRangeCleanUpFailed,
                      orphanedRangeCleanUpErrMsg + redact(deleteStatus));
        }
    }

    _moveTimingHelper.done(6);
    moveChunkHangAtStep6.pauseWhileSet();
}

void MigrationSourceManager::_cleanupOnError() noexcept {
    if (_state == kDone) {
        return;
    }

    ShardingLogging::get(_opCtx)->logChange(
        _opCtx,
        "moveChunk.error",
        NamespaceStringUtil::serialize(_args.getCommandParameter()),
        BSON("min" << *_args.getMin() << "max" << *_args.getMax() << "from" << _args.getFromShard()
                   << "to" << _args.getToShard()),
        ShardingCatalogClient::kMajorityWriteConcern);

    _cleanup(true);
}

SharedSemiFuture<void> MigrationSourceManager::abort() {
    stdx::lock_guard<Client> lk(*_opCtx->getClient());
    _opCtx->markKilled();
    _stats.countDonorMoveChunkAbortConflictingIndexOperation.addAndFetch(1);

    return _completion.getFuture();
}

CollectionMetadata MigrationSourceManager::_getCurrentMetadataAndCheckEpoch() {
    auto metadata = [&] {
        // TODO (SERVER-71444): Fix to be interruptible or document exception.
        UninterruptibleLockGuard noInterrupt(_opCtx->lockState());  // NOLINT.
        AutoGetCollection autoColl(_opCtx, _args.getCommandParameter(), MODE_IS);
        const auto scopedCsr = CollectionShardingRuntime::assertCollectionLockedAndAcquireShared(
            _opCtx, _args.getCommandParameter());

        const auto optMetadata = scopedCsr->getCurrentMetadataIfKnown();
        uassert(ErrorCodes::ConflictingOperationInProgress,
                "The collection's sharding state was cleared by a concurrent operation",
                optMetadata);
        return *optMetadata;
    }();

    uassert(ErrorCodes::ConflictingOperationInProgress,
            str::stream() << "The collection's epoch has changed since the migration began. "
                             "Expected collection epoch: "
                          << _collectionEpoch->toString() << ", but found: "
                          << (metadata.isSharded()
                                  ? metadata.getCollPlacementVersion().epoch().toString()
                                  : "unsharded collection"),
            metadata.isSharded() &&
                metadata.getCollPlacementVersion().epoch() == *_collectionEpoch);

    return metadata;
}

void MigrationSourceManager::_cleanup(bool completeMigration) noexcept {
    invariant(_state != kDone);

    auto cloneDriver = [&]() {
        // Unregister from the collection's sharding state and exit the migration critical section.
        // TODO (SERVER-71444): Fix to be interruptible or document exception.
        UninterruptibleLockGuard noInterrupt(_opCtx->lockState());  // NOLINT.
        AutoGetCollection autoColl(_opCtx, nss(), MODE_IX);
        auto scopedCsr =
            CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(_opCtx, nss());

        if (_state != kCreated) {
            invariant(_cloneDriver);
        }

        _critSec.reset();
        return std::move(_cloneDriver);
    }();

    if (_state == kCriticalSection || _state == kCloneCompleted || _state == kCommittingOnConfig) {
        LOGV2_DEBUG_OPTIONS(4817403,
                            2,
                            {logv2::LogComponent::kShardMigrationPerf},
                            "Finished critical section",
                            "migrationId"_attr = _coordinator->getMigrationId());

        LOGV2(6107802,
              "Finished critical section",
              "migrationId"_attr = _coordinator->getMigrationId(),
              "durationMillis"_attr = _cloneAndCommitTimer.millis());
    }

    // The cleanup operations below are potentially blocking or acquire other locks, so perform them
    // outside of the collection X lock

    if (cloneDriver) {
        cloneDriver->cancelClone(_opCtx);
    }

    try {
        if (_state >= kCloning) {
            invariant(_coordinator);
            if (_state < kCommittingOnConfig) {
                _coordinator->setMigrationDecision(DecisionEnum::kAborted);
            }

            auto newClient = _opCtx->getServiceContext()->makeClient("MigrationCoordinator");
            AlternativeClientRegion acr(newClient);
            auto newOpCtxPtr = cc().makeOperationContext();
            auto newOpCtx = newOpCtxPtr.get();

            if (_state >= kCriticalSection && _state <= kCommittingOnConfig) {
                _stats.totalCriticalSectionTimeMillis.addAndFetch(_cloneAndCommitTimer.millis());

                // Wait for the updates to the cache of the routing table to be fully written to
                // disk. This way, we ensure that all nodes from a shard which donated a chunk will
                // always be at the placement version of the last migration it performed.
                //
                // If the metadata is not persisted before clearing the 'inMigration' flag below, it
                // is possible that the persisted metadata is rolled back after step down, but the
                // write which cleared the 'inMigration' flag is not, a secondary node will report
                // itself at an older placement version.
                CatalogCacheLoader::get(newOpCtx).waitForCollectionFlush(newOpCtx, nss());
            }
            if (completeMigration) {
                // This can be called on an exception path after the OperationContext has been
                // interrupted, so use a new OperationContext. Note, it's valid to call
                // getServiceContext on an interrupted OperationContext.
                _cleanupCompleteFuture = _coordinator->completeMigration(newOpCtx);
            }
        }

        _state = kDone;
    } catch (const DBException& ex) {
        LOGV2_WARNING(5089001,
                      "Failed to complete the migration {migrationId} with "
                      "{chunkMigrationRequestParameters} due to: {error}",
                      "Failed to complete the migration",
                      "chunkMigrationRequestParameters"_attr = redact(_args.toBSON({})),
                      "error"_attr = redact(ex),
                      "migrationId"_attr = _coordinator->getMigrationId());
        // Something went really wrong when completing the migration just unset the metadata and let
        // the next op to recover.
        // TODO (SERVER-71444): Fix to be interruptible or document exception.
        UninterruptibleLockGuard noInterrupt(_opCtx->lockState());  // NOLINT.
        AutoGetCollection autoColl(_opCtx, nss(), MODE_IX);
        CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(_opCtx, nss())
            ->clearFilteringMetadata(_opCtx);
    }
}

BSONObj MigrationSourceManager::getMigrationStatusReport(
    const CollectionShardingRuntime::ScopedSharedCollectionShardingRuntime& scopedCsrLock) const {
    boost::optional<long long> sessionOplogEntriesToBeMigratedSoFar;
    boost::optional<long long> sessionOplogEntriesSkippedSoFarLowerBound;

    if (_cloneDriver) {
        sessionOplogEntriesToBeMigratedSoFar =
            _cloneDriver->getSessionOplogEntriesToBeMigratedSoFar();
        sessionOplogEntriesSkippedSoFarLowerBound =
            _cloneDriver->getSessionOplogEntriesSkippedSoFarLowerBound();
    }

    return migrationutil::makeMigrationStatusDocumentSource(
        _args.getCommandParameter(),
        _args.getFromShard(),
        _args.getToShard(),
        true,
        _args.getMin().value_or(BSONObj()),
        _args.getMax().value_or(BSONObj()),
        sessionOplogEntriesToBeMigratedSoFar,
        sessionOplogEntriesSkippedSoFarLowerBound);
}

MigrationSourceManager::ScopedRegisterer::ScopedRegisterer(MigrationSourceManager* msm,
                                                           CollectionShardingRuntime& csr)
    : _msm(msm) {
    invariant(nullptr == std::exchange(msmForCsr(csr), msm));
}

MigrationSourceManager::ScopedRegisterer::~ScopedRegisterer() {
    // TODO (SERVER-71444): Fix to be interruptible or document exception.
    UninterruptibleLockGuard noInterrupt(_msm->_opCtx->lockState());  // NOLINT.
    AutoGetCollection autoColl(_msm->_opCtx, _msm->_args.getCommandParameter(), MODE_IX);
    auto scopedCsr = CollectionShardingRuntime::assertCollectionLockedAndAcquireExclusive(
        _msm->_opCtx, _msm->_args.getCommandParameter());
    invariant(_msm == std::exchange(msmForCsr(*scopedCsr), nullptr));
}

}  // namespace mongo
