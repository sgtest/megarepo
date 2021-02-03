/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.search;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestStatusToXContentListener;
import org.elasticsearch.xpack.core.async.GetAsyncStatusRequest;
import org.elasticsearch.xpack.core.search.action.GetAsyncStatusAction;

import java.util.List;

import static java.util.Arrays.asList;
import static java.util.Collections.unmodifiableList;
import static org.elasticsearch.rest.RestRequest.Method.GET;

public class RestGetAsyncStatusAction extends BaseRestHandler  {
    @Override
    public List<Route> routes() {
        return unmodifiableList(asList(new Route(GET, "/_async_search/status/{id}")));
    }


    @Override
    public String getName() {
        return "async_search_status_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest request, NodeClient client) {
        GetAsyncStatusRequest statusRequest = new GetAsyncStatusRequest(request.param("id"));
        return channel -> client.execute(GetAsyncStatusAction.INSTANCE, statusRequest, new RestStatusToXContentListener<>(channel));
    }
}
