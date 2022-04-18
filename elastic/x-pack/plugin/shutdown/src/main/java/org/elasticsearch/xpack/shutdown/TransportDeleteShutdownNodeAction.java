/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.shutdown;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateTaskExecutor;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.NodesShutdownMetadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.core.SuppressForbidden;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import static org.elasticsearch.cluster.metadata.NodesShutdownMetadata.getShutdownsOrEmpty;

public class TransportDeleteShutdownNodeAction extends AcknowledgedTransportMasterNodeAction<DeleteShutdownNodeAction.Request> {
    private static final Logger logger = LogManager.getLogger(TransportDeleteShutdownNodeAction.class);

    @Inject
    public TransportDeleteShutdownNodeAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver
    ) {
        super(
            DeleteShutdownNodeAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            DeleteShutdownNodeAction.Request::new,
            indexNameExpressionResolver,
            ThreadPool.Names.SAME
        );
    }

    @Override
    protected void masterOperation(
        Task task,
        DeleteShutdownNodeAction.Request request,
        ClusterState state,
        ActionListener<AcknowledgedResponse> listener
    ) throws Exception {
        { // This block solely to ensure this NodesShutdownMetadata isn't accidentally used in the cluster state update task below
            NodesShutdownMetadata nodesShutdownMetadata = state.metadata().custom(NodesShutdownMetadata.TYPE);
            if (nodesShutdownMetadata == null || nodesShutdownMetadata.getAllNodeMetadataMap().get(request.getNodeId()) == null) {
                throw new ResourceNotFoundException("node [" + request.getNodeId() + "] is not currently shutting down");
            }
        }

        clusterService.submitStateUpdateTask("delete-node-shutdown-" + request.getNodeId(), new ClusterStateUpdateTask() {
            @Override
            public ClusterState execute(ClusterState currentState) throws Exception {
                NodesShutdownMetadata currentShutdownMetadata = getShutdownsOrEmpty(currentState);
                var existing = currentShutdownMetadata.getAllNodeMetadataMap().get(request.getNodeId());
                if (existing == null) {
                    // noop, the node has already been removed by the time we got to this update task
                    return currentState;
                }

                logger.info("removing shutdown record for node [{}]", request.getNodeId());

                return ClusterState.builder(currentState)
                    .metadata(
                        Metadata.builder(currentState.metadata())
                            .putCustom(NodesShutdownMetadata.TYPE, currentShutdownMetadata.removeSingleNodeMetadata(request.getNodeId()))
                    )
                    .build();

            }

            @Override
            public void onFailure(Exception e) {
                logger.error(new ParameterizedMessage("failed to delete shutdown for node [{}]", request.getNodeId()), e);
                listener.onFailure(e);
            }

            @Override
            public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                clusterService.getRerouteService()
                    .reroute("node registered for removal from cluster", Priority.URGENT, new ActionListener<>() {
                        @Override
                        public void onResponse(ClusterState clusterState) {}

                        @Override
                        public void onFailure(Exception e) {
                            logger.warn(() -> "failed to reroute after deleting node [" + request.getNodeId() + "] shutdown", e);
                        }
                    });
                listener.onResponse(AcknowledgedResponse.TRUE);
            }
        }, newExecutor());
    }

    @Override
    protected ClusterBlockException checkBlock(DeleteShutdownNodeAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    @SuppressForbidden(reason = "legacy usage of unbatched task") // TODO add support for batching here
    private static <T extends ClusterStateUpdateTask> ClusterStateTaskExecutor<T> newExecutor() {
        return ClusterStateTaskExecutor.unbatched();
    }
}
