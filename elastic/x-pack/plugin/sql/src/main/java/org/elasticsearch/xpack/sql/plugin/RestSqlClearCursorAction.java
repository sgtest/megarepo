/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plugin;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;

import java.io.IOException;

import static org.elasticsearch.rest.RestRequest.Method.POST;
import static org.elasticsearch.xpack.sql.plugin.SqlClearCursorAction.REST_ENDPOINT;

public class RestSqlClearCursorAction extends BaseRestHandler {
    public RestSqlClearCursorAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(POST, REST_ENDPOINT, this);
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest request, NodeClient client) throws IOException {
        SqlClearCursorRequest sqlRequest;
        try (XContentParser parser = request.contentOrSourceParamParser()) {
            sqlRequest = SqlClearCursorRequest.fromXContent(parser, AbstractSqlRequest.Mode.fromString(request.param("mode")));
        }
        return channel -> client.executeLocally(SqlClearCursorAction.INSTANCE, sqlRequest, new RestToXContentListener<>(channel));
    }

    @Override
    public String getName() {
        return "sql_translate_action";
    }
}