/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.routing.allocation;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.replication.ClusterStateCreationUtils;
import org.elasticsearch.cluster.ClusterInfo;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ESAllocationTestCase;
import org.elasticsearch.cluster.TestShardRoutingRoleStrategies;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.RoutingNode;
import org.elasticsearch.cluster.routing.RoutingTable;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.ShardRoutingState;
import org.elasticsearch.cluster.routing.TestShardRouting;
import org.elasticsearch.cluster.routing.allocation.decider.AllocationDeciders;
import org.elasticsearch.cluster.routing.allocation.decider.Decision;
import org.elasticsearch.cluster.routing.allocation.decider.SameShardAllocationDecider;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.Strings;
import org.elasticsearch.index.Index;
import org.elasticsearch.snapshots.SnapshotShardSizeInfo;

import java.util.Collections;
import java.util.List;

import static java.util.Collections.emptyMap;
import static java.util.Collections.singletonList;
import static org.elasticsearch.cluster.ClusterName.CLUSTER_NAME_SETTING;
import static org.elasticsearch.cluster.routing.RoutingNodesHelper.shardsWithState;
import static org.elasticsearch.cluster.routing.ShardRoutingState.INITIALIZING;
import static org.elasticsearch.cluster.routing.ShardRoutingState.UNASSIGNED;
import static org.elasticsearch.cluster.routing.allocation.RoutingNodesUtils.numberOfShardsOfType;
import static org.elasticsearch.common.settings.ClusterSettings.createBuiltInClusterSettings;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;

public class SameShardRoutingTests extends ESAllocationTestCase {

    public void testSameHost() {
        AllocationService strategy = createAllocationService(
            Settings.builder().put(SameShardAllocationDecider.CLUSTER_ROUTING_ALLOCATION_SAME_HOST_SETTING.getKey(), true).build()
        );

        final Settings.Builder indexSettings = settings(Version.CURRENT);
        if (randomBoolean()) {
            indexSettings.put(IndexMetadata.SETTING_AUTO_EXPAND_REPLICAS, "0-1");
        }

        Metadata metadata = Metadata.builder()
            .put(IndexMetadata.builder("test").settings(indexSettings).numberOfShards(2).numberOfReplicas(1))
            .build();

        RoutingTable routingTable = RoutingTable.builder(TestShardRoutingRoleStrategies.DEFAULT_ROLE_ONLY)
            .addAsNew(metadata.index("test"))
            .build();
        ClusterState clusterState = ClusterState.builder(CLUSTER_NAME_SETTING.getDefault(Settings.EMPTY))
            .metadata(metadata)
            .routingTable(routingTable)
            .build();

        logger.info("--> adding two nodes with the same host");
        clusterState = ClusterState.builder(clusterState)
            .nodes(
                DiscoveryNodes.builder()
                    .add(
                        new DiscoveryNode(
                            "node1",
                            "node1",
                            "node1",
                            "test1",
                            "test1",
                            buildNewFakeTransportAddress(),
                            emptyMap(),
                            MASTER_DATA_ROLES,
                            Version.CURRENT
                        )
                    )
                    .add(
                        new DiscoveryNode(
                            "node2",
                            "node2",
                            "node2",
                            "test1",
                            "test1",
                            buildNewFakeTransportAddress(),
                            emptyMap(),
                            MASTER_DATA_ROLES,
                            Version.CURRENT
                        )
                    )
            )
            .build();
        clusterState = strategy.reroute(clusterState, "reroute", ActionListener.noop());

        assertThat(numberOfShardsOfType(clusterState.getRoutingNodes(), ShardRoutingState.INITIALIZING), equalTo(2));

        logger.info("--> start all primary shards, no replica will be started since its on the same host");
        clusterState = startInitializingShardsAndReroute(strategy, clusterState);

        assertThat(numberOfShardsOfType(clusterState.getRoutingNodes(), ShardRoutingState.STARTED), equalTo(2));
        assertThat(numberOfShardsOfType(clusterState.getRoutingNodes(), ShardRoutingState.INITIALIZING), equalTo(0));

        logger.info("--> add another node, with a different host, replicas will be allocating");
        clusterState = ClusterState.builder(clusterState)
            .nodes(
                DiscoveryNodes.builder(clusterState.nodes())
                    .add(
                        new DiscoveryNode(
                            "node3",
                            "node3",
                            "node3",
                            "test2",
                            "test2",
                            buildNewFakeTransportAddress(),
                            emptyMap(),
                            MASTER_DATA_ROLES,
                            Version.CURRENT
                        )
                    )
            )
            .build();
        clusterState = strategy.reroute(clusterState, "reroute", ActionListener.noop());

        assertThat(numberOfShardsOfType(clusterState.getRoutingNodes(), ShardRoutingState.STARTED), equalTo(2));
        assertThat(numberOfShardsOfType(clusterState.getRoutingNodes(), ShardRoutingState.INITIALIZING), equalTo(2));
        for (ShardRouting shardRouting : shardsWithState(clusterState.getRoutingNodes(), INITIALIZING)) {
            assertThat(shardRouting.currentNodeId(), equalTo("node3"));
        }
    }

