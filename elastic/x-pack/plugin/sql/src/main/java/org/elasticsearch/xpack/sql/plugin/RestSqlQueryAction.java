/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.sql.plugin;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.xcontent.MediaType;
import org.elasticsearch.common.xcontent.MediaTypeRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestCancellableNodeClient;
import org.elasticsearch.xpack.sql.action.SqlQueryAction;
import org.elasticsearch.xpack.sql.action.SqlQueryRequest;
import org.elasticsearch.xpack.sql.proto.Protocol;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Set;

import static org.elasticsearch.rest.RestRequest.Method.GET;
import static org.elasticsearch.rest.RestRequest.Method.POST;
import static org.elasticsearch.xpack.sql.proto.Protocol.URL_PARAM_DELIMITER;

public class RestSqlQueryAction extends BaseRestHandler {

    @Override
    public List<Route> routes() {
        return List.of(
            new Route(GET, Protocol.SQL_QUERY_REST_ENDPOINT),
            new Route(POST, Protocol.SQL_QUERY_REST_ENDPOINT));
    }

    public MediaTypeRegistry<? extends MediaType> validAcceptMediaTypes() {
        return SqlMediaTypeParser.MEDIA_TYPE_REGISTRY;
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest request, NodeClient client)
            throws IOException {
        SqlQueryRequest sqlRequest;
        try (XContentParser parser = request.contentOrSourceParamParser()) {
            sqlRequest = SqlQueryRequest.fromXContent(parser);
        }

        return channel -> {
            RestCancellableNodeClient cancellableClient = new RestCancellableNodeClient(client, request.getHttpChannel());
            cancellableClient.execute(SqlQueryAction.INSTANCE, sqlRequest, new SqlResponseListener(channel, request, sqlRequest));
        };
    }

    @Override
    protected Set<String> responseParams() {
        return Collections.singleton(URL_PARAM_DELIMITER);
    }

    @Override
    public String getName() {
        return "sql_query";
    }
}
