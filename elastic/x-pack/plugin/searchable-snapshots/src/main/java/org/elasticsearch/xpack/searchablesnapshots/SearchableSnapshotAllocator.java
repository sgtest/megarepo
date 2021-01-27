/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.searchablesnapshots;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.RecoverySource;
import org.elasticsearch.cluster.routing.RerouteService;
import org.elasticsearch.cluster.routing.RoutingNode;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.UnassignedInfo;
import org.elasticsearch.cluster.routing.allocation.AllocateUnassignedDecision;
import org.elasticsearch.cluster.routing.allocation.AllocationDecision;
import org.elasticsearch.cluster.routing.allocation.ExistingShardsAllocator;
import org.elasticsearch.cluster.routing.allocation.FailedShard;
import org.elasticsearch.cluster.routing.allocation.NodeAllocationResult;
import org.elasticsearch.cluster.routing.allocation.RoutingAllocation;
import org.elasticsearch.cluster.routing.allocation.decider.Decision;
import org.elasticsearch.cluster.routing.allocation.decider.DiskThresholdDecider;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.gateway.AsyncShardFetch;
import org.elasticsearch.gateway.ReplicaShardAllocator;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.snapshots.Snapshot;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.xpack.searchablesnapshots.action.cache.TransportSearchableSnapshotCacheStoresAction;
import org.elasticsearch.xpack.searchablesnapshots.action.cache.TransportSearchableSnapshotCacheStoresAction.NodeCacheFilesMetadata;
import org.elasticsearch.xpack.searchablesnapshots.action.cache.TransportSearchableSnapshotCacheStoresAction.NodesCacheFilesMetadata;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentMap;

import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots.SNAPSHOT_INDEX_ID_SETTING;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots.SNAPSHOT_INDEX_NAME_SETTING;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots.SNAPSHOT_REPOSITORY_NAME_SETTING;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots.SNAPSHOT_SNAPSHOT_ID_SETTING;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots.SNAPSHOT_SNAPSHOT_NAME_SETTING;

public class SearchableSnapshotAllocator implements ExistingShardsAllocator {

    private static final Logger logger = LogManager.getLogger(SearchableSnapshotAllocator.class);

    private static final ActionListener<ClusterState> REROUTE_LISTENER = new ActionListener<>() {
        @Override
        public void onResponse(ClusterState clusterRerouteResponse) {
            logger.trace("reroute succeeded after loading snapshot cache information");
        }

        @Override
        public void onFailure(Exception e) {
            logger.warn("reroute failed", e);
        }
    };

    private final ConcurrentMap<ShardId, AsyncCacheStatusFetch> asyncFetchStore = ConcurrentCollections.newConcurrentMap();

    public static final String ALLOCATOR_NAME = "searchable_snapshot_allocator";

    private final Client client;

    private final RerouteService rerouteService;

    public SearchableSnapshotAllocator(Client client, RerouteService rerouteService) {
        this.client = client;
        this.rerouteService = rerouteService;
    }

    @Override
    public void beforeAllocation(RoutingAllocation allocation) {}

    @Override
    public void afterPrimariesBeforeReplicas(RoutingAllocation allocation) {}

    @Override
    public void allocateUnassigned(
        ShardRouting shardRouting,
        RoutingAllocation allocation,
        UnassignedAllocationHandler unassignedAllocationHandler
    ) {
        // TODO: cancel and jump to better available allocations?
        if (shardRouting.primary()
            && (shardRouting.recoverySource().getType() == RecoverySource.Type.EXISTING_STORE
                || shardRouting.recoverySource().getType() == RecoverySource.Type.EMPTY_STORE)) {
            // we always force snapshot recovery source to use the snapshot-based recovery process on the node
            final Settings indexSettings = allocation.metadata().index(shardRouting.index()).getSettings();
            final IndexId indexId = new IndexId(
                SNAPSHOT_INDEX_NAME_SETTING.get(indexSettings),
                SNAPSHOT_INDEX_ID_SETTING.get(indexSettings)
            );
            final SnapshotId snapshotId = new SnapshotId(
                SNAPSHOT_SNAPSHOT_NAME_SETTING.get(indexSettings),
                SNAPSHOT_SNAPSHOT_ID_SETTING.get(indexSettings)
            );
            final String repository = SNAPSHOT_REPOSITORY_NAME_SETTING.get(indexSettings);
            final Snapshot snapshot = new Snapshot(repository, snapshotId);

            shardRouting = unassignedAllocationHandler.updateUnassigned(
                shardRouting.unassignedInfo(),
                new RecoverySource.SnapshotRecoverySource(
                    RecoverySource.SnapshotRecoverySource.NO_API_RESTORE_UUID,
                    snapshot,
                    Version.CURRENT,
                    indexId
                ),
                allocation.changes()
            );
        }

        final AllocateUnassignedDecision allocateUnassignedDecision = decideAllocation(allocation, shardRouting);

        if (allocateUnassignedDecision.isDecisionTaken()) {
            if (allocateUnassignedDecision.getAllocationDecision() == AllocationDecision.YES) {
                unassignedAllocationHandler.initialize(
                    allocateUnassignedDecision.getTargetNode().getId(),
                    allocateUnassignedDecision.getAllocationId(),
                    DiskThresholdDecider.getExpectedShardSize(
                        shardRouting,
                        ShardRouting.UNAVAILABLE_EXPECTED_SHARD_SIZE,
                        allocation.clusterInfo(),
                        allocation.snapshotShardSizeInfo(),
                        allocation.metadata(),
                        allocation.routingTable()
                    ),
                    allocation.changes()
                );
            } else {
                unassignedAllocationHandler.removeAndIgnore(allocateUnassignedDecision.getAllocationStatus(), allocation.changes());
            }
        }
    }

