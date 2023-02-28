/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.cluster.remote;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListenerResponseHandler;
import org.elasticsearch.action.admin.cluster.node.info.NodeInfo;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoAction;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoRequest;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoResponse;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.BoundTransportAddress;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportInfo;
import org.elasticsearch.transport.TransportService;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.equalTo;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class RemoteClusterNodesActionTests extends ESTestCase {

    public void testDoExecute() {
        final ThreadPool threadPool = mock(ThreadPool.class);
        final ThreadContext threadContext = new ThreadContext(Settings.EMPTY);
        when(threadPool.getThreadContext()).thenReturn(threadContext);

        final TransportService transportService = mock(TransportService.class);
        final DiscoveryNode localNode = mock(DiscoveryNode.class);
        when(transportService.getLocalNode()).thenReturn(localNode);
        when(transportService.getThreadPool()).thenReturn(threadPool);

        // Prepare nodesInfo response
        final int numberOfNodes = randomIntBetween(1, 6);
        final List<NodeInfo> nodeInfos = new ArrayList<>();
        final Set<DiscoveryNode> expectedRemoteServerNodes = new HashSet<>();
        for (int i = 0; i < numberOfNodes; i++) {
            final DiscoveryNode node = randomNode(i);
            final boolean remoteServerEnabled = randomBoolean();
            final Settings.Builder settingsBuilder = Settings.builder();
            final Map<String, BoundTransportAddress> profileAddresses = new HashMap<>();
            final TransportAddress remoteClusterProfileAddress = buildNewFakeTransportAddress();
            if (remoteServerEnabled) {
                expectedRemoteServerNodes.add(node.withTransportAddress(remoteClusterProfileAddress));
                profileAddresses.put(
                    "_remote_cluster",
                    new BoundTransportAddress(new TransportAddress[] { remoteClusterProfileAddress }, remoteClusterProfileAddress)
                );
                settingsBuilder.put("remote_cluster_server.enabled", true);
            } else {
                // By default remote cluster server is disabled, we randomly disable it explicitly
                if (randomBoolean()) {
                    settingsBuilder.put("remote_cluster_server.enabled", false);
                }
                if (randomBoolean()) {
                    // randomly add a _remote_cluster profile when remote_cluster_server is disabled. This will just be a normal profile
                    // and this node won't be reported as remote cluster server node
                    profileAddresses.put(
                        "_remote_cluster",
                        new BoundTransportAddress(new TransportAddress[] { remoteClusterProfileAddress }, remoteClusterProfileAddress)
                    );
                }
            }
            nodeInfos.add(
                new NodeInfo(
                    Version.CURRENT,
                    null,
                    node,
                    settingsBuilder.build(),
                    null,
                    null,
                    null,
                    null,
                    new TransportInfo(
                        new BoundTransportAddress(new TransportAddress[] { node.getAddress() }, node.getAddress()),
                        profileAddresses,
                        false
                    ),
                    null,
                    null,
                    null,
                    null,
                    null
                )
            );
        }

        final NodesInfoResponse nodesInfoResponse = new NodesInfoResponse(
            new ClusterName(randomAlphaOfLengthBetween(3, 8)),
            nodeInfos,
            List.of()
        );

        doAnswer(invocation -> {
            final NodesInfoRequest nodesInfoRequest = invocation.getArgument(2);
            assertThat(
                nodesInfoRequest.requestedMetrics(),
                containsInAnyOrder(NodesInfoRequest.Metric.SETTINGS.metricName(), NodesInfoRequest.Metric.TRANSPORT.metricName())
            );
            final ActionListenerResponseHandler<NodesInfoResponse> handler = invocation.getArgument(3);
            handler.handleResponse(nodesInfoResponse);
            return null;
        }).when(transportService).sendRequest(eq(localNode), eq(NodesInfoAction.NAME), any(NodesInfoRequest.class), any());

        final RemoteClusterNodesAction.TransportAction action = new RemoteClusterNodesAction.TransportAction(
            transportService,
            mock(ActionFilters.class)
        );

        final PlainActionFuture<RemoteClusterNodesAction.Response> future = new PlainActionFuture<>();
        action.doExecute(mock(Task.class), RemoteClusterNodesAction.Request.INSTANCE, future);

        final List<DiscoveryNode> actualNodes = future.actionGet().getNodes();
        assertThat(Set.copyOf(actualNodes), equalTo(expectedRemoteServerNodes));
        assertThat(
            actualNodes.stream().map(DiscoveryNode::getAddress).collect(Collectors.toUnmodifiableSet()),
            equalTo(expectedRemoteServerNodes.stream().map(DiscoveryNode::getAddress).collect(Collectors.toUnmodifiableSet()))
        );
    }

    private DiscoveryNode randomNode(final int id) {
        return new DiscoveryNode("node-" + id, Integer.toString(id), buildNewFakeTransportAddress(), Map.of(), Set.of(), Version.CURRENT);
    }

}