    public void testSameHostCheckWithExplain() {
        Settings sameHostSetting = Settings.builder()
            .put(SameShardAllocationDecider.CLUSTER_ROUTING_ALLOCATION_SAME_HOST_SETTING.getKey(), true)
            .build();
        AllocationService strategy = createAllocationService(sameHostSetting);

        final Settings.Builder indexSettings = settings(Version.CURRENT);
        if (randomBoolean()) {
            indexSettings.put(IndexMetadata.SETTING_AUTO_EXPAND_REPLICAS, "0-1");
        }

        Metadata metadata = Metadata.builder()
            .put(IndexMetadata.builder("test").settings(indexSettings).numberOfShards(2).numberOfReplicas(1))
            .build();

        RoutingTable routingTable = RoutingTable.builder(TestShardRoutingRoleStrategies.DEFAULT_ROLE_ONLY)
            .addAsNew(metadata.index("test"))
            .build();
        ClusterState clusterState = ClusterState.builder(CLUSTER_NAME_SETTING.getDefault(Settings.EMPTY))
            .metadata(metadata)
            .routingTable(routingTable)
            .build();

        String host1 = randomFrom("test1", "test2");
        String host2 = randomFrom("test1", "test2");
        logger.info("--> adding two nodes");
        clusterState = ClusterState.builder(clusterState)
            .nodes(
                DiscoveryNodes.builder()
                    .add(
                        new DiscoveryNode(
                            "node1",
                            "node1",
                            "node1",
                            host1,
                            host1,
                            buildNewFakeTransportAddress(),
                            emptyMap(),
                            MASTER_DATA_ROLES,
                            Version.CURRENT
                        )
                    )
                    .add(
                        new DiscoveryNode(
                            "node2",
                            "node2",
                            "node2",
                            host2,
                            host2,
                            buildNewFakeTransportAddress(),
                            emptyMap(),
                            MASTER_DATA_ROLES,
                            Version.CURRENT
                        )
                    )
            )
            .build();
        clusterState = applyStartedShardsUntilNoChange(clusterState, strategy);

        List<ShardRouting> unassignedShards = shardsWithState(clusterState.getRoutingNodes(), UNASSIGNED);
        if (host1.equals(host2) == false) {
            logger.info("Two nodes on the different host");
            assertThat(0, equalTo(unassignedShards.size()));
        } else {
            final ShardRouting unassignedShard = unassignedShards.get(0);

            final SameShardAllocationDecider decider = new SameShardAllocationDecider(createBuiltInClusterSettings(sameHostSetting));

            final RoutingNode emptyNode = clusterState.getRoutingNodes()
                .stream()
                .filter(node -> node.getByShardId(unassignedShard.shardId()) == null)
                .findFirst()
                .orElseThrow(AssertionError::new);

            final RoutingNode otherNode = clusterState.getRoutingNodes()
                .stream()
                .filter(node -> node != emptyNode)
                .findFirst()
                .orElseThrow(AssertionError::new);

            final RoutingAllocation routingAllocation = new RoutingAllocation(
                new AllocationDeciders(singletonList(decider)),
                clusterState,
                null,
                null,
                0L
            );
            routingAllocation.debugDecision(true);

            final Decision decision = decider.canAllocate(unassignedShard, emptyNode, routingAllocation);
            logger.info("Two nodes on the same host");
            assertThat(decision.type(), equalTo(Decision.Type.NO));
            assertThat(
                decision.getExplanation(),
                equalTo(
                    Strings.format(
                        """
                            cannot allocate to node [%s] because a copy of this shard is already allocated to node [%s] with the same host \
                            address [%s] and [%s] is [true] which forbids more than one node on each host from holding a copy of this shard\
                            """,
                        emptyNode.nodeId(),
                        otherNode.nodeId(),
                        host1,
                        SameShardAllocationDecider.CLUSTER_ROUTING_ALLOCATION_SAME_HOST_SETTING.getKey()
                    )
                )
            );
        }
    }

