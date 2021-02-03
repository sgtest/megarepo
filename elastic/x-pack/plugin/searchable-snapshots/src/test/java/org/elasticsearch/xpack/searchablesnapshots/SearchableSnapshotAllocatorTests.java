/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.searchablesnapshots;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ESAllocationTestCase;
import org.elasticsearch.cluster.coordination.DeterministicTaskQueue;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.RecoverySource;
import org.elasticsearch.cluster.routing.RoutingNodes;
import org.elasticsearch.cluster.routing.RoutingTable;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.allocation.ExistingShardsAllocator;
import org.elasticsearch.cluster.routing.allocation.RoutingAllocation;
import org.elasticsearch.cluster.routing.allocation.decider.AllocationDeciders;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.IndexModule;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.snapshots.Snapshot;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.snapshots.SnapshotShardSizeInfo;
import org.elasticsearch.test.client.NoOpNodeClient;
import org.elasticsearch.xpack.searchablesnapshots.action.cache.TransportSearchableSnapshotCacheStoresAction;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Function;
import java.util.stream.Collectors;

import static org.elasticsearch.node.Node.NODE_NAME_SETTING;
import static org.hamcrest.Matchers.empty;

public class SearchableSnapshotAllocatorTests extends ESAllocationTestCase {

    public void testAllocateToNodeWithLargestCache() {
        final ShardId shardId = new ShardId("test", "_na_", 0);
        final List<DiscoveryNode> nodes = randomList(1, 10, () -> newNode("node-" + UUIDs.randomBase64UUID(random())));
        final DiscoveryNode localNode = randomFrom(nodes);
        final Settings localNodeSettings = Settings.builder().put(NODE_NAME_SETTING.getKey(), localNode.getName()).build();

        final DeterministicTaskQueue deterministicTaskQueue = new DeterministicTaskQueue(localNodeSettings, random());

        final Metadata metadata = buildSingleShardIndexMetadata(shardId);
        final RoutingTable.Builder routingTableBuilder = RoutingTable.builder();
        routingTableBuilder.addAsRestore(metadata.index(shardId.getIndex()), randomSnapshotSource(shardId));

        final ClusterState state = buildClusterState(nodes, metadata, routingTableBuilder);
        final long shardSize = randomNonNegativeLong();

        final AtomicInteger reroutesTriggered = new AtomicInteger(0);

        final Map<DiscoveryNode, Long> existingCacheSizes = nodes.stream()
            .collect(Collectors.toUnmodifiableMap(Function.identity(), k -> randomBoolean() ? 0L : randomLongBetween(0, shardSize)));

        final Client client = new NoOpNodeClient(deterministicTaskQueue.getThreadPool()) {

            @SuppressWarnings("unchecked")
            @Override
            public <Request extends ActionRequest, Response extends ActionResponse> void doExecute(
                ActionType<Response> action,
                Request request,
                ActionListener<Response> listener
            ) {
                if (action == TransportSearchableSnapshotCacheStoresAction.TYPE) {
                    listener.onResponse(
                        (Response) new TransportSearchableSnapshotCacheStoresAction.NodesCacheFilesMetadata(
                            state.getClusterName(),
                            existingCacheSizes.entrySet()
                                .stream()
                                .map(
                                    entry -> new TransportSearchableSnapshotCacheStoresAction.NodeCacheFilesMetadata(
                                        entry.getKey(),
                                        entry.getValue()
                                    )
                                )
                                .collect(Collectors.toList()),
                            List.of()
                        )
                    );
                } else {
                    throw new AssertionError("Unexpected action [" + action + "]");
                }
            }
        };

        final SearchableSnapshotAllocator allocator = new SearchableSnapshotAllocator(client, (reason, priority, listener) -> {
            reroutesTriggered.incrementAndGet();
            listener.onResponse(null);
        });

        final RoutingAllocation allocation = buildAllocation(deterministicTaskQueue, state, shardSize, yesAllocationDeciders());
        allocateAllUnassigned(allocation, allocator);

        assertEquals(1, reroutesTriggered.get());
        if (existingCacheSizes.values().stream().allMatch(size -> size == 0L)) {
            assertFalse("If there are no existing caches the allocator should not take a decision", allocation.routingNodesChanged());
        } else {
            assertTrue(allocation.routingNodesChanged());
            final long bestCacheSize = existingCacheSizes.values().stream().mapToLong(l -> l).max().orElseThrow();

            final ShardRouting primaryRouting = allocation.routingNodes().assignedShards(shardId).get(0);
            final String primaryNodeId = primaryRouting.currentNodeId();
            final DiscoveryNode primaryNode = state.nodes().get(primaryNodeId);
            assertEquals(bestCacheSize, (long) existingCacheSizes.get(primaryNode));
        }
    }

