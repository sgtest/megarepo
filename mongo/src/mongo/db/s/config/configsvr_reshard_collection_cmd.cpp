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


#include "mongo/platform/basic.h"

#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/commands.h"
#include "mongo/db/commands/feature_compatibility_version.h"
#include "mongo/db/query/collation/collator_factory_interface.h"
#include "mongo/db/repl/primary_only_service.h"
#include "mongo/db/repl/repl_client_info.h"
#include "mongo/db/s/config/sharding_catalog_manager.h"
#include "mongo/db/s/metrics/sharding_data_transform_metrics.h"
#include "mongo/db/s/resharding/coordinator_document_gen.h"
#include "mongo/db/s/resharding/resharding_coordinator_service.h"
#include "mongo/db/s/resharding/resharding_server_parameters_gen.h"
#include "mongo/db/s/resharding/resharding_util.h"
#include "mongo/db/vector_clock.h"
#include "mongo/logv2/log.h"
#include "mongo/s/catalog/type_tags.h"
#include "mongo/s/grid.h"
#include "mongo/s/request_types/reshard_collection_gen.h"
#include "mongo/s/resharding/resharding_coordinator_service_conflicting_op_in_progress_info.h"
#include "mongo/s/resharding/resharding_feature_flag_gen.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kCommand

