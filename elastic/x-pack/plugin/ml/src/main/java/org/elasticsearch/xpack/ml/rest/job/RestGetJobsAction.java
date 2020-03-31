/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.rest.job;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.ml.action.GetJobsAction;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.ml.MachineLearning;

import java.io.IOException;
import java.util.Collections;
import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.GET;

public class RestGetJobsAction extends BaseRestHandler {

    @Override
    public List<Route> routes() {
        return Collections.emptyList();
    }

    @Override
    public List<ReplacedRoute> replacedRoutes() {
        // TODO: remove deprecated endpoint in 8.0.0
        return List.of(
            new ReplacedRoute(GET, MachineLearning.BASE_PATH + "anomaly_detectors/{" + Job.ID.getPreferredName() + "}",
                GET, MachineLearning.PRE_V7_BASE_PATH + "anomaly_detectors/{" + Job.ID.getPreferredName() + "}"),
            new ReplacedRoute(GET, MachineLearning.BASE_PATH + "anomaly_detectors",
                GET, MachineLearning.PRE_V7_BASE_PATH + "anomaly_detectors")
        );
    }

    @Override
    public String getName() {
        return "ml_get_jobs_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        String jobId = restRequest.param(Job.ID.getPreferredName());
        if (Strings.isNullOrEmpty(jobId)) {
            jobId = Metadata.ALL;
        }
        GetJobsAction.Request request = new GetJobsAction.Request(jobId);
        request.setAllowNoJobs(restRequest.paramAsBoolean(GetJobsAction.Request.ALLOW_NO_JOBS.getPreferredName(), request.allowNoJobs()));
        return channel -> client.execute(GetJobsAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }
}
