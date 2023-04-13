/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.search.action;

import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.application.EnterpriseSearch;
import org.elasticsearch.xpack.application.search.action.QuerySearchApplicationAction.Request;

import java.io.IOException;
import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.GET;
import static org.elasticsearch.rest.RestRequest.Method.POST;

public class RestQuerySearchApplicationAction extends BaseRestHandler {

    public static final String ENDPOINT_PATH = "/" + EnterpriseSearch.SEARCH_APPLICATION_API_ENDPOINT + "/{name}" + "/_search";

    @Override
    public String getName() {
        return "search_application_query_action";
    }

    @Override
    public List<Route> routes() {
        return List.of(new Route(GET, ENDPOINT_PATH), new Route(POST, ENDPOINT_PATH));
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        final String searchAppName = restRequest.param("name");
        Request request;
        if (restRequest.hasContent()) {
            request = Request.fromXContent(searchAppName, restRequest.contentParser());
        } else {
            request = new Request(searchAppName);
        }
        final Request finalRequest = request;
        return channel -> client.execute(QuerySearchApplicationAction.INSTANCE, finalRequest, new RestToXContentListener<>(channel));
    }
}
