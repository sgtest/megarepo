/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.cluster.remote;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionListenerResponseHandler;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoAction;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoRequest;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoResponse;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.transport.BoundTransportAddress;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.transport.TransportInfo;
import org.elasticsearch.transport.TransportService;

import java.io.IOException;
import java.util.List;
import java.util.Map;
import java.util.Objects;

import static org.elasticsearch.transport.RemoteClusterPortSettings.REMOTE_CLUSTER_PROFILE;
import static org.elasticsearch.transport.RemoteClusterPortSettings.REMOTE_CLUSTER_SERVER_ENABLED;

public class RemoteClusterNodesAction extends ActionType<RemoteClusterNodesAction.Response> {

    public static final RemoteClusterNodesAction INSTANCE = new RemoteClusterNodesAction();
    public static final String NAME = "cluster:internal/remote_cluster/nodes";

    public RemoteClusterNodesAction() {
        super(NAME, RemoteClusterNodesAction.Response::new);
    }

    public static class Request extends ActionRequest {

        public static final Request INSTANCE = new Request();

        public Request() {}

        public Request(StreamInput in) throws IOException {
            super(in);
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }
    }

    public static class Response extends ActionResponse {

        private final List<DiscoveryNode> nodes;

        public Response(List<DiscoveryNode> nodes) {
            this.nodes = nodes;
        }

        public Response(StreamInput in) throws IOException {
            super(in);
            this.nodes = in.readList(DiscoveryNode::new);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeList(nodes);
        }

        public List<DiscoveryNode> getNodes() {
            return nodes;
        }
    }

    public static class TransportAction extends HandledTransportAction<Request, Response> {

        private final TransportService transportService;

        @Inject
        public TransportAction(TransportService transportService, ActionFilters actionFilters) {
            super(RemoteClusterNodesAction.NAME, transportService, actionFilters, Request::new);
            this.transportService = transportService;
        }

        @Override
        protected void doExecute(Task task, Request request, ActionListener<Response> listener) {
            final NodesInfoRequest nodesInfoRequest = new NodesInfoRequest();
            nodesInfoRequest.clear();
            nodesInfoRequest.addMetrics(NodesInfoRequest.Metric.SETTINGS.metricName(), NodesInfoRequest.Metric.TRANSPORT.metricName());
            final ThreadContext threadContext = transportService.getThreadPool().getThreadContext();
            try (var ignore = threadContext.stashContext()) {
                threadContext.markAsSystemContext();
                transportService.sendRequest(
                    transportService.getLocalNode(),
                    NodesInfoAction.NAME,
                    nodesInfoRequest,
                    new ActionListenerResponseHandler<>(ActionListener.wrap(response -> {
                        final List<DiscoveryNode> remoteClusterNodes = response.getNodes().stream().map(nodeInfo -> {
                            if (false == REMOTE_CLUSTER_SERVER_ENABLED.get(nodeInfo.getSettings())) {
                                return null;
                            }
                            final Map<String, BoundTransportAddress> profileAddresses = nodeInfo.getInfo(TransportInfo.class)
                                .getProfileAddresses();
                            final BoundTransportAddress remoteClusterServerAddress = profileAddresses.get(REMOTE_CLUSTER_PROFILE);
                            assert remoteClusterServerAddress != null
                                : "remote cluster server is enabled but corresponding transport profile is missing";
                            return nodeInfo.getNode().withTransportAddress(remoteClusterServerAddress.publishAddress());
                        }).filter(Objects::nonNull).toList();
                        listener.onResponse(new Response(remoteClusterNodes));
                    }, listener::onFailure), NodesInfoResponse::new)
                );
            }
        }
    }
}
