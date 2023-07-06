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


#include "mongo/db/s/collmod_coordinator.h"

#include <absl/container/node_hash_map.h>
#include <algorithm>
#include <boost/smart_ptr.hpp>
#include <set>
#include <string>
#include <tuple>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/bson/simple_bsonobj_comparator.h"
#include "mongo/client/connection_string.h"
#include "mongo/client/read_preference.h"
#include "mongo/db/catalog/coll_mod.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/coll_mod_gen.h"
#include "mongo/db/commands.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/s/forwardable_operation_metadata.h"
#include "mongo/db/s/participant_block_gen.h"
#include "mongo/db/s/sharded_collmod_gen.h"
#include "mongo/db/s/sharding_ddl_coordinator_gen.h"
#include "mongo/db/s/sharding_ddl_util.h"
#include "mongo/db/session/logical_session_id_gen.h"
#include "mongo/db/timeseries/catalog_helper.h"
#include "mongo/db/timeseries/timeseries_collmod.h"
#include "mongo/db/timeseries/timeseries_options.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/executor/async_rpc.h"
#include "mongo/executor/async_rpc_util.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/s/async_requests_sender.h"
#include "mongo/s/catalog/sharding_catalog_client.h"
#include "mongo/s/catalog/type_collection.h"
#include "mongo/s/catalog/type_database_gen.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/client/shard.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/cluster_commands_helpers.h"
#include "mongo/s/grid.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future_impl.h"
#include "mongo/util/read_through_cache.h"
#include "mongo/util/str.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding


