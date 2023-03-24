/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.health.node;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.health.HealthIndicatorResult;
import org.elasticsearch.health.HealthService;
import org.elasticsearch.health.HealthStatus;
import org.elasticsearch.health.metadata.HealthMetadata;
import org.elasticsearch.health.node.selection.HealthNode;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.InternalTestCluster;
import org.elasticsearch.test.NodeRoles;
import org.junit.After;
import org.junit.Before;

import java.util.List;
import java.util.concurrent.atomic.AtomicReference;

import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_NUMBER_OF_REPLICAS;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_NUMBER_OF_SHARDS;
import static org.elasticsearch.cluster.node.DiscoveryNodeRole.DATA_FROZEN_NODE_ROLE;
import static org.elasticsearch.indices.ShardLimitValidator.SETTING_CLUSTER_MAX_SHARDS_PER_NODE;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.hasSize;

@ESIntegTestCase.ClusterScope(scope = ESIntegTestCase.Scope.TEST, numDataNodes = 0)
public class ShardsCapacityHealthIndicatorServiceIT extends ESIntegTestCase {

    private static final String INDEX_NAME = "index-name";
    private InternalTestCluster internalCluster;

    @Before
    public void setUp() throws Exception {
        super.setUp();
        internalCluster = internalCluster();
        updateClusterSettings(Settings.builder().put(SETTING_CLUSTER_MAX_SHARDS_PER_NODE.getKey(), 30));
    }

    @After
    public void tearDown() throws Exception {
        super.tearDown();
        updateClusterSettings(
            Settings.builder()
                .put(SETTING_CLUSTER_MAX_SHARDS_PER_NODE.getKey(), SETTING_CLUSTER_MAX_SHARDS_PER_NODE.getDefault(Settings.EMPTY))
        );
    }

    public void testGreen() throws Exception {
        // index: 4 shards + 1 replica = 8 shards used (30 - 8 = 22 > 10 available shards)
        createIndex(4, 1);

        var result = fetchShardsCapacityIndicatorResult(internalCluster);
        assertEquals(result.status(), HealthStatus.GREEN);
        assertEquals(result.symptom(), "The cluster has enough room to add new shards.");
        assertThat(result.diagnosisList(), empty());
        assertThat(result.impacts(), empty());
    }

    public void testYellow() throws Exception {
        // index: 11 shards + 1 replica = 22 shards used (30 - 22 < 10 available shards)
        createIndex(10, 1);

        var result = fetchShardsCapacityIndicatorResult(internalCluster);
        assertEquals(result.status(), HealthStatus.YELLOW);
        assertEquals(result.symptom(), "Cluster is close to reaching the configured maximum number of shards for data nodes.");
        assertThat(result.diagnosisList(), hasSize(1));
        assertThat(result.impacts(), hasSize(2));
    }

    public void testRed() throws Exception {
        // index: 13 shards + 1 replica = 26 shards used (30 - 26 < 5 available shards)
        createIndex(13, 1);

        var result = fetchShardsCapacityIndicatorResult(internalCluster);
        assertEquals(result.status(), HealthStatus.RED);
        assertEquals(result.symptom(), "Cluster is close to reaching the configured maximum number of shards for data nodes.");
        assertThat(result.diagnosisList(), hasSize(1));
        assertThat(result.impacts(), hasSize(2));
    }

    private void createIndex(int shards, int replicas) {
        createIndex(INDEX_NAME, Settings.builder().put(SETTING_NUMBER_OF_SHARDS, shards).put(SETTING_NUMBER_OF_REPLICAS, replicas).build());
    }

    private HealthIndicatorResult fetchShardsCapacityIndicatorResult(InternalTestCluster internalCluster) throws Exception {
        var healthNode = findHealthNode().getName();
        var healthService = internalCluster.getInstance(HealthService.class, healthNode);
        var healthIndicatorResults = getHealthServiceResults(healthService, healthNode);
        assertThat(healthIndicatorResults, hasSize(1));
        return healthIndicatorResults.get(0);
    }

    private void setUpCluster(InternalTestCluster internalCluster) throws Exception {
        internalCluster.startMasterOnlyNode();
        internalCluster.startDataOnlyNode();
        internalCluster.startNode(NodeRoles.onlyRole(DATA_FROZEN_NODE_ROLE));
        ensureStableCluster(internalCluster.getNodeNames().length);
        waitForHealthMetadata();
    }

    private List<HealthIndicatorResult> getHealthServiceResults(HealthService healthService, String node) throws Exception {
        AtomicReference<List<HealthIndicatorResult>> resultListReference = new AtomicReference<>();
        ActionListener<List<HealthIndicatorResult>> listener = new ActionListener<>() {
            @Override
            public void onResponse(List<HealthIndicatorResult> healthIndicatorResults) {
                resultListReference.set(healthIndicatorResults);
            }

            @Override
            public void onFailure(Exception e) {
                throw new RuntimeException(e);
            }
        };
        healthService.getHealth(internalCluster().client(node), ShardsCapacityHealthIndicatorService.NAME, true, 1000, listener);
        assertBusy(() -> assertNotNull(resultListReference.get()));
        return resultListReference.get();
    }

    private void waitForHealthMetadata() throws Exception {
        assertBusy(() -> {
            var healthMetadata = HealthMetadata.getFromClusterState(internalCluster().clusterService().state());

            assertNotNull(healthMetadata);
            assertNotNull(healthMetadata.getShardLimitsMetadata());
            assertTrue(
                "max_shards_per_node setting must be greater than 0",
                healthMetadata.getShardLimitsMetadata().maxShardsPerNode() > 0
            );
            assertTrue(
                "max_shards_per_node.frozen setting must be greater than 0",
                healthMetadata.getShardLimitsMetadata().maxShardsPerNodeFrozen() > 0
            );
        });
    }

    private static DiscoveryNode findHealthNode() {
        var state = internalCluster().clusterService().state();
        DiscoveryNode healthNode = HealthNode.findHealthNode(state);
        assertNotNull(healthNode);
        return healthNode;
    }
}