    private AllocateUnassignedDecision decideAllocation(RoutingAllocation allocation, ShardRouting shardRouting) {
        assert shardRouting.unassigned();
        assert ExistingShardsAllocator.EXISTING_SHARDS_ALLOCATOR_SETTING.get(
            allocation.metadata().getIndexSafe(shardRouting.index()).getSettings()
        ).equals(ALLOCATOR_NAME);

        if (shardRouting.recoverySource().getType() == RecoverySource.Type.SNAPSHOT
            && allocation.snapshotShardSizeInfo().getShardSize(shardRouting) == null) {
            return AllocateUnassignedDecision.no(UnassignedInfo.AllocationStatus.FETCHING_SHARD_DATA, null);
        }

        final boolean explain = allocation.debugDecision();
        // pre-check if it can be allocated to any node that currently exists, so we won't list the cache sizes for it for nothing
        // TODO: in the following logic, we do not account for existing cache size when handling disk space checks, should and can we
        // reliably do this in a world of concurrent cache evictions or are we ok with the cache size just being a best effort hint
        // here?
        Tuple<Decision, Map<String, NodeAllocationResult>> result = ReplicaShardAllocator.canBeAllocatedToAtLeastOneNode(
            shardRouting,
            allocation
        );
        Decision allocateDecision = result.v1();
        if (allocateDecision.type() != Decision.Type.YES && (explain == false || asyncFetchStore.get(shardRouting.shardId()) == null)) {
            // only return early if we are not in explain mode, or we are in explain mode but we have not
            // yet attempted to fetch any shard data
            logger.trace("{}: ignoring allocation, can't be allocated on any node", shardRouting);
            return AllocateUnassignedDecision.no(
                UnassignedInfo.AllocationStatus.fromDecision(allocateDecision.type()),
                result.v2() != null ? new ArrayList<>(result.v2().values()) : null
            );
        }

        final AsyncShardFetch.FetchResult<NodeCacheFilesMetadata> fetchedCacheData = fetchData(shardRouting, allocation);
        if (fetchedCacheData.hasData() == false) {
            return AllocateUnassignedDecision.no(UnassignedInfo.AllocationStatus.FETCHING_SHARD_DATA, null);
        }

        final MatchingNodes matchingNodes = findMatchingNodes(shardRouting, allocation, fetchedCacheData, explain);
        assert explain == false || matchingNodes.nodeDecisions != null : "in explain mode, we must have individual node decisions";

        List<NodeAllocationResult> nodeDecisions = augmentExplanationsWithStoreInfo(result.v2(), matchingNodes.nodeDecisions);
        if (allocateDecision.type() != Decision.Type.YES) {
            return AllocateUnassignedDecision.no(UnassignedInfo.AllocationStatus.fromDecision(allocateDecision.type()), nodeDecisions);
        } else if (matchingNodes.getNodeWithHighestMatch() != null) {
            RoutingNode nodeWithHighestMatch = allocation.routingNodes().node(matchingNodes.getNodeWithHighestMatch().getId());
            // we only check on THROTTLE since we checked before on NO
            Decision decision = allocation.deciders().canAllocate(shardRouting, nodeWithHighestMatch, allocation);
            if (decision.type() == Decision.Type.THROTTLE) {
                // TODO: does this make sense? Unlike with the store we could evict the cache concurrently and wait for nothing?
                logger.debug(
                    "[{}][{}]: throttling allocation [{}] to [{}] in order to reuse its unallocated persistent cache",
                    shardRouting.index(),
                    shardRouting.id(),
                    shardRouting,
                    nodeWithHighestMatch.node()
                );
                return AllocateUnassignedDecision.throttle(nodeDecisions);
            } else {
                logger.debug(
                    "[{}][{}]: allocating [{}] to [{}] in order to reuse its persistent cache",
                    shardRouting.index(),
                    shardRouting.id(),
                    shardRouting,
                    nodeWithHighestMatch.node()
                );
                return AllocateUnassignedDecision.yes(nodeWithHighestMatch.node(), null, nodeDecisions, true);
            }
        }
        // TODO: do we need handling of delayed allocation for leaving replicas here?
        return AllocateUnassignedDecision.NOT_TAKEN;
    }

