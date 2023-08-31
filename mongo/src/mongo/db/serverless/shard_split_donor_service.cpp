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


#include "mongo/db/serverless/shard_split_donor_service.h"

// IWYU pragma: no_include "ext/alloc_traits.h"
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <boost/smart_ptr.hpp>
#include <string>
#include <tuple>

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/client/mongo_uri.h"
#include "mongo/client/replica_set_monitor_stats.h"
#include "mongo/client/sdam/sdam_configuration.h"
#include "mongo/client/sdam/topology_listener.h"
#include "mongo/client/server_discovery_monitor.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_write_path.h"
#include "mongo/db/catalog/local_oplog_info.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/index_builds_coordinator.h"
#include "mongo/db/ops/update_result.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/repl/repl_server_parameters_gen.h"
#include "mongo/db/repl/repl_set_config.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/tenant_migration_access_blocker_registry.h"
#include "mongo/db/repl/tenant_migration_donor_access_blocker.h"
#include "mongo/db/repl/wait_for_majority_service.h"
#include "mongo/db/s/resharding/resharding_util.h"
#include "mongo/db/server_options.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/serverless/shard_split_statistics.h"
#include "mongo/db/serverless/shard_split_utils.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/snapshot.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/tenant_id.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/executor/remote_command_request.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/rpc/get_status_from_command_result.h"
#include "mongo/s/database_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future_util.h"
#include "mongo/util/out_of_line_executor.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"
#include "mongo/util/timer.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTenantMigration


namespace mongo {

namespace {

MONGO_FAIL_POINT_DEFINE(abortShardSplitBeforeLeavingBlockingState);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitBeforeBlockingState);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitAfterBlocking);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitAfterRecipientCaughtUp);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitAfterDecision);
MONGO_FAIL_POINT_DEFINE(skipShardSplitGarbageCollectionTimeout);
MONGO_FAIL_POINT_DEFINE(skipShardSplitWaitForSplitAcceptance);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitBeforeRecipientCleanup);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitAfterMarkingStateGarbageCollectable);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitBeforeSplitConfigRemoval);
MONGO_FAIL_POINT_DEFINE(skipShardSplitRecipientCleanup);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitBeforeLeavingBlockingState);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitAfterUpdatingToCommittedState);
MONGO_FAIL_POINT_DEFINE(pauseShardSplitAfterReceivingAbortCmd);

const Backoff kExponentialBackoff(Seconds(1), Milliseconds::max());

bool isAbortedDocumentPersistent(WithLock, ShardSplitDonorDocument& stateDoc) {
    return !!stateDoc.getAbortReason();
}

void checkForTokenInterrupt(const CancellationToken& token) {
    uassert(ErrorCodes::CallbackCanceled, "Donor service interrupted", !token.isCanceled());
}

}  // namespace

namespace detail {

SemiFuture<HostAndPort> makeRecipientAcceptSplitFuture(
    std::shared_ptr<executor::TaskExecutor> taskExecutor,
    const CancellationToken& abortToken,
    const ConnectionString& recipientConnectionString,
    const UUID migrationId) {

    // build a vector of single server discovery monitors to listen for heartbeats
    auto eventsPublisher = std::make_shared<sdam::TopologyEventsPublisher>(taskExecutor);

    auto listener = std::make_shared<mongo::serverless::RecipientAcceptSplitListener>(
        recipientConnectionString);
    eventsPublisher->registerListener(listener);

    auto managerStats = std::make_shared<ReplicaSetMonitorManagerStats>();
    auto stats = std::make_shared<ReplicaSetMonitorStats>(managerStats);
    auto recipientNodes = recipientConnectionString.getServers();

    std::vector<SingleServerDiscoveryMonitorPtr> monitors;
    for (const auto& server : recipientNodes) {
        SdamConfiguration sdamConfiguration(std::vector<HostAndPort>{server});
        auto connectionString = ConnectionString::forStandalones(std::vector<HostAndPort>{server});

        monitors.push_back(
            std::make_shared<SingleServerDiscoveryMonitor>(MongoURI{connectionString},
                                                           server,
                                                           boost::none,
                                                           sdamConfiguration,
                                                           eventsPublisher,
                                                           taskExecutor,
                                                           stats));
        monitors.back()->init();
    }

    return future_util::withCancellation(listener->getSplitAcceptedFuture(), abortToken)
        .thenRunOn(taskExecutor)
        // Preserve lifetime of listener and monitor until the future is fulfilled and remove the
        // listener.
        .onCompletion(
            [monitors = std::move(monitors), listener, eventsPublisher, taskExecutor, migrationId](
                StatusWith<HostAndPort> s) {
                eventsPublisher->close();

                for (auto& monitor : monitors) {
                    monitor->shutdown();
                }

                return s;
            })
        .semi();
}

}  // namespace detail

ThreadPool::Limits ShardSplitDonorService::getThreadPoolLimits() const {
    ThreadPool::Limits limits;
    limits.maxThreads = repl::maxShardSplitDonorServiceThreadPoolSize;
    limits.minThreads = repl::minShardSplitDonorServiceThreadPoolSize;
    return limits;
}

void ShardSplitDonorService::checkIfConflictsWithOtherInstances(
    OperationContext* opCtx,
    BSONObj initialState,
    const std::vector<const repl::PrimaryOnlyService::Instance*>& existingInstances) {
    auto stateDoc = ShardSplitDonorDocument::parse(IDLParserContext("donorStateDoc"), initialState);

    for (auto& instance : existingInstances) {
        auto existingTypedInstance =
            checked_cast<const ShardSplitDonorService::DonorStateMachine*>(instance);
        bool isGarbageCollectable = existingTypedInstance->isGarbageCollectable();
        bool existingIsAborted =
            existingTypedInstance->getStateDocState() == ShardSplitDonorStateEnum::kAborted &&
            isGarbageCollectable;

        uassert(ErrorCodes::ConflictingOperationInProgress,
                str::stream() << "Can't start a concurent shard split operation, currently running"
                              << " migrationId: " << existingTypedInstance->getId(),
                existingIsAborted);
    }
}

std::shared_ptr<repl::PrimaryOnlyService::Instance> ShardSplitDonorService::constructInstance(
    BSONObj initialState) {
    return std::make_shared<DonorStateMachine>(
        _serviceContext,
        this,
        ShardSplitDonorDocument::parse(IDLParserContext("donorStateDoc"), initialState));
}

