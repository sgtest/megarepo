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

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/commands.h"
#include "mongo/db/database_name.h"
#include "mongo/db/free_mon/free_mon_commands_gen.h"
#include "mongo/db/free_mon/free_mon_controller.h"
#include "mongo/db/free_mon/free_mon_options.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/service_context.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/duration.h"

namespace mongo {

namespace {

const auto kRegisterSyncTimeout = Milliseconds{5000};

/**
 * Indicates the current status of Free Monitoring.
 */
class GetFreeMonitoringStatusCommand : public BasicCommand {
public:
    GetFreeMonitoringStatusCommand() : BasicCommand("getFreeMonitoringStatus") {}

    bool adminOnly() const override {
        return true;
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const final {
        return AllowedOnSecondary::kAlways;
    }

    bool supportsWriteConcern(const BSONObj& cmd) const final {
        return false;
    }

    std::string help() const final {
        return "Indicates free monitoring status";
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const final {
        if (!AuthorizationSession::get(opCtx->getClient())
                 ->isAuthorizedForActionsOnResource(
                     ResourcePattern::forClusterResource(dbName.tenantId()),
                     ActionType::checkFreeMonitoringStatus)) {
            return Status(ErrorCodes::Unauthorized, "Unauthorized");
        }
        return Status::OK();
    }

    bool run(OperationContext* opCtx,
             const DatabaseName&,
             const BSONObj& cmdObj,
             BSONObjBuilder& result) final {
        // Command has no members, invoke the parser to confirm that.
        IDLParserContext ctx("getFreeMonitoringStatus");
        GetFreeMonitoringStatus::parse(ctx, cmdObj);

        // FreeMonitoring has been deprecated and will be decomissioned.
        // Report that FreeMon is disabled even if it's running to draw attention
        // to the deprecation notice returned from the service.
        result.append("state"_sd, "disabled"_sd);

        if (globalFreeMonParams.freeMonitoringState != EnableCloudStateEnum::kOff) {
            // To aid discovery during deprecation period, add true state as context.
            auto* controller = FreeMonController::get(opCtx->getServiceContext());
            if (controller) {
                result.append(
                    "message"_sd,
                    "Free monitoring is deprecated, refer to 'debug' field for actual status"_sd);
                BSONObjBuilder debug(result.subobjStart("debug"_sd));
                controller->getStatus(opCtx, &debug);
            }
        }

        return true;
    }
} getFreeMonitoringStatusCommand;

/**
 * Enables or disables Free Monitoring service.
 */
class SetFreeMonitoringCommand : public BasicCommand {
public:
    SetFreeMonitoringCommand() : BasicCommand("setFreeMonitoring") {}

    bool adminOnly() const override {
        return true;
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const final {
        return AllowedOnSecondary::kNever;
    }

    bool supportsWriteConcern(const BSONObj& cmd) const final {
        return false;
    }

    std::string help() const final {
        return "enable or disable Free Monitoring";
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const final {
        if (!AuthorizationSession::get(opCtx->getClient())
                 ->isAuthorizedForActionsOnResource(
                     ResourcePattern::forClusterResource(dbName.tenantId()),
                     ActionType::setFreeMonitoring)) {
            return Status(ErrorCodes::Unauthorized, "Unauthorized");
        }
        return Status::OK();
    }

    bool run(OperationContext* opCtx,
             const DatabaseName&,
             const BSONObj& cmdObj,
             BSONObjBuilder& result) final {
        IDLParserContext ctx("setFreeMonitoring");
        auto cmd = SetFreeMonitoring::parse(ctx, cmdObj);

        auto* controller = FreeMonController::get(opCtx->getServiceContext());
        if (!controller) {
            // Pending operation.
            uasserted(50840,
                      "Free Monitoring has been disabled via the command-line and/or config file");
        }

        boost::optional<Status> optStatus = boost::none;
        if (cmd.getAction() == SetFreeMonActionEnum::enable) {
            optStatus = controller->registerServerCommand(kRegisterSyncTimeout);
        } else {
            optStatus = controller->unregisterServerCommand(kRegisterSyncTimeout);
        }

        if (optStatus) {
            // Completed within timeout.
            uassertStatusOK(*optStatus);
        } else {
            // Pending operation.
        }
        return true;
    }

} setFreeMonitoringCmd;

}  // namespace
}  // namespace mongo
