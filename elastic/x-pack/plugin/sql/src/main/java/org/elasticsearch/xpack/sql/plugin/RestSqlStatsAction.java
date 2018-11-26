/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.sql.plugin;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestActions;
import org.elasticsearch.xpack.sql.proto.Protocol;

import java.io.IOException;

import static org.elasticsearch.rest.RestRequest.Method.GET;

public class RestSqlStatsAction extends BaseRestHandler {

    protected RestSqlStatsAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(GET, Protocol.SQL_STATS_REST_ENDPOINT, this);
    }

    @Override
    public String getName() {
        return "xpack_sql_stats_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        SqlStatsRequest request = new SqlStatsRequest();
        return channel -> client.execute(SqlStatsAction.INSTANCE, request, new RestActions.NodesResponseRestListener<>(channel));
    }

}
