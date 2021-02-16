/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.rest.action.admin.indices;

import org.elasticsearch.action.admin.indices.segments.IndicesSegmentsRequest;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.Strings;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.DispatchingRestToXContentListener;
import org.elasticsearch.rest.action.RestCancellableNodeClient;
import org.elasticsearch.threadpool.ThreadPool;

import java.io.IOException;
import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.GET;

public class RestIndicesSegmentsAction extends BaseRestHandler {

    private final ThreadPool threadPool;

    public RestIndicesSegmentsAction(ThreadPool threadPool) {
        this.threadPool = threadPool;
    }

    @Override
    public List<Route> routes() {
        return List.of(
            new Route(GET, "/_segments"),
            new Route(GET, "/{index}/_segments"));
    }

    @Override
    public String getName() {
        return "indices_segments_action";
    }

    @Override
    public RestChannelConsumer prepareRequest(final RestRequest request, final NodeClient client) throws IOException {
        IndicesSegmentsRequest indicesSegmentsRequest = new IndicesSegmentsRequest(
                Strings.splitStringByCommaToArray(request.param("index")));
        indicesSegmentsRequest.verbose(request.paramAsBoolean("verbose", false));
        indicesSegmentsRequest.indicesOptions(IndicesOptions.fromRequest(request, indicesSegmentsRequest.indicesOptions()));
        return channel -> new RestCancellableNodeClient(client, request.getHttpChannel()).admin().indices().segments(
                indicesSegmentsRequest,
                new DispatchingRestToXContentListener<>(threadPool.executor(ThreadPool.Names.MANAGEMENT), channel, request));
    }
}
