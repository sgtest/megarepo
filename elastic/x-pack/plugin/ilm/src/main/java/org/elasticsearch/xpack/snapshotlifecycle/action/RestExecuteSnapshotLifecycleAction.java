/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.snapshotlifecycle.action;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.snapshotlifecycle.action.ExecuteSnapshotLifecycleAction;

import java.io.IOException;

public class RestExecuteSnapshotLifecycleAction extends BaseRestHandler {

    public RestExecuteSnapshotLifecycleAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(RestRequest.Method.PUT, "/_slm/policy/{name}/_execute", this);
    }

    @Override
    public String getName() {
        return "slm_execute_lifecycle";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest request, NodeClient client) throws IOException {
        String snapLifecycleId = request.param("name");
        ExecuteSnapshotLifecycleAction.Request req = new ExecuteSnapshotLifecycleAction.Request(snapLifecycleId);
        req.timeout(request.paramAsTime("timeout", req.timeout()));
        req.masterNodeTimeout(request.paramAsTime("master_timeout", req.masterNodeTimeout()));
        return channel -> client.execute(ExecuteSnapshotLifecycleAction.INSTANCE, req, new RestToXContentListener<>(channel));
    }
}