void ShardSplitDonorService::abortAllSplits(OperationContext* opCtx) {
    LOGV2(8423361, "Aborting all active shard split operations.");
    auto instances = getAllInstances(opCtx);
    for (auto& instance : instances) {
        auto typedInstance =
            checked_pointer_cast<ShardSplitDonorService::DonorStateMachine>(instance);
        typedInstance->tryAbort();
    }
}

boost::optional<TaskExecutorPtr>
    ShardSplitDonorService::DonorStateMachine::_splitAcceptanceTaskExecutorForTest;
ShardSplitDonorService::DonorStateMachine::DonorStateMachine(
    ServiceContext* serviceContext,
    ShardSplitDonorService* splitService,
    const ShardSplitDonorDocument& initialState)
    : repl::PrimaryOnlyService::TypedInstance<DonorStateMachine>(),
      _migrationId(initialState.getId()),
      _serviceContext(serviceContext),
      _shardSplitService(splitService),
      _stateDoc(initialState),
      _markKilledExecutor(std::make_shared<ThreadPool>([] {
          ThreadPool::Options options;
          options.poolName = "ShardSplitCancelableOpCtxPool";
          options.minThreads = 1;
          options.maxThreads = 1;
          return options;
      }())) {}

void ShardSplitDonorService::DonorStateMachine::tryAbort() {
    LOGV2(6086502, "Received 'abortShardSplit' command.", "id"_attr = _migrationId);
    {
        stdx::lock_guard<Latch> lg(_mutex);
        _abortRequested = true;
        if (_abortSource) {
            _abortSource->cancel();
        }
    }
    pauseShardSplitAfterReceivingAbortCmd.pauseWhileSet();
}

void ShardSplitDonorService::DonorStateMachine::tryForget() {
    LOGV2(6236601, "Received 'forgetShardSplit' command.", "id"_attr = _migrationId);
    stdx::lock_guard<Latch> lg(_mutex);
    if (_forgetShardSplitReceivedPromise.getFuture().isReady()) {
        return;
    }

    _forgetShardSplitReceivedPromise.emplaceValue();
}

void ShardSplitDonorService::DonorStateMachine::checkIfOptionsConflict(
    const BSONObj& stateDocBson) const {
    auto stateDoc = ShardSplitDonorDocument::parse(IDLParserContext("donorStateDoc"), stateDocBson);

    stdx::lock_guard<Latch> lg(_mutex);
    invariant(stateDoc.getId() == _stateDoc.getId());

    if (_stateDoc.getTenantIds() != stateDoc.getTenantIds() ||
        _stateDoc.getRecipientTagName() != stateDoc.getRecipientTagName() ||
        _stateDoc.getRecipientSetName() != stateDoc.getRecipientSetName()) {
        uasserted(ErrorCodes::ConflictingOperationInProgress,
                  str::stream() << "Found active migration for migrationId \""
                                << _stateDoc.getId().toBSON() << "\" with different options "
                                << _stateDoc.toBSON());
    }
}

