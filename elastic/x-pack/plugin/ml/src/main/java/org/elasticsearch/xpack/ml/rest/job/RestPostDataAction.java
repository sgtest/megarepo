/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.rest.job;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestStatusToXContentListener;
import org.elasticsearch.xpack.core.ml.action.PostDataAction;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.ml.MachineLearning;

import java.io.IOException;
import java.util.Collections;
import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.POST;

public class RestPostDataAction extends BaseRestHandler {

    private static final String DEFAULT_RESET_START = "";
    private static final String DEFAULT_RESET_END = "";

    @Override
    public List<Route> routes() {
        return Collections.emptyList();
    }

    @Override
    public List<DeprecatedRoute> deprecatedRoutes() {
        return Collections.singletonList(
            new DeprecatedRoute(POST, MachineLearning.BASE_PATH + "anomaly_detectors/{" + Job.ID.getPreferredName() + "}/_data",
            "Posting data directly to anomaly detection jobs is deprecated, " +
                "in a future major version it will be compulsory to use a datafeed"));
    }

    @Override
    public String getName() {
        return "ml_post_data_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        PostDataAction.Request request = new PostDataAction.Request(restRequest.param(Job.ID.getPreferredName()));
        request.setResetStart(restRequest.param(PostDataAction.Request.RESET_START.getPreferredName(), DEFAULT_RESET_START));
        request.setResetEnd(restRequest.param(PostDataAction.Request.RESET_END.getPreferredName(), DEFAULT_RESET_END));
        request.setContent(restRequest.content(), restRequest.getXContentType());

        return channel -> client.execute(PostDataAction.INSTANCE, request, new RestStatusToXContentListener<>(channel));
    }

    @Override
    public boolean supportsContentStream() {
        return true;
    }
}
