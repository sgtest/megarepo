/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ccr.action;

import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.support.master.AcknowledgedRequest;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ccr.AutoFollowMetadata.AutoFollowPattern;

import java.io.IOException;
import java.util.List;
import java.util.Objects;

import static org.elasticsearch.action.ValidateActions.addValidationError;

public class PutAutoFollowPatternAction extends Action<AcknowledgedResponse> {

    public static final String NAME = "cluster:admin/xpack/ccr/auto_follow_pattern/put";
    public static final PutAutoFollowPatternAction INSTANCE = new PutAutoFollowPatternAction();

    private PutAutoFollowPatternAction() {
        super(NAME);
    }

    @Override
    public AcknowledgedResponse newResponse() {
        return new AcknowledgedResponse();
    }

    public static class Request extends AcknowledgedRequest<Request> implements ToXContentObject {

        static final ParseField LEADER_CLUSTER_ALIAS_FIELD = new ParseField("leader_cluster_alias");
        static final ParseField LEADER_INDEX_PATTERNS_FIELD = new ParseField("leader_index_patterns");
        static final ParseField FOLLOW_INDEX_NAME_PATTERN_FIELD = new ParseField("follow_index_name_pattern");

        private static final ObjectParser<Request, String> PARSER = new ObjectParser<>("put_auto_follow_pattern_request", Request::new);

        static {
            PARSER.declareString(Request::setLeaderClusterAlias, LEADER_CLUSTER_ALIAS_FIELD);
            PARSER.declareStringArray(Request::setLeaderIndexPatterns, LEADER_INDEX_PATTERNS_FIELD);
            PARSER.declareString(Request::setFollowIndexNamePattern, FOLLOW_INDEX_NAME_PATTERN_FIELD);
            PARSER.declareInt(Request::setMaxBatchOperationCount, AutoFollowPattern.MAX_BATCH_OPERATION_COUNT);
            PARSER.declareInt(Request::setMaxConcurrentReadBatches, AutoFollowPattern.MAX_CONCURRENT_READ_BATCHES);
            PARSER.declareLong(Request::setMaxOperationSizeInBytes, AutoFollowPattern.MAX_BATCH_SIZE_IN_BYTES);
            PARSER.declareInt(Request::setMaxConcurrentWriteBatches, AutoFollowPattern.MAX_CONCURRENT_WRITE_BATCHES);
            PARSER.declareInt(Request::setMaxWriteBufferSize, AutoFollowPattern.MAX_WRITE_BUFFER_SIZE);
            PARSER.declareField(Request::setRetryTimeout,
                (p, c) -> TimeValue.parseTimeValue(p.text(), AutoFollowPattern.RETRY_TIMEOUT.getPreferredName()),
                ShardFollowTask.RETRY_TIMEOUT, ObjectParser.ValueType.STRING);
            PARSER.declareField(Request::setIdleShardRetryDelay,
                (p, c) -> TimeValue.parseTimeValue(p.text(), AutoFollowPattern.IDLE_SHARD_RETRY_DELAY.getPreferredName()),
                ShardFollowTask.IDLE_SHARD_RETRY_DELAY, ObjectParser.ValueType.STRING);
        }

        public static Request fromXContent(XContentParser parser, String remoteClusterAlias) throws IOException {
            Request request = PARSER.parse(parser, null);
            if (remoteClusterAlias != null) {
                if (request.leaderClusterAlias == null) {
                    request.leaderClusterAlias = remoteClusterAlias;
                } else {
                    if (request.leaderClusterAlias.equals(remoteClusterAlias) == false) {
                        throw new IllegalArgumentException("provided leaderClusterAlias is not equal");
                    }
                }
            }
            return request;
        }

        private String leaderClusterAlias;
        private List<String> leaderIndexPatterns;
        private String followIndexNamePattern;

        private Integer maxBatchOperationCount;
        private Integer maxConcurrentReadBatches;
        private Long maxOperationSizeInBytes;
        private Integer maxConcurrentWriteBatches;
        private Integer maxWriteBufferSize;
        private TimeValue retryTimeout;
        private TimeValue idleShardRetryDelay;

        @Override
        public ActionRequestValidationException validate() {
            ActionRequestValidationException validationException = null;
            if (leaderClusterAlias == null) {
                validationException = addValidationError("leaderClusterAlias is missing", validationException);
            }
            if (leaderIndexPatterns == null || leaderIndexPatterns.isEmpty()) {
                validationException = addValidationError("leaderIndexPatterns is missing", validationException);
            }
            return validationException;
        }

        public String getLeaderClusterAlias() {
            return leaderClusterAlias;
        }

        public void setLeaderClusterAlias(String leaderClusterAlias) {
            this.leaderClusterAlias = leaderClusterAlias;
        }

        public List<String> getLeaderIndexPatterns() {
            return leaderIndexPatterns;
        }

        public void setLeaderIndexPatterns(List<String> leaderIndexPatterns) {
            this.leaderIndexPatterns = leaderIndexPatterns;
        }

        public String getFollowIndexNamePattern() {
            return followIndexNamePattern;
        }

        public void setFollowIndexNamePattern(String followIndexNamePattern) {
            this.followIndexNamePattern = followIndexNamePattern;
        }

        public Integer getMaxBatchOperationCount() {
            return maxBatchOperationCount;
        }

        public void setMaxBatchOperationCount(Integer maxBatchOperationCount) {
            this.maxBatchOperationCount = maxBatchOperationCount;
        }

        public Integer getMaxConcurrentReadBatches() {
            return maxConcurrentReadBatches;
        }

        public void setMaxConcurrentReadBatches(Integer maxConcurrentReadBatches) {
            this.maxConcurrentReadBatches = maxConcurrentReadBatches;
        }

