/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.transform.rest.action;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.transform.TransformField;
import org.elasticsearch.xpack.core.transform.action.PreviewTransformAction;

import java.io.IOException;
import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.POST;

public class RestPreviewTransformAction extends BaseRestHandler {

    @Override
    public List<Route> routes() {
        return List.of(new Route(POST, TransformField.REST_BASE_PATH_TRANSFORMS + "_preview"));
    }

    @Override
    public String getName() {
        return "transform_preview_transform_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        XContentParser parser = restRequest.contentParser();

        PreviewTransformAction.Request request = PreviewTransformAction.Request.fromXContent(parser);
        return channel -> client.execute(PreviewTransformAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }
}
