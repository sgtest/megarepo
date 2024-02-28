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


#include "mongo/db/repl/tenant_migration_donor_service.h"

#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <boost/smart_ptr.hpp>
#include <functional>
#include <mutex>
#include <tuple>
#include <type_traits>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/async_remote_command_targeter_adapter.h"
#include "mongo/client/connection_string.h"
#include "mongo/client/remote_command_retry_scheduler.h"
#include "mongo/client/remote_command_targeter_rs.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db//shard_role.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_write_path.h"
#include "mongo/db/catalog/local_oplog_info.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/commands/tenant_migration_recipient_cmds_gen.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/index_builds_coordinator.h"
#include "mongo/db/keys_collection_document_gen.h"
#include "mongo/db/keys_collection_util.h"
#include "mongo/db/ops/update_result.h"
#include "mongo/db/persistent_task_store.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/record_id.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/repl/read_concern_level.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/repl/repl_server_parameters_gen.h"
#include "mongo/db/repl/repl_set_config.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/tenant_migration_access_blocker_registry.h"
#include "mongo/db/repl/tenant_migration_access_blocker_util.h"
#include "mongo/db/repl/tenant_migration_donor_access_blocker.h"
#include "mongo/db/repl/tenant_migration_state_machine_gen.h"
#include "mongo/db/repl/tenant_migration_statistics.h"
#include "mongo/db/repl/tenant_migration_util.h"
#include "mongo/db/repl/wait_for_majority_service.h"
#include "mongo/db/server_options.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/snapshot.h"
#include "mongo/db/storage/storage_engine.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/executor/async_rpc.h"
#include "mongo/executor/async_rpc_error_info.h"
#include "mongo/executor/async_rpc_retry_policy.h"
#include "mongo/executor/async_rpc_targeter.h"
#include "mongo/executor/connection_pool.h"
#include "mongo/executor/network_connection_hook.h"
#include "mongo/executor/network_interface_factory.h"
#include "mongo/executor/remote_command_request.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/compiler.h"
#include "mongo/rpc/get_status_from_command_result.h"
#include "mongo/rpc/metadata/egress_metadata_hook_list.h"
#include "mongo/rpc/metadata/metadata_hook.h"
#include "mongo/s/database_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/cancellation.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/concurrency/with_lock.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future_util.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/net/ssl_options.h"
#include "mongo/util/out_of_line_executor.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTenantMigration


namespace mongo {

namespace {

MONGO_FAIL_POINT_DEFINE(abortTenantMigrationBeforeLeavingBlockingState);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationAfterPersistingInitialDonorStateDoc);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeLeavingAbortingIndexBuildsState);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeLeavingBlockingState);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeLeavingDataSyncState);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeFetchingKeys);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationDonorBeforeStoringExternalClusterTimeKeyDocs);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationDonorBeforeWaitingForKeysToReplicate);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationDonorBeforeMarkingStateGarbageCollectable);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationDonorAfterMarkingStateGarbageCollectable);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationDonorBeforeDeletingStateDoc);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeEnteringFutureChain);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationAfterFetchingAndStoringKeys);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationDonorWhileUpdatingStateDoc);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeInsertingDonorStateDoc);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeCreatingStateDocumentTTLIndex);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeCreatingExternalKeysTTLIndex);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationBeforeLeavingCommittedState);
MONGO_FAIL_POINT_DEFINE(pauseTenantMigrationAfterUpdatingToCommittedState);

const std::string kTTLIndexName = "TenantMigrationDonorTTLIndex";
const std::string kExternalKeysTTLIndexName = "ExternalKeysTTLIndex";
const Backoff kExponentialBackoff(Seconds(1), Milliseconds::max());

const ReadPreferenceSetting kPrimaryOnlyReadPreference(ReadPreference::PrimaryOnly);

const int kMaxRecipientKeyDocsFindAttempts = 10;

using RecipientForgetMigrationRPCOptions = async_rpc::AsyncRPCOptions<RecipientForgetMigration>;
using RecipientSyncDataRPCOptions = async_rpc::AsyncRPCOptions<RecipientSyncData>;

/**
 * Encapsulates the retry logic for sending the ForgetMigration command.
 */
class RecipientForgetMigrationRetryPolicy
    : public async_rpc::RetryWithBackoffOnErrorCategories<ErrorCategory::RetriableError,
                                                          ErrorCategory::NetworkTimeoutError,
                                                          ErrorCategory::Interruption> {
public:
    using RetryWithBackoffOnErrorCategories::RetryWithBackoffOnErrorCategories;
    bool recordAndEvaluateRetry(Status status) override {
        if (status.isOK()) {
            return false;
        }
        auto underlyingError = async_rpc::unpackRPCStatusIgnoringWriteConcernAndWriteErrors(status);
        // Returned if findHost() is unable to target the recipient in 15 seconds, which may
        // happen after a failover.
        return RetryWithBackoffOnErrorCategories::recordAndEvaluateRetry(underlyingError) ||
            underlyingError == ErrorCodes::FailedToSatisfyReadPreference;
    }
};

/**
 * Encapsulates the retry logic for sending the SyncData command.
 */
class RecipientSyncDataRetryPolicy
    : public async_rpc::RetryWithBackoffOnErrorCategories<ErrorCategory::RetriableError,
                                                          ErrorCategory::NetworkTimeoutError> {
public:
    RecipientSyncDataRetryPolicy(MigrationProtocolEnum p, Backoff b)
        : RetryWithBackoffOnErrorCategories(b), _protocol{p} {}

    /** Returns true if we should retry sending SyncData given the error */
    bool recordAndEvaluateRetry(Status status) {
        if (_protocol == MigrationProtocolEnum::kShardMerge || status.isOK()) {
            return false;
        }
        auto underlyingError = async_rpc::unpackRPCStatusIgnoringWriteConcernAndWriteErrors(status);
        return RetryWithBackoffOnErrorCategories::recordAndEvaluateRetry(underlyingError) ||
            underlyingError == ErrorCodes::FailedToSatisfyReadPreference;
    }

private:
    MigrationProtocolEnum _protocol;
};

bool shouldStopFetchingRecipientClusterTimeKeyDocs(Status status) {
    return status.isOK() ||
        !(ErrorCodes::isRetriableError(status) || ErrorCodes::isInterruption(status) ||
          ErrorCodes::isNetworkTimeoutError(status) ||
          // Returned if findHost() is unable to target the recipient in 15 seconds, which may
          // happen after a failover.
          status == ErrorCodes::FailedToSatisfyReadPreference);
}

void checkForTokenInterrupt(const CancellationToken& token) {
    uassert(ErrorCodes::CallbackCanceled, "Donor service interrupted", !token.isCanceled());
}

template <class Promise>
void setPromiseFromStatusIfNotReady(WithLock lk, Promise& promise, Status status) {
    if (promise.getFuture().isReady()) {
        return;
    }

    if (status.isOK()) {
        promise.emplaceValue();
    } else {
        promise.setError(status);
    }
}

template <class Promise>
void setPromiseErrorIfNotReady(WithLock lk, Promise& promise, Status status) {
    if (promise.getFuture().isReady()) {
        return;
    }

    promise.setError(status);
}

template <class Promise>
void setPromiseOkIfNotReady(WithLock lk, Promise& promise) {
    if (promise.getFuture().isReady()) {
        return;
    }

    promise.emplaceValue();
}

bool isNotDurableAndServerlessConflict(WithLock lk, SharedPromise<void>& promise) {
    auto future = promise.getFuture();

    if (!future.isReady() ||
        future.getNoThrow().code() != ErrorCodes::ConflictingServerlessOperation) {
        return false;
    }

    return true;
}

}  // namespace

