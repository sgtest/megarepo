/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.indexlifecycle;


import org.elasticsearch.Version;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.IndexRoutingTable;
import org.elasticsearch.cluster.routing.RoutingTable;
import org.elasticsearch.cluster.routing.ShardRoutingState;
import org.elasticsearch.cluster.routing.TestShardRouting;
import org.elasticsearch.cluster.routing.UnassignedInfo;
import org.elasticsearch.cluster.routing.UnassignedInfo.Reason;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.node.Node;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.EqualsHashCodeTestUtils;
import org.elasticsearch.xpack.core.indexlifecycle.Step.StepKey;

import java.util.Map;

public class AllocationRoutedStepTests extends ESTestCase {

    public AllocationRoutedStep createRandomInstance() {
        StepKey stepKey = new StepKey(randomAlphaOfLength(10), randomAlphaOfLength(10), randomAlphaOfLength(10));
        StepKey nextStepKey = new StepKey(randomAlphaOfLength(10), randomAlphaOfLength(10), randomAlphaOfLength(10));

        return new AllocationRoutedStep(stepKey, nextStepKey);
    }

    public AllocationRoutedStep mutateInstance(AllocationRoutedStep instance) {
        StepKey key = instance.getKey();
        StepKey nextKey = instance.getNextStepKey();

        switch (between(0, 1)) {
        case 0:
            key = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 1:
            nextKey = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        default:
            throw new AssertionError("Illegal randomisation branch");
        }

        return new AllocationRoutedStep(key, nextKey);
    }

    public void testHashcodeAndEquals() {
        EqualsHashCodeTestUtils.checkEqualsAndHashCode(createRandomInstance(),
                instance -> new AllocationRoutedStep(instance.getKey(), instance.getNextStepKey()), this::mutateInstance);
    }

