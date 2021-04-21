/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.metadata;

import org.elasticsearch.cluster.AbstractDiffable;
import org.elasticsearch.cluster.Diffable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.Objects;

/**
 * Contains data about a single node's shutdown readiness.
 */
public class SingleNodeShutdownMetadata extends AbstractDiffable<SingleNodeShutdownMetadata>
    implements
        ToXContentObject,
        Diffable<SingleNodeShutdownMetadata> {

    private static final ParseField NODE_ID_FIELD = new ParseField("node_id");
    private static final ParseField TYPE_FIELD = new ParseField("type");
    private static final ParseField REASON_FIELD = new ParseField("reason");
    private static final ParseField STATUS_FIELD = new ParseField("shutdown_status");
    private static final String STARTED_AT_READABLE_FIELD = "shutdown_started";
    private static final ParseField STARTED_AT_MILLIS_FIELD = new ParseField(STARTED_AT_READABLE_FIELD + "millis");
    private static final ParseField SHARD_MIGRATION_FIELD = new ParseField("shard_migration");
    private static final ParseField PERSISTENT_TASKS_FIELD = new ParseField("persistent_tasks");
    private static final ParseField PLUGINS_STATUS = new ParseField("plugins");

    public static final ConstructingObjectParser<SingleNodeShutdownMetadata, Void> PARSER = new ConstructingObjectParser<>(
        "node_shutdown_info",
        a -> new SingleNodeShutdownMetadata(
            (String) a[0],
            Type.valueOf((String) a[1]),
            (String) a[2],
            Status.valueOf((String) a[3]),
            (long) a[4],
            (NodeShutdownComponentStatus) a[5],
            (NodeShutdownComponentStatus) a[6],
            (NodeShutdownComponentStatus) a[7]
        )
    );

    static {
        PARSER.declareString(ConstructingObjectParser.constructorArg(), NODE_ID_FIELD);
        PARSER.declareString(ConstructingObjectParser.constructorArg(), TYPE_FIELD);
        PARSER.declareString(ConstructingObjectParser.constructorArg(), REASON_FIELD);
        PARSER.declareString(ConstructingObjectParser.constructorArg(), STATUS_FIELD);
        PARSER.declareLong(ConstructingObjectParser.constructorArg(), STARTED_AT_MILLIS_FIELD);
        PARSER.declareObject(
            ConstructingObjectParser.constructorArg(),
            (parser, context) -> NodeShutdownComponentStatus.parse(parser),
            SHARD_MIGRATION_FIELD
        );
        PARSER.declareObject(
            ConstructingObjectParser.constructorArg(),
            (parser, context) -> NodeShutdownComponentStatus.parse(parser),
            PERSISTENT_TASKS_FIELD
        );
        PARSER.declareObject(
            ConstructingObjectParser.constructorArg(),
            (parser, context) -> NodeShutdownComponentStatus.parse(parser),
            PLUGINS_STATUS
        );
    }

    public static SingleNodeShutdownMetadata parse(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    private final String nodeId;
    private final Type type;
    private final String reason;
    private final Status status;
    private final long startedAtMillis;
    private final NodeShutdownComponentStatus shardMigrationStatus;
    private final NodeShutdownComponentStatus persistentTasksStatus;
    private final NodeShutdownComponentStatus pluginsStatus;

    /**
     * @param nodeId The node ID that this shutdown metadata refers to.
     * @param type The type of shutdown. See {@link Type}.
     * @param reason The reason for the shutdown, per the original shutdown request.
     * @param status The overall status of this shutdown.
     * @param startedAtMillis The timestamp at which this shutdown was requested.
     * @param shardMigrationStatus The status of shard migrations away from this node.
     * @param persistentTasksStatus The status of persistent task migration away from this node.
     * @param pluginsStatus The status of plugin shutdown on this node.
     */
    private SingleNodeShutdownMetadata(
        String nodeId,
        Type type,
        String reason,
        Status status,
        long startedAtMillis,
        NodeShutdownComponentStatus shardMigrationStatus,
        NodeShutdownComponentStatus persistentTasksStatus,
        NodeShutdownComponentStatus pluginsStatus
    ) {
        this.nodeId = Objects.requireNonNull(nodeId, "node ID must not be null");
        this.type = Objects.requireNonNull(type, "shutdown type must not be null");
        this.reason = Objects.requireNonNull(reason, "shutdown reason must not be null");
        this.status = status;
        this.startedAtMillis = startedAtMillis;
        this.shardMigrationStatus = Objects.requireNonNull(shardMigrationStatus, "shard migration status must not be null");
        this.persistentTasksStatus = Objects.requireNonNull(persistentTasksStatus, "persistent tasks status must not be null");
        this.pluginsStatus = Objects.requireNonNull(pluginsStatus, "plugins status must not be null");
    }

    public SingleNodeShutdownMetadata(StreamInput in) throws IOException {
        this.nodeId = in.readString();
        this.type = in.readEnum(Type.class);
        this.reason = in.readString();
        this.status = in.readEnum(Status.class);
        this.startedAtMillis = in.readVLong();
        this.shardMigrationStatus = new NodeShutdownComponentStatus(in);
        this.persistentTasksStatus = new NodeShutdownComponentStatus(in);
        this.pluginsStatus = new NodeShutdownComponentStatus(in);
    }

    /**
     * @return The ID of the node this {@link SingleNodeShutdownMetadata} concerns.
     */
    public String getNodeId() {
        return nodeId;
    }

    /**
     * @return The type of shutdown this is (shutdown vs. permanent).
     */
    public Type getType() {
        return type;
    }

    /**
     * @return The user-supplied reason this node is shutting down.
     */
    public String getReason() {
        return reason;
    }

    /**
     * @return The status of this node's shutdown.
     */
    public Status getStatus() {
        return status;
    }

    /**
     * @return The timestamp that this shutdown procedure was started.
     */
    public long getStartedAtMillis() {
        return startedAtMillis;
    }

    /**
     * @return The status of shard migrations off of this node.
     */
    public NodeShutdownComponentStatus getShardMigrationStatus() {
        return shardMigrationStatus;
    }

    /**
     * @return The status of persistent task shutdown on this node.
     */
    public NodeShutdownComponentStatus getPersistentTasksStatus() {
        return persistentTasksStatus;
    }

    /**
     * @return The status of plugin shutdown on this node.
     */
    public NodeShutdownComponentStatus getPluginsStatus() {
        return pluginsStatus;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeString(nodeId);
        out.writeEnum(type);
        out.writeString(reason);
        out.writeEnum(status);
        out.writeVLong(startedAtMillis);
        shardMigrationStatus.writeTo(out);
        persistentTasksStatus.writeTo(out);
        pluginsStatus.writeTo(out);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        {
            builder.field(NODE_ID_FIELD.getPreferredName(), nodeId);
            builder.field(TYPE_FIELD.getPreferredName(), type);
            builder.field(REASON_FIELD.getPreferredName(), reason);
            builder.field(STATUS_FIELD.getPreferredName(), status);
            builder.timeField(STARTED_AT_MILLIS_FIELD.getPreferredName(), STARTED_AT_READABLE_FIELD, startedAtMillis);
            builder.field(SHARD_MIGRATION_FIELD.getPreferredName(), shardMigrationStatus);
            builder.field(PERSISTENT_TASKS_FIELD.getPreferredName(), persistentTasksStatus);
            builder.field(PLUGINS_STATUS.getPreferredName(), pluginsStatus);
        }
        builder.endObject();

        return builder;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if ((o instanceof SingleNodeShutdownMetadata) == false) return false;
        SingleNodeShutdownMetadata that = (SingleNodeShutdownMetadata) o;
        return getStartedAtMillis() == that.getStartedAtMillis()
            && getNodeId().equals(that.getNodeId())
            && getType() == that.getType()
            && getReason().equals(that.getReason())
            && status == that.status
            && shardMigrationStatus.equals(that.shardMigrationStatus)
            && persistentTasksStatus.equals(that.persistentTasksStatus)
            && pluginsStatus.equals(that.pluginsStatus);
    }

    @Override
    public int hashCode() {
        return Objects.hash(
            getNodeId(),
            getType(),
            getReason(),
            status,
            getStartedAtMillis(),
            shardMigrationStatus,
            persistentTasksStatus,
            pluginsStatus
        );
    }

    public static Builder builder() {
        return new Builder();
    }

    public static Builder builder(SingleNodeShutdownMetadata original) {
        if (original == null) {
            return builder();
        }
        return new Builder()
            .setNodeId(original.getNodeId())
            .setType(original.getType())
            .setReason(original.getReason())
            .setStatus(original.getStatus())
            .setStartedAtMillis(original.getStartedAtMillis())
            .setShardMigrationStatus(original.getShardMigrationStatus())
            .setPersistentTasksStatus(original.getPersistentTasksStatus())
            .setPluginsStatus(original.getPluginsStatus());
    }

    public static class Builder {
        private String nodeId;
        private Type type;
        private String reason;
        private long startedAtMillis = -1;
        private Status status = Status.IN_PROGRESS;
        private NodeShutdownComponentStatus shardMigrationStatus = new NodeShutdownComponentStatus();
        private NodeShutdownComponentStatus persistentTasksStatus = new NodeShutdownComponentStatus();
        private NodeShutdownComponentStatus pluginsStatus = new NodeShutdownComponentStatus();

        private Builder() {}

        /**
         * @param nodeId The node ID this metadata refers to.
         * @return This builder.
         */
        public Builder setNodeId(String nodeId) {
            this.nodeId = nodeId;
            return this;
        }

        /**
         * @param type The type of shutdown.
         * @return This builder.
         */
        public Builder setType(Type type) {
            this.type = type;
            return this;
        }

        /**
         * @param reason The reason for the shutdown. An arbitrary string provided by the user.
         * @return This builder.
         */
        public Builder setReason(String reason) {
            this.reason = reason;
            return this;
        }

        /**
         * @param startedAtMillis The timestamp at which this shutdown was requested.
         * @return This builder.
         */
        public Builder setStartedAtMillis(long startedAtMillis) {
            this.startedAtMillis = startedAtMillis;
            return this;
        }

        /**
         * @param status The status of this shutdown.
         * @return This builder.
         */
        public Builder setStatus(Status status) {
            this.status = status;
            return this;
        }

        /**
         * @param shardMigrationStatus An object describing the status of shard migration away from this node.
         * @return This builder.
         */
        public Builder setShardMigrationStatus(NodeShutdownComponentStatus shardMigrationStatus) {
            this.shardMigrationStatus = shardMigrationStatus;
            return this;
        }

        /**
         * @param persistentTasksStatus An object describing the status of persistent task migration away from this node.
         * @return This builder.
         */
        public Builder setPersistentTasksStatus(NodeShutdownComponentStatus persistentTasksStatus) {
            this.persistentTasksStatus = persistentTasksStatus;
            return this;
        }

        /**
         * @param pluginsStatus An object describing the status of plugin shutdown on this node.
         * @return
         */
        public Builder setPluginsStatus(NodeShutdownComponentStatus pluginsStatus) {
            this.pluginsStatus = pluginsStatus;
            return this;
        }

        public SingleNodeShutdownMetadata build() {
            if (startedAtMillis == -1) {
                throw new IllegalArgumentException("start timestamp must be set");
            }
            return new SingleNodeShutdownMetadata(
                nodeId,
                type,
                reason,
                status,
                startedAtMillis,
                shardMigrationStatus,
                persistentTasksStatus,
                pluginsStatus
            );
        }
    }

    /**
     * Describes the type of node shutdown - permanent (REMOVE) or temporary (RESTART).
     */
    public enum Type {
        REMOVE,
        RESTART
    }

    /**
     * Describes the status of a component of shutdown.
     */
    public enum Status {
        NOT_STARTED,
        IN_PROGRESS,
        STALLED,
        COMPLETE
    }
}
