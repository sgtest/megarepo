/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ilm.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.cluster.AckedClusterStateUpdateTask;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ilm.OperationMode;
import org.elasticsearch.xpack.core.ilm.StopILMRequest;
import org.elasticsearch.xpack.core.ilm.action.StopILMAction;
import org.elasticsearch.xpack.ilm.OperationModeUpdateTask;

public class TransportStopILMAction extends AcknowledgedTransportMasterNodeAction<StopILMRequest> {

    @Inject
    public TransportStopILMAction(TransportService transportService, ClusterService clusterService, ThreadPool threadPool,
                                  ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver) {
        super(StopILMAction.NAME, transportService, clusterService, threadPool, actionFilters, StopILMRequest::new,
            indexNameExpressionResolver, ThreadPool.Names.SAME);
    }

    @Override
    protected void masterOperation(Task task, StopILMRequest request, ClusterState state, ActionListener<AcknowledgedResponse> listener) {
        clusterService.submitStateUpdateTask("ilm_operation_mode_update",
            new AckedClusterStateUpdateTask(Priority.IMMEDIATE, request, listener) {
                @Override
                public ClusterState execute(ClusterState currentState) {
                    return (OperationModeUpdateTask.ilmMode(OperationMode.STOPPING)).execute(currentState);
                }
            });
    }

    @Override
    protected ClusterBlockException checkBlock(StopILMRequest request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }
}