        public Long getMaxOperationSizeInBytes() {
            return maxOperationSizeInBytes;
        }

        public void setMaxOperationSizeInBytes(Long maxOperationSizeInBytes) {
            this.maxOperationSizeInBytes = maxOperationSizeInBytes;
        }

        public Integer getMaxConcurrentWriteBatches() {
            return maxConcurrentWriteBatches;
        }

        public void setMaxConcurrentWriteBatches(Integer maxConcurrentWriteBatches) {
            this.maxConcurrentWriteBatches = maxConcurrentWriteBatches;
        }

        public Integer getMaxWriteBufferSize() {
            return maxWriteBufferSize;
        }

        public void setMaxWriteBufferSize(Integer maxWriteBufferSize) {
            this.maxWriteBufferSize = maxWriteBufferSize;
        }

        public TimeValue getRetryTimeout() {
            return retryTimeout;
        }

        public void setRetryTimeout(TimeValue retryTimeout) {
            this.retryTimeout = retryTimeout;
        }

        public TimeValue getIdleShardRetryDelay() {
            return idleShardRetryDelay;
        }

        public void setIdleShardRetryDelay(TimeValue idleShardRetryDelay) {
            this.idleShardRetryDelay = idleShardRetryDelay;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            leaderClusterAlias = in.readString();
            leaderIndexPatterns = in.readList(StreamInput::readString);
            followIndexNamePattern = in.readOptionalString();
            maxBatchOperationCount = in.readOptionalVInt();
            maxConcurrentReadBatches = in.readOptionalVInt();
            maxOperationSizeInBytes = in.readOptionalLong();
            maxConcurrentWriteBatches = in.readOptionalVInt();
            maxWriteBufferSize = in.readOptionalVInt();
            retryTimeout = in.readOptionalTimeValue();
            idleShardRetryDelay = in.readOptionalTimeValue();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(leaderClusterAlias);
            out.writeStringList(leaderIndexPatterns);
            out.writeOptionalString(followIndexNamePattern);
            out.writeOptionalVInt(maxBatchOperationCount);
            out.writeOptionalVInt(maxConcurrentReadBatches);
            out.writeOptionalLong(maxOperationSizeInBytes);
            out.writeOptionalVInt(maxConcurrentWriteBatches);
            out.writeOptionalVInt(maxWriteBufferSize);
            out.writeOptionalTimeValue(retryTimeout);
            out.writeOptionalTimeValue(idleShardRetryDelay);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            {
                builder.field(LEADER_CLUSTER_ALIAS_FIELD.getPreferredName(), leaderClusterAlias);
                builder.field(LEADER_INDEX_PATTERNS_FIELD.getPreferredName(), leaderIndexPatterns);
                if (followIndexNamePattern != null) {
                    builder.field(FOLLOW_INDEX_NAME_PATTERN_FIELD.getPreferredName(), followIndexNamePattern);
                }
                if (maxBatchOperationCount != null) {
                    builder.field(ShardFollowTask.MAX_BATCH_OPERATION_COUNT.getPreferredName(), maxBatchOperationCount);
                }
                if (maxOperationSizeInBytes != null) {
                    builder.field(ShardFollowTask.MAX_BATCH_SIZE_IN_BYTES.getPreferredName(), maxOperationSizeInBytes);
                }
                if (maxWriteBufferSize != null) {
                    builder.field(ShardFollowTask.MAX_WRITE_BUFFER_SIZE.getPreferredName(), maxWriteBufferSize);
                }
                if (maxConcurrentReadBatches != null) {
                    builder.field(ShardFollowTask.MAX_CONCURRENT_READ_BATCHES.getPreferredName(), maxConcurrentReadBatches);
                }
                if (maxConcurrentWriteBatches != null) {
                    builder.field(ShardFollowTask.MAX_CONCURRENT_WRITE_BATCHES.getPreferredName(), maxConcurrentWriteBatches);
                }
                if (retryTimeout != null) {
                    builder.field(ShardFollowTask.RETRY_TIMEOUT.getPreferredName(), retryTimeout.getStringRep());
                }
                if (idleShardRetryDelay != null) {
                    builder.field(ShardFollowTask.IDLE_SHARD_RETRY_DELAY.getPreferredName(), idleShardRetryDelay.getStringRep());
                }
            }
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Request request = (Request) o;
            return Objects.equals(leaderClusterAlias, request.leaderClusterAlias) &&
                Objects.equals(leaderIndexPatterns, request.leaderIndexPatterns) &&
                Objects.equals(followIndexNamePattern, request.followIndexNamePattern) &&
                Objects.equals(maxBatchOperationCount, request.maxBatchOperationCount) &&
                Objects.equals(maxConcurrentReadBatches, request.maxConcurrentReadBatches) &&
                Objects.equals(maxOperationSizeInBytes, request.maxOperationSizeInBytes) &&
                Objects.equals(maxConcurrentWriteBatches, request.maxConcurrentWriteBatches) &&
                Objects.equals(maxWriteBufferSize, request.maxWriteBufferSize) &&
                Objects.equals(retryTimeout, request.retryTimeout) &&
                Objects.equals(idleShardRetryDelay, request.idleShardRetryDelay);
        }

        @Override
        public int hashCode() {
            return Objects.hash(
                leaderClusterAlias,
                leaderIndexPatterns,
                followIndexNamePattern,
                maxBatchOperationCount,
                maxConcurrentReadBatches,
                maxOperationSizeInBytes,
                maxConcurrentWriteBatches,
                maxWriteBufferSize,
                retryTimeout,
                idleShardRetryDelay
            );
        }
    }

}