SemiFuture<void> ShardSplitDonorService::DonorStateMachine::run(
    ScopedTaskExecutorPtr executor, const CancellationToken& primaryToken) noexcept {
    auto abortToken = [&]() {
        stdx::lock_guard<Latch> lg(_mutex);
        _abortSource = CancellationSource(primaryToken);
        if (_abortRequested || _stateDoc.getState() == ShardSplitDonorStateEnum::kAborted) {
            _abortSource->cancel();
        }

        // We must abort the migration if we try to start or resume while upgrading or downgrading.
        // (Generic FCV reference): This FCV check should exist across LTS binary versions.
        if (serverGlobalParams.featureCompatibility.isUpgradingOrDowngrading()) {
            LOGV2(8423360, "Aborting shard split since donor is upgrading or downgrading.");
            _abortSource->cancel();
        }

        return _abortSource->token();
    }();

    _markKilledExecutor->startup();
    _cancelableOpCtxFactory.emplace(primaryToken, _markKilledExecutor);

    auto criticalSectionTimer = std::make_shared<Timer>();
    auto criticalSectionWithoutCatchupTimer = std::make_shared<Timer>();

    const bool shouldRemoveStateDocumentOnRecipient = [&]() {
        auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
        stdx::lock_guard<Latch> lg(_mutex);
        return serverless::shouldRemoveStateDocumentOnRecipient(opCtx.get(), _stateDoc);
    }();

    _decisionPromise.setWith([&] {
        if (shouldRemoveStateDocumentOnRecipient) {
            pauseShardSplitBeforeRecipientCleanup.pauseWhileSet();

            return ExecutorFuture(**executor)
                .then([this, executor, primaryToken, anchor = shared_from_this()] {
                    if (MONGO_unlikely(skipShardSplitRecipientCleanup.shouldFail())) {
                        return ExecutorFuture(**executor);
                    }

                    return _cleanRecipientStateDoc(executor, primaryToken);
                })
                .then([this, executor, migrationId = _migrationId]() {
                    stdx::lock_guard<Latch> lg(_mutex);
                    return DurableState{ShardSplitDonorStateEnum::kCommitted,
                                        boost::none,
                                        _stateDoc.getBlockOpTime()};
                })
                .unsafeToInlineFuture();
        }

        LOGV2(6086506,
              "Starting shard split.",
              "id"_attr = _migrationId,
              "timeout"_attr = repl::shardSplitTimeoutMS.load());

        auto isConfigValidWithStatus = [&]() {
            stdx::lock_guard<Latch> lg(_mutex);
            auto replCoord = repl::ReplicationCoordinator::get(cc().getServiceContext());
            invariant(replCoord);
            return serverless::validateRecipientNodesForShardSplit(_stateDoc,
                                                                   replCoord->getConfig());
        }();

        if (!isConfigValidWithStatus.isOK()) {
            stdx::lock_guard<Latch> lg(_mutex);

            LOGV2_ERROR(6395900,
                        "Failed to validate recipient nodes for shard split.",
                        "id"_attr = _migrationId,
                        "status"_attr = isConfigValidWithStatus);

            _abortReason = isConfigValidWithStatus;
        }

        _initiateTimeout(executor, abortToken);
        return ExecutorFuture(**executor)
            .then([this, executor, primaryToken, abortToken] {
                // Note we do not use the abort split token here because the abortShardSplit
                // command waits for a decision to be persisted which will not happen if
                // inserting the initial state document fails.
                return _enterAbortIndexBuildsOrAbortedState(executor, primaryToken, abortToken);
            })
            .then([this, executor, abortToken] {
                // Start tracking the abortToken for killing operation contexts
                _cancelableOpCtxFactory.emplace(abortToken, _markKilledExecutor);
                return _abortIndexBuildsAndEnterBlockingState(executor, abortToken);
            })
            .then([this, executor, abortToken, criticalSectionTimer] {
                criticalSectionTimer->reset();

                auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
                pauseShardSplitAfterBlocking.pauseWhileSet(opCtx.get());

                return _waitForRecipientToReachBlockOpTime(executor, abortToken);
            })
            .then([this, executor, abortToken, criticalSectionWithoutCatchupTimer] {
                auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
                pauseShardSplitAfterRecipientCaughtUp.pauseWhileSet(opCtx.get());
                criticalSectionWithoutCatchupTimer->reset();
                return _applySplitConfigToDonor(executor, abortToken);
            })
            .then([this, executor, primaryToken, abortToken] {
                return _waitForSplitAcceptanceAndEnterCommittedState(
                    executor, primaryToken, abortToken);
            })
            // anchor ensures the instance will still exists even if the primary stepped down
            .onCompletion([this,
                           executor,
                           primaryToken,
                           abortToken,
                           criticalSectionTimer,
                           criticalSectionWithoutCatchupTimer,
                           anchor = shared_from_this()](Status status) {
                // only cancel operations on stepdown from here out
                _cancelableOpCtxFactory.emplace(primaryToken, _markKilledExecutor);

                {
                    stdx::lock_guard<Latch> lg(_mutex);
                    if (!_stateDoc.getExpireAt()) {
                        if (_abortReason) {
                            ShardSplitStatistics::get(_serviceContext)->incrementTotalAborted();
                        } else {
                            ShardSplitStatistics::get(_serviceContext)
                                ->incrementTotalCommitted(
                                    Milliseconds{criticalSectionTimer->millis()},
                                    Milliseconds{criticalSectionWithoutCatchupTimer->millis()});
                        }
                    }
                }

                if (!status.isOK()) {
                    return _handleErrorOrEnterAbortedState(
                        status, executor, primaryToken, abortToken);
                }

                LOGV2(6236700,
                      "Shard split decision reached",
                      "id"_attr = _migrationId,
                      "state"_attr = ShardSplitDonorState_serializer(_stateDoc.getState()));

                stdx::lock_guard<Latch> lg(_mutex);
                return ExecutorFuture(
                    **executor,
                    DurableState{_stateDoc.getState(), _abortReason, _stateDoc.getBlockOpTime()});
            })
            .unsafeToInlineFuture();
    });

    _garbageCollectablePromise.setWith([&] {
        if (shouldRemoveStateDocumentOnRecipient) {
            return ExecutorFuture(**executor)
                .then([&] { return _decisionPromise.getFuture().semi().ignoreValue(); })
                .unsafeToInlineFuture();
        }

        return ExecutorFuture(**executor)
            .then([&] { return _decisionPromise.getFuture().semi().ignoreValue(); })
            .then([this, executor, primaryToken]() {
                // Always remove the split config after the split decision
                auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
                pauseShardSplitBeforeSplitConfigRemoval.pauseWhileSetAndNotCanceled(opCtx.get(),
                                                                                    primaryToken);
                return _removeSplitConfigFromDonor(executor, primaryToken);
            })
            .then([this, executor, primaryToken] {
                auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
                pauseShardSplitAfterDecision.pauseWhileSet(opCtx.get());

                return _waitForForgetCmdThenMarkGarbageCollectable(executor, primaryToken);
            })
            .unsafeToInlineFuture();
    });

    _completionPromise.setWith([&] {
        if (shouldRemoveStateDocumentOnRecipient) {
            return ExecutorFuture(**executor)
                .then([&] { return _garbageCollectablePromise.getFuture().semi().ignoreValue(); })
                .onCompletion(
                    [this, executor, anchor = shared_from_this(), primaryToken](Status status) {
                        if (!status.isOK()) {
                            LOGV2_ERROR(6753100,
                                        "Failed to cleanup the state document on recipient nodes",
                                        "id"_attr = _migrationId,
                                        "abortReason"_attr = _abortReason,
                                        "status"_attr = status);
                        } else {
                            LOGV2(6753101,
                                  "Successfully cleaned up the state document on recipient nodes.",
                                  "id"_attr = _migrationId,
                                  "abortReason"_attr = _abortReason,
                                  "status"_attr = status);
                        }
                    })
                .unsafeToInlineFuture();
        }

        return ExecutorFuture(**executor)
            .then([&] { return _garbageCollectablePromise.getFuture().semi().ignoreValue(); })
            .then([this, executor, primaryToken] {
                return _waitForGarbageCollectionTimeoutThenDeleteStateDoc(executor, primaryToken);
            })
            .then([this, executor, primaryToken, anchor = shared_from_this()] {
                stdx::lock_guard<Latch> lg(_mutex);
                LOGV2(8423356,
                      "Shard split completed.",
                      "id"_attr = _stateDoc.getId(),
                      "abortReason"_attr = _abortReason);
            })
            .unsafeToInlineFuture();
    });

    return _completionPromise.getFuture().semi();
}

void ShardSplitDonorService::DonorStateMachine::interrupt(Status status) {}

boost::optional<BSONObj> ShardSplitDonorService::DonorStateMachine::reportForCurrentOp(
    MongoProcessInterface::CurrentOpConnectionsMode connMode,
    MongoProcessInterface::CurrentOpSessionsMode sessionMode) noexcept {

    stdx::lock_guard<Latch> lg(_mutex);
    BSONObjBuilder bob;
    bob.append("desc", "shard split operation");
    _migrationId.appendToBuilder(&bob, "instanceID"_sd);
    bob.append("reachedDecision", _decisionPromise.getFuture().isReady());
    if (_stateDoc.getExpireAt()) {
        bob.append("expireAt", *_stateDoc.getExpireAt());
    }
    const auto& tenantIds = _stateDoc.getTenantIds();
    if (tenantIds) {
        std::vector<std::string> tenantIdsAsStrings;
        for (const auto& tid : *tenantIds) {
            tenantIdsAsStrings.push_back(tid.toString());
        }
        bob.append("tenantIds", tenantIdsAsStrings);
    }
    if (_stateDoc.getBlockOpTime()) {
        _stateDoc.getBlockOpTime()->append(&bob, "blockOpTime");
    }
    if (_stateDoc.getCommitOrAbortOpTime()) {
        _stateDoc.getCommitOrAbortOpTime()->append(&bob, "commitOrAbortOpTime");
    }
    if (_stateDoc.getAbortReason()) {
        bob.append("abortReason", *_stateDoc.getAbortReason());
    }
    if (_stateDoc.getRecipientConnectionString()) {
        bob.append("recipientConnectionString",
                   _stateDoc.getRecipientConnectionString()->toString());
    }
    if (_stateDoc.getRecipientSetName()) {
        bob.append("recipientSetName", *_stateDoc.getRecipientSetName());
    }
    if (_stateDoc.getRecipientTagName()) {
        bob.append("recipientTagName", *_stateDoc.getRecipientTagName());
    }

    return bob.obj();
}