void TenantMigrationDonorService::checkIfConflictsWithOtherInstances(
    OperationContext* opCtx,
    BSONObj initialState,
    const std::vector<const repl::PrimaryOnlyService::Instance*>& existingInstances) {
    auto stateDoc = tenant_migration_access_blocker::parseDonorStateDocument(initialState);
    auto isNewShardMerge = stateDoc.getProtocol() == MigrationProtocolEnum::kShardMerge;

    for (auto& instance : existingInstances) {
        auto existingTypedInstance =
            checked_cast<const TenantMigrationDonorService::Instance*>(instance);
        auto existingState = existingTypedInstance->getDurableState();
        auto existingIsAborted = existingState &&
            existingState->state == TenantMigrationDonorStateEnum::kAborted &&
            existingState->expireAt;

        uassert(ErrorCodes::ConflictingOperationInProgress,
                str::stream() << "Cannot start a shard merge with existing migrations in progress",
                !isNewShardMerge || existingIsAborted);

        uassert(
            ErrorCodes::ConflictingOperationInProgress,
            str::stream() << "Cannot start a migration with an existing shard merge in progress",
            existingTypedInstance->getProtocol() != MigrationProtocolEnum::kShardMerge ||
                existingIsAborted);

        // Any existing migration for this tenant must be aborted and garbage-collectable.
        if (stateDoc.getTenantId() &&
            existingTypedInstance->getTenantId() == *stateDoc.getTenantId()) {
            uassert(ErrorCodes::ConflictingOperationInProgress,
                    str::stream() << "tenant " << stateDoc.getTenantId() << " is already migrating",
                    existingIsAborted);
        }
    }
}

std::shared_ptr<repl::PrimaryOnlyService::Instance> TenantMigrationDonorService::constructInstance(
    BSONObj initialState) {

    return std::make_shared<TenantMigrationDonorService::Instance>(
        _serviceContext, this, initialState);
}  // namespace mongo

void TenantMigrationDonorService::abortAllMigrations(OperationContext* opCtx) {
    LOGV2(5356301, "Aborting all tenant migrations on donor");
    auto instances = getAllInstances(opCtx);
    for (auto& instance : instances) {
        auto typedInstance = checked_pointer_cast<TenantMigrationDonorService::Instance>(instance);
        typedInstance->onReceiveDonorAbortMigration();
    }
}

