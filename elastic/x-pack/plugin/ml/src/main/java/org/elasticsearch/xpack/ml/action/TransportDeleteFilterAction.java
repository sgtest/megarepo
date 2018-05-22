/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.bulk.BulkAction;
import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.delete.DeleteRequest;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.action.DeleteFilterAction;
import org.elasticsearch.xpack.core.ml.MlMetaIndex;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.job.config.Detector;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.MlFilter;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

public class TransportDeleteFilterAction extends HandledTransportAction<DeleteFilterAction.Request, DeleteFilterAction.Response> {

    private final Client client;
    private final ClusterService clusterService;

    @Inject
    public TransportDeleteFilterAction(Settings settings, ThreadPool threadPool,
                                       TransportService transportService, ActionFilters actionFilters,
                                       IndexNameExpressionResolver indexNameExpressionResolver,
                                       ClusterService clusterService, Client client) {
        super(settings, DeleteFilterAction.NAME, threadPool, transportService, actionFilters,
                indexNameExpressionResolver, DeleteFilterAction.Request::new);
        this.clusterService = clusterService;
        this.client = client;
    }

    @Override
    protected void doExecute(DeleteFilterAction.Request request, ActionListener<DeleteFilterAction.Response> listener) {

        final String filterId = request.getFilterId();
        ClusterState state = clusterService.state();
        Map<String, Job> jobs = MlMetadata.getMlMetadata(state).getJobs();
        List<String> currentlyUsedBy = new ArrayList<>();
        for (Job job : jobs.values()) {
            List<Detector> detectors = job.getAnalysisConfig().getDetectors();
            for (Detector detector : detectors) {
                if (detector.extractReferencedFilters().contains(filterId)) {
                    currentlyUsedBy.add(job.getId());
                    break;
                }
            }
        }
        if (!currentlyUsedBy.isEmpty()) {
            throw ExceptionsHelper.conflictStatusException("Cannot delete filter, currently used by jobs: "
                    + currentlyUsedBy);
        }

        DeleteRequest deleteRequest = new DeleteRequest(MlMetaIndex.INDEX_NAME, MlMetaIndex.TYPE, MlFilter.documentId(filterId));
        BulkRequestBuilder bulkRequestBuilder = client.prepareBulk();
        bulkRequestBuilder.add(deleteRequest);
        bulkRequestBuilder.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        executeAsyncWithOrigin(client, ML_ORIGIN, BulkAction.INSTANCE, bulkRequestBuilder.request(),
                new ActionListener<BulkResponse>() {
                    @Override
                    public void onResponse(BulkResponse bulkResponse) {
                        if (bulkResponse.getItems()[0].status() == RestStatus.NOT_FOUND) {
                            listener.onFailure(new ResourceNotFoundException("Could not delete filter with ID [" + filterId
                                    + "] because it does not exist"));
                        } else {
                            listener.onResponse(new DeleteFilterAction.Response(true));
                        }
                    }

                    @Override
                    public void onFailure(Exception e) {
                        listener.onFailure(ExceptionsHelper.serverError("Could not delete filter with ID [" + filterId + "]", e));
                    }
        });
    }
}