bool ShardSplitDonorService::DonorStateMachine::_hasInstalledSplitConfig(WithLock lock) {
    auto replCoord = repl::ReplicationCoordinator::get(cc().getServiceContext());
    auto config = replCoord->getConfig();

    invariant(_stateDoc.getRecipientSetName());
    return config.isSplitConfig() &&
        config.getRecipientConfig()->getReplSetName() == *_stateDoc.getRecipientSetName();
}

ConnectionString ShardSplitDonorService::DonorStateMachine::_setupAcceptanceMonitoring(
    WithLock lock, const CancellationToken& abortToken) {
    auto recipientConnectionString = [stateDoc = _stateDoc]() {
        if (stateDoc.getRecipientConnectionString()) {
            return *stateDoc.getRecipientConnectionString();
        }

        auto recipientTagName = stateDoc.getRecipientTagName();
        invariant(recipientTagName);
        auto recipientSetName = stateDoc.getRecipientSetName();
        invariant(recipientSetName);
        auto config = repl::ReplicationCoordinator::get(cc().getServiceContext())->getConfig();
        return serverless::makeRecipientConnectionString(
            config, *recipientTagName, *recipientSetName);
    }();

    // Always start the replica set monitor if we haven't reached a decision yet
    _splitAcceptancePromise.setWith([&]() {
        if (_stateDoc.getState() > ShardSplitDonorStateEnum::kRecipientCaughtUp ||
            MONGO_unlikely(skipShardSplitWaitForSplitAcceptance.shouldFail())) {
            return Future<HostAndPort>::makeReady(StatusWith<HostAndPort>(HostAndPort{}));
        }

        // Optionally select a task executor for unit testing
        auto executor = _splitAcceptanceTaskExecutorForTest
            ? *_splitAcceptanceTaskExecutorForTest
            : _shardSplitService->getInstanceCleanupExecutor();

        LOGV2(6142508,
              "Monitoring recipient nodes for split acceptance.",
              "id"_attr = _migrationId,
              "recipientConnectionString"_attr = recipientConnectionString);

        return detail::makeRecipientAcceptSplitFuture(
                   executor, abortToken, recipientConnectionString, _migrationId)
            .unsafeToInlineFuture();
    });

    return recipientConnectionString;
}

ExecutorFuture<void>
ShardSplitDonorService::DonorStateMachine::_enterAbortIndexBuildsOrAbortedState(
    const ScopedTaskExecutorPtr& executor,
    const CancellationToken& primaryToken,
    const CancellationToken& abortToken) {
    ShardSplitDonorStateEnum nextState;
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() == ShardSplitDonorStateEnum::kAborted || _abortReason) {
            if (isAbortedDocumentPersistent(lg, _stateDoc)) {
                // Node has step up and created an instance using a document in abort state. No
                // need to write the document as it already exists.
                _abortReason = mongo::resharding::getStatusFromAbortReason(_stateDoc);

                return ExecutorFuture(**executor);
            }

            if (!_abortReason) {
                _abortReason =
                    Status(ErrorCodes::TenantMigrationAborted, "Aborted due to 'abortShardSplit'.");
            }
            BSONObjBuilder bob;
            _abortReason->serializeErrorToBSON(&bob);
            _stateDoc.setAbortReason(bob.obj());
            _stateDoc.setExpireAt(_serviceContext->getFastClockSource()->now() +
                                  Milliseconds{repl::shardSplitGarbageCollectionDelayMS.load()});
            nextState = ShardSplitDonorStateEnum::kAborted;

            LOGV2(6670500, "Entering 'aborted' state.", "id"_attr = _stateDoc.getId());
        } else {
            // Always set up acceptance monitoring.
            auto recipientConnectionString = _setupAcceptanceMonitoring(lg, abortToken);

            if (_stateDoc.getState() > ShardSplitDonorStateEnum::kUninitialized) {
                // Node has stepped up and resumed a shard split. No need to write the document as
                // it already exists.
                return ExecutorFuture(**executor);
            }

            _stateDoc.setRecipientConnectionString(recipientConnectionString);
            nextState = ShardSplitDonorStateEnum::kAbortingIndexBuilds;

            LOGV2(
                6670501, "Entering 'aborting index builds' state.", "id"_attr = _stateDoc.getId());
        }
    }

    return _updateStateDocument(executor, primaryToken, nextState)
        .then([this, executor, primaryToken](repl::OpTime opTime) {
            return _waitForMajorityWriteConcern(executor, std::move(opTime), primaryToken);
        })
        .then([this, executor, nextState]() {
            uassert(ErrorCodes::TenantMigrationAborted,
                    "Shard split operation aborted.",
                    nextState != ShardSplitDonorStateEnum::kAborted);
        });
}

ExecutorFuture<void>
ShardSplitDonorService::DonorStateMachine::_abortIndexBuildsAndEnterBlockingState(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& abortToken) {
    checkForTokenInterrupt(abortToken);

    boost::optional<std::vector<TenantId>> tenantIds;
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > ShardSplitDonorStateEnum::kAbortingIndexBuilds) {
            return ExecutorFuture(**executor);
        }

        tenantIds = _stateDoc.getTenantIds();
        invariant(tenantIds);
    }

    LOGV2(6436100, "Aborting index builds for shard split.", "id"_attr = _migrationId);

    // Abort any in-progress index builds. No new index builds can start while we are doing this
    // because the mtab prevents it.
    auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
    auto* indexBuildsCoordinator = IndexBuildsCoordinator::get(opCtx.get());
    for (const auto& tenantId : *tenantIds) {
        indexBuildsCoordinator->abortTenantIndexBuilds(
            opCtx.get(), MigrationProtocolEnum::kMultitenantMigrations, tenantId, "shard split");
    }

    if (MONGO_unlikely(pauseShardSplitBeforeBlockingState.shouldFail())) {
        pauseShardSplitBeforeBlockingState.pauseWhileSet();
    }

    {
        stdx::lock_guard<Latch> lg(_mutex);
        LOGV2(8423358, "Entering 'blocking' state.", "id"_attr = _stateDoc.getId());
    }

    return _updateStateDocument(executor, abortToken, ShardSplitDonorStateEnum::kBlocking)
        .then([this, self = shared_from_this(), executor, abortToken](repl::OpTime opTime) {
            return _waitForMajorityWriteConcern(executor, std::move(opTime), abortToken);
        });
}

