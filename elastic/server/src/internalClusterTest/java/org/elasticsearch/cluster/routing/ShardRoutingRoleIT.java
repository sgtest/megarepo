/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.routing;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.lucene.tests.util.LuceneTestCase;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateListener;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.routing.allocation.RoutingAllocation;
import org.elasticsearch.cluster.routing.allocation.command.AllocationCommand;
import org.elasticsearch.cluster.routing.allocation.command.CancelAllocationCommand;
import org.elasticsearch.cluster.routing.allocation.decider.AllocationDecider;
import org.elasticsearch.cluster.routing.allocation.decider.Decision;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.CollectionUtils;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.plugins.ClusterPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.PluginsService;
import org.elasticsearch.snapshots.SnapshotState;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.XContentTestUtils;

import java.io.IOException;
import java.util.Collection;
import java.util.HashSet;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.anEmptyMap;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.not;

@LuceneTestCase.AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/92915")
@SuppressWarnings("resource")
public class ShardRoutingRoleIT extends ESIntegTestCase {

    private static final Logger logger = LogManager.getLogger(ShardRoutingRoleIT.class);

    public static class TestPlugin extends Plugin implements ClusterPlugin {

        volatile int numIndexingCopies = 1;

        @Override
        public ShardRoutingRoleStrategy getShardRoutingRoleStrategy() {
            return new ShardRoutingRoleStrategy() {
                @Override
                public ShardRouting.Role newReplicaRole() {
                    return ShardRouting.Role.SEARCH_ONLY;
                }

                @Override
                public ShardRouting.Role newEmptyRole(int copyIndex) {
                    assert 0 < numIndexingCopies;
                    return copyIndex < numIndexingCopies ? ShardRouting.Role.INDEX_ONLY : ShardRouting.Role.SEARCH_ONLY;
                }
            };
        }