    @Override
    public AllocateUnassignedDecision explainUnassignedShardAllocation(ShardRouting shardRouting, RoutingAllocation routingAllocation) {
        assert shardRouting.unassigned();
        assert routingAllocation.debugDecision();
        return decideAllocation(routingAllocation, shardRouting);
    }

    @Override
    public void cleanCaches() {
        asyncFetchStore.clear();
    }

    @Override
    public void applyStartedShards(List<ShardRouting> startedShards, RoutingAllocation allocation) {
        for (ShardRouting startedShard : startedShards) {
            asyncFetchStore.remove(startedShard.shardId());
        }
    }

    @Override
    public void applyFailedShards(List<FailedShard> failedShards, RoutingAllocation allocation) {
        for (FailedShard failedShard : failedShards) {
            asyncFetchStore.remove(failedShard.getRoutingEntry().shardId());
        }
    }

    @Override
    public int getNumberOfInFlightFetches() {
        int count = 0;
        for (AsyncCacheStatusFetch fetch : asyncFetchStore.values()) {
            count += fetch.numberOfInFlightFetches();
        }
        return count;
    }

    private AsyncShardFetch.FetchResult<NodeCacheFilesMetadata> fetchData(ShardRouting shard, RoutingAllocation allocation) {
        final ShardId shardId = shard.shardId();
        final Settings indexSettings = allocation.metadata().index(shard.index()).getSettings();
        final SnapshotId snapshotId = new SnapshotId(
            SNAPSHOT_SNAPSHOT_NAME_SETTING.get(indexSettings),
            SNAPSHOT_SNAPSHOT_ID_SETTING.get(indexSettings)
        );
        final AsyncCacheStatusFetch asyncFetch = asyncFetchStore.computeIfAbsent(shardId, sid -> new AsyncCacheStatusFetch());
        final DiscoveryNodes nodes = allocation.nodes();
        final DiscoveryNode[] dataNodes = asyncFetch.addFetches(nodes.getDataNodes().values().toArray(DiscoveryNode.class));
        if (dataNodes.length > 0) {
            client.execute(
                TransportSearchableSnapshotCacheStoresAction.TYPE,
                new TransportSearchableSnapshotCacheStoresAction.Request(snapshotId, shardId, dataNodes),
                ActionListener.runAfter(new ActionListener<>() {
                    @Override
                    public void onResponse(NodesCacheFilesMetadata nodesCacheFilesMetadata) {
                        final Map<DiscoveryNode, NodeCacheFilesMetadata> res = new HashMap<>(nodesCacheFilesMetadata.getNodesMap().size());
                        for (Map.Entry<String, NodeCacheFilesMetadata> entry : nodesCacheFilesMetadata.getNodesMap().entrySet()) {
                            res.put(nodes.get(entry.getKey()), entry.getValue());
                        }
                        for (FailedNodeException entry : nodesCacheFilesMetadata.failures()) {
                            final DiscoveryNode dataNode = nodes.get(entry.nodeId());
                            logger.warn("Failed fetching cache size from datanode", entry);
                            res.put(dataNode, new NodeCacheFilesMetadata(dataNode, 0L));
                        }
                        asyncFetch.addData(res);
                    }

                    @Override
                    public void onFailure(Exception e) {
                        logger.warn("Failure when trying to fetch existing cache sizes", e);
                        final Map<DiscoveryNode, NodeCacheFilesMetadata> res = new HashMap<>(dataNodes.length);
                        for (DiscoveryNode dataNode : dataNodes) {
                            res.put(dataNode, new NodeCacheFilesMetadata(dataNode, 0L));
                        }
                        asyncFetch.addData(res);
                    }
                }, () -> {
                    if (asyncFetch.data() != null) {
                        rerouteService.reroute("async_shard_cache_fetch", Priority.HIGH, REROUTE_LISTENER);
                    }
                })
            );
        }
        return new AsyncShardFetch.FetchResult<>(shardId, asyncFetch.data(), Collections.emptySet());
    }

