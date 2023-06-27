/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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

#include <boost/smart_ptr.hpp>
#include <memory>
#include <string>
#include <tuple>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <fmt/format.h>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/cluster_role.h"
#include "mongo/db/commands.h"
#include "mongo/db/database_name.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/ops/write_ops_gen.h"
#include "mongo/db/ops/write_ops_parsers.h"
#include "mongo/db/resource_yielder.h"
#include "mongo/db/s/sharded_index_catalog_commands_gen.h"
#include "mongo/db/server_options.h"
#include "mongo/db/service_context.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/transaction/transaction_api.h"
#include "mongo/db/transaction/transaction_participant.h"
#include "mongo/db/transaction/transaction_participant_resource_yielder.h"
#include "mongo/executor/inline_executor.h"
#include "mongo/executor/task_executor.h"
#include "mongo/executor/task_executor_pool.h"
#include "mongo/rpc/op_msg.h"
#include "mongo/s/catalog/type_collection.h"
#include "mongo/s/catalog/type_collection_gen.h"
#include "mongo/s/catalog/type_index_catalog_gen.h"
#include "mongo/s/grid.h"
#include "mongo/s/sharding_feature_flags_gen.h"
#include "mongo/s/write_ops/batched_command_response.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/future.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/out_of_line_executor.h"
#include "mongo/util/str.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding


namespace mongo {
namespace {

/**
 * Insert an index in the local catalog and bumps the indexVersion in the collections collection
 * transactionally.
 */
void commitIndexInTransaction(OperationContext* opCtx,
                              std::shared_ptr<executor::TaskExecutor> executor,
                              const NamespaceString& userCollectionNss,
                              const std::string& name,
                              const BSONObj& keyPattern,
                              const BSONObj& options,
                              const UUID& collectionUUID,
                              const Timestamp& lastmod,
                              const boost::optional<UUID>& indexCollectionUUID) {
    IndexCatalogType indexCatalogEntry(name, keyPattern, options, lastmod, collectionUUID);
    indexCatalogEntry.setIndexCollectionUUID(indexCollectionUUID);

    // TODO SERVER-75189: remove the usage of shared_ptr once the executor is inlined, so the
    // variable will never be out of scope.
    auto upsertIndexOp = std::make_shared<write_ops::UpdateCommandRequest>(
        NamespaceString::kConfigsvrIndexCatalogNamespace);
    upsertIndexOp->setUpdates({[&] {
        write_ops::UpdateOpEntry entry;
        entry.setQ(BSON(IndexCatalogType::kCollectionUUIDFieldName
                        << collectionUUID << IndexCatalogType::kNameFieldName << name));
        entry.setU(
            write_ops::UpdateModification::parseFromClassicUpdate(indexCatalogEntry.toBSON()));
        entry.setUpsert(true);
        entry.setMulti(false);
        return entry;
    }()});

    auto updateCollectionOp =
        std::make_shared<write_ops::UpdateCommandRequest>(CollectionType::ConfigNS);
    updateCollectionOp->setUpdates({[&] {
        write_ops::UpdateOpEntry entry;
        entry.setQ(BSON(CollectionType::kNssFieldName
                        << NamespaceStringUtil::serialize(userCollectionNss)
                        << CollectionType::kUuidFieldName << collectionUUID));
        entry.setU(write_ops::UpdateModification::parseFromClassicUpdate(
            BSON("$set" << BSON(CollectionType::kUuidFieldName
                                << collectionUUID << CollectionType::kIndexVersionFieldName
                                << lastmod))));
        entry.setUpsert(true);
        entry.setMulti(false);
        return entry;
    }()});

    auto inlineExecutor = std::make_shared<executor::InlineExecutor>();
    txn_api::SyncTransactionWithRetries txn(
        opCtx,
        executor,
        TransactionParticipantResourceYielder::make("commitIndexCatalogEntry"),
        inlineExecutor);

    txn.run(opCtx,
            [updateCollectionOp, upsertIndexOp](const txn_api::TransactionClient& txnClient,
                                                ExecutorPtr txnExec) {
                return txnClient.runCRUDOp(*upsertIndexOp, {0})
                    .thenRunOn(txnExec)
                    .then([&txnClient, updateCollectionOp](auto upsertResponse) {
                        uassertStatusOK(upsertResponse.toStatus());
                        return txnClient.runCRUDOp(*updateCollectionOp, {1});
                    })
                    .thenRunOn(txnExec)
                    .then([](auto updateResponse) { uassertStatusOK(updateResponse.toStatus()); })
                    .semi();
            });
}


class ConfigsvrCommitIndexCommand final : public TypedCommand<ConfigsvrCommitIndexCommand> {
public:
    using Request = ConfigsvrCommitIndex;

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    std::string help() const override {
        return "Internal command. Do not call directly. Commits a globlal index in the sharding "
               "catalog.";
    }

    bool adminOnly() const override {
        return false;
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool supportsRetryableWrite() const final {
        return true;
    }

    class Invocation final : public InvocationBase {
    public:
        using InvocationBase::InvocationBase;

        void typedRun(OperationContext* opCtx) {
            uassert(ErrorCodes::CommandNotSupported,
                    format(FMT_STRING("{} command not enabled"), definition()->getName()),
                    feature_flags::gGlobalIndexesShardingCatalog.isEnabled(
                        serverGlobalParams.featureCompatibility));
            uassert(
                ErrorCodes::IllegalOperation,
                format(FMT_STRING("{} can only be run on config servers"), definition()->getName()),
                serverGlobalParams.clusterRole.has(ClusterRole::ConfigServer));

            CommandHelpers::uassertCommandRunWithMajority(Request::kCommandName,
                                                          opCtx->getWriteConcern());

            const auto txnParticipant = TransactionParticipant::get(opCtx);
            uassert(6711908,
                    str::stream() << Request::kCommandName << " must be run as a retryable write",
                    txnParticipant);

            opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();

            commitIndexInTransaction(opCtx,
                                     Grid::get(opCtx)->getExecutorPool()->getFixedExecutor(),
                                     ns(),
                                     request().getName().toString(),
                                     request().getKeyPattern(),
                                     request().getOptions(),
                                     request().getCollectionUUID(),
                                     request().getLastmod(),
                                     request().getIndexCollectionUUID());

            // Since no write that generated a retryable write oplog entry with this sessionId
            // and txnNumber happened, we need to make a dummy write so that the session gets
            // durably persisted on the oplog. This must be the last operation done on this
            // command.
            DBDirectClient client(opCtx);
            client.update(NamespaceString::kServerConfigurationNamespace,
                          BSON("_id" << Request::kCommandName),
                          BSON("$inc" << BSON("count" << 1)),
                          true /* upsert */,
                          false /* multi */);
        }

    private:
        NamespaceString ns() const override {
            return request().getCommandParameter();
        }

        bool supportsWriteConcern() const override {
            return true;
        }

        void doCheckAuthorization(OperationContext* opCtx) const override {
            uassert(ErrorCodes::Unauthorized,
                    "Unauthorized",
                    AuthorizationSession::get(opCtx->getClient())
                        ->isAuthorizedForActionsOnResource(
                            ResourcePattern::forClusterResource(request().getDbName().tenantId()),
                            ActionType::internal));
        }
    };

} configsvrCommitIndexCommand;

}  // namespace
}  // namespace mongo