ExecutorFuture<void> TenantMigrationDonorService::createStateDocumentTTLIndex(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    return AsyncTry([this] {
               auto nss = getStateDocumentsNS();

               AllowOpCtxWhenServiceRebuildingBlock allowOpCtxBlock(Client::getCurrent());
               auto opCtxHolder = cc().makeOperationContext();
               auto opCtx = opCtxHolder.get();
               DBDirectClient client(opCtx);

               pauseTenantMigrationBeforeCreatingStateDocumentTTLIndex.pauseWhileSet(opCtx);

               BSONObj result;
               client.runCommand(
                   nss.dbName(),
                   BSON("createIndexes"
                        << nss.coll().toString() << "indexes"
                        << BSON_ARRAY(BSON("key" << BSON("expireAt" << 1) << "name" << kTTLIndexName
                                                 << "expireAfterSeconds" << 0))),
                   result);
               uassertStatusOK(getStatusFromCommandResult(result));
           })
        .until([](Status status) { return status.isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<void> TenantMigrationDonorService::createExternalKeysTTLIndex(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    return AsyncTry([this] {
               const auto nss = NamespaceString::kExternalKeysCollectionNamespace;

               AllowOpCtxWhenServiceRebuildingBlock allowOpCtxBlock(Client::getCurrent());
               auto opCtxHolder = cc().makeOperationContext();
               auto opCtx = opCtxHolder.get();
               DBDirectClient client(opCtx);

               pauseTenantMigrationBeforeCreatingExternalKeysTTLIndex.pauseWhileSet(opCtx);

               BSONObj result;
               client.runCommand(
                   nss.dbName(),
                   BSON("createIndexes"
                        << nss.coll().toString() << "indexes"
                        << BSON_ARRAY(BSON("key" << BSON("ttlExpiresAt" << 1) << "name"
                                                 << kExternalKeysTTLIndexName
                                                 << "expireAfterSeconds" << 0))),
                   result);
               uassertStatusOK(getStatusFromCommandResult(result));
           })
        .until([](Status status) { return status.isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<void> TenantMigrationDonorService::_rebuildService(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    return createStateDocumentTTLIndex(executor, token).then([this, executor, token] {
        // Since a tenant migration donor and recipient both copy signing keys from each other and
        // put them in the same external keys collection, they share this TTL index (the recipient
        // service does not also build this TTL index).
        return createExternalKeysTTLIndex(executor, token);
    });
}

TenantMigrationDonorService::Instance::Instance(ServiceContext* const serviceContext,
                                                const TenantMigrationDonorService* donorService,
                                                const BSONObj& initialState)
    : repl::PrimaryOnlyService::TypedInstance<Instance>(),
      _serviceContext(serviceContext),
      _donorService(donorService),
      _stateDoc(tenant_migration_access_blocker::parseDonorStateDocument(initialState)),
      _instanceName(kServiceName + "-" + _stateDoc.getId().toString()),
      _recipientUri(
          uassertStatusOK(MongoURI::parse(_stateDoc.getRecipientConnectionString().toString()))),
      _tenantId(_stateDoc.getTenantId() ? *_stateDoc.getTenantId() : ""),
      _tenantIds(_stateDoc.getTenantIds() ? *_stateDoc.getTenantIds() : std::vector<TenantId>()),
      _protocol(_stateDoc.getProtocol().value_or(MigrationProtocolEnum::kMultitenantMigrations)),
      _recipientConnectionString(_stateDoc.getRecipientConnectionString()),
      _readPreference(_stateDoc.getReadPreference()),
      _migrationUuid(_stateDoc.getId()) {

    if (_stateDoc.getState() > TenantMigrationDonorStateEnum::kUninitialized) {
        // The migration was resumed on stepup.

        if (_stateDoc.getAbortReason()) {
            auto abortReasonBson = _stateDoc.getAbortReason().value();
            auto code = abortReasonBson["code"].Int();
            auto errmsg = abortReasonBson["errmsg"].String();
            _abortReason = Status(ErrorCodes::Error(code), errmsg);
        }
        _durableState = DurableState{_stateDoc.getState(),
                                     _stateDoc.getAbortReason(),
                                     _stateDoc.getExpireAt(),
                                     _stateDoc.getBlockTimestamp()};

        _initialDonorStateDurablePromise.emplaceValue();

        if (_stateDoc.getState() == TenantMigrationDonorStateEnum::kAborted ||
            _stateDoc.getState() == TenantMigrationDonorStateEnum::kCommitted) {
            _decisionPromise.emplaceValue();
        }
    }
}

TenantMigrationDonorService::Instance::~Instance() {
    stdx::lock_guard<Latch> lg(_mutex);
    invariant(_initialDonorStateDurablePromise.getFuture().isReady());
    invariant(_receiveDonorForgetMigrationPromise.getFuture().isReady());
}

boost::optional<BSONObj> TenantMigrationDonorService::Instance::reportForCurrentOp(
    MongoProcessInterface::CurrentOpConnectionsMode connMode,
    MongoProcessInterface::CurrentOpSessionsMode sessionMode) noexcept {

    stdx::lock_guard<Latch> lg(_mutex);

    // Ignore connMode and sessionMode because tenant migrations are not associated with
    // sessions and they run in a background thread pool.
    BSONObjBuilder bob;
    bob.append("desc", "tenant donor migration");
    bob.append("garbageCollectable", _forgetMigrationDurablePromise.getFuture().isReady());
    _migrationUuid.appendToBuilder(&bob, "instanceID"_sd);
    if (getProtocol() == MigrationProtocolEnum::kMultitenantMigrations) {
        bob.append("tenantId", _tenantId);
    } else {
        invariant(_stateDoc.getTenantIds());
        BSONArrayBuilder arrayBuilder(bob.subarrayStart("tenantIds"));
        for (const auto& tenantId : *_stateDoc.getTenantIds()) {
            tenantId.serializeToBSON(&arrayBuilder);
        }
    }

    bob.append("recipientConnectionString", _recipientConnectionString);
    bob.append("readPreference", _readPreference.toInnerBSON());
    bob.append("receivedCancellation", _abortRequested);
    if (_durableState) {
        bob.append("lastDurableState",
                   TenantMigrationDonorState_serializer(_durableState.get().state));
    } else {
        bob.appendUndefined("lastDurableState");
    }
    if (_stateDoc.getMigrationStart()) {
        bob.appendDate("migrationStart", *_stateDoc.getMigrationStart());
    }
    if (_stateDoc.getExpireAt()) {
        bob.appendDate("expireAt", *_stateDoc.getExpireAt());
    }
    if (_stateDoc.getStartMigrationDonorTimestamp()) {
        bob.append("startMigrationDonorTimestamp", *_stateDoc.getStartMigrationDonorTimestamp());
    }
    if (_stateDoc.getBlockTimestamp()) {
        bob.append("blockTimestamp", *_stateDoc.getBlockTimestamp());
    }
    if (_stateDoc.getCommitOrAbortOpTime()) {
        _stateDoc.getCommitOrAbortOpTime()->append(&bob, "commitOrAbortOpTime");
    }
    if (_stateDoc.getAbortReason()) {
        bob.append("abortReason", *_stateDoc.getAbortReason());
    }
    return bob.obj();
}

void TenantMigrationDonorService::Instance::checkIfOptionsConflict(const BSONObj& options) const {
    auto stateDoc = tenant_migration_access_blocker::parseDonorStateDocument(options);

    invariant(stateDoc.getId() == _migrationUuid);
    invariant(stateDoc.getProtocol());

    auto tenantIdsMatch = [&] {
        switch (_protocol) {
            case MigrationProtocolEnum::kShardMerge:
                invariant(stateDoc.getTenantIds());
                return *stateDoc.getTenantIds() == _tenantIds;
            case MigrationProtocolEnum::kMultitenantMigrations:
                invariant(stateDoc.getTenantId());
                return *stateDoc.getTenantId() == _tenantId;
        }
        MONGO_UNREACHABLE;
    };

    if (stateDoc.getProtocol().value() != _protocol || !tenantIdsMatch() ||
        stateDoc.getRecipientConnectionString() != _recipientConnectionString ||
        !stateDoc.getReadPreference().equals(_readPreference)) {
        uasserted(ErrorCodes::ConflictingOperationInProgress,
                  str::stream() << "Found active migration for migrationId \""
                                << _migrationUuid.toBSON() << "\" with different options "
                                << tenant_migration_util::redactStateDoc(_stateDoc.toBSON()));
    }
}

boost::optional<TenantMigrationDonorService::Instance::DurableState>
TenantMigrationDonorService::Instance::getDurableState() const {
    stdx::lock_guard<Latch> lg(_mutex);
    return _durableState;
}

void TenantMigrationDonorService::Instance::onReceiveDonorAbortMigration() {
    stdx::lock_guard<Latch> lg(_mutex);
    _abortRequested = true;
    if (_abortMigrationSource) {
        _abortMigrationSource->cancel();
    }
    if (auto fetcher = _recipientKeysFetcher.lock()) {
        fetcher->shutdown();
    }
}

void TenantMigrationDonorService::Instance::onReceiveDonorForgetMigration() {
    stdx::lock_guard<Latch> lg(_mutex);
    setPromiseOkIfNotReady(lg, _receiveDonorForgetMigrationPromise);
}

void TenantMigrationDonorService::Instance::interrupt(Status status) {
    stdx::lock_guard<Latch> lg(_mutex);
    // Resolve any unresolved promises to avoid hanging.
    setPromiseErrorIfNotReady(lg, _initialDonorStateDurablePromise, status);
    setPromiseErrorIfNotReady(lg, _receiveDonorForgetMigrationPromise, status);
    setPromiseErrorIfNotReady(lg, _forgetMigrationDurablePromise, status);
    setPromiseErrorIfNotReady(lg, _decisionPromise, status);

    if (auto fetcher = _recipientKeysFetcher.lock()) {
        fetcher->shutdown();
    }
}

ExecutorFuture<repl::OpTime> TenantMigrationDonorService::Instance::_insertStateDoc(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    stdx::lock_guard<Latch> lg(_mutex);

    invariant(_stateDoc.getState() == TenantMigrationDonorStateEnum::kUninitialized);
    _stateDoc.setState(TenantMigrationDonorStateEnum::kAbortingIndexBuilds);

    return AsyncTry([this, self = shared_from_this()] {
               auto opCtxHolder = cc().makeOperationContext();
               auto opCtx = opCtxHolder.get();

               pauseTenantMigrationBeforeInsertingDonorStateDoc.pauseWhileSet(opCtx);

               auto collection =
                   acquireCollection(opCtx,
                                     CollectionAcquisitionRequest(
                                         _stateDocumentsNS,
                                         PlacementConcern{boost::none, ShardVersion::UNSHARDED()},
                                         repl::ReadConcernArgs::get(opCtx),
                                         AcquisitionPrerequisites::kWrite),
                                     MODE_IX);

               writeConflictRetry(
                   opCtx, "TenantMigrationDonorInsertStateDoc", _stateDocumentsNS, [&] {
                       const auto filter =
                           BSON(TenantMigrationDonorDocument::kIdFieldName << _migrationUuid);
                       const auto updateMod = [&]() {
                           stdx::lock_guard<Latch> lg(_mutex);
                           return BSON("$setOnInsert" << _stateDoc.toBSON());
                       }();
                       auto updateResult = Helpers::upsert(
                           opCtx, collection, filter, updateMod, /*fromMigrate=*/false);

                       // '$setOnInsert' update operator can never modify an existing on-disk state
                       // doc.
                       invariant(!updateResult.numDocsModified);
                   });

               return repl::ReplClientInfo::forClient(opCtx->getClient()).getLastOp();
           })
        .until([&](StatusWith<repl::OpTime> swOpTime) {
            if (swOpTime.getStatus().code() == ErrorCodes::ConflictingServerlessOperation) {
                LOGV2(6531508,
                      "Tenant migration failed to start due to serverless lock error",
                      "id"_attr = _migrationUuid,
                      "status"_attr = swOpTime.getStatus());
                uassertStatusOK(swOpTime);
            }
            return swOpTime.getStatus().isOK();
        })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<repl::OpTime> TenantMigrationDonorService::Instance::_updateStateDoc(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    const TenantMigrationDonorStateEnum nextState,
    const CancellationToken& token) {
    stdx::lock_guard<Latch> lg(_mutex);

    const auto originalStateDocBson = _stateDoc.toBSON();

    return AsyncTry([this, self = shared_from_this(), executor, nextState, originalStateDocBson] {
               boost::optional<repl::OpTime> updateOpTime;

               auto opCtxHolder = cc().makeOperationContext();
               auto opCtx = opCtxHolder.get();

               pauseTenantMigrationDonorWhileUpdatingStateDoc.pauseWhileSet(opCtx);

               AutoGetCollection collection(opCtx, _stateDocumentsNS, MODE_IX);

               uassert(ErrorCodes::NamespaceNotFound,
                       str::stream()
                           << _stateDocumentsNS.toStringForErrorMsg() << " does not exist",
                       collection);

               writeConflictRetry(
                   opCtx, "TenantMigrationDonorUpdateStateDoc", _stateDocumentsNS, [&] {
                       WriteUnitOfWork wuow(opCtx);

                       const auto originalRecordId = Helpers::findOne(
                           opCtx, collection.getCollection(), originalStateDocBson);
                       const auto originalSnapshot = Snapshotted<BSONObj>(
                           shard_role_details::getRecoveryUnit(opCtx)->getSnapshotId(),
                           originalStateDocBson);
                       invariant(!originalRecordId.isNull());

                       if (nextState == TenantMigrationDonorStateEnum::kBlocking) {
                           // Start blocking writes before getting an oplog slot to guarantee no
                           // writes to the tenant's data can commit with a timestamp after the
                           // block timestamp.
                           auto mtabVector =
                               TenantMigrationAccessBlockerRegistry::get(_serviceContext)
                                   .getDonorAccessBlockersForMigration(_migrationUuid);
                           invariant(!mtabVector.empty());
                           for (auto& mtab : mtabVector) {
                               mtab->startBlockingWrites();
                           }

                           shard_role_details::getRecoveryUnit(opCtx)->onRollback(
                               [mtabVector](OperationContext*) {
                                   for (auto& mtab : mtabVector) {
                                       mtab->rollBackStartBlocking();
                                   }
                               });
                       }

                       // Reserve an opTime for the write.
                       auto oplogSlot = LocalOplogInfo::get(opCtx)->getNextOpTimes(opCtx, 1U)[0];
                       {
                           stdx::lock_guard<Latch> lg(_mutex);

                           // Update the state.
                           _stateDoc.setState(nextState);
                           switch (nextState) {
                               case TenantMigrationDonorStateEnum::kDataSync: {
                                   _stateDoc.setStartMigrationDonorTimestamp(
                                       oplogSlot.getTimestamp());
                                   break;
                               }
                               case TenantMigrationDonorStateEnum::kBlocking: {
                                   _stateDoc.setBlockTimestamp(oplogSlot.getTimestamp());
                                   break;
                               }
                               case TenantMigrationDonorStateEnum::kCommitted:
                                   _stateDoc.setCommitOrAbortOpTime(oplogSlot);
                                   break;
                               case TenantMigrationDonorStateEnum::kAborted: {
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
                       }

                       const auto updatedStateDocBson = [&]() {
                           stdx::lock_guard<Latch> lg(_mutex);
                           return _stateDoc.toBSON();
                       }();

                       CollectionUpdateArgs args{originalSnapshot.value()};
                       args.criteria = BSON("_id" << _migrationUuid);
                       args.oplogSlots = {oplogSlot};
                       args.update = updatedStateDocBson;

                       collection_internal::updateDocument(opCtx,
                                                           *collection,
                                                           originalRecordId,
                                                           originalSnapshot,
                                                           updatedStateDocBson,
                                                           collection_internal::kUpdateNoIndexes,
                                                           nullptr /* indexesAffected */,
                                                           nullptr /* OpDebug* */,
                                                           &args);

                       wuow.commit();

                       if (nextState == TenantMigrationDonorStateEnum::kCommitted) {
                           pauseTenantMigrationAfterUpdatingToCommittedState.pauseWhileSet();
                       }

                       updateOpTime = oplogSlot;
                   });

               invariant(updateOpTime);
               return updateOpTime.value();
           })
        .until([](StatusWith<repl::OpTime> swOpTime) { return swOpTime.getStatus().isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<repl::OpTime>
TenantMigrationDonorService::Instance::_markStateDocAsGarbageCollectable(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    stdx::lock_guard<Latch> lg(_mutex);

    _stateDoc.setExpireAt(_serviceContext->getFastClockSource()->now() +
                          Milliseconds{repl::tenantMigrationGarbageCollectionDelayMS.load()});
    return AsyncTry([this, self = shared_from_this()] {
               auto opCtxHolder = cc().makeOperationContext();
               auto opCtx = opCtxHolder.get();

               pauseTenantMigrationDonorBeforeMarkingStateGarbageCollectable.pauseWhileSet(opCtx);

               auto collection =
                   acquireCollection(opCtx,
                                     CollectionAcquisitionRequest(
                                         _stateDocumentsNS,
                                         PlacementConcern{boost::none, ShardVersion::UNSHARDED()},
                                         repl::ReadConcernArgs::get(opCtx),
                                         AcquisitionPrerequisites::kWrite),
                                     MODE_IX);

               writeConflictRetry(
                   opCtx,
                   "TenantMigrationDonorMarkStateDocAsGarbageCollectable",
                   _stateDocumentsNS,
                   [&] {
                       const auto filter =
                           BSON(TenantMigrationDonorDocument::kIdFieldName << _migrationUuid);
                       const auto updateMod = [&]() {
                           stdx::lock_guard<Latch> lg(_mutex);
                           return _stateDoc.toBSON();
                       }();
                       auto updateResult = Helpers::upsert(
                           opCtx, collection, filter, updateMod, /*fromMigrate=*/false);

                       invariant(updateResult.numDocsModified == 1);
                   });

               return repl::ReplClientInfo::forClient(opCtx->getClient()).getLastOp();
           })
        .until([](StatusWith<repl::OpTime> swOpTime) { return swOpTime.getStatus().isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<void> TenantMigrationDonorService::Instance::_removeStateDoc(
    std::shared_ptr<executor::ScopedTaskExecutor> executor, const CancellationToken& token) {
    return AsyncTry([this, self = shared_from_this()] {
               auto opCtxHolder = cc().makeOperationContext();
               auto opCtx = opCtxHolder.get();

               pauseTenantMigrationDonorBeforeDeletingStateDoc.pauseWhileSet(opCtx);

               PersistentTaskStore<TenantMigrationDonorDocument> store(_stateDocumentsNS);
               store.remove(
                   opCtx,
                   BSON(TenantMigrationDonorDocument::kIdFieldName << _migrationUuid),
                   WriteConcernOptions(1, WriteConcernOptions::SyncMode::UNSET, Seconds(0)));
           })
        .until([](Status status) { return status.isOK(); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<void> TenantMigrationDonorService::Instance::_waitForMajorityWriteConcern(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    repl::OpTime opTime,
    const CancellationToken& token) {
    return WaitForMajorityService::get(_serviceContext)
        .waitUntilMajorityForWrite(_serviceContext, std::move(opTime), token)
        .thenRunOn(**executor)
        .then([this, self = shared_from_this()] {
            stdx::lock_guard<Latch> lg(_mutex);
            switch (_stateDoc.getState()) {
                case TenantMigrationDonorStateEnum::kAbortingIndexBuilds:
                    setPromiseOkIfNotReady(lg, _initialDonorStateDurablePromise);
                    break;
                case TenantMigrationDonorStateEnum::kDataSync:
                case TenantMigrationDonorStateEnum::kBlocking:
                case TenantMigrationDonorStateEnum::kCommitted:
                case TenantMigrationDonorStateEnum::kAborted:
                    break;
                default:
                    MONGO_UNREACHABLE;
            }

            _durableState = DurableState{_stateDoc.getState(),
                                         _stateDoc.getAbortReason(),
                                         _stateDoc.getExpireAt(),
                                         _stateDoc.getBlockTimestamp()};
        });
}

ExecutorFuture<void> TenantMigrationDonorService::Instance::_sendRecipientSyncDataCommand(
    std::shared_ptr<executor::ScopedTaskExecutor> exec,
    std::shared_ptr<RemoteCommandTargeter> recipientTargeterRS,
    const CancellationToken& token) {
    auto donorConnString =
        repl::ReplicationCoordinator::get(_serviceContext)->getConfigConnectionString();

    RecipientSyncData request;
    request.setDbName(DatabaseName::kAdmin);

    MigrationRecipientCommonData commonData(
        _migrationUuid, donorConnString.toString(), _readPreference);
    if (_protocol == MigrationProtocolEnum::kMultitenantMigrations) {
        commonData.setTenantId(boost::optional<StringData>(_tenantId));
    } else {
        commonData.setTenantIds(_tenantIds);
    }

    commonData.setProtocol(_protocol);
    request.setMigrationRecipientCommonData(commonData);

    {
        stdx::lock_guard<Latch> lg(_mutex);
        invariant(_stateDoc.getStartMigrationDonorTimestamp());
        request.setStartMigrationDonorTimestamp(*_stateDoc.getStartMigrationDonorTimestamp());
        request.setReturnAfterReachingDonorTimestamp(_stateDoc.getBlockTimestamp());
    }

    auto asyncTargeter = std::make_unique<async_rpc::AsyncRemoteCommandTargeterAdapter>(
        kPrimaryOnlyReadPreference, recipientTargeterRS);
    auto retryPolicy =
        std::make_shared<RecipientSyncDataRetryPolicy>(getProtocol(), kExponentialBackoff);
    auto options =
        std::make_shared<RecipientSyncDataRPCOptions>(**exec, token, request, retryPolicy);
    auto cmdRes = async_rpc::sendCommand(options, _serviceContext, std::move(asyncTargeter));
    return std::move(cmdRes).ignoreValue().onError(
        [returnAfterReachingDonorTs =
             request.getReturnAfterReachingDonorTimestamp().has_value()](Status status) {
            std::stringstream errMsg;
            errMsg << "'recipientSyncData' command";
            if (returnAfterReachingDonorTs)
                errMsg << " with "
                       << RecipientSyncData::kReturnAfterReachingDonorTimestampFieldName;
            errMsg << " failed";

            return async_rpc::unpackRPCStatusIgnoringWriteConcernAndWriteErrors(status).addContext(
                errMsg.str());
        });
}

ExecutorFuture<void> TenantMigrationDonorService::Instance::_sendRecipientForgetMigrationCommand(
    std::shared_ptr<executor::ScopedTaskExecutor> exec,
    std::shared_ptr<RemoteCommandTargeter> recipientTargeterRS,
    const CancellationToken& token) {

    auto donorConnString =
        repl::ReplicationCoordinator::get(_serviceContext)->getConfigConnectionString();

    RecipientForgetMigration request;
    request.setDbName(DatabaseName::kAdmin);

    MigrationRecipientCommonData commonData(
        _migrationUuid, donorConnString.toString(), _readPreference);
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_protocol == MigrationProtocolEnum::kMultitenantMigrations) {
            commonData.setTenantId(boost::optional<StringData>(_tenantId));
        } else {
            commonData.setTenantIds(_tenantIds);
            if (_stateDoc.getState() == TenantMigrationDonorStateEnum::kCommitted) {
                request.setDecision(MigrationDecisionEnum::kCommitted);
            } else {
                request.setDecision(MigrationDecisionEnum::kAborted);
            }
        }
    }

    commonData.setProtocol(_protocol);
    request.setMigrationRecipientCommonData(commonData);

    auto asyncTargeter = std::make_unique<async_rpc::AsyncRemoteCommandTargeterAdapter>(
        kPrimaryOnlyReadPreference, recipientTargeterRS);
    auto retryPolicy = std::make_shared<RecipientForgetMigrationRetryPolicy>(kExponentialBackoff);
    auto options =
        std::make_shared<RecipientForgetMigrationRPCOptions>(**exec, token, request, retryPolicy);
    auto cmdRes = async_rpc::sendCommand(options, _serviceContext, std::move(asyncTargeter));
    return std::move(cmdRes).ignoreValue().onError([](Status status) {
        return async_rpc::unpackRPCStatusIgnoringWriteConcernAndWriteErrors(status).addContext(
            "'recipientForgetMigration' command failed");
    });
}

void TenantMigrationDonorService::Instance::validateTenantIdsForProtocol() {
    switch (_protocol) {
        case MigrationProtocolEnum::kShardMerge:
            uassert(ErrorCodes::InvalidOptions,
                    "The field tenantIds must be set and not empty for protocol 'shard merge'",
                    !_tenantIds.empty());
            break;
        case MigrationProtocolEnum::kMultitenantMigrations:
            uassert(ErrorCodes::InvalidOptions,
                    "The field tenantIds must not be set for protocol 'multitenant migration'",
                    _tenantIds.empty());
            break;
        default:
            MONGO_UNREACHABLE;
    }
}

CancellationToken TenantMigrationDonorService::Instance::_initAbortMigrationSource(
    const CancellationToken& token) {
    stdx::lock_guard<Latch> lg(_mutex);
    invariant(!_abortMigrationSource);
    _abortMigrationSource = CancellationSource(token);

    if (_abortRequested) {
        // An abort was requested before the abort source was set up so immediately cancel it.
        _abortMigrationSource->cancel();
    }

    return _abortMigrationSource->token();
}

SemiFuture<void> TenantMigrationDonorService::Instance::run(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    const CancellationToken& token) noexcept {
    pauseTenantMigrationBeforeEnteringFutureChain.pauseWhileSet();

    LOGV2(7559500,
          "Starting tenant migration donor instance: ",
          "migrationId"_attr = _migrationUuid,
          "protocol"_attr = MigrationProtocol_serializer(_protocol),
          "recipientConnectionString"_attr = _recipientConnectionString,
          "readPreference"_attr = _readPreference);

    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (!_stateDoc.getMigrationStart()) {
            _stateDoc.setMigrationStart(_serviceContext->getFastClockSource()->now());
        }
    }

    auto isFCVUpgradingOrDowngrading = [&]() -> bool {
        // We must abort the migration if we try to start or resume while upgrading or downgrading.
        // (Generic FCV reference): This FCV check should exist across LTS binary versions.
        if (serverGlobalParams.featureCompatibility.acquireFCVSnapshot()
                .isUpgradingOrDowngrading()) {
            LOGV2(5356302, "Must abort tenant migration as donor is upgrading or downgrading");
            return true;
        }
        return false;
    };


    // Tenant migrations gets aborted on FCV upgrading or downgrading state. But,
    // due to race between between Instance::getOrCreate() and
    // SetFeatureCompatibilityVersionCommand::_cancelTenantMigrations(), we might miss aborting this
    // tenant migration and FCV might have updated or downgraded at this point. So, need to ensure
    // that the protocol is still compatible with FCV.
    if (isFCVUpgradingOrDowngrading()) {
        onReceiveDonorAbortMigration();
    }

    // Any FCV changes after this point will abort this migration.
    auto abortToken = _initAbortMigrationSource(token);

    auto recipientTargeterRS = std::make_shared<RemoteCommandTargeterRS>(
        _recipientUri.getSetName(), _recipientUri.getServers());
    auto scopedOutstandingMigrationCounter =
        TenantMigrationStatistics::get(_serviceContext)->getScopedOutstandingDonatingCount();

    return ExecutorFuture(**executor)
        .then([this, self = shared_from_this(), executor, token] {
            // Validate the field is correctly set
            validateTenantIdsForProtocol();

            LOGV2(6104900,
                  "Entering 'aborting index builds' state.",
                  "migrationId"_attr = _migrationUuid);
            // Note we do not use the abort migration token here because the donorAbortMigration
            // command waits for a decision to be persisted which will not happen if inserting
            // the initial state document fails.
            return _enterAbortingIndexBuildsState(executor, token);
        })
        .then([this, self = shared_from_this(), executor, abortToken] {
            LOGV2(6104901, "Aborting index builds.", "migrationId"_attr = _migrationUuid);
            _abortIndexBuilds(abortToken);
        })
        .then([this, self = shared_from_this(), executor, recipientTargeterRS, abortToken] {
            LOGV2(6104902,
                  "Fetching cluster time key documents from recipient.",
                  "migrationId"_attr = _migrationUuid);
            return _fetchAndStoreRecipientClusterTimeKeyDocs(
                executor, recipientTargeterRS, abortToken);
        })
        .then([this, self = shared_from_this(), executor, abortToken] {
            LOGV2(6104903, "Entering 'data sync' state.", "migrationId"_attr = _migrationUuid);
            return _enterDataSyncState(executor, abortToken);
        })
        .then([this, self = shared_from_this(), executor, recipientTargeterRS, abortToken] {
            LOGV2(6104904,
                  "Waiting for recipient to finish data sync and become consistent.",
                  "migrationId"_attr = _migrationUuid);
            return _waitForRecipientToBecomeConsistentAndEnterBlockingState(
                executor, recipientTargeterRS, abortToken);
        })
        .then([this, self = shared_from_this(), executor, recipientTargeterRS, abortToken, token] {
            LOGV2(6104905,
                  "Waiting for recipient to reach the block timestamp.",
                  "migrationId"_attr = _migrationUuid);
            return _waitForRecipientToReachBlockTimestampAndEnterCommittedState(
                executor, recipientTargeterRS, abortToken, token);
        })
        // Note from here on the migration cannot be aborted, so only the token from the primary
        // only service should be used.
        .onError([this, self = shared_from_this(), executor, token, abortToken](Status status) {
            return _handleErrorOrEnterAbortedState(executor, token, abortToken, status);
        })
        .onCompletion([this, self = shared_from_this()](Status status) {
            stdx::lock_guard<Latch> lg(_mutex);
            if (!_stateDoc.getExpireAt()) {
                // Avoid double counting tenant migration statistics after failover.
                // Double counting may still happen if the failover to the same primary
                // happens after this block and before the state doc GC is persisted.
                if (_abortReason) {
                    TenantMigrationStatistics::get(_serviceContext)
                        ->incTotalMigrationDonationsAborted();
                } else {
                    TenantMigrationStatistics::get(_serviceContext)
                        ->incTotalMigrationDonationsCommitted();
                }
            }

            return Status::OK();
        })
        .then([this, self = shared_from_this(), executor, token, recipientTargeterRS] {
            return _waitForForgetMigrationThenMarkMigrationGarbageCollectable(
                executor, recipientTargeterRS, token);
        })
        .then([this, self = shared_from_this(), executor, token] {
            pauseTenantMigrationDonorAfterMarkingStateGarbageCollectable.pauseWhileSet();
            {
                stdx::lock_guard<Latch> lg(_mutex);
                setPromiseOkIfNotReady(lg, _forgetMigrationDurablePromise);
            }
            return _waitForGarbageCollectionDelayThenDeleteStateDoc(executor, token);
        })
        .onCompletion([this,
                       self = shared_from_this(),
                       token,
                       scopedCounter{std::move(scopedOutstandingMigrationCounter)}](Status status) {
            // Don't set the forget migration durable promise if the instance has been canceled. We
            // assume whatever canceled the token will also set the promise with an appropriate
            // error.
            checkForTokenInterrupt(token);

            stdx::lock_guard<Latch> lg(_mutex);

            setPromiseFromStatusIfNotReady(lg, _forgetMigrationDurablePromise, status);

            // If a ConflictingServerlessOperation was thrown, ensure a valid _abortReason exists.
            if (!_abortReason &&
                isNotDurableAndServerlessConflict(lg, _initialDonorStateDurablePromise)) {
                _abortReason.emplace(_initialDonorStateDurablePromise.getFuture().getNoThrow());
            }

            LOGV2(5006601,
                  "Tenant migration completed",
                  "migrationId"_attr = _migrationUuid,
                  "status"_attr = status,
                  "abortReason"_attr = _abortReason);

            // If a ConflictingServerlessOperation was thrown during the initial insertion we do not
            // have a state document. In that case return the error to PrimaryOnlyService so it
            // frees the instance from its map.
            if (isNotDurableAndServerlessConflict(lg, _initialDonorStateDurablePromise)) {
                uassertStatusOK(_initialDonorStateDurablePromise.getFuture().getNoThrow());
            }
        })
        .semi();
}

ExecutorFuture<void> TenantMigrationDonorService::Instance::_enterAbortingIndexBuildsState(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor, const CancellationToken& token) {
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > TenantMigrationDonorStateEnum::kUninitialized) {
            return ExecutorFuture(**executor);
        }
    }

    // Enter "abortingIndexBuilds" state.
    return _insertStateDoc(executor, token)
        .then([this, self = shared_from_this(), executor, token](repl::OpTime opTime) {
            return _waitForMajorityWriteConcern(executor, std::move(opTime), token);
        })
        .then([this, self = shared_from_this()] {
            auto opCtxHolder = cc().makeOperationContext();
            auto opCtx = opCtxHolder.get();
            pauseTenantMigrationAfterPersistingInitialDonorStateDoc.pauseWhileSet(opCtx);
        });
}

void TenantMigrationDonorService::Instance::_abortIndexBuilds(const CancellationToken& token) {
    checkForTokenInterrupt(token);

    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > TenantMigrationDonorStateEnum::kAbortingIndexBuilds) {
            return;
        }
    }

    // Before starting data sync, abort any in-progress index builds.  No new index
    // builds can start while we are doing this because the mtab prevents it.
    {
        auto opCtxHolder = cc().makeOperationContext();
        auto* opCtx = opCtxHolder.get();
        auto* indexBuildsCoordinator = IndexBuildsCoordinator::get(opCtx);
        boost::optional<TenantId> tid = boost::none;
        if (!_tenantId.empty()) {
            tid = TenantId::parseFromString(_tenantId);
        }
        indexBuildsCoordinator->abortTenantIndexBuilds(opCtx, _protocol, tid, "tenant migration");
    }
}

ExecutorFuture<void>
TenantMigrationDonorService::Instance::_fetchAndStoreRecipientClusterTimeKeyDocs(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    std::shared_ptr<RemoteCommandTargeter> recipientTargeterRS,
    const CancellationToken& token) {
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > TenantMigrationDonorStateEnum::kAbortingIndexBuilds) {
            return ExecutorFuture(**executor);
        }
    }

    return AsyncTry([this, self = shared_from_this(), executor, recipientTargeterRS, token] {
               return recipientTargeterRS->findHost(kPrimaryOnlyReadPreference, token)
                   .thenRunOn(**executor)
                   .then([this, self = shared_from_this(), executor, token](HostAndPort host) {
                       pauseTenantMigrationBeforeFetchingKeys.pauseWhileSet();

                       const auto nss = NamespaceString::kKeysCollectionNamespace;

                       const auto cmdObj = [&] {
                           FindCommandRequest request(NamespaceStringOrUUID{nss});
                           request.setReadConcern(
                               repl::ReadConcernArgs(repl::ReadConcernLevel::kMajorityReadConcern)
                                   .toBSONInner());
                           return request.toBSON(BSONObj());
                       }();

                       auto keyDocs =
                           std::make_shared<std::vector<ExternalKeysCollectionDocument>>();
                       auto fetchStatus = std::make_shared<boost::optional<Status>>();

                       auto fetcherCallback = [this,
                                               self = shared_from_this(),
                                               fetchStatus,
                                               keyDocs](
                                                  const Fetcher::QueryResponseStatus& dataStatus,
                                                  Fetcher::NextAction* nextAction,
                                                  BSONObjBuilder* getMoreBob) {
                           // Throw out any accumulated results on error
                           if (!dataStatus.isOK()) {
                               *fetchStatus = dataStatus.getStatus();
                               keyDocs->clear();
                               return;
                           }

                           const auto& data = dataStatus.getValue();
                           for (const BSONObj& doc : data.documents) {
                               keyDocs->push_back(
                                   keys_collection_util::makeExternalClusterTimeKeyDoc(
                                       doc.getOwned(), _migrationUuid, boost::none /* expireAt */));
                           }
                           *fetchStatus = Status::OK();

                           if (!getMoreBob) {
                               return;
                           }
                           getMoreBob->append("getMore", data.cursorId);
                           getMoreBob->append("collection", data.nss.coll());
                       };

                       auto fetcher = std::make_shared<Fetcher>(
                           *executor,
                           host,
                           nss.dbName(),
                           cmdObj,
                           fetcherCallback,
                           kPrimaryOnlyReadPreference.toContainingBSON(),
                           executor::RemoteCommandRequest::kNoTimeout, /* findNetworkTimeout */
                           executor::RemoteCommandRequest::kNoTimeout, /* getMoreNetworkTimeout */
                           RemoteCommandRetryScheduler::makeRetryPolicy<
                               ErrorCategory::RetriableError>(
                               kMaxRecipientKeyDocsFindAttempts,
                               executor::RemoteCommandRequest::kNoTimeout));

                       {
                           stdx::lock_guard<Latch> lg(_mutex);
                           // Note the fetcher cannot be canceled via token, so this check for
                           // interrupt is required otherwise stepdown/shutdown could block waiting
                           // for the fetcher to complete.
                           checkForTokenInterrupt(token);
                           _recipientKeysFetcher = fetcher;
                       }

                       uassertStatusOK(fetcher->schedule());

                       // We use the instance cleanup executor instead of the scoped task executor
                       // here in order to avoid a self-deadlock situation in the Fetcher during
                       // failovers.
                       return fetcher->onCompletion()
                           .thenRunOn(_donorService->getInstanceCleanupExecutor())
                           .then(
                               [this, self = shared_from_this(), fetchStatus, keyDocs, fetcher]() {
                                   {
                                       stdx::lock_guard<Latch> lg(_mutex);
                                       _recipientKeysFetcher.reset();
                                   }

                                   if (!*fetchStatus) {
                                       // The callback never got invoked.
                                       uasserted(
                                           5340400,
                                           "Internal error running cursor callback in command");
                                   }

                                   uassertStatusOK(fetchStatus->get());

                                   return *keyDocs;
                               });
                   })
                   .then([this, self = shared_from_this(), executor, token](auto keyDocs) {
                       checkForTokenInterrupt(token);

                       auto opCtx = cc().makeOperationContext();
                       pauseTenantMigrationDonorBeforeStoringExternalClusterTimeKeyDocs
                           .pauseWhileSet(opCtx.get());
                       return keys_collection_util::storeExternalClusterTimeKeyDocs(
                           opCtx.get(), std::move(keyDocs));
                   })
                   .then([this, self = shared_from_this(), token](repl::OpTime lastKeyOpTime) {
                       pauseTenantMigrationDonorBeforeWaitingForKeysToReplicate.pauseWhileSet();

                       auto allMembersWriteConcern =
                           WriteConcernOptions(repl::ReplSetConfig::kConfigAllWriteConcernName,
                                               WriteConcernOptions::SyncMode::NONE,
                                               WriteConcernOptions::kNoTimeout);
                       auto writeConcernFuture = repl::ReplicationCoordinator::get(_serviceContext)
                                                     ->awaitReplicationAsyncNoWTimeout(
                                                         lastKeyOpTime, allMembersWriteConcern);
                       return future_util::withCancellation(std::move(writeConcernFuture), token);
                   });
           })
        .until([](Status status) { return shouldStopFetchingRecipientClusterTimeKeyDocs(status); })
        .withBackoffBetweenIterations(kExponentialBackoff)
        .on(**executor, token);
}

ExecutorFuture<void> TenantMigrationDonorService::Instance::_enterDataSyncState(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
    const CancellationToken& abortToken) {
    pauseTenantMigrationAfterFetchingAndStoringKeys.pauseWhileSet();
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > TenantMigrationDonorStateEnum::kAbortingIndexBuilds) {
            return ExecutorFuture(**executor);
        }
    }

    pauseTenantMigrationBeforeLeavingAbortingIndexBuildsState.pauseWhileSet();

    // Enter "dataSync" state.
    return _updateStateDoc(executor, TenantMigrationDonorStateEnum::kDataSync, abortToken)
        .then([this, self = shared_from_this(), executor, abortToken](repl::OpTime opTime) {
            return _waitForMajorityWriteConcern(executor, std::move(opTime), abortToken);
        });
}

ExecutorFuture<void>
TenantMigrationDonorService::Instance::_waitUntilStartMigrationDonorTimestampIsCheckpointed(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
    const CancellationToken& abortToken) {

    if (getProtocol() != MigrationProtocolEnum::kShardMerge) {
        return ExecutorFuture(**executor);
    }

    auto opCtxHolder = cc().makeOperationContext();
    auto opCtx = opCtxHolder.get();
    auto startMigrationDonorTimestamp = [&] {
        stdx::lock_guard<Latch> lg(_mutex);
        return *_stateDoc.getStartMigrationDonorTimestamp();
    }();

    invariant(startMigrationDonorTimestamp <= repl::ReplicationCoordinator::get(opCtx)
                                                  ->getCurrentCommittedSnapshotOpTime()
                                                  .getTimestamp());

    // For shard merge, we set startApplyingDonorOpTime timestamp on the recipient to the donor's
    // backup cursor checkpoint timestamp, and startMigrationDonorTimestamp to the timestamp after
    // aborting all index builds. As a result, startApplyingDonorOpTime timestamp can be <
    // startMigrationDonorTimestamp, which means we can erroneously fetch and apply index build
    // operations before startMigrationDonorTimestamp. Trigger a stable checkpoint to ensure that
    // the recipient does not fetch and apply donor index build entries before
    // startMigrationDonorTimestamp.
    return AsyncTry([this, self = shared_from_this(), startMigrationDonorTimestamp] {
               auto opCtxHolder = cc().makeOperationContext();
               auto opCtx = opCtxHolder.get();
               auto storageEngine = opCtx->getServiceContext()->getStorageEngine();
               if (storageEngine->getLastStableRecoveryTimestamp() < startMigrationDonorTimestamp) {
                   shard_role_details::getRecoveryUnit(opCtx)->waitUntilUnjournaledWritesDurable(
                       opCtx,
                       /*stableCheckpoint*/ true);
               }
           })
        .until([this, self = shared_from_this(), startMigrationDonorTimestamp](Status status) {
            uassertStatusOK(status);
            auto storageEngine = _serviceContext->getStorageEngine();
            if (storageEngine->getLastStableRecoveryTimestamp() < startMigrationDonorTimestamp) {
                return false;
            }
            return true;
        })
        .withBackoffBetweenIterations(Backoff(Milliseconds(100), Milliseconds(100)))
        .on(**executor, abortToken);
}

ExecutorFuture<void>
TenantMigrationDonorService::Instance::_waitForRecipientToBecomeConsistentAndEnterBlockingState(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
    std::shared_ptr<RemoteCommandTargeter> recipientTargeterRS,
    const CancellationToken& abortToken) {
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > TenantMigrationDonorStateEnum::kDataSync) {
            return ExecutorFuture(**executor);
        }
    }

    return _waitUntilStartMigrationDonorTimestampIsCheckpointed(executor, abortToken)
        .then([this, self = shared_from_this(), executor, recipientTargeterRS, abortToken] {
            return _sendRecipientSyncDataCommand(executor, recipientTargeterRS, abortToken);
        })
        .then([this, self = shared_from_this()] {
            auto opCtxHolder = cc().makeOperationContext();
            auto opCtx = opCtxHolder.get();
            pauseTenantMigrationBeforeLeavingDataSyncState.pauseWhileSet(opCtx);
        })
        .then([this, self = shared_from_this(), executor, abortToken] {
            // Enter "blocking" state.
            LOGV2(6104907,
                  "Updating its state doc to enter 'blocking' state.",
                  "migrationId"_attr = _migrationUuid);
            return _updateStateDoc(executor, TenantMigrationDonorStateEnum::kBlocking, abortToken)
                .then([this, self = shared_from_this(), executor, abortToken](repl::OpTime opTime) {
                    return _waitForMajorityWriteConcern(executor, std::move(opTime), abortToken);
                });
        });
}

ExecutorFuture<void>
TenantMigrationDonorService::Instance::_waitForRecipientToReachBlockTimestampAndEnterCommittedState(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
    std::shared_ptr<RemoteCommandTargeter> recipientTargeterRS,
    const CancellationToken& abortToken,
    const CancellationToken& token) {
    {
        stdx::lock_guard<Latch> lg(_mutex);
        if (_stateDoc.getState() > TenantMigrationDonorStateEnum::kBlocking) {
            return ExecutorFuture(**executor);
        }

        invariant(_stateDoc.getBlockTimestamp());
    }
    // Source to cancel the timeout if the operation completed in time.
    CancellationSource cancelTimeoutSource;
    CancellationSource recipientSyncDataSource(abortToken);

    auto deadlineReachedFuture =
        (*executor)->sleepFor(Milliseconds(repl::tenantMigrationBlockingStateTimeoutMS.load()),
                              cancelTimeoutSource.token());

    return whenAny(std::move(deadlineReachedFuture),
                   _sendRecipientSyncDataCommand(
                       executor, recipientTargeterRS, recipientSyncDataSource.token()))
        .thenRunOn(**executor)
        .then([this, self = shared_from_this(), cancelTimeoutSource, recipientSyncDataSource](
                  auto result) mutable {
            const auto& [status, idx] = result;

            if (idx == 0) {
                LOGV2(5290301,
                      "Tenant migration blocking stage timeout expired",
                      "timeoutMs"_attr = repl::tenantMigrationBlockingStateTimeoutMS.load());
                // Deadline reached, cancel the pending '_sendRecipientSyncDataCommand()'...
                recipientSyncDataSource.cancel();
                // ...and return error.
                uasserted(ErrorCodes::ExceededTimeLimit, "Blocking state timeout expired");
            } else if (idx == 1) {
                // '_sendRecipientSyncDataCommand()' finished first, cancel the timeout.
                cancelTimeoutSource.cancel();
                return status;
            }
            MONGO_UNREACHABLE;
        })
        .then([this, self = shared_from_this()]() -> void {
            auto opCtxHolder = cc().makeOperationContext();
            auto opCtx = opCtxHolder.get();

            pauseTenantMigrationBeforeLeavingBlockingState.executeIf(
                [&](const BSONObj& data) {
                    if (!data.hasField("blockTimeMS")) {
                        pauseTenantMigrationBeforeLeavingBlockingState.pauseWhileSet(opCtx);
                    } else {
                        const auto blockTime = Milliseconds{data.getIntField("blockTimeMS")};
                        LOGV2(5010400,
                              "Keep migration in blocking state",
                              "blockTime"_attr = blockTime);
                        opCtx->sleepFor(blockTime);
                    }
                },
                [&](const BSONObj& data) {
                    return !data.hasField("tenantId") || _tenantId == data["tenantId"].str();
                });

            if (MONGO_unlikely(abortTenantMigrationBeforeLeavingBlockingState.shouldFail())) {
                uasserted(ErrorCodes::InternalError, "simulate a tenant migration error");
            }
        })
        .then([this, self = shared_from_this(), executor, abortToken, token] {
            // Last chance to abort
            checkForTokenInterrupt(abortToken);

            // Enter "commit" state.
            LOGV2(6104908, "Entering 'committed' state.", "migrationId"_attr = _migrationUuid);
            // Ignore the abort token once we've entered the committed state
            return _updateStateDoc(executor, TenantMigrationDonorStateEnum::kCommitted, token)
                .then([this, self = shared_from_this(), executor, token](repl::OpTime opTime) {
                    return _waitForMajorityWriteConcern(executor, std::move(opTime), token)
                        .then([this, self = shared_from_this()] {
                            pauseTenantMigrationBeforeLeavingCommittedState.pauseWhileSet();
                            stdx::lock_guard<Latch> lg(_mutex);
                            // If interrupt is called at some point during execution, it is
                            // possible that interrupt() will fulfill the promise before we
                            // do.
                            setPromiseOkIfNotReady(lg, _decisionPromise);
                        });
                });
        });
}

ExecutorFuture<void> TenantMigrationDonorService::Instance::_handleErrorOrEnterAbortedState(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
    const CancellationToken& token,
    const CancellationToken& abortToken,
    Status status) {
    // Don't handle errors if the instance token is canceled to guarantee we don't enter the abort
    // state because of an earlier error from token cancellation.
    checkForTokenInterrupt(token);

    {
        if (_stateDoc.getState() == TenantMigrationDonorStateEnum::kAborted) {
            // The migration was resumed on stepup and it was already aborted.
            return ExecutorFuture(**executor);
        }
    }

    // Note we must check the parent token has not been canceled so we don't change the error if the
    // abortToken was canceled because of an instance interruption. The checks don't need to be
    // atomic because a token cannot be uncanceled.
    if (abortToken.isCanceled() && !token.isCanceled()) {
        status = Status(ErrorCodes::TenantMigrationAborted, "Aborted due to donorAbortMigration.");
    }


    auto mtabVector = TenantMigrationAccessBlockerRegistry::get(_serviceContext)
                          .getDonorAccessBlockersForMigration(_migrationUuid);
    if (!_initialDonorStateDurablePromise.getFuture().isReady()) {
        // The migration failed either before or during inserting the state doc. Use the status to
        // fulfill the _initialDonorStateDurablePromise to fail the donorStartMigration command
        // immediately.
        stdx::lock_guard<Latch> lg(_mutex);
        setPromiseErrorIfNotReady(lg, _initialDonorStateDurablePromise, status);

        return ExecutorFuture(**executor);
    } else if (ErrorCodes::isNotPrimaryError(status) || ErrorCodes::isShutdownError(status)) {
        // Don't abort the migration on retriable errors that may have been generated by the local
        // server shutting/stepping down because it can be resumed when the client retries.
        stdx::lock_guard<Latch> lg(_mutex);
        setPromiseErrorIfNotReady(lg, _initialDonorStateDurablePromise, status);

        return ExecutorFuture(**executor);
    } else {
        LOGV2(6104912,
              "Entering 'aborted' state.",
              "migrationId"_attr = _migrationUuid,
              "status"_attr = status);
        // Enter "abort" state.
        _abortReason.emplace(status);
        return _updateStateDoc(executor, TenantMigrationDonorStateEnum::kAborted, token)
            .then([this, self = shared_from_this(), executor, token](repl::OpTime opTime) {
                return _waitForMajorityWriteConcern(executor, std::move(opTime), token)
                    .then([this, self = shared_from_this()] {
                        stdx::lock_guard<Latch> lg(_mutex);
                        // If interrupt is called at some point during execution, it is
                        // possible that interrupt() will fulfill the promise before we do.
                        setPromiseOkIfNotReady(lg, _decisionPromise);
                    });
            });
    }
}

ExecutorFuture<void>
TenantMigrationDonorService::Instance::_waitForForgetMigrationThenMarkMigrationGarbageCollectable(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor,
    std::shared_ptr<RemoteCommandTargeter> recipientTargeterRS,
    const CancellationToken& token) {
    const bool skipWaitingForForget = [&]() {
        stdx::lock_guard<Latch> lg(_mutex);
        if (!isNotDurableAndServerlessConflict(lg, _initialDonorStateDurablePromise)) {
            return false;
        }
        setPromiseErrorIfNotReady(lg,
                                  _receiveDonorForgetMigrationPromise,
                                  _initialDonorStateDurablePromise.getFuture().getNoThrow());
        return true;
    }();

    if (skipWaitingForForget) {
        return ExecutorFuture(**executor);
    }

    LOGV2(6104909,
          "Waiting to receive 'donorForgetMigration' command.",
          "migrationId"_attr = _migrationUuid);
    auto expiredAt = [&]() {
        stdx::lock_guard<Latch> lg(_mutex);
        return _stateDoc.getExpireAt();
    }();

    if (expiredAt) {
        // The migration state has already been marked as garbage collectable. Set the
        // donorForgetMigration promise here since the Instance's destructor has an
        // invariant that _receiveDonorForgetMigrationPromise is ready.
        onReceiveDonorForgetMigration();
        return ExecutorFuture(**executor);
    }

    // Wait for the donorForgetMigration command.
    // If donorAbortMigration has already canceled work, the abortMigrationSource would be
    // canceled and continued usage of the source would lead to incorrect behavior. Thus, we
    // need to use the token after the migration has reached a decision state in order to continue
    // work, such as sending donorForgetMigration, successfully.
    return std::move(_receiveDonorForgetMigrationPromise.getFuture())
        .thenRunOn(**executor)
        .then([this, self = shared_from_this(), executor, recipientTargeterRS, token] {
            {
                // If the abortReason is ConflictingServerlessOperation, it means there are no
                // document on the recipient. Do not send the forget command.
                stdx::lock_guard<Latch> lg(_mutex);
                if (_protocol == MigrationProtocolEnum::kMultitenantMigrations && _abortReason &&
                    _abortReason->code() == ErrorCodes::ConflictingServerlessOperation) {
                    return ExecutorFuture(**executor);
                }
            }

            LOGV2(6104910,
                  "Waiting for recipientForgetMigration response.",
                  "migrationId"_attr = _migrationUuid);
            return _sendRecipientForgetMigrationCommand(executor, recipientTargeterRS, token);
        })
        .then([this, self = shared_from_this(), executor, token] {
            LOGV2(6104911,
                  "Marking external keys as garbage collectable.",
                  "migrationId"_attr = _migrationUuid);
            // Note marking the keys as garbage collectable is not atomic with marking the
            // state document garbage collectable, so an interleaved failover can lead the
            // keys to be deleted before the state document has an expiration date. This is
            // acceptable because the decision to forget a migration is not reversible.
            return tenant_migration_util::markExternalKeysAsGarbageCollectable(
                _serviceContext,
                executor,
                _donorService->getInstanceCleanupExecutor(),
                _migrationUuid,
                token);
        })
        .then([this, self = shared_from_this(), executor, token] {
            LOGV2(6523600,
                  "Marking state document as garbage collectable.",
                  "migrationId"_attr = _migrationUuid);
            return _markStateDocAsGarbageCollectable(executor, token);
        })
        .then([this, self = shared_from_this(), executor, token](repl::OpTime opTime) {
            return _waitForMajorityWriteConcern(executor, std::move(opTime), token);
        });
}

ExecutorFuture<void>
TenantMigrationDonorService::Instance::_waitForGarbageCollectionDelayThenDeleteStateDoc(
    const std::shared_ptr<executor::ScopedTaskExecutor>& executor, const CancellationToken& token) {
    // If the state document was not inserted due to a conflicting serverless operation, do not
    // try to delete it.
    stdx::lock_guard<Latch> lg(_mutex);
    if (isNotDurableAndServerlessConflict(lg, _initialDonorStateDurablePromise)) {
        return ExecutorFuture(**executor);
    }

    LOGV2(8423362,
          "Waiting for garbage collection delay before deleting state document",
          "migrationId"_attr = _migrationUuid,
          "expireAt"_attr = *_stateDoc.getExpireAt());

    return (*executor)
        ->sleepUntil(*_stateDoc.getExpireAt(), token)
        .then([this, self = shared_from_this(), executor, token]() {
            LOGV2(8423363, "Deleting state document", "migrationId"_attr = _migrationUuid);
            return _removeStateDoc(executor, token);
        });
}

}  // namespace mongo
