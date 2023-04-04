/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.analytics.action;

import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.Strings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.application.EnterpriseSearch;

import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.GET;

public class RestGetAnalyticsCollectionAction extends BaseRestHandler {

    @Override
    public String getName() {
        return "get_analytics_collection_action";
    }

    @Override
    public List<Route> routes() {
        return List.of(
            new Route(GET, "/" + EnterpriseSearch.BEHAVIORAL_ANALYTICS_API_ENDPOINT + "/{collection_name}"),
            new Route(GET, "/" + EnterpriseSearch.BEHAVIORAL_ANALYTICS_API_ENDPOINT)
        );
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) {
        GetAnalyticsCollectionAction.Request request = new GetAnalyticsCollectionAction.Request(
            Strings.splitStringByCommaToArray(restRequest.param("collection_name"))
        );
        return channel -> client.execute(GetAnalyticsCollectionAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }
}
