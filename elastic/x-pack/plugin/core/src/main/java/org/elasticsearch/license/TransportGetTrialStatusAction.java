/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.license;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeReadAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.io.IOException;

public class TransportGetTrialStatusAction extends TransportMasterNodeReadAction<GetTrialStatusRequest, GetTrialStatusResponse> {

    @Inject
    public TransportGetTrialStatusAction(TransportService transportService, ClusterService clusterService, ThreadPool threadPool,
                                         ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver) {
        super(GetTrialStatusAction.NAME, transportService, clusterService, threadPool, actionFilters,
                GetTrialStatusRequest::new, indexNameExpressionResolver);
    }

    @Override
    protected String executor() {
        return ThreadPool.Names.SAME;
    }

    @Override
    protected GetTrialStatusResponse read(StreamInput in) throws IOException {
        return new GetTrialStatusResponse(in);
    }

    @Override
    protected void masterOperation(Task task, GetTrialStatusRequest request, ClusterState state,
                                   ActionListener<GetTrialStatusResponse> listener) throws Exception {
        LicensesMetadata licensesMetadata = state.metadata().custom(LicensesMetadata.TYPE);
        listener.onResponse(new GetTrialStatusResponse(licensesMetadata == null || licensesMetadata.isEligibleForTrial()));

    }

    @Override
    protected ClusterBlockException checkBlock(GetTrialStatusRequest request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_READ);
    }
}
