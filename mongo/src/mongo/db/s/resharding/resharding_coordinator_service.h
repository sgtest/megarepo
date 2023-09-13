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

#include <memory>
#include <utility>
#include <vector>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/cancelable_operation_context.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/process_interface/mongo_process_interface.h"
#include "mongo/db/repl/primary_only_service.h"
#include "mongo/db/s/resharding/coordinator_document_gen.h"
#include "mongo/db/s/resharding/resharding_coordinator_observer.h"
#include "mongo/db/s/resharding/resharding_metrics.h"
#include "mongo/db/service_context.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/shard_id.h"
#include "mongo/executor/async_rpc.h"
#include "mongo/executor/scoped_task_executor.h"
#include "mongo/executor/task_executor.h"
#include "mongo/platform/mutex.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/catalog/type_collection.h"
#include "mongo/s/catalog/type_tags.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/index_version.h"
#include "mongo/s/resharding/common_types_gen.h"
#include "mongo/util/cancellation.h"
#include "mongo/util/concurrency/thread_pool.h"
#include "mongo/util/future.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace resharding {
class CoordinatorCommitMonitor;

CollectionType createTempReshardingCollectionType(
    OperationContext* opCtx,
    const ReshardingCoordinatorDocument& coordinatorDoc,
    const ChunkVersion& chunkVersion,
    const BSONObj& collation,
    boost::optional<CollectionIndexes> indexVersion,
    boost::optional<bool> isUnsplittable);

void removeChunkDocs(OperationContext* opCtx, const UUID& collUUID);

void writeDecisionPersistedState(OperationContext* opCtx,
                                 ReshardingMetrics* metrics,
                                 const ReshardingCoordinatorDocument& coordinatorDoc,
                                 OID newCollectionEpoch,
                                 Timestamp newCollectionTimestamp,
                                 boost::optional<CollectionIndexes> indexVersion,
                                 const std::vector<ShardId>& reshardedCollectionPlacement);

void updateTagsDocsForTempNss(OperationContext* opCtx,
                              const ReshardingCoordinatorDocument& coordinatorDoc,
                              TxnNumber txnNumber);

void insertCoordDocAndChangeOrigCollEntry(OperationContext* opCtx,
                                          ReshardingMetrics* metrics,
                                          const ReshardingCoordinatorDocument& coordinatorDoc);

void writeParticipantShardsAndTempCollInfo(OperationContext* opCtx,
                                           ReshardingMetrics* metrics,
                                           const ReshardingCoordinatorDocument& coordinatorDoc,
                                           std::vector<ChunkType> initialChunks,
                                           std::vector<BSONObj> zones,
                                           boost::optional<CollectionIndexes> indexVersion,
                                           boost::optional<bool> isUnsplittable);

void writeStateTransitionAndCatalogUpdatesThenBumpCollectionPlacementVersions(
    OperationContext* opCtx,
    ReshardingMetrics* metrics,
    const ReshardingCoordinatorDocument& coordinatorDoc);

ReshardingCoordinatorDocument removeOrQuiesceCoordinatorDocAndRemoveReshardingFields(
    OperationContext* opCtx,
    ReshardingMetrics* metrics,
    const ReshardingCoordinatorDocument& coordinatorDoc,
    boost::optional<Status> abortReason = boost::none);
}  // namespace resharding

class ReshardingCoordinatorExternalState {
public:
    struct ParticipantShardsAndChunks {
        std::vector<DonorShardEntry> donorShards;
        std::vector<RecipientShardEntry> recipientShards;
        std::vector<ChunkType> initialChunks;
    };

    virtual ~ReshardingCoordinatorExternalState() = default;

    virtual ParticipantShardsAndChunks calculateParticipantShardsAndChunks(
        OperationContext* opCtx, const ReshardingCoordinatorDocument& coordinatorDoc) = 0;

    ChunkVersion calculateChunkVersionForInitialChunks(OperationContext* opCtx);