    public void testNoFetchesOnDeciderNo() {
        final ShardId shardId = new ShardId("test", "_na_", 0);
        final List<DiscoveryNode> nodes = randomList(1, 10, () -> newNode("node-" + UUIDs.randomBase64UUID(random())));
        final DiscoveryNode localNode = randomFrom(nodes);
        final Settings localNodeSettings = Settings.builder().put(NODE_NAME_SETTING.getKey(), localNode.getName()).build();

        final DeterministicTaskQueue deterministicTaskQueue = new DeterministicTaskQueue(localNodeSettings, random());

        final Metadata metadata = buildSingleShardIndexMetadata(shardId);
        final RoutingTable.Builder routingTableBuilder = RoutingTable.builder();
        routingTableBuilder.addAsRestore(metadata.index(shardId.getIndex()), randomSnapshotSource(shardId));

        final ClusterState state = buildClusterState(nodes, metadata, routingTableBuilder);
        final RoutingAllocation allocation = buildAllocation(
            deterministicTaskQueue,
            state,
            randomNonNegativeLong(),
            noAllocationDeciders()
        );

        final Client client = new NoOpNodeClient(deterministicTaskQueue.getThreadPool()) {
            @Override
            public <Request extends ActionRequest, Response extends ActionResponse> void doExecute(
                ActionType<Response> action,
                Request request,
                ActionListener<Response> listener
            ) {
                throw new AssertionError("Expecting no requests but received [" + action + "]");
            }
        };

        final SearchableSnapshotAllocator allocator = new SearchableSnapshotAllocator(
            client,
            (reason, priority, listener) -> { throw new AssertionError("Expecting no reroutes"); }
        );
        allocateAllUnassigned(allocation, allocator);
        assertTrue(allocation.routingNodesChanged());
        assertThat(allocation.routingNodes().assignedShards(shardId), empty());
        assertTrue(allocation.routingTable().index(shardId.getIndex()).allPrimaryShardsUnassigned());
    }

    private static Metadata buildSingleShardIndexMetadata(ShardId shardId) {
        return Metadata.builder()
            .put(
                IndexMetadata.builder(shardId.getIndexName())
                    .settings(
                        settings(Version.CURRENT).put(
                            ExistingShardsAllocator.EXISTING_SHARDS_ALLOCATOR_SETTING.getKey(),
                            SearchableSnapshotAllocator.ALLOCATOR_NAME
                        ).put(IndexModule.INDEX_STORE_TYPE_SETTING.getKey(), SearchableSnapshotsConstants.SNAPSHOT_DIRECTORY_FACTORY_KEY)
                    )
                    .numberOfShards(1)
                    .numberOfReplicas(0)
                    .putInSyncAllocationIds(shardId.id(), Collections.emptySet())
            )
            .build();
    }

    private ClusterState buildClusterState(List<DiscoveryNode> nodes, Metadata metadata, RoutingTable.Builder routingTableBuilder) {
        final DiscoveryNodes.Builder nodesBuilder = DiscoveryNodes.builder();
        for (DiscoveryNode node : nodes) {
            nodesBuilder.add(node);
        }
        return ClusterState.builder(ClusterName.CLUSTER_NAME_SETTING.getDefault(Settings.EMPTY))
            .metadata(metadata)
            .routingTable(routingTableBuilder.build())
            .nodes(nodesBuilder)
            .build();
    }

    private static RoutingAllocation buildAllocation(
        DeterministicTaskQueue deterministicTaskQueue,
        ClusterState state,
        long shardSize,
        AllocationDeciders allocationDeciders
    ) {
        return new RoutingAllocation(
            allocationDeciders,
            new RoutingNodes(state, false),
            state,
            null,
            new SnapshotShardSizeInfo(ImmutableOpenMap.of()) {
                @Override
                public Long getShardSize(ShardRouting shardRouting) {
                    return shardSize;
                }
            },
            TimeUnit.MILLISECONDS.toNanos(deterministicTaskQueue.getCurrentTimeMillis())
        );
    }

    private static void allocateAllUnassigned(RoutingAllocation allocation, ExistingShardsAllocator allocator) {
        final RoutingNodes.UnassignedShards.UnassignedIterator iterator = allocation.routingNodes().unassigned().iterator();
        while (iterator.hasNext()) {
            allocator.allocateUnassigned(iterator.next(), allocation, iterator);
        }
    }

    private static RecoverySource.SnapshotRecoverySource randomSnapshotSource(ShardId shardId) {
        return new RecoverySource.SnapshotRecoverySource(
            UUIDs.randomBase64UUID(random()),
            new Snapshot("test-repo", new SnapshotId("test-snap", UUIDs.randomBase64UUID(random()))),
            Version.CURRENT,
            new IndexId(shardId.getIndexName(), UUIDs.randomBase64UUID(random()))
        );
    }
}
