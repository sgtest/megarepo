/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.rest.filter;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestStatusToXContentListener;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.core.ml.action.GetFiltersAction;
import org.elasticsearch.xpack.core.ml.action.util.PageParams;
import org.elasticsearch.xpack.core.ml.job.config.MlFilter;

import java.io.IOException;

public class RestGetFiltersAction extends BaseRestHandler {

    public RestGetFiltersAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(RestRequest.Method.GET, MachineLearning.BASE_PATH + "filters/{" + MlFilter.ID.getPreferredName() + "}",
                this);
        controller.registerHandler(RestRequest.Method.GET, MachineLearning.BASE_PATH + "filters/", this);
    }

    @Override
    public String getName() {
        return "xpack_ml_get_filters_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        GetFiltersAction.Request getListRequest = new GetFiltersAction.Request();
        String filterId = restRequest.param(MlFilter.ID.getPreferredName());
        if (!Strings.isNullOrEmpty(filterId)) {
            getListRequest.setFilterId(filterId);
        }
        if (restRequest.hasParam(PageParams.FROM.getPreferredName()) || restRequest.hasParam(PageParams.SIZE.getPreferredName())) {
            getListRequest.setPageParams(
                    new PageParams(restRequest.paramAsInt(PageParams.FROM.getPreferredName(), PageParams.DEFAULT_FROM),
                    restRequest.paramAsInt(PageParams.SIZE.getPreferredName(), PageParams.DEFAULT_SIZE)));
        }
        return channel -> client.execute(GetFiltersAction.INSTANCE, getListRequest, new RestStatusToXContentListener<>(channel));
    }

}