namespace mongo {

MONGO_FAIL_POINT_DEFINE(collModBeforeConfigServerUpdate);

namespace {

bool isShardedColl(OperationContext* opCtx, const NamespaceString& nss) {
    try {
        auto coll = Grid::get(opCtx)->catalogClient()->getCollection(opCtx, nss);
        return true;
    } catch (const ExceptionFor<ErrorCodes::NamespaceNotFound>&) {
        // The collection is not sharded or doesn't exist.
        return false;
    }
}

bool hasTimeSeriesBucketingUpdate(const CollModRequest& request) {
    if (!request.getTimeseries().has_value()) {
        return false;
    }
    auto& ts = request.getTimeseries();
    return ts->getGranularity() || ts->getBucketMaxSpanSeconds() || ts->getBucketRoundingSeconds();
}

template <typename CommandType>
std::vector<AsyncRequestsSender::Response> sendAuthenticatedCommandWithOsiToShards(
    OperationContext* opCtx,
    std::shared_ptr<async_rpc::AsyncRPCOptions<CommandType>> opts,
    const std::vector<ShardId>& shardIds,
    const OperationSessionInfo& osi,
    WriteConcernOptions wc = WriteConcernOptions()) {
    async_rpc::AsyncRPCCommandHelpers::appendMajorityWriteConcern(opts->genericArgs, wc);
    async_rpc::AsyncRPCCommandHelpers::appendOSI(opts->genericArgs, osi);
    return sharding_ddl_util::sendAuthenticatedCommandToShards(opCtx, opts, shardIds);
}

}  // namespace

CollModCoordinator::CollModCoordinator(ShardingDDLCoordinatorService* service,
                                       const BSONObj& initialState)
    : RecoverableShardingDDLCoordinator(service, "CollModCoordinator", initialState),
      _request{_doc.getCollModRequest()} {}

void CollModCoordinator::checkIfOptionsConflict(const BSONObj& doc) const {
    const auto otherDoc =
        CollModCoordinatorDocument::parse(IDLParserContext("CollModCoordinatorDocument"), doc);

    const auto& selfReq = _request.toBSON();
    const auto& otherReq = otherDoc.getCollModRequest().toBSON();

    uassert(ErrorCodes::ConflictingOperationInProgress,
            str::stream() << "Another collMod for namespace " << originalNss().toStringForErrorMsg()
                          << " is being executed with different parameters: " << selfReq,
            SimpleBSONObjComparator::kInstance.evaluate(selfReq == otherReq));
}

void CollModCoordinator::appendCommandInfo(BSONObjBuilder* cmdInfoBuilder) const {
    cmdInfoBuilder->appendElements(_request.toBSON());
};

void CollModCoordinator::_performNoopRetryableWriteOnParticipants(
    OperationContext* opCtx, const std::shared_ptr<executor::TaskExecutor>& executor) {
    auto shardsAndConfigsvr = [&] {
        const auto shardRegistry = Grid::get(opCtx)->shardRegistry();
        auto participants = shardRegistry->getAllShardIds(opCtx);
        if (std::find(participants.begin(), participants.end(), ShardId::kConfigServerId) ==
            participants.end()) {
            // The config server may be a shard, so only add if it isn't already in participants.
            participants.emplace_back(shardRegistry->getConfigShard()->getId());
        }
        return participants;
    }();

    sharding_ddl_util::performNoopRetryableWriteOnShards(
        opCtx, shardsAndConfigsvr, getNewSession(opCtx), executor);
}

void CollModCoordinator::_saveCollectionInfoOnCoordinatorIfNecessary(OperationContext* opCtx) {
    if (!_collInfo) {
        CollectionInfo info;
        info.timeSeriesOptions = timeseries::getTimeseriesOptions(opCtx, originalNss(), true);
        info.nsForTargeting =
            info.timeSeriesOptions ? originalNss().makeTimeseriesBucketsNamespace() : originalNss();
        info.isSharded = isShardedColl(opCtx, info.nsForTargeting);
        _collInfo = std::move(info);
    }
}

void CollModCoordinator::_saveShardingInfoOnCoordinatorIfNecessary(OperationContext* opCtx) {
    tassert(
        6522700, "Sharding information must be gathered after collection information", _collInfo);
    if (!_shardingInfo && _collInfo->isSharded) {
        ShardingInfo info;
        const auto [chunkManager, _] = uassertStatusOK(
            Grid::get(opCtx)->catalogCache()->getCollectionRoutingInfoWithPlacementRefresh(
                opCtx, _collInfo->nsForTargeting));

        info.primaryShard = chunkManager.dbPrimary();
        std::set<ShardId> shardIdsSet;
        chunkManager.getAllShardIds(&shardIdsSet);
        std::vector<ShardId> shardIdsVec{shardIdsSet.begin(), shardIdsSet.end()};
        info.shardsOwningChunks = std::move(shardIdsVec);
        _shardingInfo = std::move(info);
    }
}

ExecutorFuture<void> CollModCoordinator::_runImpl(
    std::shared_ptr<executor::ScopedTaskExecutor> executor,
    const CancellationToken& token) noexcept {
    return ExecutorFuture<void>(**executor)
        .then([this, executor = executor, anchor = shared_from_this()] {
            auto opCtxHolder = cc().makeOperationContext();
            auto* opCtx = opCtxHolder.get();
            getForwardableOpMetadata().setOn(opCtx);

            if (_doc.getPhase() > Phase::kUnset) {
                _performNoopRetryableWriteOnParticipants(opCtx, **executor);
            }

            {
                AutoGetCollection coll{opCtx,
                                       nss(),
                                       MODE_IS,
                                       AutoGetCollection::Options{}
                                           .viewMode(auto_get_collection::ViewMode::kViewsPermitted)
                                           .expectedUUID(_request.getCollectionUUID())};
            }

            _saveCollectionInfoOnCoordinatorIfNecessary(opCtx);

            auto isGranularityUpdate = hasTimeSeriesBucketingUpdate(_request);
            uassert(6201808,
                    "Cannot use time-series options for a non-timeseries collection",
                    _collInfo->timeSeriesOptions || !isGranularityUpdate);
            if (isGranularityUpdate) {
                uassertStatusOK(timeseries::isTimeseriesGranularityValidAndUnchanged(
                    _collInfo->timeSeriesOptions.get(), _request.getTimeseries().get()));
            }
        })
        .then([this, executor = executor, anchor = shared_from_this()] {
            _buildPhaseHandler(
                Phase::kFreezeMigrations, [this, executor = executor, anchor = shared_from_this()] {
                    auto opCtxHolder = cc().makeOperationContext();
                    auto* opCtx = opCtxHolder.get();
                    getForwardableOpMetadata().setOn(opCtx);

                    _saveCollectionInfoOnCoordinatorIfNecessary(opCtx);

                    if (_collInfo->isSharded) {
                        const auto& collUUID =
                            sharding_ddl_util::getCollectionUUID(opCtx, _collInfo->nsForTargeting);
                        _doc.setCollUUID(collUUID);
                        sharding_ddl_util::stopMigrations(
                            opCtx, _collInfo->nsForTargeting, collUUID, getNewSession(opCtx));
                    }
                })();
        })
        .then(_buildPhaseHandler(
            Phase::kBlockShards,
            [this, token, executor = executor, anchor = shared_from_this()] {
                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                _saveCollectionInfoOnCoordinatorIfNecessary(opCtx);
                _saveShardingInfoOnCoordinatorIfNecessary(opCtx);

                if (_collInfo->isSharded && hasTimeSeriesBucketingUpdate(_request)) {
                    ShardsvrParticipantBlock blockCRUDOperationsRequest(_collInfo->nsForTargeting);
                    blockCRUDOperationsRequest.setBlockType(
                        CriticalSectionBlockTypeEnum::kReadsAndWrites);
                    auto opts =
                        std::make_shared<async_rpc::AsyncRPCOptions<ShardsvrParticipantBlock>>(
                            blockCRUDOperationsRequest,
                            **executor,
                            token,
                            async_rpc::GenericArgs());
                    sendAuthenticatedCommandWithOsiToShards(
                        opCtx, opts, _shardingInfo->shardsOwningChunks, getNewSession(opCtx));
                }
            }))
        .then(_buildPhaseHandler(
            Phase::kUpdateConfig,
            [this, executor = executor, anchor = shared_from_this()] {
                collModBeforeConfigServerUpdate.pauseWhileSet();

                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                _saveCollectionInfoOnCoordinatorIfNecessary(opCtx);
                _saveShardingInfoOnCoordinatorIfNecessary(opCtx);

                if (_collInfo->isSharded && _collInfo->timeSeriesOptions &&
                    hasTimeSeriesBucketingUpdate(_request)) {
                    ConfigsvrCollMod request(_collInfo->nsForTargeting, _request);
                    const auto cmdObj =
                        CommandHelpers::appendMajorityWriteConcern(request.toBSON({}));

                    const auto& configShard = Grid::get(opCtx)->shardRegistry()->getConfigShard();
                    uassertStatusOK(Shard::CommandResponse::getEffectiveStatus(
                        configShard->runCommand(opCtx,
                                                ReadPreferenceSetting(ReadPreference::PrimaryOnly),
                                                nss().db().toString(),
                                                cmdObj,
                                                Shard::RetryPolicy::kIdempotent)));
                }
            }))
        .then(_buildPhaseHandler(
            Phase::kUpdateShards, [this, token, executor = executor, anchor = shared_from_this()] {
                auto opCtxHolder = cc().makeOperationContext();
                auto* opCtx = opCtxHolder.get();
                getForwardableOpMetadata().setOn(opCtx);

                _saveCollectionInfoOnCoordinatorIfNecessary(opCtx);
                _saveShardingInfoOnCoordinatorIfNecessary(opCtx);

                if (_collInfo->isSharded) {
                    try {
                        if (!_firstExecution) {
                            bool allowMigrations = sharding_ddl_util::checkAllowMigrations(
                                opCtx, _collInfo->nsForTargeting);
                            if (_result.is_initialized() && allowMigrations) {
                                // The command finished and we have the response. Return it.
                                return;
                            } else if (allowMigrations) {
                                // Previous run on a different node completed, but we lost the
                                // result in the stepdown. Restart from kFreezeMigrations.
                                _enterPhase(Phase::kFreezeMigrations);
                                uasserted(ErrorCodes::Interrupted,
                                          "Retriable error to move to previous stage");
                            }
                        }

                        ShardsvrCollModParticipant request(originalNss(), _request);
                        bool needsUnblock =
                            _collInfo->timeSeriesOptions && hasTimeSeriesBucketingUpdate(_request);
                        request.setNeedsUnblock(needsUnblock);

                        std::vector<AsyncRequestsSender::Response> responses;
                        auto shardsOwningChunks = _shardingInfo->shardsOwningChunks;
                        auto primaryShardOwningChunk = std::find(shardsOwningChunks.begin(),
                                                                 shardsOwningChunks.end(),
                                                                 _shardingInfo->primaryShard);

                        // If trying to convert an index to unique, executes a dryRun first to find
                        // any duplicates without actually changing the indexes to avoid
                        // inconsistent index specs on different shards. Example:
                        //   Shard0: {_id: 0, a: 1}
                        //   Shard1: {_id: 1, a: 2}, {_id: 2, a: 2}
                        //   When trying to convert index {a: 1} to unique, the dry run will return
                        //   the duplicate errors to the user without converting the indexes.
                        if (isCollModIndexUniqueConversion(_request)) {
                            // The 'dryRun' option only works with 'unique' index option. We need to
                            // strip out other incompatible options.
                            auto dryRunRequest = ShardsvrCollModParticipant{
                                originalNss(), makeCollModDryRunRequest(_request)};
                            async_rpc::GenericArgs args;
                            async_rpc::AsyncRPCCommandHelpers::appendMajorityWriteConcern(args);
                            auto optsDryRun = std::make_shared<
                                async_rpc::AsyncRPCOptions<ShardsvrCollModParticipant>>(
                                dryRunRequest, **executor, token, args);
                            sharding_ddl_util::sendAuthenticatedCommandToShards(
                                opCtx, optsDryRun, shardsOwningChunks);
                        }

                        // A view definition will only be present on the primary shard. So we pass
                        // an addition 'performViewChange' flag only to the primary shard.
                        if (primaryShardOwningChunk != shardsOwningChunks.end()) {
                            request.setPerformViewChange(true);
                            auto opts = std::make_shared<
                                async_rpc::AsyncRPCOptions<ShardsvrCollModParticipant>>(
                                request, **executor, token, async_rpc::GenericArgs());
                            const auto& primaryResponse = sendAuthenticatedCommandWithOsiToShards(
                                opCtx, opts, {_shardingInfo->primaryShard}, getNewSession(opCtx));

                            responses.insert(
                                responses.end(), primaryResponse.begin(), primaryResponse.end());
                            shardsOwningChunks.erase(primaryShardOwningChunk);
                        }

                        request.setPerformViewChange(false);
                        auto opts = std::make_shared<
                            async_rpc::AsyncRPCOptions<ShardsvrCollModParticipant>>(
                            request, **executor, token, async_rpc::GenericArgs());
                        const auto& secondaryResponses = sendAuthenticatedCommandWithOsiToShards(
                            opCtx, opts, shardsOwningChunks, getNewSession(opCtx));

                        responses.insert(
                            responses.end(), secondaryResponses.begin(), secondaryResponses.end());

                        BSONObjBuilder builder;
                        std::string errmsg;
                        auto ok =
                            appendRawResponses(opCtx, &errmsg, &builder, responses).responseOK;
                        if (!errmsg.empty()) {
                            CommandHelpers::appendSimpleCommandStatus(builder, ok, errmsg);
                        }
                        _result = builder.obj();

                        const auto collUUID = _doc.getCollUUID();
                        sharding_ddl_util::resumeMigrations(
                            opCtx, _collInfo->nsForTargeting, collUUID, getNewSession(opCtx));
                    } catch (DBException& ex) {
                        if (!_isRetriableErrorForDDLCoordinator(ex.toStatus())) {
                            const auto collUUID = _doc.getCollUUID();
                            sharding_ddl_util::resumeMigrations(
                                opCtx, _collInfo->nsForTargeting, collUUID, getNewSession(opCtx));
                        }
                        throw;
                    }
                } else {
                    CollMod cmd(originalNss());
                    cmd.setCollModRequest(_request);
                    BSONObjBuilder collModResBuilder;
                    uassertStatusOK(timeseries::processCollModCommandWithTimeSeriesTranslation(
                        opCtx, originalNss(), cmd, true, &collModResBuilder));
                    auto collModRes = collModResBuilder.obj();

                    const auto dbInfo = uassertStatusOK(
                        Grid::get(opCtx)->catalogCache()->getDatabase(opCtx, nss().db()));
                    const auto shard = uassertStatusOK(
                        Grid::get(opCtx)->shardRegistry()->getShard(opCtx, dbInfo->getPrimary()));
                    BSONObjBuilder builder;
                    builder.appendElements(collModRes);
                    BSONObjBuilder subBuilder(builder.subobjStart("raw"));
                    subBuilder.append(shard->getConnString().toString(), collModRes);
                    subBuilder.doneFast();
                    _result = builder.obj();
                }
            }));
}

}  // namespace mongo
