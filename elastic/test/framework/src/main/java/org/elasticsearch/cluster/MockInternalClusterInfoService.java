/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.cluster;

import org.elasticsearch.action.admin.cluster.node.stats.NodeStats;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.monitor.fs.FsInfo;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.threadpool.ThreadPool;

import java.util.List;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.stream.Collectors;
import java.util.stream.StreamSupport;

public class MockInternalClusterInfoService extends InternalClusterInfoService {

    /** This is a marker plugin used to trigger MockNode to use this mock info service. */
    public static class TestPlugin extends Plugin {}

    @Nullable // if no fakery should take place
    private volatile Function<ShardRouting, Long> shardSizeFunction;

    @Nullable // if no fakery should take place
    private volatile BiFunction<DiscoveryNode, FsInfo.Path, FsInfo.Path> diskUsageFunction;

    public MockInternalClusterInfoService(Settings settings, ClusterService clusterService,
                                          ThreadPool threadPool, NodeClient client) {
        super(settings, clusterService, threadPool, client);
    }

    public void setDiskUsageFunctionAndRefresh(BiFunction<DiscoveryNode, FsInfo.Path, FsInfo.Path> diskUsageFunction) {
        this.diskUsageFunction = diskUsageFunction;
        ClusterInfoServiceUtils.refresh(this);
    }

    public void setShardSizeFunctionAndRefresh(Function<ShardRouting, Long> shardSizeFunction) {
        this.shardSizeFunction = shardSizeFunction;
        ClusterInfoServiceUtils.refresh(this);
    }

    @Override
    public ClusterInfo getClusterInfo() {
        final ClusterInfo clusterInfo = super.getClusterInfo();
        return new SizeFakingClusterInfo(clusterInfo);
    }

    @Override
    List<NodeStats> adjustNodesStats(List<NodeStats> nodesStats) {
        final BiFunction<DiscoveryNode, FsInfo.Path, FsInfo.Path> diskUsageFunction = this.diskUsageFunction;
        if (diskUsageFunction == null) {
            return nodesStats;
        }

        return nodesStats.stream().map(nodeStats -> {
            final DiscoveryNode discoveryNode = nodeStats.getNode();
            final FsInfo oldFsInfo = nodeStats.getFs();
            return new NodeStats(discoveryNode, nodeStats.getTimestamp(), nodeStats.getIndices(), nodeStats.getOs(),
                nodeStats.getProcess(), nodeStats.getJvm(), nodeStats.getThreadPool(), new FsInfo(oldFsInfo.getTimestamp(),
                oldFsInfo.getIoStats(),
                StreamSupport.stream(oldFsInfo.spliterator(), false)
                    .map(fsInfoPath -> diskUsageFunction.apply(discoveryNode, fsInfoPath))
                    .toArray(FsInfo.Path[]::new)), nodeStats.getTransport(),
                nodeStats.getHttp(), nodeStats.getBreaker(), nodeStats.getScriptStats(), nodeStats.getDiscoveryStats(),
                nodeStats.getIngestStats(), nodeStats.getAdaptiveSelectionStats(), nodeStats.getIndexingPressureStats());
        }).collect(Collectors.toList());
    }

    class SizeFakingClusterInfo extends ClusterInfo {
        SizeFakingClusterInfo(ClusterInfo delegate) {
            super(delegate.getNodeLeastAvailableDiskUsages(), delegate.getNodeMostAvailableDiskUsages(),
                delegate.shardSizes, delegate.shardDataSetSizes, delegate.routingToDataPath, delegate.reservedSpace);
        }

        @Override
        public Long getShardSize(ShardRouting shardRouting) {
            final Function<ShardRouting, Long> shardSizeFunction = MockInternalClusterInfoService.this.shardSizeFunction;
            if (shardSizeFunction == null) {
                return super.getShardSize(shardRouting);
            }

            return shardSizeFunction.apply(shardRouting);
        }
    }

    @Override
    public void setUpdateFrequency(TimeValue updateFrequency) {
        super.setUpdateFrequency(updateFrequency);
    }
}
