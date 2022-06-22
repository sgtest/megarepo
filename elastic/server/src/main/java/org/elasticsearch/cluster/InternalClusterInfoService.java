/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.action.admin.cluster.node.stats.NodeStats;
import org.elasticsearch.action.admin.cluster.node.stats.NodesStatsRequest;
import org.elasticsearch.action.admin.cluster.node.stats.NodesStatsResponse;
import org.elasticsearch.action.admin.indices.stats.IndicesStatsRequest;
import org.elasticsearch.action.admin.indices.stats.IndicesStatsResponse;
import org.elasticsearch.action.admin.indices.stats.ShardStats;
import org.elasticsearch.action.support.DefaultShardOperationFailedException;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.allocation.DiskThresholdSettings;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Setting.Property;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.CountDown;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.store.StoreStats;
import org.elasticsearch.monitor.fs.FsInfo;
import org.elasticsearch.threadpool.ThreadPool;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.Consumer;

import static org.elasticsearch.core.Strings.format;

/**
 * InternalClusterInfoService provides the ClusterInfoService interface,
 * routinely updated on a timer. The timer can be dynamically changed by
 * setting the <code>cluster.info.update.interval</code> setting (defaulting
 * to 30 seconds). The InternalClusterInfoService only runs on the master node.
 * Listens for changes in the number of data nodes and immediately submits a
 * ClusterInfoUpdateJob if a node has been added.
 *
 * Every time the timer runs, gathers information about the disk usage and
 * shard sizes across the cluster.
 */
public class InternalClusterInfoService implements ClusterInfoService, ClusterStateListener {

    private static final Logger logger = LogManager.getLogger(InternalClusterInfoService.class);

    public static final Setting<TimeValue> INTERNAL_CLUSTER_INFO_UPDATE_INTERVAL_SETTING = Setting.timeSetting(
        "cluster.info.update.interval",
        TimeValue.timeValueSeconds(30),
        TimeValue.timeValueSeconds(10),
        Property.Dynamic,
        Property.NodeScope
    );
    public static final Setting<TimeValue> INTERNAL_CLUSTER_INFO_TIMEOUT_SETTING = Setting.positiveTimeSetting(
        "cluster.info.update.timeout",
        TimeValue.timeValueSeconds(15),
        Property.Dynamic,
        Property.NodeScope
    );

    private volatile boolean enabled;
    private volatile TimeValue updateFrequency;
    private volatile TimeValue fetchTimeout;

    private volatile Map<String, DiskUsage> leastAvailableSpaceUsages;
    private volatile Map<String, DiskUsage> mostAvailableSpaceUsages;
    private volatile IndicesStatsSummary indicesStatsSummary;

    private final ThreadPool threadPool;
    private final Client client;
    private final List<Consumer<ClusterInfo>> listeners = new CopyOnWriteArrayList<>();

    private final Object mutex = new Object();
    private final List<ActionListener<ClusterInfo>> nextRefreshListeners = new ArrayList<>();
    private AsyncRefresh currentRefresh;
    private RefreshScheduler refreshScheduler;

    public InternalClusterInfoService(Settings settings, ClusterService clusterService, ThreadPool threadPool, Client client) {
        this.leastAvailableSpaceUsages = Map.of();
        this.mostAvailableSpaceUsages = Map.of();
        this.indicesStatsSummary = IndicesStatsSummary.EMPTY;
        this.threadPool = threadPool;
        this.client = client;
        this.updateFrequency = INTERNAL_CLUSTER_INFO_UPDATE_INTERVAL_SETTING.get(settings);
        this.fetchTimeout = INTERNAL_CLUSTER_INFO_TIMEOUT_SETTING.get(settings);
        this.enabled = DiskThresholdSettings.CLUSTER_ROUTING_ALLOCATION_DISK_THRESHOLD_ENABLED_SETTING.get(settings);
        ClusterSettings clusterSettings = clusterService.getClusterSettings();
        clusterSettings.addSettingsUpdateConsumer(INTERNAL_CLUSTER_INFO_TIMEOUT_SETTING, this::setFetchTimeout);
        clusterSettings.addSettingsUpdateConsumer(INTERNAL_CLUSTER_INFO_UPDATE_INTERVAL_SETTING, this::setUpdateFrequency);
        clusterSettings.addSettingsUpdateConsumer(
            DiskThresholdSettings.CLUSTER_ROUTING_ALLOCATION_DISK_THRESHOLD_ENABLED_SETTING,
            this::setEnabled
        );
    }

