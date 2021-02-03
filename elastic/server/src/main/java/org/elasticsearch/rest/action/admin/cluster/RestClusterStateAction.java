/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.rest.action.admin.cluster;

import org.elasticsearch.ElasticsearchTimeoutException;
import org.elasticsearch.action.ActionRunnable;
import org.elasticsearch.action.admin.cluster.state.ClusterStateAction;
import org.elasticsearch.action.admin.cluster.state.ClusterStateRequest;
import org.elasticsearch.action.admin.cluster.state.ClusterStateResponse;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.client.Requests;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.BytesRestResponse;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.rest.action.RestActionListener;
import org.elasticsearch.rest.action.RestBuilderListener;
import org.elasticsearch.rest.action.RestCancellableNodeClient;
import org.elasticsearch.tasks.TaskCancelledException;
import org.elasticsearch.threadpool.ThreadPool;

import java.io.IOException;
import java.util.Collections;
import java.util.EnumSet;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

import static java.util.Collections.singletonMap;
import static org.elasticsearch.rest.RestRequest.Method.GET;

public class RestClusterStateAction extends BaseRestHandler {

    private final SettingsFilter settingsFilter;

    private final ThreadPool threadPool;

    public RestClusterStateAction(SettingsFilter settingsFilter, ThreadPool threadPool) {
        this.settingsFilter = settingsFilter;
        this.threadPool = threadPool;
    }

    @Override
    public String getName() {
        return "cluster_state_action";
    }

    @Override
    public List<Route> routes() {
        return List.of(
            new Route(GET, "/_cluster/state"),
            new Route(GET, "/_cluster/state/{metric}"),
            new Route(GET, "/_cluster/state/{metric}/{indices}"));
    }

    @Override
    public boolean allowSystemIndexAccessByDefault() {
        return true;
    }

    @Override
    public RestChannelConsumer prepareRequest(final RestRequest request, final NodeClient client) throws IOException {
        final ClusterStateRequest clusterStateRequest = Requests.clusterStateRequest();
        clusterStateRequest.indicesOptions(IndicesOptions.fromRequest(request, clusterStateRequest.indicesOptions()));
        clusterStateRequest.local(request.paramAsBoolean("local", clusterStateRequest.local()));
        clusterStateRequest.masterNodeTimeout(request.paramAsTime("master_timeout", clusterStateRequest.masterNodeTimeout()));
        if (request.hasParam("wait_for_metadata_version")) {
            clusterStateRequest.waitForMetadataVersion(request.paramAsLong("wait_for_metadata_version", 0));
        }
        clusterStateRequest.waitForTimeout(request.paramAsTime("wait_for_timeout", ClusterStateRequest.DEFAULT_WAIT_FOR_NODE_TIMEOUT));

        final String[] indices = Strings.splitStringByCommaToArray(request.param("indices", "_all"));
        boolean isAllIndicesOnly = indices.length == 1 && "_all".equals(indices[0]);
        if (isAllIndicesOnly == false) {
            clusterStateRequest.indices(indices);
        }

        if (request.hasParam("metric")) {
            EnumSet<ClusterState.Metric> metrics = ClusterState.Metric.parseString(request.param("metric"), true);
            // do not ask for what we do not need.
            clusterStateRequest.nodes(metrics.contains(ClusterState.Metric.NODES) || metrics.contains(ClusterState.Metric.MASTER_NODE));
            /*
             * there is no distinction in Java api between routing_table and routing_nodes, it's the same info set over the wire, one single
             * flag to ask for it
             */
            clusterStateRequest.routingTable(
                    metrics.contains(ClusterState.Metric.ROUTING_TABLE) || metrics.contains(ClusterState.Metric.ROUTING_NODES));
            clusterStateRequest.metadata(metrics.contains(ClusterState.Metric.METADATA));
            clusterStateRequest.blocks(metrics.contains(ClusterState.Metric.BLOCKS));
            clusterStateRequest.customs(metrics.contains(ClusterState.Metric.CUSTOMS));
        }
        settingsFilter.addFilterSettingParams(request);

        return channel -> new RestCancellableNodeClient(client, request.getHttpChannel())
                .execute(ClusterStateAction.INSTANCE, clusterStateRequest, new RestActionListener<>(channel) {

                    private void ensureOpen() {
                        if (request.getHttpChannel().isOpen() == false) {
                            throw new TaskCancelledException("response channel [" + request.getHttpChannel() + "] closed");
                        }
                    }

                    @Override
                    protected void processResponse(ClusterStateResponse response) {
                        ensureOpen();
                        final long startTimeMs = threadPool.relativeTimeInMillis();
                        // Process serialization on MANAGEMENT pool since the serialization of the cluster state to XContent
                        // can be too slow to execute on an IO thread
                        threadPool.executor(ThreadPool.Names.MANAGEMENT).execute(
                                ActionRunnable.wrap(this, l -> new RestBuilderListener<ClusterStateResponse>(channel) {
                                    @Override
                                    public RestResponse buildResponse(final ClusterStateResponse response,
                                                                      final XContentBuilder builder) throws Exception {
                                        ensureOpen();
                                        if (clusterStateRequest.local() == false &&
                                                threadPool.relativeTimeInMillis() - startTimeMs >
                                                        clusterStateRequest.masterNodeTimeout().millis()) {
                                            throw new ElasticsearchTimeoutException("Timed out getting cluster state");
                                        }
                                        builder.startObject();
                                        if (clusterStateRequest.waitForMetadataVersion() != null) {
                                            builder.field(Fields.WAIT_FOR_TIMED_OUT, response.isWaitForTimedOut());
                                        }
                                        builder.field(Fields.CLUSTER_NAME, response.getClusterName().value());
                                        ToXContent.Params params = new ToXContent.DelegatingMapParams(
                                                singletonMap(Metadata.CONTEXT_MODE_PARAM, Metadata.CONTEXT_MODE_API), request);
                                        final ClusterState responseState = response.getState();
                                        if (responseState != null) {
                                            responseState.toXContent(builder, params);
                                        }
                                        builder.endObject();
                                        return new BytesRestResponse(RestStatus.OK, builder);
                                    }
                                }.onResponse(response)));
                    }
                });
    }

    private static final Set<String> RESPONSE_PARAMS;

    static {
        final Set<String> responseParams = new HashSet<>();
        responseParams.add("metric");
        responseParams.addAll(Settings.FORMAT_PARAMS);
        RESPONSE_PARAMS = Collections.unmodifiableSet(responseParams);
    }

    @Override
    protected Set<String> responseParams() {
        return RESPONSE_PARAMS;
    }

    @Override
    public boolean canTripCircuitBreaker() {
        return false;
    }

    static final class Fields {
        static final String WAIT_FOR_TIMED_OUT = "wait_for_timed_out";
        static final String CLUSTER_NAME = "cluster_name";
    }

}
