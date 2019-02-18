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
import org.elasticsearch.xpack.dataframe.action.GetDataFrameTransformsAction;

public class RestGetDataFrameTransformsAction extends BaseRestHandler {

    public RestGetDataFrameTransformsAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(RestRequest.Method.GET, DataFrameField.REST_BASE_PATH_TRANSFORMS_BY_ID, this);
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) {
        String id = restRequest.param(DataFrameField.ID.getPreferredName());
        GetDataFrameTransformsAction.Request request = new GetDataFrameTransformsAction.Request(id);
        return channel -> client.execute(GetDataFrameTransformsAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }

    @Override
    public String getName() {
        return "data_frame_get_transforms_action";
    }
}