ExecutorFuture<void> ShardSplitDonorService::DonorStateMachine::_waitForRecipientToReachBlockOpTime(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& abortToken) {
    checkForTokenInterrupt(abortToken);

    stdx::lock_guard<Latch> lg(_mutex);
    if (_stateDoc.getState() >= ShardSplitDonorStateEnum::kRecipientCaughtUp ||
        _hasInstalledSplitConfig(lg)) {
        return ExecutorFuture(**executor);
    }

    auto replCoord = repl::ReplicationCoordinator::get(cc().getServiceContext());

    // It's possible that there has been an election since the blockOpTime was recorded, so we use
    // the blockOpTime's timestamp and the current configTerm when waiting for recipient nodes to
    // reach the blockTimestamp. This is okay because these timestamps are cluster times, and so are
    // guaranteed to increase even across terms.
    invariant(_stateDoc.getBlockOpTime());
    auto blockOpTime =
        repl::OpTime(_stateDoc.getBlockOpTime()->getTimestamp(), replCoord->getConfigTerm());

    invariant(_stateDoc.getRecipientTagName());
    auto recipientTagName = *_stateDoc.getRecipientTagName();
    auto recipientNodes = serverless::getRecipientMembers(replCoord->getConfig(), recipientTagName);

    WriteConcernOptions writeConcern;
    writeConcern.w = WTags{{recipientTagName.toString(), recipientNodes.size()}};

    LOGV2(
        6177201, "Waiting for recipient nodes to reach block timestamp.", "id"_attr = _migrationId);

    return ExecutorFuture(**executor)
        .then([this, blockOpTime, writeConcern]() {
            auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
            auto replCoord = repl::ReplicationCoordinator::get(cc().getServiceContext());
            uassertStatusOK(
                replCoord->awaitReplication(opCtx.get(), blockOpTime, writeConcern).status);
        })
        .then([this, executor, abortToken]() {
            {
                stdx::lock_guard<Latch> lg(_mutex);
                LOGV2(8423389,
                      "Entering 'recipient caught up' state.",
                      "id"_attr = _stateDoc.getId());
            }

            return _updateStateDocument(
                       executor, abortToken, ShardSplitDonorStateEnum::kRecipientCaughtUp)
                .then([this, self = shared_from_this(), executor, abortToken](repl::OpTime opTime) {
                    return _waitForMajorityWriteConcern(executor, std::move(opTime), abortToken);
                });
        });
}

ExecutorFuture<void> ShardSplitDonorService::DonorStateMachine::_applySplitConfigToDonor(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& abortToken) {
    checkForTokenInterrupt(abortToken);

    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() >= ShardSplitDonorStateEnum::kCommitted ||
            _hasInstalledSplitConfig(lg)) {
            return ExecutorFuture(**executor);
        }
    }

    auto splitConfig = [&]() {
        stdx::lock_guard<Latch> lg(_mutex);
        invariant(_stateDoc.getRecipientSetName());
        auto recipientSetName = _stateDoc.getRecipientSetName()->toString();
        invariant(_stateDoc.getRecipientTagName());
        auto recipientTagName = _stateDoc.getRecipientTagName()->toString();

        auto replCoord = repl::ReplicationCoordinator::get(cc().getServiceContext());
        invariant(replCoord);

        return serverless::makeSplitConfig(
            replCoord->getConfig(), recipientSetName, recipientTagName);
    }();

    LOGV2(6309100,
          "Applying the split config.",
          "id"_attr = _migrationId,
          "config"_attr = splitConfig);

    return AsyncTry([this, splitConfig] {
               auto opCtxHolder = _cancelableOpCtxFactory->makeOperationContext(&cc());
               DBDirectClient client(opCtxHolder.get());
               BSONObj result;
               const bool returnValue = client.runCommand(
                   DatabaseName::kAdmin, BSON("replSetReconfig" << splitConfig.toBSON()), result);
               uassert(ErrorCodes::BadValue,
                       "Invalid return value for 'replSetReconfig' command.",
                       returnValue);
               uassertStatusOK(getStatusFromCommandResult(result));
           })
        .until([](Status status) { return status.isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, abortToken);
}

ExecutorFuture<void> remoteAdminCommand(TaskExecutorPtr executor,
                                        const CancellationToken& token,
                                        const HostAndPort remoteNode,
                                        const BSONObj& command) {
    return AsyncTry([executor, token, remoteNode, command] {
               executor::RemoteCommandRequest request(
                   remoteNode, DatabaseName::kAdmin, command, nullptr);
               auto hasWriteConcern = command.hasField(WriteConcernOptions::kWriteConcernField);

               return executor->scheduleRemoteCommand(request, token)
                   .then([hasWriteConcern](const auto& response) {
                       auto status = getStatusFromCommandResult(response.data);
                       if (status.isOK() && hasWriteConcern) {
                           return getWriteConcernStatusFromCommandResult(response.data);
                       }

                       return status;
                   });
           })
        .until([](Status status) { return status.isOK(); })
        .on(executor, token);
}

ExecutorFuture<void> sendStepUpToRecipient(TaskExecutorPtr executor,
                                           const CancellationToken& token,
                                           const HostAndPort recipientPrimary) {
    return remoteAdminCommand(
        executor, token, recipientPrimary, BSON("replSetStepUp" << 1 << "skipDryRun" << true));
}

ExecutorFuture<void> waitForMajorityWriteOnRecipient(TaskExecutorPtr executor,
                                                     const CancellationToken& token,
                                                     const HostAndPort recipientPrimary) {
    return remoteAdminCommand(
        executor,
        token,
        recipientPrimary,
        BSON("appendOplogNote" << 1 << "data"
                               << BSON("noop write for shard split recipient primary election" << 1)
                               << WriteConcernOptions::kWriteConcernField
                               << BSON("w" << WriteConcernOptions::kMajority)));
}