    public void testConditionMet() {
        Index index = new Index(randomAlphaOfLengthBetween(1, 20), randomAlphaOfLengthBetween(1, 20));
        Map<String, String> includes = AllocateActionTests.randomMap(1, 5);
        Map<String, String> excludes = AllocateActionTests.randomMap(1, 5);
        Map<String, String> requires = AllocateActionTests.randomMap(1, 5);
        Settings.Builder existingSettings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT.id)
                .put(IndexMetaData.SETTING_INDEX_UUID, index.getUUID());
        Settings.Builder expectedSettings = Settings.builder();
        Settings.Builder node1Settings = Settings.builder();
        Settings.Builder node2Settings = Settings.builder();
        includes.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_INCLUDE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_INCLUDE_GROUP_SETTING.getKey() + k, v);
            node1Settings.put(Node.NODE_ATTRIBUTES.getKey() + k, v);
        });
        excludes.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_EXCLUDE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_EXCLUDE_GROUP_SETTING.getKey() + k, v);
        });
        requires.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getKey() + k, v);
            node1Settings.put(Node.NODE_ATTRIBUTES.getKey() + k, v);
        });

        IndexRoutingTable.Builder indexRoutingTable = IndexRoutingTable.builder(index)
                .addShard(TestShardRouting.newShardRouting(new ShardId(index, 0), "node1", true, ShardRoutingState.STARTED));

        AllocationRoutedStep step = createRandomInstance();
        assertAllocateStatus(index, 1, 0, step, existingSettings, node1Settings, node2Settings, indexRoutingTable, true);
    }

    public void testExecuteAllocateNotComplete() throws Exception {
        Index index = new Index(randomAlphaOfLengthBetween(1, 20), randomAlphaOfLengthBetween(1, 20));
        Map<String, String> includes = AllocateActionTests.randomMap(1, 5);
        Map<String, String> excludes = AllocateActionTests.randomMap(1, 5);
        Map<String, String> requires = AllocateActionTests.randomMap(1, 5);
        Settings.Builder existingSettings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT.id)
                .put(IndexMetaData.SETTING_INDEX_UUID, index.getUUID());
        Settings.Builder expectedSettings = Settings.builder();
        Settings.Builder node1Settings = Settings.builder();
        Settings.Builder node2Settings = Settings.builder();
        includes.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_INCLUDE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_INCLUDE_GROUP_SETTING.getKey() + k, v);
            node1Settings.put(Node.NODE_ATTRIBUTES.getKey() + k, v);
        });
        excludes.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_EXCLUDE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_EXCLUDE_GROUP_SETTING.getKey() + k, v);
        });
        requires.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getKey() + k, v);
            node1Settings.put(Node.NODE_ATTRIBUTES.getKey() + k, v);
        });

        IndexRoutingTable.Builder indexRoutingTable = IndexRoutingTable.builder(index)
                .addShard(TestShardRouting.newShardRouting(new ShardId(index, 0), "node1", true, ShardRoutingState.STARTED))
                .addShard(TestShardRouting.newShardRouting(new ShardId(index, 1), "node2", true, ShardRoutingState.STARTED));

        AllocationRoutedStep step = createRandomInstance();
        assertAllocateStatus(index, 2, 0, step, existingSettings, node1Settings, node2Settings, indexRoutingTable, false);
    }

    public void testExecuteAllocateUnassigned() throws Exception {
        Index index = new Index(randomAlphaOfLengthBetween(1, 20), randomAlphaOfLengthBetween(1, 20));
        Map<String, String> includes = AllocateActionTests.randomMap(1, 5);
        Map<String, String> excludes = AllocateActionTests.randomMap(1, 5);
        Map<String, String> requires = AllocateActionTests.randomMap(1, 5);
        Settings.Builder existingSettings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT.id)
                .put(IndexMetaData.SETTING_INDEX_UUID, index.getUUID());
        Settings.Builder expectedSettings = Settings.builder();
        Settings.Builder node1Settings = Settings.builder();
        Settings.Builder node2Settings = Settings.builder();
        includes.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_INCLUDE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_INCLUDE_GROUP_SETTING.getKey() + k, v);
            node1Settings.put(Node.NODE_ATTRIBUTES.getKey() + k, v);
        });
        excludes.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_EXCLUDE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_EXCLUDE_GROUP_SETTING.getKey() + k, v);
        });
        requires.forEach((k, v) -> {
            existingSettings.put(IndexMetaData.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getKey() + k, v);
            expectedSettings.put(IndexMetaData.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getKey() + k, v);
            node1Settings.put(Node.NODE_ATTRIBUTES.getKey() + k, v);
        });

        IndexRoutingTable.Builder indexRoutingTable = IndexRoutingTable.builder(index)
                .addShard(TestShardRouting.newShardRouting(new ShardId(index, 0), "node1", true, ShardRoutingState.STARTED))
                .addShard(TestShardRouting.newShardRouting(new ShardId(index, 1), null, null, true, ShardRoutingState.UNASSIGNED,
                        new UnassignedInfo(randomFrom(Reason.values()), "the shard is intentionally unassigned")));

        AllocationRoutedStep step = createRandomInstance();
        assertAllocateStatus(index, 2, 0, step, existingSettings, node1Settings, node2Settings, indexRoutingTable, false);
    }

    public void testExecuteIndexMissing() throws Exception {
        Index index = new Index(randomAlphaOfLengthBetween(1, 20), randomAlphaOfLengthBetween(1, 20));
        ClusterState clusterState = ClusterState.builder(ClusterState.EMPTY_STATE).build();

        AllocationRoutedStep step = createRandomInstance();

        IndexNotFoundException thrownException = expectThrows(IndexNotFoundException.class, () -> step.isConditionMet(index, clusterState));
        assertEquals("Index not found when executing " + step.getKey().getAction() + " lifecycle action.", thrownException.getMessage());
        assertEquals(index.getName(), thrownException.getIndex().getName());
    }

    private void assertAllocateStatus(Index index, int shards, int replicas, AllocationRoutedStep step, Settings.Builder existingSettings,
                                      Settings.Builder node1Settings, Settings.Builder node2Settings,
                                      IndexRoutingTable.Builder indexRoutingTable, boolean expectComplete) {
        IndexMetaData indexMetadata = IndexMetaData.builder(index.getName()).settings(existingSettings).numberOfShards(shards)
                .numberOfReplicas(replicas).build();
        ImmutableOpenMap.Builder<String, IndexMetaData> indices = ImmutableOpenMap.<String, IndexMetaData> builder().fPut(index.getName(),
                indexMetadata);

        ClusterState clusterState = ClusterState.builder(ClusterState.EMPTY_STATE).metaData(MetaData.builder().indices(indices.build()))
                .nodes(DiscoveryNodes.builder()
                        .add(DiscoveryNode.createLocal(node1Settings.build(), new TransportAddress(TransportAddress.META_ADDRESS, 9200),
                                "node1"))
                        .add(DiscoveryNode.createLocal(node2Settings.build(), new TransportAddress(TransportAddress.META_ADDRESS, 9201),
                                "node2")))
                .routingTable(RoutingTable.builder().add(indexRoutingTable).build()).build();
        assertEquals(expectComplete, step.isConditionMet(index, clusterState));
    }
}
