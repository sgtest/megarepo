/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.datastreams.action;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.DataStream;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.action.PromoteDataStreamAction;

public class PromoteDataStreamTransportAction extends AcknowledgedTransportMasterNodeAction<PromoteDataStreamAction.Request> {

    @Inject
    public PromoteDataStreamTransportAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver
    ) {
        super(
            PromoteDataStreamAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            PromoteDataStreamAction.Request::new,
            indexNameExpressionResolver,
            ThreadPool.Names.SAME
        );
    }

    @Override
    protected void masterOperation(
        Task task,
        PromoteDataStreamAction.Request request,
        ClusterState state,
        ActionListener<AcknowledgedResponse> listener
    ) throws Exception {
        clusterService.submitStateUpdateTask(
            "promote-data-stream [" + request.getName() + "]",
            new ClusterStateUpdateTask(Priority.HIGH, request.masterNodeTimeout()) {

                @Override
                public void onFailure(String source, Exception e) {
                    listener.onFailure(e);
                }

                @Override
                public ClusterState execute(ClusterState currentState) {
                    return promoteDataStream(currentState, request);
                }

                @Override
                public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
                    listener.onResponse(AcknowledgedResponse.TRUE);
                }
            }
        );
    }

    static ClusterState promoteDataStream(ClusterState currentState, PromoteDataStreamAction.Request request) {
        DataStream dataStream = currentState.getMetadata().dataStreams().get(request.getName());
        if (dataStream == null) {
            throw new ResourceNotFoundException("data stream [" + request.getName() + "] does not exist");
        }

        DataStream promotedDataStream = dataStream.promoteDataStream();
        Metadata.Builder metadata = Metadata.builder(currentState.metadata());
        metadata.put(promotedDataStream);
        return ClusterState.builder(currentState).metadata(metadata).build();
    }

    @Override
    protected ClusterBlockException checkBlock(PromoteDataStreamAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }
}
