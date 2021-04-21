/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.searchablesnapshots.action.cache;

import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.nodes.BaseNodeResponse;
import org.elasticsearch.action.support.nodes.BaseNodesRequest;
import org.elasticsearch.action.support.nodes.BaseNodesResponse;
import org.elasticsearch.action.support.nodes.TransportNodesAction;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.xcontent.ToXContentFragment;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots;
import org.elasticsearch.xpack.searchablesnapshots.cache.shared.FrozenCacheService;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.function.Supplier;
import java.util.stream.Collectors;

/**
 * Node level stats about searchable snapshots caches.
 */
public class TransportSearchableSnapshotsNodeCachesStatsAction extends TransportNodesAction<
    TransportSearchableSnapshotsNodeCachesStatsAction.NodesRequest,
    TransportSearchableSnapshotsNodeCachesStatsAction.NodesCachesStatsResponse,
    TransportSearchableSnapshotsNodeCachesStatsAction.NodeRequest,
    TransportSearchableSnapshotsNodeCachesStatsAction.NodeCachesStatsResponse> {

    public static final String ACTION_NAME = "cluster:admin/xpack/searchable_snapshots/cache/stats";

    public static final ActionType<NodesCachesStatsResponse> TYPE = new ActionType<>(ACTION_NAME, NodesCachesStatsResponse::new);

    private final Supplier<FrozenCacheService> frozenCacheService;
    private final XPackLicenseState licenseState;

    @Inject
    public TransportSearchableSnapshotsNodeCachesStatsAction(
        ThreadPool threadPool,
        ClusterService clusterService,
        TransportService transportService,
        ActionFilters actionFilters,
        SearchableSnapshots.FrozenCacheServiceSupplier frozenCacheService,
        XPackLicenseState licenseState
    ) {
        super(
            ACTION_NAME,
            threadPool,
            clusterService,
            transportService,
            actionFilters,
            NodesRequest::new,
            NodeRequest::new,
            ThreadPool.Names.MANAGEMENT,
            ThreadPool.Names.SAME,
            NodeCachesStatsResponse.class
        );
        this.frozenCacheService = frozenCacheService;
        this.licenseState = licenseState;
    }

    @Override
    protected NodesCachesStatsResponse newResponse(
        NodesRequest request,
        List<NodeCachesStatsResponse> responses,
        List<FailedNodeException> failures
    ) {
        return new NodesCachesStatsResponse(clusterService.getClusterName(), responses, failures);
    }

    @Override
    protected NodeRequest newNodeRequest(NodesRequest request) {
        return new NodeRequest();
    }

    @Override
    protected NodeCachesStatsResponse newNodeResponse(StreamInput in) throws IOException {
        return new NodeCachesStatsResponse(in);
    }

    @Override
    protected void resolveRequest(NodesRequest request, ClusterState clusterState) {
        final ImmutableOpenMap<String, DiscoveryNode> dataNodes = clusterState.getNodes().getDataNodes();

        final DiscoveryNode[] resolvedNodes;
        if (request.nodesIds() == null || request.nodesIds().length == 0) {
            resolvedNodes = dataNodes.values().toArray(DiscoveryNode.class);
        } else {
            resolvedNodes = Arrays.stream(request.nodesIds())
                .filter(dataNodes::containsKey)
                .map(dataNodes::get)
                .collect(Collectors.toList())
                .toArray(DiscoveryNode[]::new);
        }
        request.setConcreteNodes(resolvedNodes);
    }

    @Override
    protected NodeCachesStatsResponse nodeOperation(NodeRequest request, Task task) {
        SearchableSnapshots.ensureValidLicense(licenseState);
        final FrozenCacheService.Stats frozenCacheStats;
        if (frozenCacheService.get() != null) {
            frozenCacheStats = frozenCacheService.get().getStats();
        } else {
            frozenCacheStats = FrozenCacheService.Stats.EMPTY;
        }
        return new NodeCachesStatsResponse(
            clusterService.localNode(),
            frozenCacheStats.getNumberOfRegions(),
            frozenCacheStats.getSize(),
            frozenCacheStats.getRegionSize(),
            frozenCacheStats.getWriteCount(),
            frozenCacheStats.getWriteBytes(),
            frozenCacheStats.getReadCount(),
            frozenCacheStats.getReadBytes(),
            frozenCacheStats.getEvictCount()
        );
    }

    public static final class NodeRequest extends TransportRequest {

        public NodeRequest() {}

        public NodeRequest(StreamInput in) throws IOException {
            super(in);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
        }
    }

    public static final class NodesRequest extends BaseNodesRequest<NodesRequest> {

        public NodesRequest(String[] nodes) {
            super(nodes);
        }

        public NodesRequest(StreamInput in) throws IOException {
            super(in);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
        }
    }

    public static class NodeCachesStatsResponse extends BaseNodeResponse implements ToXContentFragment {

        private final int numRegions;
        private final long size;
        private final long regionSize;
        private final long writes;
        private final long bytesWritten;
        private final long reads;
        private final long bytesRead;
        private final long evictions;

        public NodeCachesStatsResponse(
            DiscoveryNode node,
            int numRegions,
            long size,
            long regionSize,
            long writes,
            long bytesWritten,
            long reads,
            long bytesRead,
            long evictions
        ) {
            super(node);
            this.numRegions = numRegions;
            this.size = size;
            this.regionSize = regionSize;
            this.writes = writes;
            this.bytesWritten = bytesWritten;
            this.reads = reads;
            this.bytesRead = bytesRead;
            this.evictions = evictions;
        }

        public NodeCachesStatsResponse(StreamInput in) throws IOException {
            super(in);
            this.numRegions = in.readVInt();
            this.size = in.readVLong();
            this.regionSize = in.readVLong();
            this.writes = in.readVLong();
            this.bytesWritten = in.readVLong();
            this.reads = in.readVLong();
            this.bytesRead = in.readVLong();
            this.evictions = in.readVLong();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeVInt(numRegions);
            out.writeVLong(size);
            out.writeVLong(regionSize);
            out.writeVLong(writes);
            out.writeVLong(bytesWritten);
            out.writeVLong(reads);
            out.writeVLong(bytesRead);
            out.writeVLong(evictions);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject(getNode().getId());
            {
                builder.startObject("shared_cache");
                {
                    builder.field("reads", reads);
                    builder.humanReadableField("bytes_read_in_bytes", "bytes_read", ByteSizeValue.ofBytes(bytesRead));
                    builder.field("writes", writes);
                    builder.humanReadableField("bytes_written_in_bytes", "bytes_written", ByteSizeValue.ofBytes(bytesWritten));
                    builder.field("evictions", evictions);
                    builder.field("num_regions", numRegions);
                    builder.humanReadableField("size_in_bytes", "size", ByteSizeValue.ofBytes(size));
                    builder.humanReadableField("region_size_in_bytes", "region_size", ByteSizeValue.ofBytes(regionSize));
                }
                builder.endObject();
            }
            builder.endObject();
            return builder;
        }
    }

    public static class NodesCachesStatsResponse extends BaseNodesResponse<NodeCachesStatsResponse> implements ToXContentObject {

        public NodesCachesStatsResponse(StreamInput in) throws IOException {
            super(in);
        }

        public NodesCachesStatsResponse(ClusterName clusterName, List<NodeCachesStatsResponse> nodes, List<FailedNodeException> failures) {
            super(clusterName, nodes, failures);
        }

        @Override
        protected List<NodeCachesStatsResponse> readNodesFrom(StreamInput in) throws IOException {
            return in.readList(NodeCachesStatsResponse::new);
        }

        @Override
        protected void writeNodesTo(StreamOutput out, List<NodeCachesStatsResponse> nodes) throws IOException {
            out.writeList(nodes);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            {
                builder.startObject("nodes");
                for (NodeCachesStatsResponse node : getNodes()) {
                    node.toXContent(builder, params);
                }
                builder.endObject();
            }
            builder.endObject();
            return builder;
        }

    }
}