    private void setEnabled(boolean enabled) {
        this.enabled = enabled;
    }

    private void setFetchTimeout(TimeValue fetchTimeout) {
        this.fetchTimeout = fetchTimeout;
    }

    void setUpdateFrequency(TimeValue updateFrequency) {
        this.updateFrequency = updateFrequency;
    }

    @Override
    public void clusterChanged(ClusterChangedEvent event) {
        final Runnable newRefresh;
        synchronized (mutex) {
            if (event.localNodeMaster() == false) {
                refreshScheduler = null;
                return;
            }

            if (refreshScheduler == null) {
                logger.trace("elected as master, scheduling cluster info update tasks");
                refreshScheduler = new RefreshScheduler();
                nextRefreshListeners.add(refreshScheduler.getListener());
            }
            newRefresh = getNewRefresh();
            assert assertRefreshInvariant();
        }
        newRefresh.run();

        // Refresh if a data node was added
        for (DiscoveryNode addedNode : event.nodesDelta().addedNodes()) {
            if (addedNode.canContainData()) {
                refreshAsync(new PlainActionFuture<>());
                break;
            }
        }
    }

    private class AsyncRefresh {

        private final List<ActionListener<ClusterInfo>> thisRefreshListeners;
        private final CountDown countDown = new CountDown(2);

        AsyncRefresh(List<ActionListener<ClusterInfo>> thisRefreshListeners) {
            this.thisRefreshListeners = thisRefreshListeners;
        }