        @Override
        public Collection<AllocationDecider> createAllocationDeciders(Settings settings, ClusterSettings clusterSettings) {
            return List.of(new AllocationDecider() {
                @Override
                public Decision canForceAllocatePrimary(ShardRouting shardRouting, RoutingNode node, RoutingAllocation allocation) {
                    // once a primary is cancelled it _stays_ cancelled
                    if (shardRouting.unassignedInfo().getReason() == UnassignedInfo.Reason.REROUTE_CANCELLED) {
                        return Decision.NO;
                    }
                    return super.canForceAllocatePrimary(shardRouting, node, allocation);
                }
            });
        }
    }

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return CollectionUtils.appendToCopy(super.nodePlugins(), TestPlugin.class);
    }

    private static TestPlugin getMasterNodePlugin() {
        return internalCluster().getCurrentMasterNodeInstance(PluginsService.class)
            .filterPlugins(TestPlugin.class)
            .stream()
            .findFirst()
            .orElseThrow(() -> new AssertionError("no plugin"));
    }

    private static final String INDEX_NAME = "test";

    private static class RoutingTableWatcher implements ClusterStateListener {

        final int numShards = between(1, 3);
        int numIndexingCopies = between(1, 2);
        int numReplicas = between(numIndexingCopies - 1, 3);

        @Override
        public void clusterChanged(ClusterChangedEvent event) {
            assertRoles(event.state().getRoutingTable().index(INDEX_NAME));
        }

        private void assertRoles(IndexRoutingTable indexRoutingTable) {
            if (indexRoutingTable == null) {
                return;
            }
            var message = indexRoutingTable.prettyPrint();
            assertEquals("number_of_shards: " + message, numShards, indexRoutingTable.size());
            for (int shardId = 0; shardId < numShards; shardId++) {
                final var indexShardRoutingTable = indexRoutingTable.shard(shardId);
                assertEquals("number_of_replicas: " + message, numReplicas + 1, indexShardRoutingTable.size());
                var indexingShards = 0;
                for (int shardCopy = 0; shardCopy < numReplicas + 1; shardCopy++) {
                    final var shardRouting = indexShardRoutingTable.shard(shardCopy);
                    switch (shardRouting.role()) {
                        case INDEX_ONLY -> indexingShards += 1;
                        case SEARCH_ONLY -> assertFalse(shardRouting.primary());
                        case DEFAULT -> fail("should not have any DEFAULT shards");
                    }
                    if (shardRouting.relocating()) {
                        assertEquals("role on relocation: " + message, shardRouting.role(), shardRouting.getTargetRelocatingShard().role());
                    }
                }
                assertEquals("number_of_indexing_shards: " + message, Math.min(numIndexingCopies, numReplicas + 1), indexingShards);
            }
        }

        Settings getIndexSettings() {
            logger.info("--> numShards={}, numReplicas={}", numShards, numReplicas);
            return Settings.builder()
                .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, numShards)
                .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, numReplicas)
                .build();
        }
    }

    @SuppressWarnings("unchecked")
    private static void assertRolesInRoutingTableXContent(ClusterState state) {
        try {
            final var routingTable = (Map<String, Object>) XContentTestUtils.convertToMap(state).get("routing_table");
            final var routingTableIndices = (Map<String, Object>) routingTable.get("indices");
            final var routingTableIndex = (Map<String, Object>) routingTableIndices.get("test");
            final var routingTableShards = (Map<String, Object>) routingTableIndex.get("shards");
            for (final var routingTableShardValue : routingTableShards.values()) {
                for (Object routingTableShardCopy : (List<Object>) routingTableShardValue) {
                    final var routingTableShard = (Map<String, String>) routingTableShardCopy;
                    assertNotNull(ShardRouting.Role.valueOf(routingTableShard.get("role")));
                }
            }
        } catch (IOException e) {
            throw new AssertionError("unexpected", e);
        }
    }

    public void testShardCreation() {
        var routingTableWatcher = new RoutingTableWatcher();

        var numDataNodes = routingTableWatcher.numReplicas + 2;
        internalCluster().ensureAtLeastNumDataNodes(numDataNodes);
        getMasterNodePlugin().numIndexingCopies = routingTableWatcher.numIndexingCopies;

        final var masterClusterService = internalCluster().getCurrentMasterNodeInstance(ClusterService.class);
        try {
            // verify the correct number of shard copies of each role as the routing table evolves
            masterClusterService.addListener(routingTableWatcher);

            createIndex(INDEX_NAME, routingTableWatcher.getIndexSettings());

            final var clusterState = client().admin().cluster().prepareState().clear().setRoutingTable(true).get().getState();

            // verify non-DEFAULT roles reported in cluster state XContent
            assertRolesInRoutingTableXContent(clusterState);

            // verify non-DEFAULT roles reported in cluster state string representation
            var stateAsString = clusterState.toString();
            assertThat(stateAsString, containsString("[" + ShardRouting.Role.INDEX_ONLY + "]"));
            assertThat(stateAsString, not(containsString("[" + ShardRouting.Role.DEFAULT + "]")));
            if (routingTableWatcher.numReplicas + 1 > routingTableWatcher.numIndexingCopies) {
                assertThat(stateAsString, containsString("[" + ShardRouting.Role.SEARCH_ONLY + "]"));
            }

            ensureGreen(INDEX_NAME);

            // new replicas get the SEARCH_ONLY role
            routingTableWatcher.numReplicas += 1;
            assertAcked(
                client().admin()
                    .indices()
                    .prepareUpdateSettings("test")
                    .setSettings(Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, routingTableWatcher.numReplicas))
            );

            ensureGreen(INDEX_NAME);

            // removing replicas drops SEARCH_ONLY copies first
            while (routingTableWatcher.numReplicas > 0) {
                routingTableWatcher.numReplicas -= 1;
                assertAcked(
                    client().admin()
                        .indices()
                        .prepareUpdateSettings("test")
                        .setSettings(Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, routingTableWatcher.numReplicas))
                );
            }

            // restoring the index from a snapshot may change the number of indexing replicas because the routing table is created afresh
            var repoPath = randomRepoPath();
            assertAcked(
                client().admin()
                    .cluster()
                    .preparePutRepository("repo")
                    .setType("fs")
                    .setSettings(Settings.builder().put("location", repoPath))
            );

            assertEquals(
                SnapshotState.SUCCESS,
                client().admin().cluster().prepareCreateSnapshot("repo", "snap").setWaitForCompletion(true).get().getSnapshotInfo().state()
            );

            if (randomBoolean()) {
                assertAcked(client().admin().indices().prepareDelete(INDEX_NAME));
            } else {
                assertAcked(client().admin().indices().prepareClose(INDEX_NAME));
                ensureGreen(INDEX_NAME);
            }

            routingTableWatcher.numReplicas = between(0, numDataNodes - 1);
            routingTableWatcher.numIndexingCopies = between(1, 2);
            getMasterNodePlugin().numIndexingCopies = routingTableWatcher.numIndexingCopies;

            assertEquals(
                0,
                client().admin()
                    .cluster()
                    .prepareRestoreSnapshot("repo", "snap")
                    .setIndices("test")
                    .setIndexSettings(Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, routingTableWatcher.numReplicas))
                    .setWaitForCompletion(true)
                    .get()
                    .getRestoreInfo()
                    .failedShards()
            );
            ensureGreen("test");
        } finally {
            masterClusterService.removeListener(routingTableWatcher);
        }
    }

    public void testRelocation() {
        var routingTableWatcher = new RoutingTableWatcher();

        var numDataNodes = routingTableWatcher.numReplicas + 2;
        internalCluster().ensureAtLeastNumDataNodes(numDataNodes);
        getMasterNodePlugin().numIndexingCopies = routingTableWatcher.numIndexingCopies;

        final var masterClusterService = internalCluster().getCurrentMasterNodeInstance(ClusterService.class);
        try {
            // verify the correct number of shard copies of each role as the routing table evolves
            masterClusterService.addListener(routingTableWatcher);

            createIndex(INDEX_NAME, routingTableWatcher.getIndexSettings());

            for (String nodeName : internalCluster().getNodeNames()) {
                assertAcked(
                    client().admin()
                        .indices()
                        .prepareUpdateSettings("test")
                        .setSettings(Settings.builder().put(IndexMetadata.INDEX_ROUTING_EXCLUDE_GROUP_PREFIX + "._name", nodeName))
                );
                ensureGreen(INDEX_NAME);
            }
        } finally {
            masterClusterService.removeListener(routingTableWatcher);
        }
    }

    public void testPromotion() {
        var routingTableWatcher = new RoutingTableWatcher();

        var numDataNodes = routingTableWatcher.numReplicas + 2;
        internalCluster().ensureAtLeastNumDataNodes(numDataNodes);
        getMasterNodePlugin().numIndexingCopies = routingTableWatcher.numIndexingCopies;

        final var masterClusterService = internalCluster().getCurrentMasterNodeInstance(ClusterService.class);
        try {
            // verify the correct number of shard copies of each role as the routing table evolves
            masterClusterService.addListener(routingTableWatcher);

            createIndex(INDEX_NAME, routingTableWatcher.getIndexSettings());
            ensureGreen(INDEX_NAME);

            assertAcked(
                client().admin()
                    .indices()
                    .prepareUpdateSettings("test")
                    .setSettings(Settings.builder().put(IndexMetadata.INDEX_ROUTING_REQUIRE_GROUP_PREFIX + "._name", "not-a-node"))
            );

            AllocationCommand cancelPrimaryCommand;
            while ((cancelPrimaryCommand = getCancelPrimaryCommand()) != null) {
                client().admin().cluster().prepareReroute().add(cancelPrimaryCommand).get();
            }
        } finally {
            masterClusterService.removeListener(routingTableWatcher);
        }
    }

    @Nullable
    public AllocationCommand getCancelPrimaryCommand() {
        final var indexRoutingTable = client().admin()
            .cluster()
            .prepareState()
            .clear()
            .setRoutingTable(true)
            .get()
            .getState()
            .routingTable()
            .index(INDEX_NAME);
        for (int shardId = 0; shardId < indexRoutingTable.size(); shardId++) {
            final var indexShardRoutingTable = indexRoutingTable.shard(shardId);
            if (indexShardRoutingTable.primaryShard().assignedToNode()) {
                return new CancelAllocationCommand(INDEX_NAME, shardId, indexShardRoutingTable.primaryShard().currentNodeId(), true);
            } else {
                assertThat(indexShardRoutingTable.assignedShards(), empty());
                for (int copy = 0; copy < indexShardRoutingTable.size(); copy++) {
                    final var shardRouting = indexShardRoutingTable.shard(copy);
                    assertEquals(
                        shardRouting.role().isPromotableToPrimary()
                            ? UnassignedInfo.Reason.REROUTE_CANCELLED
                            : UnassignedInfo.Reason.UNPROMOTABLE_REPLICA,
                        shardRouting.unassignedInfo().getReason()
                    );
                }
            }
        }
        return null;
    }

    public void testSearchRouting() throws InterruptedException {

        var routingTableWatcher = new RoutingTableWatcher();
        routingTableWatcher.numReplicas = Math.max(1, routingTableWatcher.numReplicas);
        routingTableWatcher.numIndexingCopies = Math.min(routingTableWatcher.numIndexingCopies, routingTableWatcher.numReplicas);
        getMasterNodePlugin().numIndexingCopies = routingTableWatcher.numIndexingCopies;

        internalCluster().ensureAtLeastNumDataNodes(routingTableWatcher.numReplicas + 1);

        final var masterClusterService = internalCluster().getCurrentMasterNodeInstance(ClusterService.class);
        try {
            // verify the correct number of shard copies of each role as the routing table evolves
            masterClusterService.addListener(routingTableWatcher);

            createIndex(INDEX_NAME, routingTableWatcher.getIndexSettings());
            indexRandom(true, INDEX_NAME, between(1, 100));
            ensureGreen(INDEX_NAME);

            final var searchShardProfileKeys = new HashSet<String>();
            final var indexRoutingTable = client().admin()
                .cluster()
                .prepareState()
                .clear()
                .setRoutingTable(true)
                .get()
                .getState()
                .routingTable()
                .index(INDEX_NAME);

            for (int shardId = 0; shardId < indexRoutingTable.size(); shardId++) {
                final var indexShardRoutingTable = indexRoutingTable.shard(shardId);
                for (int shardCopy = 0; shardCopy < indexShardRoutingTable.size(); shardCopy++) {
                    final var shardRouting = indexShardRoutingTable.shard(shardCopy);
                    if (shardRouting.role() == ShardRouting.Role.SEARCH_ONLY) {
                        searchShardProfileKeys.add("[" + shardRouting.currentNodeId() + "][" + INDEX_NAME + "][" + shardId + "]");
                    }
                }
            }

            for (int i = 0; i < 10; i++) {
                final var profileResults = client().prepareSearch(INDEX_NAME).setProfile(true).get().getProfileResults();
                assertThat(profileResults, not(anEmptyMap()));
                for (final var searchShardProfileKey : profileResults.keySet()) {
                    assertThat(searchShardProfileKeys, hasItem(searchShardProfileKey));
                }
            }
        } finally {
            masterClusterService.removeListener(routingTableWatcher);
        }
    }

}