namespace mongo {

MONGO_FAIL_POINT_DEFINE(reshardCollectionJoinedExistingOperation);

namespace {

class ConfigsvrReshardCollectionCommand final
    : public TypedCommand<ConfigsvrReshardCollectionCommand> {
public:
    using Request = ConfigsvrReshardCollection;

    class Invocation final : public InvocationBase {
    public:
        using InvocationBase::InvocationBase;

        void typedRun(OperationContext* opCtx) {
            uassert(ErrorCodes::IllegalOperation,
                    "_configsvrReshardCollection can only be run on config servers",
                    serverGlobalParams.clusterRole.has(ClusterRole::ConfigServer));
            CommandHelpers::uassertCommandRunWithMajority(Request::kCommandName,
                                                          opCtx->getWriteConcern());
            repl::ReadConcernArgs::get(opCtx) =
                repl::ReadConcernArgs(repl::ReadConcernLevel::kLocalReadConcern);

            const NamespaceString& nss = ns();

            const auto catalogClient = ShardingCatalogManager::get(opCtx)->localCatalogClient();
            try {
                const auto collEntry = catalogClient->getCollection(opCtx, nss);
                uassert(ErrorCodes::NotImplemented,
                        "reshardCollection command of a sharded time-series collection is not "
                        "supported",
                        !collEntry.getTimeseriesFields());
            } catch (const ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
                // collection doesn't exist or not sharded, skip check for time-series collection.
            }

            uassert(ErrorCodes::BadValue,
                    "The unique field must be false",
                    !request().getUnique().get_value_or(false));

            if (request().getCollation()) {
                auto& collation = request().getCollation().value();
                auto collator =
                    uassertStatusOK(CollatorFactoryInterface::get(opCtx->getServiceContext())
                                        ->makeFromBSON(collation));
                uassert(ErrorCodes::BadValue,
                        str::stream()
                            << "The collation for reshardCollection must be {locale: 'simple'}, "
                            << "but found: " << collation,
                        !collator);
            }

            const auto& authoritativeTags =
                uassertStatusOK(catalogClient->getTagsForCollection(opCtx, nss));
            if (!authoritativeTags.empty() && !request().getForceRedistribution()) {
                uassert(ErrorCodes::BadValue,
                        "Must specify value for zones field",
                        request().getZones());
            }

            if (const auto& presetChunks = request().get_presetReshardedChunks()) {
                uassert(ErrorCodes::BadValue,
                        "Test commands must be enabled when a value is provided for field: "
                        "_presetReshardedChunks",
                        getTestCommandsEnabled());

                uassert(ErrorCodes::BadValue,
                        "Must specify only one of _presetReshardedChunks or numInitialChunks",
                        !(bool(request().getNumInitialChunks())));

                resharding::validateReshardedChunks(
                    *presetChunks, opCtx, ShardKeyPattern(request().getKey()).getKeyPattern());
            }

            if (!resharding::gFeatureFlagReshardingImprovements.isEnabled(
                    serverGlobalParams.featureCompatibility)) {
                uassert(
                    ErrorCodes::InvalidOptions,
                    "Resharding improvements is not enabled, reject shardDistribution parameter",
                    !request().getShardDistribution().has_value());
                uassert(
                    ErrorCodes::InvalidOptions,
                    "Resharding improvements is not enabled, reject forceRedistribution parameter",
                    !request().getForceRedistribution().has_value());
                uassert(ErrorCodes::InvalidOptions,
                        "Resharding improvements is not enabled, reject reshardingUUID parameter",
                        !request().getReshardingUUID().has_value());
            }

            if (const auto& shardDistribution = request().getShardDistribution()) {
                resharding::validateShardDistribution(
                    *shardDistribution, opCtx, ShardKeyPattern(request().getKey()));
            }

            // Returns boost::none if there isn't any work to be done by the resharding operation.
            auto instance =
                ([&]() -> boost::optional<std::shared_ptr<const ReshardingCoordinator>> {
                    FixedFCVRegion fixedFcv(opCtx);

                    uassert(ErrorCodes::CommandNotSupported,
                            "reshardCollection command not enabled",
                            resharding::gFeatureFlagResharding.isEnabled(
                                serverGlobalParams.featureCompatibility));

                    // (Generic FCV reference): To run this command and ensure the consistency of
                    // the metadata we need to make sure we are on a stable state.
                    uassert(
                        ErrorCodes::CommandNotSupported,
                        "Resharding is not supported for this version, please update the FCV to "
                        "latest.",
                        !serverGlobalParams.featureCompatibility.isUpgradingOrDowngrading());

                    const auto [cm, _] = uassertStatusOK(
                        Grid::get(opCtx)
                            ->catalogCache()
                            ->getShardedCollectionRoutingInfoWithPlacementRefresh(opCtx, nss));

                    auto tempReshardingNss =
                        resharding::constructTemporaryReshardingNss(nss.db(), cm.getUUID());


                    if (auto zones = request().getZones()) {
                        resharding::checkForOverlappingZones(*zones);
                    }

                    auto coordinatorDoc =
                        ReshardingCoordinatorDocument(std::move(CoordinatorStateEnum::kUnused),
                                                      {},   // donorShards
                                                      {});  // recipientShards

                    // Generate the resharding metadata for the ReshardingCoordinatorDocument.
                    auto reshardingUUID = UUID::gen();
                    auto existingUUID = cm.getUUID();
                    auto commonMetadata = CommonReshardingMetadata(std::move(reshardingUUID),
                                                                   ns(),
                                                                   std::move(existingUUID),
                                                                   std::move(tempReshardingNss),
                                                                   request().getKey());
                    commonMetadata.setStartTime(
                        opCtx->getServiceContext()->getFastClockSource()->now());
                    if (request().getReshardingUUID()) {
                        commonMetadata.setUserReshardingUUID(*request().getReshardingUUID());
                    }

                    coordinatorDoc.setCommonReshardingMetadata(std::move(commonMetadata));
                    coordinatorDoc.setZones(request().getZones());
                    coordinatorDoc.setPresetReshardedChunks(request().get_presetReshardedChunks());
                    coordinatorDoc.setNumInitialChunks(request().getNumInitialChunks());
                    coordinatorDoc.setShardDistribution(request().getShardDistribution());
                    coordinatorDoc.setForceRedistribution(request().getForceRedistribution());

                    opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();
                    auto instance = getOrCreateReshardingCoordinator(opCtx, coordinatorDoc);
                    instance->getCoordinatorDocWrittenFuture().get(opCtx);
                    return instance;
                })();

            if (instance) {
                // There is work to be done in order to have the collection's shard key match the
                // requested shard key. Wait until the work is complete.
                instance.value()->getCompletionFuture().get(opCtx);
            }
            repl::ReplClientInfo::forClient(opCtx->getClient()).setLastOpToSystemLastOpTime(opCtx);
        }

        /**
         * Helper function to create a new instance or join the existing resharding operation to
         * prevent generating a new resharding instance if the same command is issued consecutively
         * due to client disconnect etc.
         */
        std::shared_ptr<const ReshardingCoordinator> getOrCreateReshardingCoordinator(
            OperationContext* opCtx, const ReshardingCoordinatorDocument& coordinatorDoc);

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

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    std::string help() const override {
        return "Internal command, which is exported by the sharding config server. Do not call "
               "directly. Reshards a collection on a new shard key.";
    }

    bool adminOnly() const override {
        return true;
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

} configsvrReshardCollectionCmd;

std::shared_ptr<const ReshardingCoordinator>
ConfigsvrReshardCollectionCommand::Invocation::getOrCreateReshardingCoordinator(
    OperationContext* opCtx, const ReshardingCoordinatorDocument& coordinatorDoc) {
    try {
        auto registry = repl::PrimaryOnlyServiceRegistry::get(opCtx->getServiceContext());
        auto service = registry->lookupServiceByName(ReshardingCoordinatorService::kServiceName);
        auto instance = ReshardingCoordinator::getOrCreate(opCtx, service, coordinatorDoc.toBSON());

        return std::shared_ptr<const ReshardingCoordinator>(instance);
    } catch (
        const ExceptionFor<ErrorCodes::ReshardingCoordinatorServiceConflictingOperationInProgress>&
            ex) {
        reshardCollectionJoinedExistingOperation.pauseWhileSet(opCtx);
        return checked_pointer_cast<const ReshardingCoordinator>(
            ex.extraInfo<ReshardingCoordinatorServiceConflictingOperationInProgressInfo>()
                ->getInstance());

    } catch (const ExceptionFor<ErrorCodes::ConflictingOperationInProgress>& ex) {
        uasserted(ErrorCodes::ReshardCollectionInProgress, ex.toStatus().reason());
    }
}

}  // namespace
}  // namespace mongo