    boost::optional<CollectionIndexes> getCatalogIndexVersion(OperationContext* opCtx,
                                                              const NamespaceString& nss,
                                                              const UUID& uuid);

    bool getIsUnsplittable(OperationContext* opCtx, const NamespaceString& nss);

    boost::optional<CollectionIndexes> getCatalogIndexVersionForCommit(OperationContext* opCtx,
                                                                       const NamespaceString& nss);

    template <typename CommandType>
    void sendCommandToShards(OperationContext* opCtx,
                             std::shared_ptr<async_rpc::AsyncRPCOptions<CommandType>> opts,
                             const std::vector<ShardId>& shardIds);
};

class ReshardingCoordinatorExternalStateImpl final : public ReshardingCoordinatorExternalState {
public:
    ParticipantShardsAndChunks calculateParticipantShardsAndChunks(
        OperationContext* opCtx, const ReshardingCoordinatorDocument& coordinatorDoc) override;
};

/**
 * Construct to encapsulate cancellation tokens and related semantics on the ReshardingCoordinator.
 */
class CoordinatorCancellationTokenHolder {
public:
    CoordinatorCancellationTokenHolder(CancellationToken stepdownToken)
        : _stepdownToken(stepdownToken),
          _abortSource(CancellationSource(stepdownToken)),
          _abortToken(_abortSource.token()),
          _commitMonitorCancellationSource(CancellationSource(_abortToken)),
          _quiesceCancellationSource(CancellationSource(_stepdownToken)) {}

    /**
     * Returns whether the any token has been canceled.
     */
    bool isCanceled() {
        return _stepdownToken.isCanceled() || _abortToken.isCanceled();
    }

    /**
     * Returns whether the abort token has been canceled, indicating that the resharding operation
     * was explicitly aborted by an external user.
     */
    bool isAborted() {
        return !_stepdownToken.isCanceled() && _abortToken.isCanceled();
    }

    /**
     * Returns whether the stepdownToken has been canceled, indicating that the shard's underlying
     * replica set node is stepping down or shutting down.
     */
    bool isSteppingOrShuttingDown() {
        return _stepdownToken.isCanceled();
    }

    /**
     * Cancels the source created by this class, in order to indicate to holders of the abortToken
     * that the resharding operation has been aborted.
     */
    void abort() {
        _abortSource.cancel();
    }

    void cancelCommitMonitor() {
        _commitMonitorCancellationSource.cancel();
    }

    void cancelQuiescePeriod() {
        _quiesceCancellationSource.cancel();
    }

    const CancellationToken& getStepdownToken() {
        return _stepdownToken;
    }

    const CancellationToken& getAbortToken() {
        return _abortToken;
    }

    CancellationToken getCommitMonitorToken() {
        return _commitMonitorCancellationSource.token();
    }

    CancellationToken getCancelQuiesceToken() {
        return _quiesceCancellationSource.token();
    }

private:
    // The token passed in by the PrimaryOnlyService runner that is canceled when this shard's
    // underlying replica set node is stepping down or shutting down.
    CancellationToken _stepdownToken;

    // The source created by inheriting from the stepdown token.
    CancellationSource _abortSource;

    // The token to wait on in cases where a user wants to wait on either a resharding operation
    // being aborted or the replica set node stepping/shutting down.
    CancellationToken _abortToken;

    // The source created by inheriting from the abort token.
    // Provides the means to cancel the commit monitor (e.g., due to receiving the commit command).
    CancellationSource _commitMonitorCancellationSource;

    // A source created by inheriting from the stepdown token.
    // Provides the means to cancel the quiesce period.
    CancellationSource _quiesceCancellationSource;
};

class ReshardingCoordinator;

class ReshardingCoordinatorService : public repl::PrimaryOnlyService {
public:
    static constexpr StringData kServiceName = "ReshardingCoordinatorService"_sd;

    explicit ReshardingCoordinatorService(ServiceContext* serviceContext)
        : PrimaryOnlyService(serviceContext), _serviceContext(serviceContext) {}
    ~ReshardingCoordinatorService() = default;

