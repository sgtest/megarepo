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
import org.elasticsearch.cluster.metadata.SingleNodeShutdownMetadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.core.SuppressForbidden;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.Objects;

import static org.elasticsearch.cluster.metadata.NodesShutdownMetadata.getShutdownsOrEmpty;

public class TransportPutShutdownNodeAction extends AcknowledgedTransportMasterNodeAction<PutShutdownNodeAction.Request> {
    private static final Logger logger = LogManager.getLogger(TransportPutShutdownNodeAction.class);

    @Inject
    public TransportPutShutdownNodeAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver
    ) {
        super(
            PutShutdownNodeAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            PutShutdownNodeAction.Request::new,
            indexNameExpressionResolver,
            ThreadPool.Names.SAME
        );
    }

    @Override
    protected void masterOperation(
        Task task,
        PutShutdownNodeAction.Request request,
        ClusterState state,
        ActionListener<AcknowledgedResponse> listener
    ) throws Exception {
        if (isNoop(state, request)) {
            listener.onResponse(AcknowledgedResponse.TRUE);
            return;
        }
        clusterService.submitStateUpdateTask("put-node-shutdown-" + request.getNodeId(), new ClusterStateUpdateTask() {
            @Override
            public ClusterState execute(ClusterState currentState) {
                if (isNoop(currentState, request)) {
                    return currentState;
                }

                final boolean nodeSeen = currentState.getNodes().nodeExists(request.getNodeId());
                SingleNodeShutdownMetadata newNodeMetadata = SingleNodeShutdownMetadata.builder()
                    .setNodeId(request.getNodeId())
                    .setType(request.getType())
                    .setReason(request.getReason())
                    .setStartedAtMillis(System.currentTimeMillis())
                    .setNodeSeen(nodeSeen)
                    .setAllocationDelay(request.getAllocationDelay())
                    .setTargetNodeName(request.getTargetNodeName())
                    .build();

                // log the update
                var currentShutdownMetadata = getShutdownsOrEmpty(currentState);
                SingleNodeShutdownMetadata existingRecord = currentShutdownMetadata.getAllNodeMetadataMap().get(request.getNodeId());
                if (existingRecord != null) {
                    logger.info("updating existing shutdown record {} with new record {}", existingRecord, newNodeMetadata);
                } else {
                    logger.info("creating shutdown record {}", newNodeMetadata);
                }

                return ClusterState.builder(currentState)
                    .metadata(
                        Metadata.builder(currentState.metadata())
                            .putCustom(NodesShutdownMetadata.TYPE, currentShutdownMetadata.putSingleNodeMetadata(newNodeMetadata))
                    )
                    .build();
            }

            @Override
            public void onFailure(Exception e) {
                logger.error(new ParameterizedMessage("failed to put shutdown for node [{}]", request.getNodeId()), e);
                listener.onFailure(e);
            }

            @Override
            public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                boolean shouldReroute = switch (request.getType()) {
                    case REMOVE, REPLACE -> true;
                    default -> false;
                };

                if (shouldReroute) {
                    clusterService.getRerouteService()
                        .reroute("node registered for removal from cluster", Priority.URGENT, new ActionListener<>() {
                            @Override
                            public void onResponse(ClusterState clusterState) {}

                            @Override
                            public void onFailure(Exception e) {
                                logger.warn(() -> "failed to reroute after registering node [" + request.getNodeId() + "] for shutdown", e);
                            }
                        });
                } else {
                    logger.trace(
                        () -> "not starting reroute after registering node ["
                            + request.getNodeId()
                            + "] for shutdown of type ["
                            + request.getType()
                            + "]"
                    );
                }
                listener.onResponse(AcknowledgedResponse.TRUE);
            }
        }, newExecutor());
    }

    private static boolean isNoop(ClusterState state, PutShutdownNodeAction.Request request) {
        var currentShutdownMetadata = getShutdownsOrEmpty(state);
        var existing = currentShutdownMetadata.getAllNodeMetadataMap().get(request.getNodeId());
        return existing != null
            && existing.getType().equals(request.getType())
            && existing.getReason().equals(request.getReason())
            && Objects.equals(existing.getAllocationDelay(), request.getAllocationDelay())
            && Objects.equals(existing.getTargetNodeName(), request.getTargetNodeName());
    }

    @Override
    protected ClusterBlockException checkBlock(PutShutdownNodeAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    @SuppressForbidden(reason = "legacy usage of unbatched task") // TODO add support for batching here
    private static <T extends ClusterStateUpdateTask> ClusterStateTaskExecutor<T> newExecutor() {
        return ClusterStateTaskExecutor.unbatched();
    }
}
