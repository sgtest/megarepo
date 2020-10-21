/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.autoscaling.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterInfoService;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.autoscaling.capacity.AutoscalingCalculateCapacityService;

public class TransportGetAutoscalingCapacityAction extends TransportMasterNodeAction<
    GetAutoscalingCapacityAction.Request,
    GetAutoscalingCapacityAction.Response> {

    private final AutoscalingCalculateCapacityService capacityService;
    private final ClusterInfoService clusterInfoService;

    @Inject
    public TransportGetAutoscalingCapacityAction(
        final TransportService transportService,
        final ClusterService clusterService,
        final ThreadPool threadPool,
        final ActionFilters actionFilters,
        final IndexNameExpressionResolver indexNameExpressionResolver,
        final AutoscalingCalculateCapacityService.Holder capacityServiceHolder,
        final ClusterInfoService clusterInfoService
    ) {
        super(
            GetAutoscalingCapacityAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            GetAutoscalingCapacityAction.Request::new,
            indexNameExpressionResolver,
            GetAutoscalingCapacityAction.Response::new,
            ThreadPool.Names.SAME
        );
        this.capacityService = capacityServiceHolder.get();
        this.clusterInfoService = clusterInfoService;
        assert this.capacityService != null;
    }

    @Override
    protected void masterOperation(
        final Task task,
        final GetAutoscalingCapacityAction.Request request,
        final ClusterState state,
        final ActionListener<GetAutoscalingCapacityAction.Response> listener
    ) {
        listener.onResponse(
            new GetAutoscalingCapacityAction.Response(capacityService.calculate(state, clusterInfoService.getClusterInfo()))
        );
    }

    @Override
    protected ClusterBlockException checkBlock(final GetAutoscalingCapacityAction.Request request, final ClusterState state) {
        return null;
    }

}