        void execute() {
            assert countDown.isCountedDown() == false;

            logger.trace("starting async refresh");

            final NodesStatsRequest nodesStatsRequest = new NodesStatsRequest("data:true");
            nodesStatsRequest.clear();
            nodesStatsRequest.addMetric(NodesStatsRequest.Metric.FS.metricName());
            nodesStatsRequest.timeout(fetchTimeout);
            client.admin().cluster().nodesStats(nodesStatsRequest, ActionListener.runAfter(new ActionListener<>() {
                @Override
                public void onResponse(NodesStatsResponse nodesStatsResponse) {
                    logger.trace("received node stats response");

                    for (final FailedNodeException failure : nodesStatsResponse.failures()) {
                        logger.warn(() -> "failed to retrieve stats for node [" + failure.nodeId() + "]", failure.getCause());
                    }

                    Map<String, DiskUsage> leastAvailableUsagesBuilder = new HashMap<>();
                    Map<String, DiskUsage> mostAvailableUsagesBuilder = new HashMap<>();
                    fillDiskUsagePerNode(
                        adjustNodesStats(nodesStatsResponse.getNodes()),
                        leastAvailableUsagesBuilder,
                        mostAvailableUsagesBuilder
                    );
                    leastAvailableSpaceUsages = Collections.unmodifiableMap(leastAvailableUsagesBuilder);
                    mostAvailableSpaceUsages = Collections.unmodifiableMap(mostAvailableUsagesBuilder);
                }

                @Override
                public void onFailure(Exception e) {
                    if (e instanceof ClusterBlockException) {
                        logger.trace("failed to retrieve node stats", e);
                    } else {
                        logger.warn("failed to retrieve node stats", e);
                    }
                    leastAvailableSpaceUsages = Map.of();
                    mostAvailableSpaceUsages = Map.of();
                }
            }, this::onStatsProcessed));

            final IndicesStatsRequest indicesStatsRequest = new IndicesStatsRequest();
            indicesStatsRequest.clear();
            indicesStatsRequest.store(true);
            indicesStatsRequest.indicesOptions(IndicesOptions.STRICT_EXPAND_OPEN_CLOSED_HIDDEN);
            indicesStatsRequest.timeout(fetchTimeout);
            client.admin().indices().stats(indicesStatsRequest, ActionListener.runAfter(new ActionListener<>() {
                @Override
                public void onResponse(IndicesStatsResponse indicesStatsResponse) {
                    logger.trace("received indices stats response");

                    if (indicesStatsResponse.getShardFailures().length > 0) {
                        final Set<String> failedNodeIds = new HashSet<>();
                        for (final DefaultShardOperationFailedException shardFailure : indicesStatsResponse.getShardFailures()) {
                            if (shardFailure.getCause()instanceof final FailedNodeException failedNodeException) {
                                if (failedNodeIds.add(failedNodeException.nodeId())) {
                                    logger.warn(
                                        () -> format("failed to retrieve shard stats from node [%s]", failedNodeException.nodeId()),
                                        failedNodeException.getCause()
                                    );
                                }
                                logger.trace(
                                    () -> format(
                                        "failed to retrieve stats for shard [%s][%s]",
                                        shardFailure.index(),
                                        shardFailure.shardId()
                                    ),
                                    shardFailure.getCause()
                                );
                            } else {
                                logger.warn(
                                    () -> format(
                                        "failed to retrieve stats for shard [%s][%s]",
                                        shardFailure.index(),
                                        shardFailure.shardId()
                                    ),
                                    shardFailure.getCause()
                                );
                            }
                        }
                    }

                    final ShardStats[] stats = indicesStatsResponse.getShards();
                    final Map<String, Long> shardSizeByIdentifierBuilder = new HashMap<>();
                    final Map<ShardId, Long> shardDataSetSizeBuilder = new HashMap<>();
                    final Map<ShardRouting, String> dataPathByShardRoutingBuilder = new HashMap<>();
                    final Map<ClusterInfo.NodeAndPath, ClusterInfo.ReservedSpace.Builder> reservedSpaceBuilders = new HashMap<>();
                    buildShardLevelInfo(
                        stats,
                        shardSizeByIdentifierBuilder,
                        shardDataSetSizeBuilder,
                        dataPathByShardRoutingBuilder,
                        reservedSpaceBuilders
                    );

                    final Map<ClusterInfo.NodeAndPath, ClusterInfo.ReservedSpace> rsrvdSpace = new HashMap<>();
                    reservedSpaceBuilders.forEach((nodeAndPath, builder) -> rsrvdSpace.put(nodeAndPath, builder.build()));

                    indicesStatsSummary = new IndicesStatsSummary(
                        Collections.unmodifiableMap(shardSizeByIdentifierBuilder),
                        Collections.unmodifiableMap(shardDataSetSizeBuilder),
                        Collections.unmodifiableMap(dataPathByShardRoutingBuilder),
                        Collections.unmodifiableMap(rsrvdSpace)
                    );
                }

                @Override
                public void onFailure(Exception e) {
                    if (e instanceof ClusterBlockException) {
                        logger.trace("failed to retrieve indices stats", e);
                    } else {
                        logger.warn("failed to retrieve indices stats", e);
                    }
                    indicesStatsSummary = IndicesStatsSummary.EMPTY;
                }
            }, this::onStatsProcessed));
        }

        private void onStatsProcessed() {
            if (countDown.countDown()) {
                logger.trace("stats all received, computing cluster info and notifying listeners");
                try {
                    final ClusterInfo clusterInfo = getClusterInfo();
                    boolean anyListeners = false;
                    for (final Consumer<ClusterInfo> listener : listeners) {
                        anyListeners = true;
                        try {
                            logger.trace("notifying [{}] of new cluster info", listener);
                            listener.accept(clusterInfo);
                        } catch (Exception e) {
                            logger.info(() -> "failed to notify [" + listener + "] of new cluster info", e);
                        }
                    }
                    assert anyListeners : "expected to notify at least one listener";

                    for (final ActionListener<ClusterInfo> listener : thisRefreshListeners) {
                        listener.onResponse(clusterInfo);
                    }
                } finally {
                    onRefreshComplete(this);
                }
            }
        }
    }

    private void onRefreshComplete(AsyncRefresh completedRefresh) {
        final Runnable newRefresh;
        synchronized (mutex) {
            assert currentRefresh == completedRefresh;
            currentRefresh = null;

            // We only ever run one refresh at once; if another refresh was requested while this one was running then we must start another
            // to ensure that the stats it sees are up-to-date.
            newRefresh = getNewRefresh();
            assert assertRefreshInvariant();
        }
        newRefresh.run();
    }

