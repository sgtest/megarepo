/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.dlm.rest;

import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.Strings;
import org.elasticsearch.dlm.action.ExplainDataLifecycleAction;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.Scope;
import org.elasticsearch.rest.ServerlessScope;
import org.elasticsearch.rest.action.RestChunkedToXContentListener;

import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.GET;

@ServerlessScope(Scope.PUBLIC)
public class RestExplainDataLifecycleAction extends BaseRestHandler {

    @Override
    public String getName() {
        return "dlm_explain_action";
    }

    @Override
    public List<Route> routes() {
        return List.of(new Route(GET, "/{index}/_lifecycle/explain"));
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) {
        String[] indices = Strings.splitStringByCommaToArray(restRequest.param("index"));
        ExplainDataLifecycleAction.Request explainRequest = new ExplainDataLifecycleAction.Request(indices);
        explainRequest.includeDefaults(restRequest.paramAsBoolean("include_defaults", false));
        explainRequest.indicesOptions(IndicesOptions.fromRequest(restRequest, IndicesOptions.strictExpandOpen()));
        String masterNodeTimeout = restRequest.param("master_timeout");
        if (masterNodeTimeout != null) {
            explainRequest.masterNodeTimeout(masterNodeTimeout);
        }
        return channel -> client.execute(ExplainDataLifecycleAction.INSTANCE, explainRequest, new RestChunkedToXContentListener<>(channel));
    }

    @Override
    public boolean allowSystemIndexAccessByDefault() {
        return true;
    }
}
