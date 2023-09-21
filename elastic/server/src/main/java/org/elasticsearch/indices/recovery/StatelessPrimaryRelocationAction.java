/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.indices.recovery;

import org.elasticsearch.TransportVersions;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.index.shard.ShardId;

import java.io.IOException;
import java.util.Objects;

public class StatelessPrimaryRelocationAction {

    public static final ActionType<ActionResponse.Empty> INSTANCE = new ActionType<>(
        "internal:index/shard/recovery/stateless_primary_relocation",
        in -> ActionResponse.Empty.INSTANCE
    );

    public static class Request extends ActionRequest {

        private final long recoveryId;
        private final ShardId shardId;
        private final DiscoveryNode targetNode;
        private final String targetAllocationId;
        private final long clusterStateVersion;

        public Request(long recoveryId, ShardId shardId, DiscoveryNode targetNode, String targetAllocationId, long clusterStateVersion) {
            this.recoveryId = recoveryId;
            this.shardId = shardId;
            this.targetNode = targetNode;
            this.targetAllocationId = targetAllocationId;
            this.clusterStateVersion = clusterStateVersion;
        }

        public Request(StreamInput in) throws IOException {
            super(in);
            recoveryId = in.readVLong();
            shardId = new ShardId(in);
            targetNode = new DiscoveryNode(in);
            targetAllocationId = in.readString();
            if (in.getTransportVersion().onOrAfter(TransportVersions.WAIT_FOR_CLUSTER_STATE_IN_RECOVERY_ADDED)) {
                clusterStateVersion = in.readVLong();
            } else {
                clusterStateVersion = 0L; // temporary bwc: do not wait for cluster state to be applied
            }
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeVLong(recoveryId);
            shardId.writeTo(out);
            targetNode.writeTo(out);
            out.writeString(targetAllocationId);
            if (out.getTransportVersion().onOrAfter(TransportVersions.WAIT_FOR_CLUSTER_STATE_IN_RECOVERY_ADDED)) {
                out.writeVLong(clusterStateVersion);
            } // temporary bwc: just omit it, the receiver doesn't wait for a cluster state anyway
        }

        public long recoveryId() {
            return recoveryId;
        }

        public ShardId shardId() {
            return shardId;
        }

        public DiscoveryNode targetNode() {
            return targetNode;
        }

        public String targetAllocationId() {
            return targetAllocationId;
        }

        public long clusterStateVersion() {
            return clusterStateVersion;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Request request = (Request) o;
            return recoveryId == request.recoveryId
                && shardId.equals(request.shardId)
                && targetNode.equals(request.targetNode)
                && targetAllocationId.equals(request.targetAllocationId)
                && clusterStateVersion == request.clusterStateVersion;
        }

        @Override
        public int hashCode() {
            return Objects.hash(recoveryId, shardId, targetNode, targetAllocationId, clusterStateVersion);
        }
    }
}