ExecutorFuture<void>
ShardSplitDonorService::DonorStateMachine::_waitForSplitAcceptanceAndEnterCommittedState(
    const ScopedTaskExecutorPtr& executor,
    const CancellationToken& primaryToken,
    const CancellationToken& abortToken) {

    checkForTokenInterrupt(abortToken);
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > ShardSplitDonorStateEnum::kRecipientCaughtUp) {
            return ExecutorFuture(**executor);
        }
    }

    LOGV2(6142501, "Waiting for recipient to accept the split.", "id"_attr = _migrationId);

    return ExecutorFuture(**executor)
        .then([&]() { return _splitAcceptancePromise.getFuture(); })
        .then([this, executor, abortToken](const HostAndPort& recipientPrimary) {
            auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
            if (MONGO_unlikely(pauseShardSplitBeforeLeavingBlockingState.shouldFail())) {
                pauseShardSplitBeforeLeavingBlockingState.execute([&](const BSONObj& data) {
                    if (!data.hasField("blockTimeMS")) {
                        pauseShardSplitBeforeLeavingBlockingState.pauseWhileSet(opCtx.get());
                    } else {
                        const auto blockTime = Milliseconds{data.getIntField("blockTimeMS")};
                        LOGV2(8423359,
                              "Keeping shard split in blocking state.",
                              "blockTime"_attr = blockTime);
                        opCtx->sleepFor(blockTime);
                    }
                });
            }

            if (MONGO_unlikely(abortShardSplitBeforeLeavingBlockingState.shouldFail())) {
                uasserted(ErrorCodes::InternalError, "simulate a shard split error");
            }

            // If the split acceptance step was cancelled, its future will produce a default
            // constructed HostAndPort. Skipping split acceptance implies skipping triggering an
            // election.
            if (recipientPrimary.empty()) {
                return ExecutorFuture(**executor);
            }

            LOGV2(6493901,
                  "Triggering an election after recipient has accepted the split.",
                  "id"_attr = _migrationId);

            auto remoteCommandExecutor = _splitAcceptanceTaskExecutorForTest
                ? *_splitAcceptanceTaskExecutorForTest
                : **executor;

            return sendStepUpToRecipient(remoteCommandExecutor, abortToken, recipientPrimary)
                .then([this, remoteCommandExecutor, abortToken, recipientPrimary]() {
                    LOGV2(8423365,
                          "Waiting for majority commit on recipient primary",
                          "id"_attr = _migrationId);

                    return waitForMajorityWriteOnRecipient(
                        remoteCommandExecutor, abortToken, recipientPrimary);
                });
        })
        .thenRunOn(**executor)
        .then([this, executor, primaryToken]() {
            // only cancel operations on stepdown from here out
            _cancelableOpCtxFactory.emplace(primaryToken, _markKilledExecutor);

            LOGV2(6142503, "Entering 'committed' state.", "id"_attr = _stateDoc.getId());
            auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
            pauseShardSplitAfterUpdatingToCommittedState.pauseWhileSet(opCtx.get());

            return _updateStateDocument(
                       executor, primaryToken, ShardSplitDonorStateEnum::kCommitted)
                .then([this, executor, primaryToken](repl::OpTime opTime) {
                    return _waitForMajorityWriteConcern(executor, std::move(opTime), primaryToken);
                });
        });
}

