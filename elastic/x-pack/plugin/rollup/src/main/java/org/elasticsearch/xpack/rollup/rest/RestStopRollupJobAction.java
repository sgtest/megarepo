/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.rollup.rest;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.rollup.RollupField;
import org.elasticsearch.xpack.rollup.Rollup;
import org.elasticsearch.xpack.core.rollup.action.StopRollupJobAction;

import java.io.IOException;

public class RestStopRollupJobAction extends BaseRestHandler {

    public RestStopRollupJobAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(RestRequest.Method.POST, Rollup.BASE_PATH +  "job/{id}/_stop", this);
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        String id = restRequest.param(RollupField.ID.getPreferredName());
        StopRollupJobAction.Request request = new StopRollupJobAction.Request(id);

        return channel -> client.execute(StopRollupJobAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }

    @Override
    public String getName() {
        return "rollup_stop_job_action";
    }
}
