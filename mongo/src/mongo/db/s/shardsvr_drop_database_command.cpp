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


#include <boost/optional.hpp>
#include <memory>
#include <string>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/commands.h"
#include "mongo/db/commands/feature_compatibility_version.h"
#include "mongo/db/curop.h"
#include "mongo/db/database_name.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/s/drop_database_coordinator.h"
#include "mongo/db/s/drop_database_coordinator_document_gen.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/s/sharding_ddl_coordinator.h"
#include "mongo/db/s/sharding_ddl_coordinator_gen.h"
#include "mongo/db/s/sharding_ddl_coordinator_service.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/service_context.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/rpc/op_msg.h"
#include "mongo/s/request_types/sharded_ddl_commands_gen.h"
#include "mongo/s/sharding_feature_flags_gen.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/future.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding


namespace mongo {
namespace {

class ShardsvrDropDatabaseCommand final : public TypedCommand<ShardsvrDropDatabaseCommand> {
public:
    using Request = ShardsvrDropDatabase;

    std::string help() const override {
        return "Internal command, which is exported by the primary sharding server. Do not call "
               "directly. Drops a database.";
    }

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return Command::AllowedOnSecondary::kNever;
    }

    class Invocation final : public InvocationBase {
    public:
        using InvocationBase::InvocationBase;

        void typedRun(OperationContext* opCtx) {
            uassertStatusOK(ShardingState::get(opCtx)->canAcceptShardedCommands());

            CommandHelpers::uassertCommandRunWithMajority(Request::kCommandName,
                                                          opCtx->getWriteConcern());

            opCtx->setAlwaysInterruptAtStepDownOrUp_UNSAFE();

            // Since this operation is not directly writing locally we need to force its db
            // profile level increase in order to be logged in "<db>.system.profile"
            CurOp::get(opCtx)->raiseDbProfileLevel(
                CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(ns().dbName()));

            auto service = ShardingDDLCoordinatorService::getService(opCtx);
            const auto requestVersion =
                OperationShardingState::get(opCtx).getDbVersion(ns().dbName());
            auto dropDatabaseCoordinator = [&]() {
                while (true) {
                    // TODO SERVER-73627: Remove once 7.0 becomes last LTS.
                    boost::optional<FixedFCVRegion> fixedFcvRegion;
                    fixedFcvRegion.emplace(opCtx);

                    DropDatabaseCoordinatorDocument coordinatorDoc;
                    const DDLCoordinatorTypeEnum coordType =
                        feature_flags::gDropCollectionHoldingCriticalSection.isEnabled(
                            **fixedFcvRegion)
                        ? DDLCoordinatorTypeEnum::kDropDatabase
                        : DDLCoordinatorTypeEnum::kDropDatabasePre70Compatible;

                    coordinatorDoc.setShardingDDLCoordinatorMetadata({{ns(), coordType}});

                    auto currentCoordinator = checked_pointer_cast<DropDatabaseCoordinator>(
                        service->getOrCreateInstance(opCtx, coordinatorDoc.toBSON()));
                    const auto currentDbVersion = currentCoordinator->getDatabaseVersion();
                    if (currentDbVersion == requestVersion) {
                        return currentCoordinator;
                    }

                    fixedFcvRegion.reset();
                    LOGV2_DEBUG(6073000,
                                2,
                                "DbVersion mismatch, waiting for existing coordinator to finish",
                                "requestedVersion"_attr = requestVersion,
                                "coordinatorVersion"_attr = currentDbVersion);
                    currentCoordinator->getCompletionFuture().wait(opCtx);
                }
            }();
            dropDatabaseCoordinator->getCompletionFuture().get(opCtx);
        }

    private:
        NamespaceString ns() const override {
            return NamespaceString(request().getDbName());
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
} shardsvrDropDatabaseCommand;

}  // namespace
}  // namespace mongo
