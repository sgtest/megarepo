/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.shutdown;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.NodesShutdownMetadata;
import org.elasticsearch.cluster.metadata.ShutdownPersistentTasksStatus;
import org.elasticsearch.cluster.metadata.ShutdownPluginsStatus;
import org.elasticsearch.cluster.metadata.ShutdownShardMigrationStatus;
import org.elasticsearch.cluster.metadata.SingleNodeShutdownMetadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.stream.Collectors;

public class TransportGetShutdownStatusAction extends TransportMasterNodeAction<
    GetShutdownStatusAction.Request,
    GetShutdownStatusAction.Response> {
    @Inject
    public TransportGetShutdownStatusAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver
    ) {
        super(
            GetShutdownStatusAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            GetShutdownStatusAction.Request::readFrom,
            indexNameExpressionResolver,
            GetShutdownStatusAction.Response::new,
            ThreadPool.Names.SAME
        );
    }

    @Override
    protected void masterOperation(
        Task task,
        GetShutdownStatusAction.Request request,
        ClusterState state,
        ActionListener<GetShutdownStatusAction.Response> listener
    ) throws Exception {
        NodesShutdownMetadata nodesShutdownMetadata = state.metadata().custom(NodesShutdownMetadata.TYPE);

        GetShutdownStatusAction.Response response;
        if (nodesShutdownMetadata == null) {
            response = new GetShutdownStatusAction.Response(new ArrayList<>());
        } else if (request.getNodeIds().length == 0) {
            final List<SingleNodeShutdownStatus> shutdownStatuses = nodesShutdownMetadata.getAllNodeMetadataMap()
                .values()
                .stream()
                .map(
                    ns -> new SingleNodeShutdownStatus(
                        ns,
                        new ShutdownShardMigrationStatus(),
                        new ShutdownPersistentTasksStatus(),
                        new ShutdownPluginsStatus()
                    )
                )
                .collect(Collectors.toList());
            response = new GetShutdownStatusAction.Response(shutdownStatuses);
        } else {
            new ArrayList<>();
            final Map<String, SingleNodeShutdownMetadata> nodeShutdownMetadataMap = nodesShutdownMetadata.getAllNodeMetadataMap();
            final List<SingleNodeShutdownStatus> shutdownStatuses = Arrays.stream(request.getNodeIds())
                .map(nodeShutdownMetadataMap::get)
                .filter(Objects::nonNull)
                .map(
                    ns -> new SingleNodeShutdownStatus(
                        ns,
                        new ShutdownShardMigrationStatus(),
                        new ShutdownPersistentTasksStatus(),
                        new ShutdownPluginsStatus()
                    )

                )
                .collect(Collectors.toList());
            response = new GetShutdownStatusAction.Response(shutdownStatuses);
        }

        listener.onResponse(response);
    }

    @Override
    protected ClusterBlockException checkBlock(GetShutdownStatusAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }
}
