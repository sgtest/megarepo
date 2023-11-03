/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include "mongo/base/shim.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/commands.h"
#include "mongo/db/commands/cluster_server_parameter_cmds_gen.h"
#include "mongo/db/commands/query_settings_cmds_gen.h"
#include "mongo/db/commands/set_cluster_parameter_command_impl.h"
#include "mongo/db/commands/set_cluster_parameter_invocation.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/query_settings_cluster_parameter_gen.h"
#include "mongo/db/query/query_settings_gen.h"
#include "mongo/db/query/query_settings_manager.h"
#include "mongo/db/query/query_settings_utils.h"
#include "mongo/db/query/query_shape/query_shape.h"
#include "mongo/db/query/sbe_plan_cache.h"
#include "mongo/platform/basic.h"
#include "mongo/stdx/variant.h"
#include "mongo/util/assert_util.h"

namespace mongo {
namespace {

using namespace query_settings;

MONGO_FAIL_POINT_DEFINE(querySettingsPlanCacheInvalidation);

static constexpr auto kQuerySettingsClusterParameterName = "querySettings"_sd;

SetClusterParameter makeSetClusterParameterRequest(
    const std::vector<QueryShapeConfiguration>& settingsArray, const mongo::DatabaseName& dbName) {
    BSONObjBuilder bob;
    BSONArrayBuilder arrayBuilder(
        bob.subarrayStart(QuerySettingsClusterParameterValue::kSettingsArrayFieldName));
    for (const auto& item : settingsArray) {
        arrayBuilder.append(item.toBSON());
    }
    arrayBuilder.done();
    SetClusterParameter setClusterParameterRequest(
        BSON(QuerySettingsManager::kQuerySettingsClusterParameterName << bob.done()));
    setClusterParameterRequest.setDbName(dbName);
    return setClusterParameterRequest;
}

/**
 * Invokes the setClusterParameter() weak function, which is an abstraction over the corresponding
 * command implementation in the router-role vs. the shard-role/the replica-set or standalone impl.
 */
void setClusterParameter(OperationContext* opCtx,
                         const SetClusterParameter& request,
                         boost::optional<Timestamp> clusterParameterTime,
                         boost::optional<LogicalTime> previousTime) {
    auto w = getSetClusterParameterImpl(opCtx);
    w(opCtx, request, clusterParameterTime, previousTime);
}

/**
 * Merges the query settings 'lhs' with query settings 'rhs', by replacing all attributes in 'lhs'
 * with the existing attributes in 'rhs'.
 */
QuerySettings mergeQuerySettings(const QuerySettings& lhs, const QuerySettings& rhs) {
    QuerySettings querySettings = lhs;

    if (rhs.getQueryEngineVersion()) {
        querySettings.setQueryEngineVersion(rhs.getQueryEngineVersion());
    }

    if (rhs.getIndexHints()) {
        querySettings.setIndexHints(rhs.getIndexHints());
    }

    return querySettings;
}

/**
 * Clears the SBE plan cache if 'querySettingsPlanCacheInvalidation' failpoint is set.
 * Used when setting index filters via query settings interface. See query_settings_passthrough
 * suite.
 */
void testOnlyClearPlanCache(OperationContext* opCtx) {
    if (MONGO_unlikely(querySettingsPlanCacheInvalidation.shouldFail())) {
        sbe::getPlanCache(opCtx).clear();
    }
}

class SetQuerySettingsCommand final : public TypedCommand<SetQuerySettingsCommand> {
public:
    using Request = SetQuerySettingsCommandRequest;

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool adminOnly() const override {
        return true;
    }

    std::string help() const override {
        return "Sets the query settings for the query shape of a given query.";
    }

    bool allowedWithSecurityToken() const final {
        return true;
    }

    class Invocation final : public InvocationBase {
    public:
        using InvocationBase::InvocationBase;

        SetQuerySettingsCommandReply insertQuerySettings(
            OperationContext* opCtx,
            QueryShapeConfiguration queryShapeConfiguration,
            const RepresentativeQueryInfo& representativeQueryInfo) {
            // Assert querySettings command is valid.
            utils::validateQuerySettings(
                queryShapeConfiguration, representativeQueryInfo, request().getDbName().tenantId());

            // Build the new 'settingsArray' by appending 'newConfig' to the list of all
            // QueryShapeConfigurations for the given tenant.
            auto& querySettingsManager = QuerySettingsManager::get(opCtx);
            auto settingsArray = querySettingsManager.getAllQueryShapeConfigurations(
                opCtx, request().getDbName().tenantId());
            settingsArray.push_back(queryShapeConfiguration);

            // Run SetClusterParameter command with the new value of the 'querySettings' cluster
            // parameter.
            setClusterParameter(
                opCtx,
                makeSetClusterParameterRequest(settingsArray, request().getDbName()),
                boost::none,
                querySettingsManager.getClusterParameterTime(opCtx,
                                                             request().getDbName().tenantId()));
            SetQuerySettingsCommandReply reply;
            reply.setQueryShapeConfiguration(std::move(queryShapeConfiguration));
            return reply;
        }