    private Runnable getNewRefresh() {
        assert Thread.holdsLock(mutex) : "mutex not held";

        if (currentRefresh != null) {
            return () -> {};
        }

        if (nextRefreshListeners.isEmpty()) {
            return () -> {};
        }

        final ArrayList<ActionListener<ClusterInfo>> thisRefreshListeners = new ArrayList<>(nextRefreshListeners);
        nextRefreshListeners.clear();

        if (enabled) {
            currentRefresh = new AsyncRefresh(thisRefreshListeners);
            return currentRefresh::execute;
        } else {
            return () -> {
                leastAvailableSpaceUsages = Map.of();
                mostAvailableSpaceUsages = Map.of();
                indicesStatsSummary = IndicesStatsSummary.EMPTY;
                thisRefreshListeners.forEach(l -> l.onResponse(ClusterInfo.EMPTY));
            };
        }
    }

    private boolean assertRefreshInvariant() {
        assert Thread.holdsLock(mutex) : "mutex not held";
        // We never leave a refresh listener waiting unless we're already refreshing (which will pick up the waiting listener on completion)
        assert nextRefreshListeners.isEmpty() || currentRefresh != null;
        return true;
    }

    private class RefreshScheduler {

        ActionListener<ClusterInfo> getListener() {
            return ActionListener.wrap(() -> {
                if (shouldRefresh()) {
                    threadPool.scheduleUnlessShuttingDown(updateFrequency, ThreadPool.Names.SAME, () -> {
                        if (shouldRefresh()) {
                            refreshAsync(getListener());
                        }
                    });
                }
            });
        }

        private boolean shouldRefresh() {
            synchronized (mutex) {
                return refreshScheduler == this;
            }
        }
    }

    @Override
    public ClusterInfo getClusterInfo() {
        final IndicesStatsSummary indicesStatsSummary = this.indicesStatsSummary; // single volatile read
        return new ClusterInfo(
            leastAvailableSpaceUsages,
            mostAvailableSpaceUsages,
            indicesStatsSummary.shardSizes,
            indicesStatsSummary.shardDataSetSizes,
            indicesStatsSummary.shardRoutingToDataPath,
            indicesStatsSummary.reservedSpace
        );
    }

    // allow tests to adjust the node stats on receipt
    List<NodeStats> adjustNodesStats(List<NodeStats> nodeStats) {
        return nodeStats;
    }

    void refreshAsync(ActionListener<ClusterInfo> future) {
        final Runnable newRefresh;
        synchronized (mutex) {
            nextRefreshListeners.add(future);
            newRefresh = getNewRefresh();
            assert assertRefreshInvariant();
        }
        newRefresh.run();
    }

    @Override
    public void addListener(Consumer<ClusterInfo> clusterInfoConsumer) {
        listeners.add(clusterInfoConsumer);
    }

    static void buildShardLevelInfo(
        ShardStats[] stats,
        Map<String, Long> shardSizes,
        Map<ShardId, Long> shardDataSetSizeBuilder,
        Map<ShardRouting, String> newShardRoutingToDataPath,
        Map<ClusterInfo.NodeAndPath, ClusterInfo.ReservedSpace.Builder> reservedSpaceByShard
    ) {
        for (ShardStats s : stats) {
            final ShardRouting shardRouting = s.getShardRouting();
            newShardRoutingToDataPath.put(shardRouting, s.getDataPath());

            final StoreStats storeStats = s.getStats().getStore();
            if (storeStats == null) {
                continue;
            }
            final long size = storeStats.sizeInBytes();
            final long dataSetSize = storeStats.totalDataSetSizeInBytes();
            final long reserved = storeStats.getReservedSize().getBytes();

            final String shardIdentifier = ClusterInfo.shardIdentifierFromRouting(shardRouting);
            logger.trace("shard: {} size: {} reserved: {}", shardIdentifier, size, reserved);
            shardSizes.put(shardIdentifier, size);
            if (dataSetSize > shardDataSetSizeBuilder.getOrDefault(shardRouting.shardId(), -1L)) {
                shardDataSetSizeBuilder.put(shardRouting.shardId(), dataSetSize);
            }
            if (reserved != StoreStats.UNKNOWN_RESERVED_BYTES) {
                final ClusterInfo.ReservedSpace.Builder reservedSpaceBuilder = reservedSpaceByShard.computeIfAbsent(
                    new ClusterInfo.NodeAndPath(shardRouting.currentNodeId(), s.getDataPath()),
                    t -> new ClusterInfo.ReservedSpace.Builder()
                );
                reservedSpaceBuilder.add(shardRouting.shardId(), reserved);
            }
        }
    }

