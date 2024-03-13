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

#include <algorithm>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <memory>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/connection_string.h"
#include "mongo/client/mongo_uri.h"
#include "mongo/client/read_preference.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db/catalog/database.h"
#include "mongo/db/client.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/keys_collection_document_gen.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/repl/repl_server_parameters_gen.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/server_options.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/storage_engine.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/db/tenant_id.h"
#include "mongo/executor/scoped_task_executor.h"
#include "mongo/executor/task_executor.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/cancellation.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/net/ssl_util.h"
#include "mongo/util/str.h"
#include "mongo/util/uuid.h"

namespace mongo {

constexpr auto kDefaultMigrationProtocol = MigrationProtocolEnum::kMultitenantMigrations;

namespace {

const std::set<std::string> kUnsupportedTenantIds{"", "admin", "local", "config"};

}  // namespace

namespace repl {

// Common failpoints between ShardMergeRecipientService and TenantMigrationRecipientService.
extern FailPoint pauseBeforeRunTenantMigrationRecipientInstance;
extern FailPoint pauseAfterRunTenantMigrationRecipientInstance;
extern FailPoint skipTenantMigrationRecipientAuth;
extern FailPoint skipComparingRecipientAndDonorFCV;
extern FailPoint autoRecipientForgetMigration;
extern FailPoint skipFetchingCommittedTransactions;
extern FailPoint skipFetchingRetryableWritesEntriesBeforeStartOpTime;
extern FailPoint pauseTenantMigrationRecipientBeforeDeletingStateDoc;
extern FailPoint failWhilePersistingTenantMigrationRecipientInstanceStateDoc;
extern FailPoint fpAfterPersistingTenantMigrationRecipientInstanceStateDoc;
extern FailPoint fpBeforeFetchingDonorClusterTimeKeys;
extern FailPoint fpAfterConnectingTenantMigrationRecipientInstance;
extern FailPoint fpAfterRecordingRecipientPrimaryStartingFCV;
extern FailPoint fpAfterComparingRecipientAndDonorFCV;
extern FailPoint fpAfterRetrievingStartOpTimesMigrationRecipientInstance;
extern FailPoint fpSetSmallAggregationBatchSize;
extern FailPoint fpBeforeWaitingForRetryableWritePreFetchMajorityCommitted;
extern FailPoint pauseAfterRetrievingRetryableWritesBatch;
extern FailPoint fpAfterFetchingRetryableWritesEntriesBeforeStartOpTime;
extern FailPoint fpAfterStartingOplogFetcherMigrationRecipientInstance;
extern FailPoint setTenantMigrationRecipientInstanceHostTimeout;
extern FailPoint pauseAfterRetrievingLastTxnMigrationRecipientInstance;
extern FailPoint fpBeforeMarkingCloneSuccess;
extern FailPoint fpBeforeFetchingCommittedTransactions;
extern FailPoint fpAfterFetchingCommittedTransactions;
extern FailPoint fpAfterStartingOplogApplierMigrationRecipientInstance;
extern FailPoint fpBeforeFulfillingDataConsistentPromise;
extern FailPoint fpAfterDataConsistentMigrationRecipientInstance;
extern FailPoint fpBeforePersistingRejectReadsBeforeTimestamp;
extern FailPoint fpAfterWaitForRejectReadsBeforeTimestamp;
extern FailPoint hangBeforeTaskCompletion;
extern FailPoint fpAfterReceivingRecipientForgetMigration;
extern FailPoint hangAfterCreatingRSM;
extern FailPoint skipRetriesWhenConnectingToDonorHost;
extern FailPoint fpBeforeDroppingTempCollections;
extern FailPoint fpWaitUntilTimestampMajorityCommitted;
extern FailPoint hangAfterUpdatingTransactionEntry;
extern FailPoint fpBeforeAdvancingStableTimestamp;

}  // namespace repl

namespace tenant_migration_util {

inline Status validateDatabasePrefix(const std::string& tenantId) {
    const auto isValidTenantId = OID::parse(tenantId);
    if (!isValidTenantId.isOK()) {
        return Status(ErrorCodes::BadValue,
                      str::stream() << "Invalid tenant id format for tenant \'" << tenantId << "'");
    }

    const bool isPrefixSupported =
        kUnsupportedTenantIds.find(tenantId) == kUnsupportedTenantIds.end() &&
        tenantId.find('_') == std::string::npos;

    return isPrefixSupported
        ? Status::OK()
        : Status(ErrorCodes::BadValue,
                 str::stream() << "cannot migrate databases for tenant \'" << tenantId << "'");
}

inline Status validateDatabasePrefix(const TenantId& tenantId) {
    auto tId = tenantId.toString();
    const bool isPrefixSupported = kUnsupportedTenantIds.find(tId) == kUnsupportedTenantIds.end() &&
        tId.find('_') == std::string::npos;

    return isPrefixSupported
        ? Status::OK()
        : Status(ErrorCodes::BadValue,
                 str::stream() << "cannot migrate databases for tenant \'" << tId << "'");
}

inline Status validateDatabasePrefix(const std::vector<TenantId>& tenantsId) {
    std::set<TenantId> tenants;
    for (const auto& tenantId : tenantsId) {
        auto res = tenants.emplace(tenantId);
        uassert(ErrorCodes::InvalidOptions,
                str::stream() << "Duplicate tenantIds are not allowed. Duplicate tenantId : "
                              << tenantId,
                res.second);
        auto status = validateDatabasePrefix(tenantId);
        if (!status.isOK()) {
            return status;
        }
    }

    return Status::OK();
}

inline Status validateProtocolFCVCompatibility(
    const boost::optional<MigrationProtocolEnum>& protocol) {
    if (!protocol)
        return Status::OK();

    if (*protocol == MigrationProtocolEnum::kShardMerge &&
        !repl::feature_flags::gShardMerge.isEnabled(
            serverGlobalParams.featureCompatibility.acquireFCVSnapshot())) {
        return Status(ErrorCodes::IllegalOperation,
                      str::stream() << "protocol '" << MigrationProtocol_serializer(*protocol)
                                    << "' not supported");
    }
    return Status::OK();
}

inline Status validateTimestampNotNull(const Timestamp& ts) {
    return (!ts.isNull())
        ? Status::OK()
        : Status(ErrorCodes::BadValue, str::stream() << "Timestamp can't be null");
}

inline Status validateConnectionString(StringData donorOrRecipientConnectionString) {
    const auto donorOrRecipientUri =
        uassertStatusOK(MongoURI::parse(donorOrRecipientConnectionString.toString()));
    const auto donorOrRecipientServers = donorOrRecipientUri.getServers();

    // Sanity check to make sure that the given donor or recipient connection string corresponds
    // to a replica set connection string with at least one host.
    try {
        const auto donorOrRecipientRsConnectionString = ConnectionString::forReplicaSet(
            donorOrRecipientUri.getSetName(),
            std::vector<HostAndPort>(donorOrRecipientServers.begin(),
                                     donorOrRecipientServers.end()));
    } catch (const ExceptionFor<ErrorCodes::FailedToParse>& ex) {
        return Status(ErrorCodes::BadValue,
                      str::stream()
                          << "Donor and recipient must be a replica set with at least one host: "
                          << ex.toStatus());
    }

    // Sanity check to make sure that donor and recipient do not share any hosts.
    const auto servers = repl::ReplicationCoordinator::get(cc().getServiceContext())
                             ->getConfigConnectionString()
                             .getServers();

    for (auto&& server : servers) {
        bool foundMatch = std::any_of(
            donorOrRecipientServers.begin(),
            donorOrRecipientServers.end(),
            [&](const HostAndPort& donorOrRecipient) { return server == donorOrRecipient; });
        if (foundMatch) {
            return Status(ErrorCodes::BadValue,
                          str::stream() << "Donor and recipient hosts must be different.");
        }
    }
    return Status::OK();
}

inline void protocolTenantIdCompatibilityCheck(const MigrationProtocolEnum protocol,
                                               const boost::optional<StringData>& tenantId) {
    switch (protocol) {
        case MigrationProtocolEnum::kShardMerge: {
            uassert(ErrorCodes::InvalidOptions,
                    str::stream() << "'tenantId' must be empty for protocol '"
                                  << MigrationProtocol_serializer(protocol) << "'",
                    !tenantId);
            break;
        }
        case MigrationProtocolEnum::kMultitenantMigrations: {
            uassert(ErrorCodes::InvalidOptions,
                    str::stream() << "'tenantId' is required for protocol '"
                                  << MigrationProtocol_serializer(protocol) << "'",
                    tenantId);
            break;
        }
        default:
            MONGO_UNREACHABLE;
    }
}

inline void protocolTenantIdsCompatibilityCheck(
    const MigrationProtocolEnum protocol, const boost::optional<std::vector<TenantId>>& tenantIds) {
    switch (protocol) {
        case MigrationProtocolEnum::kShardMerge: {
            {
                uassert(ErrorCodes::InvalidOptions,
                        str::stream() << "'tenantIds' is required for protocol '"
                                      << MigrationProtocol_serializer(protocol) << "'",
                        tenantIds && !tenantIds->empty());
            }
            break;
        }
        case MigrationProtocolEnum::kMultitenantMigrations: {
            uassert(ErrorCodes::InvalidOptions,
                    str::stream() << "'tenantId' must be empty for protocol '"
                                  << MigrationProtocol_serializer(protocol) << "'",
                    !tenantIds);
            break;
        }
        default:
            MONGO_UNREACHABLE;
    }
}

inline void protocolStorageOptionsCompatibilityCheck(OperationContext* opCtx,
                                                     const MigrationProtocolEnum protocol) {
    if (protocol != MigrationProtocolEnum::kShardMerge)
        return;

    uassert(ErrorCodes::InvalidOptions,
            str::stream() << "protocol '" << MigrationProtocol_serializer(protocol)
                          << "' is not allowed when storage option 'directoryPerDb' is enabled",
            !storageGlobalParams.directoryperdb);
    uassert(
        ErrorCodes::InvalidOptions,
        str::stream() << "protocol '" << MigrationProtocol_serializer(protocol)
                      << "' is not allowed when storage option 'directoryForIndexes' is enabled",
        !opCtx->getServiceContext()->getStorageEngine()->isUsingDirectoryForIndexes());
}

inline void protocolReadPreferenceCompatibilityCheck(OperationContext* opCtx,
                                                     const MigrationProtocolEnum protocol,
                                                     const ReadPreferenceSetting& readPreference) {
    if (protocol != MigrationProtocolEnum::kShardMerge)
        return;

    uassert(ErrorCodes::FailedToSatisfyReadPreference,
            "Shard Merge protocol only supports primary read preference",
            !readPreference.canRunOnSecondary());
}

inline void protocolCheckRecipientForgetDecision(
    const MigrationProtocolEnum protocol, const boost::optional<MigrationDecisionEnum>& decision) {
    switch (protocol) {
        case MigrationProtocolEnum::kShardMerge:
            uassert(ErrorCodes::InvalidOptions,
                    str::stream() << "'decision' is required for protocol '"
                                  << MigrationProtocol_serializer(protocol) << "'",
                    decision.has_value());
            break;
        case MigrationProtocolEnum::kMultitenantMigrations:
            uassert(ErrorCodes::InvalidOptions,
                    str::stream() << "'decision' must be empty for protocol '"
                                  << MigrationProtocol_serializer(protocol) << "'",
                    !decision.has_value());
            break;
        default:
            MONGO_UNREACHABLE;
    }
}

/**
 * Sets the "ttlExpiresAt" field for the external keys so they can be garbage collected by the ttl
 * monitor.
 */
ExecutorFuture<void> markExternalKeysAsGarbageCollectable(
    ServiceContext* serviceContext,
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    std::shared_ptr<executor::TaskExecutor> parentExecutor,
    UUID migrationId,
    const CancellationToken& token);

/**
 * Creates a view on the oplog that allows a tenant migration recipient to fetch retryable writes
 * and transactions from a tenant migration donor.
 */
void createOplogViewForTenantMigrations(OperationContext* opCtx, Database* db);

/**
 * Creates a pipeline for fetching committed transactions on the donor before or at
 * 'startApplyingDonorOpTime'. We use 'tenantId' to fetch transaction entries specific to a
 * particular set of tenant databases.
 */
std::unique_ptr<Pipeline, PipelineDeleter> createCommittedTransactionsPipelineForTenantMigrations(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const Timestamp& startApplyingDonorOpTime,
    const std::string& tenantId);

/**
 * Creates a pipeline that can be serialized into a query for fetching retryable writes oplog
 * entries before `startFetchingTimestamp`. We use `tenantId` to fetch entries specific to a
 * particular set of tenant databases. This is for the multi-tenant migration protocol.
 */
std::unique_ptr<Pipeline, PipelineDeleter> createRetryableWritesOplogFetchingPipeline(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const Timestamp& startFetchingTimestamp,
    const std::string& tenantId);

/**
 * Creates a pipeline that can be serialized into a query for fetching retryable writes oplog
 * entries before `startFetchingTimestamp` for all tenants. This is for shard merge protocol.
 */
std::unique_ptr<Pipeline, PipelineDeleter> createRetryableWritesOplogFetchingPipelineForAllTenants(
    const boost::intrusive_ptr<ExpressionContext>& expCtx, const Timestamp& startFetchingTimestamp);

/**
 * Returns a new BSONObj created from 'stateDoc' with sensitive fields redacted.
 */
BSONObj redactStateDoc(BSONObj stateDoc);

}  // namespace tenant_migration_util

}  // namespace mongo