        SetQuerySettingsCommandReply updateQuerySettings(
            OperationContext* opCtx,
            const QuerySettings& newQuerySettings,
            const QueryShapeConfiguration& currentQueryShapeConfiguration) {
            // Compute the merged query settings.
            auto mergedQuerySettings =
                mergeQuerySettings(currentQueryShapeConfiguration.getSettings(), newQuerySettings);

            // Build the new 'settingsArray' by updating the existing QueryShapeConfiguration with
            // the 'mergedQuerySettings'.
            auto& querySettingsManager = QuerySettingsManager::get(opCtx);
            auto settingsArray = querySettingsManager.getAllQueryShapeConfigurations(
                opCtx, request().getDbName().tenantId());

            // Ensure the to be updated QueryShapeConfiguration is present in the 'settingsArray'.
            auto updatedQueryShapeConfigurationIt =
                std::find_if(settingsArray.begin(),
                             settingsArray.end(),
                             [&](const QueryShapeConfiguration& queryShapeConfiguration) {
                                 return queryShapeConfiguration.getQueryShapeHash() ==
                                     currentQueryShapeConfiguration.getQueryShapeHash();
                             });
            tassert(7746500,
                    "In order to perform an update, QueryShapeConfiguration must be present in "
                    "QuerySettingsManager",
                    updatedQueryShapeConfigurationIt != settingsArray.end());
            updatedQueryShapeConfigurationIt->setSettings(mergedQuerySettings);

            // Run SetClusterParameter command with the new value of the 'querySettings' cluster
            // parameter.
            setClusterParameter(
                opCtx,
                makeSetClusterParameterRequest(settingsArray, request().getDbName()),
                boost::none,
                querySettingsManager.getClusterParameterTime(opCtx,
                                                             request().getDbName().tenantId()));
            SetQuerySettingsCommandReply reply;
            reply.setQueryShapeConfiguration(*updatedQueryShapeConfigurationIt);
            return reply;
        }

        SetQuerySettingsCommandReply setQuerySettingsByQueryShapeHash(
            OperationContext* opCtx, const query_shape::QueryShapeHash& queryShapeHash) {
            auto& querySettingsManager = QuerySettingsManager::get(opCtx);
            auto tenantId = request().getDbName().tenantId();

            auto querySettings = querySettingsManager.getQuerySettingsForQueryShapeHash(
                opCtx, queryShapeHash, tenantId);
            uassert(7746401,
                    "New query settings can only be created with a query instance, but a query "
                    "hash was given.",
                    querySettings.has_value());

            auto representativeQueryInfo =
                createRepresentativeInfo(querySettings->second, opCtx, tenantId);
            return updateQuerySettings(opCtx,
                                       request().getSettings(),
                                       QueryShapeConfiguration(queryShapeHash,
                                                               std::move(querySettings->first),
                                                               std::move(querySettings->second)));
        }

        SetQuerySettingsCommandReply setQuerySettingsByQueryInstance(
            OperationContext* opCtx, const QueryInstance& queryInstance) {
            auto& querySettingsManager = QuerySettingsManager::get(opCtx);
            auto tenantId = request().getDbName().tenantId();
            auto representativeQueryInfo = createRepresentativeInfo(queryInstance, opCtx, tenantId);
            auto& queryShapeHash = representativeQueryInfo.queryShapeHash;

            // If there is already an entry for a given QueryShapeHash, then perform
            // an update, otherwise insert.
            if (auto lookupResult = querySettingsManager.getQuerySettingsForQueryShapeHash(
                    opCtx,
                    [&]() { return queryShapeHash; },
                    representativeQueryInfo.namespaceString)) {
                return updateQuerySettings(
                    opCtx,
                    request().getSettings(),
                    QueryShapeConfiguration(std::move(queryShapeHash),
                                            std::move(lookupResult->first),
                                            std::move(lookupResult->second)));
            } else {
                return insertQuerySettings(
                    opCtx,
                    QueryShapeConfiguration(std::move(queryShapeHash),
                                            std::move(request().getSettings()),
                                            queryInstance),
                    representativeQueryInfo);
            }
        }