    static void fillDiskUsagePerNode(
        List<NodeStats> nodeStatsArray,
        Map<String, DiskUsage> newLeastAvailableUsages,
        Map<String, DiskUsage> newMostAvailableUsages
    ) {
        for (NodeStats nodeStats : nodeStatsArray) {
            if (nodeStats.getFs() == null) {
                logger.warn("node [{}/{}] did not return any filesystem stats", nodeStats.getNode().getName(), nodeStats.getNode().getId());
                continue;
            }

            FsInfo.Path leastAvailablePath = null;
            FsInfo.Path mostAvailablePath = null;
            for (FsInfo.Path info : nodeStats.getFs()) {
                if (leastAvailablePath == null) {
                    // noinspection ConstantConditions this assertion is for the benefit of readers, it's always true
                    assert mostAvailablePath == null;
                    mostAvailablePath = leastAvailablePath = info;
                } else if (leastAvailablePath.getAvailable().getBytes() > info.getAvailable().getBytes()) {
                    leastAvailablePath = info;
                } else if (mostAvailablePath.getAvailable().getBytes() < info.getAvailable().getBytes()) {
                    mostAvailablePath = info;
                }
            }
            if (leastAvailablePath == null) {
                // noinspection ConstantConditions this assertion is for the benefit of readers, it's always true
                assert mostAvailablePath == null;
                logger.warn("node [{}/{}] did not return any filesystem stats", nodeStats.getNode().getName(), nodeStats.getNode().getId());
                continue;
            }

            final String nodeId = nodeStats.getNode().getId();
            final String nodeName = nodeStats.getNode().getName();
            if (logger.isTraceEnabled()) {
                logger.trace(
                    "node [{}]: most available: total: {}, available: {} / least available: total: {}, available: {}",
                    nodeId,
                    mostAvailablePath.getTotal(),
                    mostAvailablePath.getAvailable(),
                    leastAvailablePath.getTotal(),
                    leastAvailablePath.getAvailable()
                );
            }
            if (leastAvailablePath.getTotal().getBytes() < 0) {
                if (logger.isTraceEnabled()) {
                    logger.trace(
                        "node: [{}] least available path has less than 0 total bytes of disk [{}], skipping",
                        nodeId,
                        leastAvailablePath.getTotal().getBytes()
                    );
                }
            } else {
                newLeastAvailableUsages.put(
                    nodeId,
                    new DiskUsage(
                        nodeId,
                        nodeName,
                        leastAvailablePath.getPath(),
                        leastAvailablePath.getTotal().getBytes(),
                        leastAvailablePath.getAvailable().getBytes()
                    )
                );
            }
            if (mostAvailablePath.getTotal().getBytes() < 0) {
                if (logger.isTraceEnabled()) {
                    logger.trace(
                        "node: [{}] most available path has less than 0 total bytes of disk [{}], skipping",
                        nodeId,
                        mostAvailablePath.getTotal().getBytes()
                    );
                }
            } else {
                newMostAvailableUsages.put(
                    nodeId,
                    new DiskUsage(
                        nodeId,
                        nodeName,
                        mostAvailablePath.getPath(),
                        mostAvailablePath.getTotal().getBytes(),
                        mostAvailablePath.getAvailable().getBytes()
                    )
                );
            }

        }
    }

    private record IndicesStatsSummary(
        Map<String, Long> shardSizes,
        Map<ShardId, Long> shardDataSetSizes,
        Map<ShardRouting, String> shardRoutingToDataPath,
        Map<ClusterInfo.NodeAndPath, ClusterInfo.ReservedSpace> reservedSpace
    ) {
        static final IndicesStatsSummary EMPTY = new IndicesStatsSummary(Map.of(), Map.of(), Map.of(), Map.of());
    }

}