    public void testSameHostCheckDisabledByAutoExpandReplicas() {
        final AllocationService strategy = createAllocationService(
            Settings.builder().put(SameShardAllocationDecider.CLUSTER_ROUTING_ALLOCATION_SAME_HOST_SETTING.getKey(), true).build()
        );

        final Metadata metadata = Metadata.builder()
            .put(
                IndexMetadata.builder("test")
                    .settings(
                        settings(Version.CURRENT).put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
                            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 99)
                            .put(IndexMetadata.SETTING_AUTO_EXPAND_REPLICAS, "0-all")
                    )
            )
            .build();

        final DiscoveryNode node1 = new DiscoveryNode(
            "node1",
            "node1",
            "node1",
            "test1",
            "test1",
            buildNewFakeTransportAddress(),
            emptyMap(),
            MASTER_DATA_ROLES,
            Version.CURRENT
        );

        final DiscoveryNode node2 = new DiscoveryNode(
            "node2",
            "node2",
            "node2",
            "test1",
            "test1",
            buildNewFakeTransportAddress(),
            emptyMap(),
            MASTER_DATA_ROLES,
            Version.CURRENT
        );

        final ClusterState clusterState = applyStartedShardsUntilNoChange(
            ClusterState.builder(CLUSTER_NAME_SETTING.getDefault(Settings.EMPTY))
                .metadata(metadata)
                .routingTable(
                    RoutingTable.builder(TestShardRoutingRoleStrategies.DEFAULT_ROLE_ONLY).addAsNew(metadata.index("test")).build()
                )
                .nodes(DiscoveryNodes.builder().add(node1).add(node2))
                .build(),
            strategy
        );

        assertThat(shardsWithState(clusterState.getRoutingNodes(), UNASSIGNED), empty());
    }

    public void testForceAllocatePrimaryOnSameNodeNotAllowed() {
        SameShardAllocationDecider decider = new SameShardAllocationDecider(createBuiltInClusterSettings());
        ClusterState clusterState = ClusterStateCreationUtils.state("idx", randomIntBetween(2, 4), 1);
        Index index = clusterState.getMetadata().index("idx").getIndex();
        ShardRouting primaryShard = clusterState.routingTable().index(index).shard(0).primaryShard();
        RoutingNode routingNode = clusterState.getRoutingNodes().node(primaryShard.currentNodeId());
        RoutingAllocation routingAllocation = new RoutingAllocation(
            new AllocationDeciders(Collections.emptyList()),
            clusterState.mutableRoutingNodes(),
            clusterState,
            ClusterInfo.EMPTY,
            SnapshotShardSizeInfo.EMPTY,
            System.nanoTime()
        );

        // can't force allocate same shard copy to the same node
        ShardRouting newPrimary = TestShardRouting.newShardRouting(primaryShard.shardId(), null, true, ShardRoutingState.UNASSIGNED);
        Decision decision = decider.canForceAllocatePrimary(newPrimary, routingNode, routingAllocation);
        assertEquals(Decision.Type.NO, decision.type());

        // can force allocate to a different node
        RoutingNode unassignedNode = null;
        for (RoutingNode node : clusterState.getRoutingNodes()) {
            if (node.isEmpty()) {
                unassignedNode = node;
                break;
            }
        }
        decision = decider.canForceAllocatePrimary(newPrimary, unassignedNode, routingAllocation);
        assertEquals(Decision.Type.YES, decision.type());
    }
}