ExecutorFuture<repl::OpTime> ShardSplitDonorService::DonorStateMachine::_updateStateDocument(
    const ScopedTaskExecutorPtr& executor,
    const CancellationToken& token,
    ShardSplitDonorStateEnum nextState) {
    auto [isInsert, originalStateDocBson] = [&]() {
        stdx::lock_guard<Latch> lg(_mutex);
        auto currentState = _stateDoc.getState();
        auto isInsert = currentState == ShardSplitDonorStateEnum::kUninitialized ||
            currentState == ShardSplitDonorStateEnum::kAborted;
        return std::make_tuple(isInsert, _stateDoc.toBSON());
    }();

    return AsyncTry([this,
                     isInsert = isInsert,
                     originalStateDocBson = originalStateDocBson,
                     uuid = _migrationId,
                     nextState] {
               auto opCtxHolder = _cancelableOpCtxFactory->makeOperationContext(&cc());
               auto opCtx = opCtxHolder.get();

               auto collection =
                   acquireCollection(opCtx,
                                     CollectionAcquisitionRequest(
                                         _stateDocumentsNS,
                                         PlacementConcern{boost::none, ShardVersion::UNSHARDED()},
                                         repl::ReadConcernArgs::get(opCtx),
                                         AcquisitionPrerequisites::kWrite),
                                     MODE_IX);

               if (!isInsert) {
                   uassert(ErrorCodes::NamespaceNotFound,
                           str::stream()
                               << _stateDocumentsNS.toStringForErrorMsg() << " does not exist",
                           collection.exists());
               }

               writeConflictRetry(opCtx, "ShardSplitDonorUpdateStateDoc", _stateDocumentsNS, [&]() {
                   WriteUnitOfWork wuow(opCtx);

                   if (nextState == ShardSplitDonorStateEnum::kBlocking) {
                       // Start blocking writes before getting an oplog slot to guarantee no
                       // writes to the tenant's data can commit with a timestamp after the
                       // block timestamp.
                       auto mtabVector =
                           TenantMigrationAccessBlockerRegistry::get(opCtx->getServiceContext())
                               .getDonorAccessBlockersForMigration(uuid);
                       invariant(!mtabVector.empty());

                       for (auto& mtab : mtabVector) {
                           invariant(mtab);
                           mtab->startBlockingWrites();

                           opCtx->recoveryUnit()->onRollback(
                               [mtab](OperationContext*) { mtab->rollBackStartBlocking(); });
                       }
                   }

                   // Reserve an opTime for the write.
                   auto oplogSlot = LocalOplogInfo::get(opCtx)->getNextOpTimes(opCtx, 1U)[0];
                   auto updatedStateDocBson = [&]() {
                       stdx::lock_guard<Latch> lg(_mutex);
                       _stateDoc.setState(nextState);
                       switch (nextState) {
                           case ShardSplitDonorStateEnum::kUninitialized:
                           case ShardSplitDonorStateEnum::kAbortingIndexBuilds:
                           case ShardSplitDonorStateEnum::kRecipientCaughtUp:
                               break;
                           case ShardSplitDonorStateEnum::kBlocking:
                               _stateDoc.setBlockOpTime(oplogSlot);
                               break;
                           case ShardSplitDonorStateEnum::kCommitted:
                               _stateDoc.setCommitOrAbortOpTime(oplogSlot);
                               break;
                           case ShardSplitDonorStateEnum::kAborted: {
                               _stateDoc.setCommitOrAbortOpTime(oplogSlot);

                               invariant(_abortReason);
                               BSONObjBuilder bob;
                               _abortReason.value().serializeErrorToBSON(&bob);
                               _stateDoc.setAbortReason(bob.obj());
                               break;
                           }
                           default:
                               MONGO_UNREACHABLE;
                       }
                       if (isInsert) {
                           return BSON("$setOnInsert" << _stateDoc.toBSON());
                       }

                       return _stateDoc.toBSON();
                   }();

                   auto updateOpTime = [&]() {
                       if (isInsert) {
                           const auto filter = BSON(ShardSplitDonorDocument::kIdFieldName << uuid);
                           auto updateResult = Helpers::upsert(opCtx,
                                                               collection,
                                                               filter,
                                                               updatedStateDocBson,
                                                               /*fromMigrate=*/false);

                           // '$setOnInsert' update operator can never modify an existing
                           // on-disk state doc.
                           invariant(!updateResult.existing);
                           invariant(!updateResult.numDocsModified);

                           return repl::ReplClientInfo::forClient(opCtx->getClient()).getLastOp();
                       }

                       const auto originalRecordId =
                           Helpers::findOne(opCtx,
                                            collection.getCollectionPtr(),
                                            BSON("_id" << originalStateDocBson["_id"]));
                       const auto originalSnapshot = Snapshotted<BSONObj>(
                           opCtx->recoveryUnit()->getSnapshotId(), originalStateDocBson);
                       invariant(!originalRecordId.isNull());

                       CollectionUpdateArgs args{originalSnapshot.value()};
                       args.criteria = BSON("_id" << uuid);
                       args.oplogSlots = {oplogSlot};
                       args.update = updatedStateDocBson;

                       collection_internal::updateDocument(opCtx,
                                                           collection.getCollectionPtr(),
                                                           originalRecordId,
                                                           originalSnapshot,
                                                           updatedStateDocBson,
                                                           collection_internal::kUpdateNoIndexes,
                                                           nullptr /* indexesAffected */,
                                                           nullptr /* OpDebug* */,
                                                           &args);

                       return oplogSlot;
                   }();

                   wuow.commit();
                   return updateOpTime;
               });

               return repl::ReplClientInfo::forClient(opCtx->getClient()).getLastOp();
           })
        .until([&](StatusWith<repl::OpTime> swOpTime) {
            if (swOpTime.getStatus().code() == ErrorCodes::ConflictingServerlessOperation) {
                LOGV2(6531509,
                      "Shard split failed due to serverless lock error",
                      "id"_attr = _migrationId,
                      "status"_attr = swOpTime.getStatus());
                stdx::lock_guard<Latch> lg(_mutex);

                uassertStatusOK(swOpTime);
            }
            return swOpTime.getStatus().isOK();
        })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<void> ShardSplitDonorService::DonorStateMachine::_waitForMajorityWriteConcern(
    const ScopedTaskExecutorPtr& executor, repl::OpTime opTime, const CancellationToken& token) {
    return WaitForMajorityService::get(_serviceContext)
        .waitUntilMajorityForWrite(std::move(opTime), token)
        .thenRunOn(**executor);
}

void ShardSplitDonorService::DonorStateMachine::_initiateTimeout(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& abortToken) {
    auto timeoutFuture =
        (*executor)->sleepFor(Milliseconds(repl::shardSplitTimeoutMS.load()), abortToken);

    auto timeoutOrCompletionFuture =
        whenAny(std::move(timeoutFuture),
                decisionFuture().semi().ignoreValue().thenRunOn(**executor))
            .thenRunOn(**executor)
            .then([this, executor, abortToken, anchor = shared_from_this()](auto result) {
                stdx::lock_guard<Latch> lg(_mutex);
                if (_stateDoc.getState() != ShardSplitDonorStateEnum::kCommitted &&
                    _stateDoc.getState() != ShardSplitDonorStateEnum::kAborted &&
                    !abortToken.isCanceled()) {
                    LOGV2(6236500,
                          "Timeout expired, aborting shard split.",
                          "id"_attr = _migrationId,
                          "timeout"_attr = repl::shardSplitTimeoutMS.load());
                    _abortReason = Status(ErrorCodes::ExceededTimeLimit,
                                          "Aborting shard split as it exceeded its time limit.");
                    _abortSource->cancel();
                }
            })
            .semi();
}

ExecutorFuture<ShardSplitDonorService::DonorStateMachine::DurableState>
ShardSplitDonorService::DonorStateMachine::_handleErrorOrEnterAbortedState(
    Status status,
    const ScopedTaskExecutorPtr& executor,
    const CancellationToken& primaryToken,
    const CancellationToken& abortToken) {
    ON_BLOCK_EXIT([&] {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_abortSource) {
            // Cancel source to ensure all child threads (RSM monitor, etc) terminate.
            _abortSource->cancel();
        }
    });

    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (isAbortedDocumentPersistent(lg, _stateDoc)) {
            // The document is already in aborted state. No need to write it.
            LOGV2(8423376,
                  "Shard split already aborted.",
                  "id"_attr = _migrationId,
                  "abortReason"_attr = _abortReason.value());

            return ExecutorFuture(**executor,
                                  DurableState{ShardSplitDonorStateEnum::kAborted,
                                               _abortReason,
                                               _stateDoc.getBlockOpTime()});
        }
    }

    if (ErrorCodes::isNotPrimaryError(status) || ErrorCodes::isShutdownError(status) ||
        status.code() == ErrorCodes::ConflictingServerlessOperation) {
        // Don't abort the split on retriable errors that may have been generated by the local
        // server shutting/stepping down because it can be resumed when the client retries.
        return ExecutorFuture(**executor, StatusWith<DurableState>{status});
    }

    // Make sure we don't change the status if the abortToken is cancelled due to a POS instance
    // interruption.
    if (abortToken.isCanceled() && !primaryToken.isCanceled()) {
        status =
            Status(ErrorCodes::TenantMigrationAborted, "Aborted due to 'abortShardSplit' command.");
    }

    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (!_abortReason) {
            _abortReason = status;
        }

        BSONObjBuilder bob;
        _abortReason->serializeErrorToBSON(&bob);
        _stateDoc.setAbortReason(bob.obj());

        LOGV2(6086508,
              "Entering 'aborted' state.",
              "id"_attr = _migrationId,
              "abortReason"_attr = _abortReason.value());
    }

    return ExecutorFuture<void>(**executor)
        .then([this, executor, primaryToken] {
            return _updateStateDocument(executor, primaryToken, ShardSplitDonorStateEnum::kAborted);
        })
        .then([this, executor, primaryToken](repl::OpTime opTime) {
            return _waitForMajorityWriteConcern(executor, std::move(opTime), primaryToken);
        })
        .then([this, executor] {
            stdx::lock_guard<Latch> lg(_mutex);
            return DurableState{_stateDoc.getState(), _abortReason, _stateDoc.getBlockOpTime()};
        });
}

