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
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots;
import org.elasticsearch.xpack.searchablesnapshots.cache.CacheService;

import java.io.IOException;
import java.util.List;

public class TransportSearchableSnapshotCacheStoresAction extends TransportNodesAction<
    TransportSearchableSnapshotCacheStoresAction.Request,
    TransportSearchableSnapshotCacheStoresAction.NodesCacheFilesMetadata,
    TransportSearchableSnapshotCacheStoresAction.NodeRequest,
    TransportSearchableSnapshotCacheStoresAction.NodeCacheFilesMetadata> {

    public static final String ACTION_NAME = "internal:admin/xpack/searchable_snapshots/cache/store";

    public static final ActionType<NodesCacheFilesMetadata> TYPE = new ActionType<>(ACTION_NAME, NodesCacheFilesMetadata::new);

    private final CacheService cacheService;

    @Inject
    public TransportSearchableSnapshotCacheStoresAction(
        ThreadPool threadPool,
        ClusterService clusterService,
        TransportService transportService,
        SearchableSnapshots.CacheServiceSupplier cacheService,
        ActionFilters actionFilters
    ) {
        super(
            ACTION_NAME,
            threadPool,
            clusterService,
            transportService,
            actionFilters,
            Request::new,
            NodeRequest::new,
            ThreadPool.Names.MANAGEMENT,
            ThreadPool.Names.SAME,
            NodeCacheFilesMetadata.class
        );
        this.cacheService = cacheService.get();
    }

    @Override
    protected NodesCacheFilesMetadata newResponse(
        Request request,
        List<NodeCacheFilesMetadata> nodesCacheFilesMetadata,
        List<FailedNodeException> failures
    ) {
        return new NodesCacheFilesMetadata(clusterService.getClusterName(), nodesCacheFilesMetadata, failures);
    }

    @Override
    protected NodeRequest newNodeRequest(Request request) {
        return new NodeRequest(request);
    }

    @Override
    protected NodeCacheFilesMetadata newNodeResponse(StreamInput in) throws IOException {
        return new NodeCacheFilesMetadata(in);
    }

    @Override
    protected NodeCacheFilesMetadata nodeOperation(NodeRequest request, Task task) {
        assert cacheService != null;
        return new NodeCacheFilesMetadata(clusterService.localNode(), cacheService.getCachedSize(request.shardId, request.snapshotId));
    }

    public static final class Request extends BaseNodesRequest<Request> {

        private final SnapshotId snapshotId;
        private final ShardId shardId;

        public Request(SnapshotId snapshotId, ShardId shardId, DiscoveryNode[] nodes) {
            super(nodes);
            this.snapshotId = snapshotId;
            this.shardId = shardId;
        }

        public Request(StreamInput in) throws IOException {
            super(in);
            snapshotId = new SnapshotId(in);
            shardId = new ShardId(in);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            snapshotId.writeTo(out);
            shardId.writeTo(out);
        }
    }

    public static final class NodeRequest extends TransportRequest {

        private final SnapshotId snapshotId;
        private final ShardId shardId;

        public NodeRequest(Request request) {
            this.snapshotId = request.snapshotId;
            this.shardId = request.shardId;
        }

        public NodeRequest(StreamInput in) throws IOException {
            super(in);
            this.snapshotId = new SnapshotId(in);
            this.shardId = new ShardId(in);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            snapshotId.writeTo(out);
            shardId.writeTo(out);
        }
    }

    public static class NodeCacheFilesMetadata extends BaseNodeResponse {

        private final long bytesCached;

        public NodeCacheFilesMetadata(StreamInput in) throws IOException {
            super(in);
            bytesCached = in.readLong();
        }

        public NodeCacheFilesMetadata(DiscoveryNode node, long bytesCached) {
            super(node);
            this.bytesCached = bytesCached;
        }

        public long bytesCached() {
            return bytesCached;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeLong(bytesCached);
        }
    }

    public static class NodesCacheFilesMetadata extends BaseNodesResponse<NodeCacheFilesMetadata> {

        public NodesCacheFilesMetadata(StreamInput in) throws IOException {
            super(in);
        }

        public NodesCacheFilesMetadata(ClusterName clusterName, List<NodeCacheFilesMetadata> nodes, List<FailedNodeException> failures) {
            super(clusterName, nodes, failures);
        }

        @Override
        protected List<NodeCacheFilesMetadata> readNodesFrom(StreamInput in) throws IOException {
            return in.readList(NodeCacheFilesMetadata::new);
        }

        @Override
        protected void writeNodesTo(StreamOutput out, List<NodeCacheFilesMetadata> nodes) throws IOException {
            out.writeList(nodes);
        }
    }
}
