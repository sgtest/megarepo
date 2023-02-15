/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.indices.refresh;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionListenerResponseHandler;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.replication.BasicReplicationRequest;
import org.elasticsearch.action.support.replication.ReplicationOperation;
import org.elasticsearch.action.support.replication.ReplicationResponse;
import org.elasticsearch.action.support.replication.TransportReplicationAction;
import org.elasticsearch.cluster.action.shard.ShardStateAction;
import org.elasticsearch.cluster.routing.IndexShardRoutingTable;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.indices.IndicesService;
import org.elasticsearch.logging.LogManager;
import org.elasticsearch.logging.Logger;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.io.IOException;

public class TransportShardRefreshAction extends TransportReplicationAction<
    BasicReplicationRequest,
    ShardRefreshReplicaRequest,
    ReplicationResponse> {

    private static final Logger logger = LogManager.getLogger(TransportShardRefreshAction.class);

    public static final String NAME = RefreshAction.NAME + "[s]";
    public static final ActionType<ReplicationResponse> TYPE = new ActionType<>(NAME, ReplicationResponse::new);
    public static final String SOURCE_API = "api";

    @Inject
    public TransportShardRefreshAction(
        Settings settings,
        TransportService transportService,
        ClusterService clusterService,
        IndicesService indicesService,
        ThreadPool threadPool,
        ShardStateAction shardStateAction,
        ActionFilters actionFilters
    ) {
        super(
            settings,
            NAME,
            transportService,
            clusterService,
            indicesService,
            threadPool,
            shardStateAction,
            actionFilters,
            BasicReplicationRequest::new,
            ShardRefreshReplicaRequest::new,
            ThreadPool.Names.REFRESH
        );
        // registers the unpromotable version of shard refresh action
        new TransportUnpromotableShardRefreshAction(clusterService, transportService, actionFilters, indicesService);
    }

    @Override
    protected ReplicationResponse newResponseInstance(StreamInput in) throws IOException {
        return new ReplicationResponse(in);
    }

    @Override
    protected void shardOperationOnPrimary(
        BasicReplicationRequest shardRequest,
        IndexShard primary,
        ActionListener<PrimaryResult<ShardRefreshReplicaRequest, ReplicationResponse>> listener
    ) {
        ActionListener.completeWith(listener, () -> {
            ShardRefreshReplicaRequest replicaRequest = new ShardRefreshReplicaRequest(shardRequest.shardId(), primary.refresh(SOURCE_API));
            replicaRequest.setParentTask(shardRequest.getParentTask());
            logger.trace("{} refresh request executed on primary", primary.shardId());
            return new PrimaryResult<>(replicaRequest, new ReplicationResponse());
        });
    }

    @Override
    protected void shardOperationOnReplica(ShardRefreshReplicaRequest request, IndexShard replica, ActionListener<ReplicaResult> listener) {
        ActionListener.completeWith(listener, () -> {
            replica.refresh(SOURCE_API);
            logger.trace("{} refresh request executed on replica", replica.shardId());
            return new ReplicaResult();
        });
    }

    @Override
    protected ReplicationOperation.Replicas<ShardRefreshReplicaRequest> newReplicasProxy() {
        return new UnpromotableReplicasRefreshProxy();
    }

    protected class UnpromotableReplicasRefreshProxy extends ReplicasProxy {

        @Override
        public void onPrimaryOperationComplete(
            ShardRefreshReplicaRequest replicaRequest,
            IndexShardRoutingTable indexShardRoutingTable,
            ActionListener<Void> listener
        ) {
            assert replicaRequest.primaryRefreshResult.refreshed() : "primary has not refreshed";
            UnpromotableShardRefreshRequest unpromotableReplicaRequest = new UnpromotableShardRefreshRequest(
                indexShardRoutingTable,
                replicaRequest.primaryRefreshResult.generation()
            );
            transportService.sendRequest(
                transportService.getLocalNode(),
                TransportUnpromotableShardRefreshAction.NAME,
                unpromotableReplicaRequest,
                new ActionListenerResponseHandler<>(
                    listener.delegateFailure((l, r) -> l.onResponse(null)),
                    (in) -> ActionResponse.Empty.INSTANCE,
                    ThreadPool.Names.REFRESH
                )
            );
        }
    }
}
