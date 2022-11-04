/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.shard;

import org.elasticsearch.action.admin.indices.stats.IndexShardStats;
import org.elasticsearch.action.admin.indices.stats.IndexStats;
import org.elasticsearch.action.admin.indices.stats.IndicesStatsResponse;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.xcontent.ConstructingObjectParser;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xcontent.ToXContentFragment;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentParser;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.OptionalDouble;
import java.util.OptionalLong;

public class IndexWriteLoad implements Writeable, ToXContentFragment {
    public static final ParseField SHARDS_WRITE_LOAD_FIELD = new ParseField("loads");
    public static final ParseField SHARDS_UPTIME_IN_MILLIS = new ParseField("uptimes");
    private static final Double UNKNOWN_LOAD = -1.0;
    private static final long UNKNOWN_UPTIME = -1;

    @SuppressWarnings("unchecked")
    private static final ConstructingObjectParser<IndexWriteLoad, Void> PARSER = new ConstructingObjectParser<>(
        "index_write_load_parser",
        false,
        (args, unused) -> IndexWriteLoad.create((List<Double>) args[0], (List<Long>) args[1])
    );

    static {
        PARSER.declareDoubleArray(ConstructingObjectParser.constructorArg(), SHARDS_WRITE_LOAD_FIELD);
        PARSER.declareLongArray(ConstructingObjectParser.constructorArg(), SHARDS_UPTIME_IN_MILLIS);
    }

    public static IndexWriteLoad create(List<Double> shardsWriteLoad, List<Long> shardsUptimeInMillis) {
        if (shardsWriteLoad.size() != shardsUptimeInMillis.size()) {
            assert false;
            throw new IllegalArgumentException(
                "The same number of shard write loads and shard uptimes should be provided, but "
                    + shardsWriteLoad
                    + " "
                    + shardsUptimeInMillis
                    + " were provided"
            );
        }

        if (shardsWriteLoad.isEmpty()) {
            assert false;
            throw new IllegalArgumentException("At least one shard write load and uptime should be provided, but none was provided");
        }

        return new IndexWriteLoad(
            shardsWriteLoad.stream().mapToDouble(shardLoad -> shardLoad).toArray(),
            shardsUptimeInMillis.stream().mapToLong(shardUptime -> shardUptime).toArray()
        );
    }

    @Nullable
    public static IndexWriteLoad fromStats(IndexMetadata indexMetadata, @Nullable IndicesStatsResponse indicesStatsResponse) {
        if (indicesStatsResponse == null) {
            return null;
        }

        final IndexStats indexStats = indicesStatsResponse.getIndex(indexMetadata.getIndex().getName());
        if (indexStats == null) {
            return null;
        }

        final int numberOfShards = indexMetadata.getNumberOfShards();
        final var indexWriteLoadBuilder = IndexWriteLoad.builder(numberOfShards);
        final var indexShards = indexStats.getIndexShards();
        for (IndexShardStats indexShardsStats : indexShards.values()) {
            final var shardStats = Arrays.stream(indexShardsStats.getShards())
                .filter(stats -> stats.getShardRouting().primary())
                .findFirst()
                // Fallback to a replica if for some reason we couldn't find the primary stats
                .orElse(indexShardsStats.getAt(0));
            final var indexingShardStats = shardStats.getStats().getIndexing().getTotal();
            indexWriteLoadBuilder.withShardWriteLoad(
                shardStats.getShardRouting().id(),
                indexingShardStats.getWriteLoad(),
                indexingShardStats.getTotalActiveTimeInMillis()
            );
        }
        return indexWriteLoadBuilder.build();
    }

    private final double[] shardWriteLoad;
    private final long[] shardUptimeInMillis;

    private IndexWriteLoad(double[] shardWriteLoad, long[] shardUptimeInMillis) {
        assert shardWriteLoad.length == shardUptimeInMillis.length;
        this.shardWriteLoad = shardWriteLoad;
        this.shardUptimeInMillis = shardUptimeInMillis;
    }

    public IndexWriteLoad(StreamInput in) throws IOException {
        this(in.readDoubleArray(), in.readLongArray());
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeDoubleArray(shardWriteLoad);
        out.writeLongArray(shardUptimeInMillis);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.field(SHARDS_WRITE_LOAD_FIELD.getPreferredName(), shardWriteLoad);
        builder.field(SHARDS_UPTIME_IN_MILLIS.getPreferredName(), shardUptimeInMillis);
        return builder;
    }

    public static IndexWriteLoad fromXContent(XContentParser parser) throws IOException {
        return PARSER.parse(parser, null);
    }

    public OptionalDouble getWriteLoadForShard(int shardId) {
        assertShardInBounds(shardId);

        double load = shardWriteLoad[shardId];
        return load != UNKNOWN_LOAD ? OptionalDouble.of(load) : OptionalDouble.empty();
    }

    public OptionalLong getUptimeInMillisForShard(int shardId) {
        assertShardInBounds(shardId);

        long uptime = shardUptimeInMillis[shardId];
        return uptime != UNKNOWN_UPTIME ? OptionalLong.of(uptime) : OptionalLong.empty();
    }

    // Visible for testing
    public int numberOfShards() {
        return shardWriteLoad.length;
    }

    private void assertShardInBounds(int shardId) {
        assert shardId >= 0 : "Unexpected shard id " + shardId;
        assert shardId < shardWriteLoad.length : "Unexpected shard id " + shardId + ", expected < " + shardWriteLoad.length;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        IndexWriteLoad that = (IndexWriteLoad) o;
        return Arrays.equals(shardWriteLoad, that.shardWriteLoad) && Arrays.equals(shardUptimeInMillis, that.shardUptimeInMillis);
    }

    @Override
    public int hashCode() {
        int result = Arrays.hashCode(shardWriteLoad);
        result = 31 * result + Arrays.hashCode(shardUptimeInMillis);
        return result;
    }

    public static Builder builder(int numShards) {
        assert numShards > 0 : "A positive number of shards should be provided";
        return new Builder(numShards);
    }

    public static class Builder {
        final double[] shardWriteLoad;
        final long[] uptimeInMillis;

        private Builder(int numShards) {
            this.shardWriteLoad = new double[numShards];
            this.uptimeInMillis = new long[numShards];
            Arrays.fill(shardWriteLoad, UNKNOWN_LOAD);
            Arrays.fill(uptimeInMillis, UNKNOWN_UPTIME);
        }

        public void withShardWriteLoad(int shardId, double load, long uptimeInMillis) {
            if (shardId >= this.shardWriteLoad.length) {
                throw new IllegalArgumentException();
            }

            this.shardWriteLoad[shardId] = load;
            this.uptimeInMillis[shardId] = uptimeInMillis;
        }

        public IndexWriteLoad build() {
            return new IndexWriteLoad(shardWriteLoad, uptimeInMillis);
        }
    }
}
