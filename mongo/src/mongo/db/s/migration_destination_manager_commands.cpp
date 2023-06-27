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

#include <string>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/commands.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/s/active_migrations_registry.h"
#include "mongo/db/s/chunk_move_write_concern_options.h"
#include "mongo/db/s/collection_metadata.h"
#include "mongo/db/s/collection_sharding_runtime.h"
#include "mongo/db/s/migration_destination_manager.h"
#include "mongo/db/s/migration_session_id.h"
#include "mongo/db/s/shard_filtering_metadata_refresh.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/s/start_chunk_clone_request.h"
#include "mongo/db/service_context.h"
#include "mongo/db/shard_id.h"
#include "mongo/db/tenant_id.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/index_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/s/shard_version_factory.h"
#include "mongo/s/stale_exception.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/database_name_util.h"
#include "mongo/util/future.h"
#include "mongo/util/namespace_string_util.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {
namespace {

// This shard version is used as the received version in StaleConfigInfo since we do not have
// information about the received version of the operation.
ShardVersion ShardVersionPlacementIgnoredNoIndexes() {
    return ShardVersionFactory::make(ChunkVersion::IGNORED(),
                                     boost::optional<CollectionIndexes>(boost::none));
}

class RecvChunkStartCommand : public ErrmsgCommandDeprecated {
public:
    RecvChunkStartCommand() : ErrmsgCommandDeprecated("_recvChunkStart") {}

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    std::string help() const override {
        return "internal";
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool adminOnly() const override {
        return true;
    }

    bool supportsWriteConcern(const BSONObj& cmd) const override {
        // This is required to be true to support moveChunk.
        return true;
    }

    NamespaceString parseNs(const DatabaseName& dbName, const BSONObj& cmdObj) const override {
        return NamespaceStringUtil::parseNamespaceFromRequest(
            dbName.tenantId(), CommandHelpers::parseNsFullyQualified(cmdObj));
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const override {
        auto* as = AuthorizationSession::get(opCtx->getClient());
        if (!as->isAuthorizedForActionsOnResource(
                ResourcePattern::forClusterResource(dbName.tenantId()), ActionType::internal)) {
            return {ErrorCodes::Unauthorized, "unauthorized"};
        }

        return Status::OK();
    }

    bool supportsRetryableWrite() const final {
        return true;
    }

    bool shouldCheckoutSession() const final {
        return false;
    }

    bool errmsgRun(OperationContext* opCtx,
                   const std::string& dbname,
                   const BSONObj& cmdObj,
                   std::string& errmsg,
                   BSONObjBuilder& result) override {
        opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();
        uassertStatusOK(ShardingState::get(opCtx)->canAcceptShardedCommands());

        auto nss = parseNs(DatabaseNameUtil::deserialize(boost::none, dbname), cmdObj);

        auto cloneRequest = uassertStatusOK(StartChunkCloneRequest::createFromCommand(nss, cmdObj));

        const auto chunkRange = uassertStatusOK(ChunkRange::fromBSON(cmdObj));

        const auto writeConcern =
            uassertStatusOK(ChunkMoveWriteConcernOptions::getEffectiveWriteConcern(
                opCtx, cloneRequest.getSecondaryThrottle()));

        // Ensure this shard is not currently receiving or donating any chunks.
        auto scopedReceiveChunk(
            uassertStatusOK(ActiveMigrationsRegistry::get(opCtx).registerReceiveChunk(
                opCtx,
                nss,
                chunkRange,
                cloneRequest.getFromShardId(),
                false /* waitForCompletionOfConflictingOps*/)));

        // We force a refresh immediately after registering this migration to guarantee that this
        // shard will not receive a chunk after refreshing.
        onCollectionPlacementVersionMismatch(opCtx, nss, boost::none);
        const auto shardId = ShardingState::get(opCtx)->shardId();

        const auto collectionEpoch = [&] {
            AutoGetCollection autoColl(opCtx, nss, MODE_IS);
            const auto scopedCsr =
                CollectionShardingRuntime::assertCollectionLockedAndAcquireShared(opCtx, nss);
            auto optMetadata = scopedCsr->getCurrentMetadataIfKnown();
            uassert(StaleConfigInfo(nss,
                                    ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                                    boost::none /* wantedVersion */,
                                    shardId,
                                    boost::none),
                    "The collection's sharding state was cleared by a concurrent operation",
                    optMetadata);
            return optMetadata->getShardPlacementVersion().epoch();
        }();

        uassertStatusOK(
            MigrationDestinationManager::get(opCtx)->start(opCtx,
                                                           nss,
                                                           std::move(scopedReceiveChunk),
                                                           cloneRequest,
                                                           collectionEpoch,
                                                           writeConcern));

        result.appendBool("started", true);
        return true;
    }

} recvChunkStartCmd;

class RecvChunkStatusCommand : public BasicCommand {
public:
    RecvChunkStatusCommand() : BasicCommand("_recvChunkStatus") {}

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    std::string help() const override {
        return "internal";
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool adminOnly() const override {
        return true;
    }

    bool supportsWriteConcern(const BSONObj& cmd) const override {
        return false;
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const override {
        auto* as = AuthorizationSession::get(opCtx->getClient());
        if (!as->isAuthorizedForActionsOnResource(
                ResourcePattern::forClusterResource(dbName.tenantId()), ActionType::internal)) {
            return {ErrorCodes::Unauthorized, "unauthorized"};
        }

        return Status::OK();
    }

    bool run(OperationContext* opCtx,
             const DatabaseName&,
             const BSONObj& cmdObj,
             BSONObjBuilder& result) override {
        bool waitForSteadyOrDone = cmdObj["waitForSteadyOrDone"].boolean();
        MigrationDestinationManager::get(opCtx)->report(result, opCtx, waitForSteadyOrDone);
        return true;
    }

} recvChunkStatusCommand;

class RecvChunkCommitCommand : public BasicCommand {
public:
    RecvChunkCommitCommand() : BasicCommand("_recvChunkCommit") {}

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    std::string help() const override {
        return "internal";
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool adminOnly() const override {
        return true;
    }


    bool supportsWriteConcern(const BSONObj& cmd) const override {
        return false;
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const override {
        auto* as = AuthorizationSession::get(opCtx->getClient());
        if (!as->isAuthorizedForActionsOnResource(
                ResourcePattern::forClusterResource(dbName.tenantId()), ActionType::internal)) {
            return {ErrorCodes::Unauthorized, "unauthorized"};
        }

        return Status::OK();
    }

    bool run(OperationContext* opCtx,
             const DatabaseName& dbName,
             const BSONObj& cmdObj,
             BSONObjBuilder& result) override {
        auto const sessionId = uassertStatusOK(MigrationSessionId::extractFromBSON(cmdObj));
        auto const mdm = MigrationDestinationManager::get(opCtx);

        Status const status = mdm->startCommit(sessionId);
        mdm->report(result, opCtx, false);
        if (!status.isOK()) {
            LOGV2(22014,
                  "_recvChunkCommit failed: {error}",
                  "_recvChunkCommit failed",
                  "error"_attr = redact(status));
            uassertStatusOK(status);
        }
        return true;
    }

} recvChunkCommitCommand;

class RecvChunkAbortCommand : public BasicCommand {
public:
    RecvChunkAbortCommand() : BasicCommand("_recvChunkAbort") {}

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    std::string help() const override {
        return "internal";
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool adminOnly() const override {
        return true;
    }

    bool supportsWriteConcern(const BSONObj& cmd) const override {
        return false;
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const override {
        auto* as = AuthorizationSession::get(opCtx->getClient());
        if (!as->isAuthorizedForActionsOnResource(
                ResourcePattern::forClusterResource(dbName.tenantId()), ActionType::internal)) {
            return {ErrorCodes::Unauthorized, "unauthorized"};
        }

        return Status::OK();
    }

    bool run(OperationContext* opCtx,
             const DatabaseName&,
             const BSONObj& cmdObj,
             BSONObjBuilder& result) override {
        auto const mdm = MigrationDestinationManager::get(opCtx);

        auto migrationSessionIdStatus(MigrationSessionId::extractFromBSON(cmdObj));

        if (migrationSessionIdStatus.isOK()) {
            Status const status = mdm->abort(migrationSessionIdStatus.getValue());
            mdm->report(result, opCtx, false);
            if (!status.isOK()) {
                LOGV2(22015,
                      "_recvChunkAbort failed: {error}",
                      "_recvChunkAbort failed",
                      "error"_attr = redact(status));
                uassertStatusOK(status);
            }
        } else if (migrationSessionIdStatus == ErrorCodes::NoSuchKey) {
            mdm->abortWithoutSessionIdCheck();
            mdm->report(result, opCtx, false);
        }

        uassertStatusOK(migrationSessionIdStatus.getStatus());
        return true;
    }

} recvChunkAbortCommand;

class RecvChunkReleaseCritSecCommand : public BasicCommand {
public:
    RecvChunkReleaseCritSecCommand() : BasicCommand("_recvChunkReleaseCritSec") {}

    std::string help() const override {
        return "internal";
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool adminOnly() const override {
        return true;
    }


    bool supportsWriteConcern(const BSONObj& cmd) const override {
        return true;
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const override {
        auto* as = AuthorizationSession::get(opCtx->getClient());
        if (!as->isAuthorizedForActionsOnResource(
                ResourcePattern::forClusterResource(dbName.tenantId()), ActionType::internal)) {
            return {ErrorCodes::Unauthorized, "unauthorized"};
        }

        return Status::OK();
    }

    bool run(OperationContext* opCtx,
             const DatabaseName&,
             const BSONObj& cmdObj,
             BSONObjBuilder& result) override {
        opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();

        CommandHelpers::uassertCommandRunWithMajority(getName(), opCtx->getWriteConcern());
        const auto sessionId = uassertStatusOK(MigrationSessionId::extractFromBSON(cmdObj));

        LOGV2_DEBUG(5899101, 2, "Received _recvChunkReleaseCritSec", "sessionId"_attr = sessionId);

        const auto mdm = MigrationDestinationManager::get(opCtx);
        const auto status = mdm->exitCriticalSection(opCtx, sessionId);
        if (!status.isOK()) {
            LOGV2(5899109,
                  "_recvChunkReleaseCritSec failed: {error}",
                  "_recvChunkReleaseCritSec failed",
                  "error"_attr = redact(status));
            uassertStatusOK(status);
        }
        return true;
    }

} recvChunkReleaseCritSecCommand;

}  // namespace
}  // namespace mongo
