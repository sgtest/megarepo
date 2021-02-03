/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.rest.action.admin.indices;

import org.elasticsearch.action.admin.indices.template.delete.DeleteComponentTemplateAction;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;

import java.io.IOException;
import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.DELETE;

public class RestDeleteComponentTemplateAction extends BaseRestHandler {

    @Override
    public List<Route> routes() {
        return List.of(new Route(DELETE, "/_component_template/{name}"));
    }

    @Override
    public String getName() {
        return "delete_component_template_action";
    }

    @Override
    public RestChannelConsumer prepareRequest(final RestRequest request, final NodeClient client) throws IOException {

        DeleteComponentTemplateAction.Request deleteReq = new DeleteComponentTemplateAction.Request(request.param("name"));
        deleteReq.masterNodeTimeout(request.paramAsTime("master_timeout", deleteReq.masterNodeTimeout()));

        return channel -> client.execute(DeleteComponentTemplateAction.INSTANCE, deleteReq, new RestToXContentListener<>(channel));
    }
}
