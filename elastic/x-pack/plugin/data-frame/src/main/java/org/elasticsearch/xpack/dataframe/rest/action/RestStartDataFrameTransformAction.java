/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.rest.action;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.dataframe.DataFrameField;
import org.elasticsearch.xpack.core.rollup.RollupField;
import org.elasticsearch.xpack.dataframe.action.StartDataFrameTransformAction;

import java.io.IOException;

public class RestStartDataFrameTransformAction extends BaseRestHandler {

    public RestStartDataFrameTransformAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(RestRequest.Method.POST, DataFrameField.REST_BASE_PATH_TRANSFORMS_BY_ID + "_start", this);
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        String id = restRequest.param(RollupField.ID.getPreferredName());
        StartDataFrameTransformAction.Request request = new StartDataFrameTransformAction.Request(id);

        return channel -> client.execute(StartDataFrameTransformAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }

    @Override
    public String getName() {
        return "data_frame_start_transform_action";
    }
}