        SetQuerySettingsCommandReply typedRun(OperationContext* opCtx) {
            uassert(7746400,
                    "setQuerySettings command is unknown",
                    feature_flags::gFeatureFlagQuerySettings.isEnabled(
                        serverGlobalParams.featureCompatibility));
            auto response =
                stdx::visit(OverloadedVisitor{
                                [&](const query_shape::QueryShapeHash& queryShapeHash) {
                                    return setQuerySettingsByQueryShapeHash(opCtx, queryShapeHash);
                                },
                                [&](const QueryInstance& queryInstance) {
                                    return setQuerySettingsByQueryInstance(opCtx, queryInstance);
                                },
                            },
                            request().getCommandParameter());
            testOnlyClearPlanCache(opCtx);
            return response;
        }

    private:
        bool supportsWriteConcern() const override {
            return false;
        }

        NamespaceString ns() const override {
            return NamespaceString::kEmpty;
        }

        void doCheckAuthorization(OperationContext* opCtx) const override {
            uassert(ErrorCodes::Unauthorized,
                    "Unauthorized",
                    AuthorizationSession::get(opCtx->getClient())
                        ->isAuthorizedForPrivilege(Privilege{
                            ResourcePattern::forClusterResource(request().getDbName().tenantId()),
                            ActionType::querySettings}));
        }
    };
};
MONGO_REGISTER_COMMAND(SetQuerySettingsCommand).forRouter().forShard();

class RemoveQuerySettingsCommand final : public TypedCommand<RemoveQuerySettingsCommand> {
public:
    using Request = RemoveQuerySettingsCommandRequest;

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool adminOnly() const override {
        return true;
    }

    std::string help() const override {
        return "Removes the query settings for the query shape of a given query.";
    }

    bool allowedWithSecurityToken() const final {
        return true;
    }

    class Invocation final : public InvocationBase {
    public:
        using InvocationBase::InvocationBase;

        void typedRun(OperationContext* opCtx) {
            uassert(7746700,
                    "removeQuerySettings command is unknown",
                    feature_flags::gFeatureFlagQuerySettings.isEnabled(
                        serverGlobalParams.featureCompatibility));
            auto tenantId = request().getDbName().tenantId();
            auto queryShapeHash =
                stdx::visit(OverloadedVisitor{
                                [&](const query_shape::QueryShapeHash& queryShapeHash) {
                                    return queryShapeHash;
                                },
                                [&](const QueryInstance& queryInstance) {
                                    // Converts 'queryInstance' into QueryShapeHash, for convenient
                                    // comparison during search for the matching
                                    // QueryShapeConfiguration.
                                    auto representativeQueryInfo =
                                        createRepresentativeInfo(queryInstance, opCtx, tenantId);

                                    return representativeQueryInfo.queryShapeHash;
                                },
                            },
                            request().getCommandParameter());
            auto& querySettingsManager = QuerySettingsManager::get(opCtx);

            // Build the new 'settingsArray' by removing the QueryShapeConfiguration with a matching
            // QueryShapeHash.
            auto settingsArray =
                querySettingsManager.getAllQueryShapeConfigurations(opCtx, tenantId);
            auto matchingQueryShapeConfigurationIt =
                std::find_if(settingsArray.begin(),
                             settingsArray.end(),
                             [&](const QueryShapeConfiguration& configuration) {
                                 return configuration.getQueryShapeHash() == queryShapeHash;
                             });
            uassert(7746701,
                    "A matching query settings entry does not exist",
                    matchingQueryShapeConfigurationIt != settingsArray.end());
            settingsArray.erase(matchingQueryShapeConfigurationIt);

            // Run SetClusterParameter command with the new value of the 'querySettings' cluster
            // parameter.
            setClusterParameter(
                opCtx,
                makeSetClusterParameterRequest(settingsArray, request().getDbName()),
                boost::none,
                querySettingsManager.getClusterParameterTime(opCtx,
                                                             request().getDbName().tenantId()));

            testOnlyClearPlanCache(opCtx);
        }

    private:
        bool supportsWriteConcern() const override {
            return false;
        }

        NamespaceString ns() const override {
            return NamespaceString::kEmpty;
        }

        void doCheckAuthorization(OperationContext* opCtx) const override {
            uassert(ErrorCodes::Unauthorized,
                    "Unauthorized",
                    AuthorizationSession::get(opCtx->getClient())
                        ->isAuthorizedForPrivilege(Privilege{
                            ResourcePattern::forClusterResource(request().getDbName().tenantId()),
                            ActionType::querySettings}));
        }
    };
};
MONGO_REGISTER_COMMAND(RemoveQuerySettingsCommand).forRouter().forShard();
}  // namespace
}  // namespace mongo