ExecutorFuture<void>
ShardSplitDonorService::DonorStateMachine::_waitForForgetCmdThenMarkGarbageCollectable(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& primaryToken) {
    stdx::lock_guard<Latch> lg(_mutex);
    if (_stateDoc.getExpireAt()) {
        return ExecutorFuture(**executor);
    }

    LOGV2(6236603, "Waiting to receive 'forgetShardSplit' command.", "id"_attr = _migrationId);

    return future_util::withCancellation(_forgetShardSplitReceivedPromise.getFuture(), primaryToken)
        .thenRunOn(**executor)
        .then([this, self = shared_from_this(), executor, primaryToken] {
            LOGV2(6236606, "Marking shard split as garbage-collectable.", "id"_attr = _migrationId);

            stdx::lock_guard<Latch> lg(_mutex);
            _stateDoc.setExpireAt(_serviceContext->getFastClockSource()->now() +
                                  Milliseconds{repl::shardSplitGarbageCollectionDelayMS.load()});

            return AsyncTry([this, self = shared_from_this()] {
                       auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
                       uassertStatusOK(serverless::updateStateDoc(opCtx.get(), _stateDoc));
                       return repl::ReplClientInfo::forClient(opCtx->getClient()).getLastOp();
                   })
                .until(
                    [](StatusWith<repl::OpTime> swOpTime) { return swOpTime.getStatus().isOK(); })
                .withBackoffBetweenIterations(kExponentialBackoff)
                .on(**executor, primaryToken);
        })
        .then([this, self = shared_from_this(), executor, primaryToken](repl::OpTime opTime) {
            return _waitForMajorityWriteConcern(executor, std::move(opTime), primaryToken);
        })
        .then([this, self = shared_from_this()] {
            pauseShardSplitAfterMarkingStateGarbageCollectable.pauseWhileSet();
        });
}

ExecutorFuture<void>
ShardSplitDonorService::DonorStateMachine::_waitForGarbageCollectionTimeoutThenDeleteStateDoc(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& primaryToken) {
    auto expireAt = [&]() {
        stdx::lock_guard<Latch> lg(_mutex);
        return _stateDoc.getExpireAt();
    }();

    if (!expireAt) {
        return ExecutorFuture(**executor);
    }

    if (skipShardSplitGarbageCollectionTimeout.shouldFail()) {
        LOGV2(673701, "Skipping shard split garbage collection timeout");
        return ExecutorFuture(**executor);
    }

    LOGV2(6737300,
          "Waiting until the garbage collection timeout expires",
          "id"_attr = _migrationId,
          "expireAt"_attr = *expireAt);
    return (*executor)->sleepUntil(*expireAt, primaryToken).then([this, executor, primaryToken] {
        return AsyncTry([this, executor, primaryToken] {
                   auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
                   auto deleted =
                       uassertStatusOK(serverless::deleteStateDoc(opCtx.get(), _migrationId));
                   uassert(ErrorCodes::ConflictingOperationInProgress,
                           str::stream() << "Did not find active shard split with migration id "
                                         << _migrationId,
                           deleted);
                   return repl::ReplClientInfo::forClient(opCtx.get()->getClient()).getLastOp();
               })
            .until([](StatusWith<repl::OpTime> swOpTime) { return swOpTime.getStatus().isOK(); })
            .withBackoffBetweenIterations(kExponentialBackoff)
            .on(**executor, primaryToken)
            .then([](StatusWith<repl::OpTime> swOpTime) { return swOpTime.getStatus(); });
    });
}

ExecutorFuture<void> ShardSplitDonorService::DonorStateMachine::_removeSplitConfigFromDonor(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& token) {
    checkForTokenInterrupt(token);

    auto replCoord = repl::ReplicationCoordinator::get(cc().getServiceContext());
    invariant(replCoord);

    return AsyncTry([this, replCoord] {
               auto config = replCoord->getConfig();
               if (!config.isSplitConfig()) {
                   return;
               }

               LOGV2(6573000,
                     "Reconfiguring the donor to remove the split config.",
                     "id"_attr = _migrationId,
                     "config"_attr = config);

               BSONObjBuilder newConfigBob(
                   config.toBSON().removeField("recipientConfig").removeField("version"));
               newConfigBob.append("version", config.getConfigVersion() + 1);

               auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
               DBDirectClient client(opCtx.get());

               BSONObj result;
               const bool returnValue = client.runCommand(
                   DatabaseName::kAdmin, BSON("replSetReconfig" << newConfigBob.obj()), result);
               uassert(
                   ErrorCodes::BadValue, "Invalid return value for replSetReconfig", returnValue);
               uassertStatusOK(getStatusFromCommandResult(result));
           })
        .until([](Status status) { return status.isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<void> ShardSplitDonorService::DonorStateMachine::_cleanRecipientStateDoc(
    const ScopedTaskExecutorPtr& executor, const CancellationToken& primaryToken) {
    LOGV2(6309000, "Cleaning up shard split operation on recipient.", "id"_attr = _migrationId);
    return AsyncTry([this, self = shared_from_this()] {
               auto opCtx = _cancelableOpCtxFactory->makeOperationContext(&cc());
               auto deleted =
                   uassertStatusOK(serverless::deleteStateDoc(opCtx.get(), _migrationId));
               uassert(ErrorCodes::ConflictingOperationInProgress,
                       str::stream()
                           << "Did not find active shard split with migration id " << _migrationId,
                       deleted);
               return repl::ReplClientInfo::forClient(opCtx.get()->getClient()).getLastOp();
           })
        .until([](StatusWith<repl::OpTime> swOpTime) { return swOpTime.getStatus().isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, primaryToken)
        .ignoreValue();
}
}  // namespace mongo
