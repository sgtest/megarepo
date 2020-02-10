/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.rest.inference;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.ml.action.PutTrainedModelAction;
import org.elasticsearch.xpack.core.ml.inference.TrainedModelConfig;
import org.elasticsearch.xpack.ml.MachineLearning;

import java.io.IOException;
import java.util.List;

import static java.util.Collections.singletonList;
import static org.elasticsearch.rest.RestRequest.Method.PUT;

public class RestPutTrainedModelAction extends BaseRestHandler {

    @Override
    public List<Route> routes() {
        return singletonList(
            new Route(PUT, MachineLearning.BASE_PATH + "inference/{" + TrainedModelConfig.MODEL_ID.getPreferredName() + "}"));
    }

    @Override
    public String getName() {
        return "xpack_ml_put_trained_model_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        String id = restRequest.param(TrainedModelConfig.MODEL_ID.getPreferredName());
        XContentParser parser = restRequest.contentParser();
        PutTrainedModelAction.Request putRequest = PutTrainedModelAction.Request.parseRequest(id, parser);
        putRequest.timeout(restRequest.paramAsTime("timeout", putRequest.timeout()));

        return channel -> client.execute(PutTrainedModelAction.INSTANCE, putRequest, new RestToXContentListener<>(channel));
    }
}
