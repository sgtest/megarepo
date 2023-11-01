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

#include "mongo/db/replica_set_endpoint_sharding_state.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {
namespace replica_set_endpoint {

namespace {

const auto getReplicaSetEndpointShardingState =
    ServiceContext::declareDecoration<ReplicaSetEndpointShardingState>();

}  // namespace

ReplicaSetEndpointShardingState::ReplicaSetEndpointShardingState() = default;

ReplicaSetEndpointShardingState::~ReplicaSetEndpointShardingState() = default;

ReplicaSetEndpointShardingState* ReplicaSetEndpointShardingState::get(
    ServiceContext* serviceContext) {
    return &getReplicaSetEndpointShardingState(serviceContext);
}

ReplicaSetEndpointShardingState* ReplicaSetEndpointShardingState::get(OperationContext* opCtx) {
    return ReplicaSetEndpointShardingState::get(opCtx->getServiceContext());
}

void ReplicaSetEndpointShardingState::setIsConfigShard(bool value) {
    invariant(serverGlobalParams.clusterRole.has(ClusterRole::ConfigServer));

    stdx::unique_lock<Latch> ul(_mutex);
    _isConfigShard = value;
}

bool ReplicaSetEndpointShardingState::isConfigShardForTest() {
    stdx::unique_lock<Latch> ul(_mutex);
    return _isConfigShard;
}

}  // namespace replica_set_endpoint
}  // namespace mongo