    friend ReshardingCoordinator;

    StringData getServiceName() const override {
        return kServiceName;
    }

    NamespaceString getStateDocumentsNS() const override {
        return NamespaceString::kConfigReshardingOperationsNamespace;
    }

    ThreadPool::Limits getThreadPoolLimits() const override;

    // The service implemented its own conflict check before this method was added.
    void checkIfConflictsWithOtherInstances(
        OperationContext* opCtx,
        BSONObj initialState,
        const std::vector<const PrimaryOnlyService::Instance*>& existingInstances) override;

    std::shared_ptr<PrimaryOnlyService::Instance> constructInstance(BSONObj initialState) override;

    std::vector<std::shared_ptr<PrimaryOnlyService::Instance>> getAllReshardingInstances(
        OperationContext* opCtx) {
        return getAllInstances(opCtx);
    }

    /**
     * Tries to abort all active reshardCollection operations. Note that this doesn't differentiate
     * between operations interrupted due to stepdown or abort. Callers who wish to confirm that
     * the abort successfully went through should follow up with an inspection on the resharding
     * coordinator docs to ensure that they are empty.
     *
     * This call skips quiesce periods for all aborted coordinators.
     */
    void abortAllReshardCollection(OperationContext* opCtx);

private:
    ExecutorFuture<void> _rebuildService(std::shared_ptr<executor::ScopedTaskExecutor> executor,
                                         const CancellationToken& token) override;

    ServiceContext* _serviceContext;
};

