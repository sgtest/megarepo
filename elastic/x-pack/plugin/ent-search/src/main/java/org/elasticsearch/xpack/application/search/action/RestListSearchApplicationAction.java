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
import org.elasticsearch.xpack.core.action.util.PageParams;

import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.GET;

public class RestListSearchApplicationAction extends BaseRestHandler {

    @Override
    public String getName() {
        return "search_application_list_action";
    }

    @Override
    public List<Route> routes() {
        return List.of(new Route(GET, "/" + EnterpriseSearch.SEARCH_APPLICATION_API_ENDPOINT));
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) {

        int from = restRequest.paramAsInt("from", PageParams.DEFAULT_FROM);
        int size = restRequest.paramAsInt("size", PageParams.DEFAULT_SIZE);
        ListSearchApplicationAction.Request request = new ListSearchApplicationAction.Request(
            restRequest.param("q"),
            new PageParams(from, size)
        );

        return channel -> client.execute(ListSearchApplicationAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }
}
