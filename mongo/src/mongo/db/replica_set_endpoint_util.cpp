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

#include "mongo/db/replica_set_endpoint_util.h"

#include "mongo/db/commands.h"
#include "mongo/db/multitenancy_gen.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/replica_set_endpoint_sharding_state.h"
#include "mongo/db/s/replica_set_endpoint_feature_flag_gen.h"
#include "mongo/s/grid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {
namespace replica_set_endpoint {

namespace {

/**
 * Returns true if this is an operation from an internal client.
 */
bool isInternalClient(OperationContext* opCtx) {
    return !opCtx->getClient()->session() || opCtx->getClient()->isInternalClient() ||
        opCtx->getClient()->isInDirectClient();
}

/**
 * Returns true if this is a request for an aggregate command with a $currentOp stage.
 */
bool isCurrentOpAggregateCommandRequest(const OpMsgRequest& opMsgReq) {
    if (opMsgReq.getDbName().isAdminDB() && opMsgReq.getCommandName() == "aggregate") {
        auto aggRequest = AggregateCommandRequest::parse(
            IDLParserContext("ServiceEntryPointMongod::isCurrentOp"), opMsgReq.body);
        return aggRequest.getPipeline()[0].firstElementFieldNameStringData() == "$currentOp";
    }
    return false;
}

/**
 * Returns true if this is a request for a command that needs to run on the mongod it arrives on.
 */
bool isTargetedCommandRequest(OperationContext* opCtx, const OpMsgRequest& opMsgReq) {
    return kTargetedCmdNames.contains(opMsgReq.getCommandName()) ||
        isCurrentOpAggregateCommandRequest(opMsgReq);
}

/**
 * Returns true if this is a request for a command that does not exist on a router.
 */
bool isRoutableCommandRequest(OperationContext* opCtx, const OpMsgRequest& opMsgReq) {
    Service routerService{opCtx->getServiceContext(), ClusterRole::RouterServer};
    return CommandHelpers::findCommand(&routerService, opMsgReq.getCommandName());
}

}  // namespace

bool isReplicaSetEndpointClient(Client* client) {
    if (client->isRouterClient()) {
        return false;
    }
    return replica_set_endpoint::ReplicaSetEndpointShardingState::get(client->getServiceContext())
        ->supportsReplicaSetEndpoint();
}

bool shouldRouteRequest(OperationContext* opCtx, const OpMsgRequest& opMsgReq) {
    // The request must have come in through a client on the shard port.
    invariant(!opCtx->getClient()->isRouterClient());

    if (!replica_set_endpoint::ReplicaSetEndpointShardingState::get(opCtx)
             ->supportsReplicaSetEndpoint()) {
        return false;
    }

    if (gMultitenancySupport) {
        // Currently, serverless does not support sharding.
        return false;
    }

    if (!Grid::get(opCtx)->isShardingInitialized()) {
        return false;
    }

    if (isInternalClient(opCtx) || opMsgReq.getDbName().isLocalDB() ||
        isTargetedCommandRequest(opCtx, opMsgReq) || !isRoutableCommandRequest(opCtx, opMsgReq)) {
        return false;
    }

    // There is nothing that will prevent the cluster from becoming multi-shard (i.e. no longer
    // supporting as replica set endpoint) after the check here is done. However, the contract is
    // that users must have transitioned to the sharded connection string (i.e. connect to mongoses
    // and/or router port of mongods) before adding a second shard. Also, commands that make it to
    // here should be safe to route even when the cluster has more than one shard.
    return true;
}

}  // namespace replica_set_endpoint
}  // namespace mongo
