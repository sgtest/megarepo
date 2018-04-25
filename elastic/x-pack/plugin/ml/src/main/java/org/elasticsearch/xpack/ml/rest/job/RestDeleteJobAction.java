/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.rest.job;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.ml.action.CloseJobAction;
import org.elasticsearch.xpack.core.ml.action.DeleteJobAction;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.ml.MachineLearning;

import java.io.IOException;

public class RestDeleteJobAction extends BaseRestHandler {

    public RestDeleteJobAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(RestRequest.Method.DELETE, MachineLearning.BASE_PATH
                + "anomaly_detectors/{" + Job.ID.getPreferredName() + "}", this);
    }

    @Override
    public String getName() {
        return "xpack_ml_delete_job_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        DeleteJobAction.Request deleteJobRequest = new DeleteJobAction.Request(restRequest.param(Job.ID.getPreferredName()));
        deleteJobRequest.setForce(restRequest.paramAsBoolean(CloseJobAction.Request.FORCE.getPreferredName(), deleteJobRequest.isForce()));
        deleteJobRequest.timeout(restRequest.paramAsTime("timeout", deleteJobRequest.timeout()));
        deleteJobRequest.masterNodeTimeout(restRequest.paramAsTime("master_timeout", deleteJobRequest.masterNodeTimeout()));
        return channel -> client.execute(DeleteJobAction.INSTANCE, deleteJobRequest, new RestToXContentListener<>(channel));
    }
}
