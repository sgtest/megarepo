/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.coordination;

import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.disruption.BlockClusterStateProcessing;
import org.elasticsearch.threadpool.Scheduler;
import org.junit.Before;

import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.emptyOrNullString;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;

@ESIntegTestCase.ClusterScope(scope = ESIntegTestCase.Scope.TEST, numDataNodes = 0, autoManageMasterNodes = false)
public class CoordinationDiagnosticsServiceIT extends ESIntegTestCase {
    @Before
    private void setBootstrapMasterNodeIndex() {
        internalCluster().setBootstrapMasterNodeIndex(0);
    }

    public void testBlockClusterStateProcessingOnOneNode() throws Exception {
        /*
         * This test picks a node that is not elected master, and then blocks cluster state processing on it. The reason is so that we
         * can call CoordinationDiagnosticsService#beginPollingClusterFormationInfo without a cluster changed event resulting in the
         * values we pass in being overwritten.
         */
        final List<String> nodeNames = internalCluster().startNodes(3);

        final String master = internalCluster().getMasterName();
        assertThat(nodeNames, hasItem(master));
        String blockedNode = nodeNames.stream().filter(n -> n.equals(master) == false).findAny().get();
        assertNotNull(blockedNode);
        ensureStableCluster(3);

        DiscoveryNodes discoveryNodes = internalCluster().getInstance(ClusterService.class, master).state().nodes();
        Set<DiscoveryNode> nodesWithoutBlockedNode = discoveryNodes.getNodes()
            .values()
            .stream()
            .filter(n -> n.getName().equals(blockedNode) == false)
            .collect(Collectors.toSet());

        BlockClusterStateProcessing disruption = new BlockClusterStateProcessing(blockedNode, random());
        internalCluster().setDisruptionScheme(disruption);
        // stop processing cluster state changes
        disruption.startDisrupting();

        CoordinationDiagnosticsService diagnosticsOnBlockedNode = internalCluster().getInstance(
            CoordinationDiagnosticsService.class,
            blockedNode
        );
        ConcurrentMap<DiscoveryNode, CoordinationDiagnosticsService.ClusterFormationStateOrException> nodeToClusterFormationStateMap =
            new ConcurrentHashMap<>();
        ConcurrentHashMap<DiscoveryNode, Scheduler.Cancellable> cancellables = new ConcurrentHashMap<>();
        diagnosticsOnBlockedNode.clusterFormationResponses = nodeToClusterFormationStateMap;
        diagnosticsOnBlockedNode.clusterFormationInfoTasks = cancellables;

        diagnosticsOnBlockedNode.remoteRequestInitialDelay = TimeValue.ZERO;
        diagnosticsOnBlockedNode.beginPollingClusterFormationInfo(
            nodesWithoutBlockedNode,
            nodeToClusterFormationStateMap::put,
            cancellables
        );

        // while the node is blocked from processing cluster state changes it should reach out to the other 2
        // master eligible nodes and get a successful response
        assertBusy(() -> {
            assertThat(cancellables.size(), is(2));
            assertThat(nodeToClusterFormationStateMap.size(), is(2));
            nodesWithoutBlockedNode.forEach(node -> {
                CoordinationDiagnosticsService.ClusterFormationStateOrException result = nodeToClusterFormationStateMap.get(node);
                assertNotNull(result);
                assertNotNull(result.clusterFormationState());
                assertNull(result.exception());
                ClusterFormationFailureHelper.ClusterFormationState clusterFormationState = result.clusterFormationState();
                assertThat(clusterFormationState.getDescription(), not(emptyOrNullString()));
            });
        });

        disruption.stopDisrupting();
    }
}