class ReshardingCoordinator final
    : public repl::PrimaryOnlyService::TypedInstance<ReshardingCoordinator> {
public:
    explicit ReshardingCoordinator(
        ReshardingCoordinatorService* coordinatorService,
        const ReshardingCoordinatorDocument& coordinatorDoc,
        std::shared_ptr<ReshardingCoordinatorExternalState> externalState,
        ServiceContext* serviceContext);
    ~ReshardingCoordinator() = default;

    SemiFuture<void> run(std::shared_ptr<executor::ScopedTaskExecutor> executor,
                         const CancellationToken& token) noexcept override;

    void interrupt(Status status) override {}

    /**
     * Attempts to cancel the underlying resharding operation using the abort token.
     * If 'skipQuiescePeriod' is set, will also skip the quiesce period used to allow retries.
     */
    void abort(bool skipQuiescePeriod = false);

    /**
     * Replace in-memory representation of the CoordinatorDoc
     */
    void installCoordinatorDoc(OperationContext* opCtx,
                               const ReshardingCoordinatorDocument& doc) noexcept;

    CommonReshardingMetadata getMetadata() const {
        return _metadata;
    }

    /**
     * Returns a Future that will be resolved when all work associated with this Instance has
     * completed running.
     */
    SharedSemiFuture<void> getCompletionFuture() const {
        return _completionPromise.getFuture();
    }

    /**
     * Returns a Future that will be resolved when the service has written the coordinator doc to
     * storage
     */
    SharedSemiFuture<void> getCoordinatorDocWrittenFuture() const {
        return _coordinatorDocWrittenPromise.getFuture();
    }

    /**
     * Returns a Future that will be resolved when the service has finished its quiesce period
     * and deleted the coordinator document.
     */
    SharedSemiFuture<void> getQuiescePeriodFinishedFuture() const {
        return _quiescePeriodFinishedPromise.getFuture();
    }

    boost::optional<BSONObj> reportForCurrentOp(
        MongoProcessInterface::CurrentOpConnectionsMode,
        MongoProcessInterface::CurrentOpSessionsMode) noexcept override;

    /**
     * This coordinator will not enter the critical section until this member
     * function is called at least once. There are two ways this is called:
     *
     * - Metrics-based heuristics will automatically call this at a strategic
     *   time chosen to minimize the critical section's time window.
     *
     * - The "commitReshardCollection" command is an elaborate wrapper for this
     *   function, providing a shortcut to make the critical section happen
     *   sooner, even if it takes longer to complete.
     */
    void onOkayToEnterCritical();

    std::shared_ptr<ReshardingCoordinatorObserver> getObserver();

    void checkIfOptionsConflict(const BSONObj& stateDoc) const final {}

private:
    struct ChunksAndZones {
        std::vector<ChunkType> initialChunks;
        std::vector<TagsType> newZones;
    };

    /**
     * Construct the initial chunks splits and write down the initial coordinator state to storage.
     */
    ExecutorFuture<void> _initializeCoordinator(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Runs resharding up through preparing to persist the decision.
     */
    ExecutorFuture<ReshardingCoordinatorDocument> _runUntilReadyToCommit(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor) noexcept;

    /**
     * Runs resharding through persisting the decision until cleanup.
     */
    ExecutorFuture<void> _commitAndFinishReshardOperation(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
        const ReshardingCoordinatorDocument& updatedCoordinatorDoc) noexcept;

    /**
     * Inform all of the donors and recipients of this resharding operation to begin.
     */
    ExecutorFuture<void> _tellAllParticipantsReshardingStarted(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Runs abort cleanup logic when only the coordinator is aware of the resharding operation.
     *
     * Only safe to call if an unrecoverable error is encountered before the coordinator completes
     * its transition to kPreparingToDonate.
     */
    ExecutorFuture<void> _onAbortCoordinatorOnly(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor, const Status& status);

    /*
     * Runs abort cleanup logic when both the coordinator and participants are aware of the
     * resharding operation.
     *
     * Only safe to call if the coordinator progressed past kInitializing before encountering an
     * unrecoverable error.
     */
    ExecutorFuture<void> _onAbortCoordinatorAndParticipants(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor, const Status& status);

    /**
     * Checks if the new shard key is same as the existing one in order to return early and avoid
     * redundant work.
     */
    ExecutorFuture<bool> _isReshardingOpRedundant(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Runs resharding operation to completion from _initializeCoordinator().
     */
    ExecutorFuture<void> _runReshardingOp(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Keep the instance in a quiesced state in order to handle retries.
     */
    ExecutorFuture<void> _quiesce(const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
                                  Status status);

    /**
     * Does the following writes:
     * 1. Inserts the coordinator document into config.reshardingOperations
     * 2. Adds reshardingFields to the config.collections entry for the original collection
     *
     * Transitions to 'kInitializing'.
     */
    void _insertCoordDocAndChangeOrigCollEntry();

    /**
     * Calculates the participant shards and target chunks under the new shard key, then does the
     * following writes:
     * 1. Updates the coordinator state to 'kPreparingToDonate'.
     * 2. Updates reshardingFields to reflect the state change on the original collection entry.
     * 3. Inserts an entry into config.collections for the temporary collection
     * 4. Inserts entries into config.chunks for ranges based on the new shard key
     * 5. Upserts entries into config.tags for any zones associated with the new shard key
     *
     * Transitions to 'kPreparingToDonate'.
     */
    void _calculateParticipantsAndChunksThenWriteToDisk();

    /**
     * Waits on _reshardingCoordinatorObserver to notify that all donors have picked a
     * minFetchTimestamp and are ready to donate. Transitions to 'kCloning'.
     */
    ExecutorFuture<void> _awaitAllDonorsReadyToDonate(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Starts a new coordinator commit monitor to periodically query recipient shards for the
     * remaining operation time, and engage the critical section as soon as the remaining time falls
     * below a configurable threshold (i.e., `remainingReshardingOperationTimeThresholdMillis`).
     */
    void _startCommitMonitor(const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Waits on _reshardingCoordinatorObserver to notify that all recipients have finished
     * cloning. Transitions to 'kApplying'.
     */
    ExecutorFuture<void> _awaitAllRecipientsFinishedCloning(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Waits on _reshardingCoordinatorObserver to notify that all recipients have finished
     * applying oplog entries. Transitions to 'kBlockingWrites'.
     */
    ExecutorFuture<void> _awaitAllRecipientsFinishedApplying(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Waits on _reshardingCoordinatorObserver to notify that all recipients have entered
     * strict-consistency.
     */
    ExecutorFuture<ReshardingCoordinatorDocument> _awaitAllRecipientsInStrictConsistency(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Does the following writes:
     * 1. Updates the config.collections entry for the new sharded collection
     * 2. Updates config.chunks entries for the new sharded collection
     *
     * Transitions to 'kCommitting'.
     */
    void _commit(const ReshardingCoordinatorDocument& updatedDoc);

    /**
     * Sends a _shardsvrNotifyShardingEvent commands to the primary shard of the resharded
     * collection to trigger the generation of the related internal op entry.
     */
    void _generateOpEventOnCoordinatingShard(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Waits on _reshardingCoordinatorObserver to notify that:
     * 1. All recipient shards have renamed the temporary collection to the original collection
     *    namespace or have finished aborting, and
     * 2. All donor shards that were not also recipient shards have dropped the original
     *    collection or have finished aborting.
     *
     * Transitions to 'kDone'.
     */
    ExecutorFuture<void> _awaitAllParticipantShardsDone(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Updates the entry for this resharding operation in config.reshardingOperations and the
     * catalog entries for the original and temporary namespaces in config.collections.
     */
    void _updateCoordinatorDocStateAndCatalogEntries(
        CoordinatorStateEnum nextState,
        ReshardingCoordinatorDocument coordinatorDoc,
        boost::optional<Timestamp> cloneTimestamp = boost::none,
        boost::optional<ReshardingApproxCopySize> approxCopySize = boost::none,
        boost::optional<Status> abortReason = boost::none);

    /**
     * Updates the entry for this resharding operation in config.reshardingOperations to the
     * quiesced state, or removes it if quiesce isn't being done.  Removes the resharding fields
     * from the catalog entries.
     */
    void _removeOrQuiesceCoordinatorDocAndRemoveReshardingFields(
        OperationContext* opCtx, boost::optional<Status> abortReason = boost::none);

    /**
     * Sends the command to the specified participants asynchronously.
     */
    template <typename CommandType>
    void _sendCommandToAllParticipants(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
        std::shared_ptr<async_rpc::AsyncRPCOptions<CommandType>> opts);
    template <typename CommandType>
    void _sendCommandToAllDonors(const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
                                 std::shared_ptr<async_rpc::AsyncRPCOptions<CommandType>> opts);
    template <typename CommandType>
    void _sendCommandToAllRecipients(const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
                                     std::shared_ptr<async_rpc::AsyncRPCOptions<CommandType>> opts);

    /**
     * Sends '_flushRoutingTableCacheUpdatesWithWriteConcern' to ensure donor state machine creation
     * by the time the refresh completes.
     */
    void _establishAllDonorsAsParticipants(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Sends '_flushRoutingTableCacheUpdatesWithWriteConcern' to ensure recipient state machine
     * creation by the time the refresh completes.
     */
    void _establishAllRecipientsAsParticipants(
        const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Sends '_flushReshardingStateChange' to all recipient shards.
     *
     * When the coordinator is in a state before 'kCommitting', refreshes the temporary
     * namespace. When the coordinator is in a state at or after 'kCommitting', refreshes the
     * original namespace.
     */
    void _tellAllRecipientsToRefresh(const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Sends '_flushReshardingStateChange' for the original namespace to all donor shards.
     */
    void _tellAllDonorsToRefresh(const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Sends '_shardsvrCommitReshardCollection' to all participant shards.
     */
    void _tellAllParticipantsToCommit(
        const NamespaceString& nss, const std::shared_ptr<executor::ScopedTaskExecutor>& executor);

    /**
     * Sends '_shardsvrAbortReshardCollection' to all participant shards.
     */
    void _tellAllParticipantsToAbort(const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
                                     bool isUserAborted);

    /**
     * Best effort attempt to update the chunk imbalance metrics.
     */
    void _updateChunkImbalanceMetrics(const NamespaceString& nss);

    /**
     * When called with Status::OK(), the coordinator will eventually enter the critical section.
     *
     * When called with an error Status, the coordinator will never enter the critical section.
     */
    void _fulfillOkayToEnterCritical(Status status);

    // Waits for majority replication of the latest opTime unless token is cancelled.
    SemiFuture<void> _waitForMajority(const CancellationToken& token);

    /**
     * Print a log containing the information of this resharding operation.
     */
    void _logStatsOnCompletion(bool success);

    // The unique key for a given resharding operation. InstanceID is an alias for BSONObj. The
    // value of this is the UUID that will be used as the collection UUID for the new sharded
    // collection. The object looks like: {_id: 'reshardingUUID'}
    const repl::PrimaryOnlyService::InstanceID _id;

    // The primary-only service instance corresponding to the coordinator instance. Not owned.
    ReshardingCoordinatorService* const _coordinatorService;

    ServiceContext* _serviceContext;

    std::shared_ptr<ReshardingMetrics> _metrics;

    // The in-memory representation of the immutable portion of the document in
    // config.reshardingOperations.
    const CommonReshardingMetadata _metadata;

    // Observes writes that indicate state changes for this resharding operation and notifies
    // 'this' when all donors/recipients have entered some state so that 'this' can transition
    // states.
    std::shared_ptr<ReshardingCoordinatorObserver> _reshardingCoordinatorObserver;

    // The updated coordinator state document.
    ReshardingCoordinatorDocument _coordinatorDoc;

    // Holds the cancellation tokens relevant to the ReshardingCoordinator.
    std::unique_ptr<CoordinatorCancellationTokenHolder> _ctHolder;

    // ThreadPool used by CancelableOperationContext.
    // CancelableOperationContext must have a thread that is always available to it to mark its
    // opCtx as killed when the cancelToken has been cancelled.
    const std::shared_ptr<ThreadPool> _markKilledExecutor;
    boost::optional<CancelableOperationContextFactory> _cancelableOpCtxFactory;

    /**
     * Must be locked while the `_canEnterCritical` promise is being fulfilled.
     */
    mutable Mutex _fulfillmentMutex =
        MONGO_MAKE_LATCH("ReshardingCoordinatorService::_fulfillmentMutex");

    /**
     * Must be locked while the _abortCalled is being set to true.
     */
    mutable Mutex _abortCalledMutex =
        MONGO_MAKE_LATCH("ReshardingCoordinatorService::_abortCalledMutex");


    /**
     * Coordinator does not enter the critical section until this is fulfilled.
     * Can be set by "commitReshardCollection" command or by metrics determining
     * that it's okay to proceed.
     */
    SharedPromise<void> _canEnterCritical;

    // Promise that is fulfilled when coordinator doc has been written.
    SharedPromise<void> _coordinatorDocWrittenPromise;

    // Promise that is fulfilled when the chain of work kicked off by run() has completed.
    SharedPromise<void> _completionPromise;

    // Promise that is fulfilled when the quiesce period is finished
    SharedPromise<void> _quiescePeriodFinishedPromise;

    // Callback handle for scheduled work to handle critical section timeout.
    boost::optional<executor::TaskExecutor::CallbackHandle> _criticalSectionTimeoutCbHandle;

    SharedSemiFuture<void> _commitMonitorQuiesced;
    std::shared_ptr<resharding::CoordinatorCommitMonitor> _commitMonitor;

    std::shared_ptr<ReshardingCoordinatorExternalState> _reshardingCoordinatorExternalState;

    // Used to catch the case when an abort() is called but the cancellation source (_ctHolder) has
    // not been initialized.
    enum AbortType {
        kNoAbort = 0,
        kAbortWithQuiesce,
        kAbortSkipQuiesce
    } _abortCalled{AbortType::kNoAbort};

    // If we recovered a completed resharding coordinator (quiesced) on failover, the
    // resharding status when it actually ran.
    boost::optional<Status> _originalReshardingStatus;
};

}  // namespace mongo