    /**
     * Takes the store info for nodes that have a shard store and adds them to the node decisions,
     * leaving the node explanations untouched for those nodes that do not have any store information.
     */
    private static List<NodeAllocationResult> augmentExplanationsWithStoreInfo(
        Map<String, NodeAllocationResult> nodeDecisions,
        Map<String, NodeAllocationResult> withShardStores
    ) {
        if (nodeDecisions == null || withShardStores == null) {
            return null;
        }
        List<NodeAllocationResult> augmented = new ArrayList<>();
        for (Map.Entry<String, NodeAllocationResult> entry : nodeDecisions.entrySet()) {
            if (withShardStores.containsKey(entry.getKey())) {
                augmented.add(withShardStores.get(entry.getKey()));
            } else {
                augmented.add(entry.getValue());
            }
        }
        return augmented;
    }

    private MatchingNodes findMatchingNodes(
        ShardRouting shard,
        RoutingAllocation allocation,
        AsyncShardFetch.FetchResult<NodeCacheFilesMetadata> data,
        boolean explain
    ) {
        final Map<DiscoveryNode, Long> matchingNodesCacheSizes = new HashMap<>();
        final Map<String, NodeAllocationResult> nodeDecisionsDebug = explain ? new HashMap<>() : null;
        for (Map.Entry<DiscoveryNode, NodeCacheFilesMetadata> nodeStoreEntry : data.getData().entrySet()) {
            DiscoveryNode discoNode = nodeStoreEntry.getKey();
            NodeCacheFilesMetadata nodeCacheFilesMetadata = nodeStoreEntry.getValue();
            // we don't have any existing cached bytes at all
            if (nodeCacheFilesMetadata.bytesCached() == 0L) {
                continue;
            }

            RoutingNode node = allocation.routingNodes().node(discoNode.getId());
            if (node == null) {
                continue;
            }

            // check if we can allocate on the node
            Decision decision = allocation.deciders().canAllocate(shard, node, allocation);
            Long matchingBytes = null;
            if (explain) {
                matchingBytes = nodeCacheFilesMetadata.bytesCached();
                NodeAllocationResult.ShardStoreInfo shardStoreInfo = new NodeAllocationResult.ShardStoreInfo(matchingBytes);
                nodeDecisionsDebug.put(node.nodeId(), new NodeAllocationResult(discoNode, shardStoreInfo, decision));
            }

            if (decision.type() == Decision.Type.NO) {
                continue;
            }

            if (matchingBytes == null) {
                matchingBytes = nodeCacheFilesMetadata.bytesCached();
            }
            matchingNodesCacheSizes.put(discoNode, matchingBytes);
            if (logger.isTraceEnabled()) {
                logger.trace(
                    "{}: node [{}] has [{}/{}] bytes of re-usable cache data",
                    shard,
                    discoNode.getName(),
                    new ByteSizeValue(matchingBytes),
                    matchingBytes
                );
            }
        }

        return new MatchingNodes(matchingNodesCacheSizes, nodeDecisionsDebug);
    }

    private static final class AsyncCacheStatusFetch {

        private final Set<DiscoveryNode> fetchingDataNodes = new HashSet<>();

        private final Map<DiscoveryNode, NodeCacheFilesMetadata> data = new HashMap<>();

        AsyncCacheStatusFetch() {}

        synchronized DiscoveryNode[] addFetches(DiscoveryNode[] nodes) {
            final Collection<DiscoveryNode> nodesToFetch = new ArrayList<>();
            for (DiscoveryNode node : nodes) {
                if (data.containsKey(node) == false && fetchingDataNodes.add(node)) {
                    nodesToFetch.add(node);
                }
            }
            return nodesToFetch.toArray(new DiscoveryNode[0]);
        }

        synchronized void addData(Map<DiscoveryNode, NodeCacheFilesMetadata> newData) {
            data.putAll(newData);
            fetchingDataNodes.removeAll(newData.keySet());
        }

        @Nullable
        synchronized Map<DiscoveryNode, NodeCacheFilesMetadata> data() {
            return fetchingDataNodes.size() > 0 ? null : Map.copyOf(data);
        }

        synchronized int numberOfInFlightFetches() {
            return fetchingDataNodes.size();
        }
    }

    private static final class MatchingNodes {
        private final DiscoveryNode nodeWithHighestMatch;
        @Nullable
        private final Map<String, NodeAllocationResult> nodeDecisions;

        MatchingNodes(Map<DiscoveryNode, Long> matchingNodes, @Nullable Map<String, NodeAllocationResult> nodeDecisions) {
            this.nodeDecisions = nodeDecisions;
            this.nodeWithHighestMatch = matchingNodes.entrySet()
                .stream()
                .filter(entry -> entry.getValue() > 0L)
                .max(Map.Entry.comparingByValue())
                .map(Map.Entry::getKey)
                .orElse(null);
        }

        /**
         * Returns the node with the highest number of bytes cached for the shard or {@code null} if no node with any bytes matched exists.
         */
        @Nullable
        public DiscoveryNode getNodeWithHighestMatch() {
            return this.nodeWithHighestMatch;
        }
    }
}
