/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.indices.stats;

import org.elasticsearch.action.support.DefaultShardOperationFailedException;
import org.elasticsearch.action.support.broadcast.BroadcastResponse;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

public class FieldUsageStatsResponse extends BroadcastResponse {
    private final Map<String, List<FieldUsageShardResponse>> stats;

    FieldUsageStatsResponse(int totalShards, int successfulShards, int failedShards,
                            List<DefaultShardOperationFailedException> shardFailures,
                            Map<String, List<FieldUsageShardResponse>> stats) {
        super(totalShards, successfulShards, failedShards, shardFailures);
        this.stats = stats;
    }

    FieldUsageStatsResponse(StreamInput in) throws IOException {
        super(in);
        stats = in.readMap(StreamInput::readString, i -> i.readList(FieldUsageShardResponse::new));
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeMap(stats, StreamOutput::writeString, StreamOutput::writeList);
    }

    public Map<String, List<FieldUsageShardResponse>> getStats() {
        return stats;
    }

    @Override
    protected void addCustomXContentFields(XContentBuilder builder, Params params) throws IOException {
        final List<Map.Entry<String, List<FieldUsageShardResponse>>> sortedEntries =
            stats.entrySet().stream().sorted(Map.Entry.comparingByKey()).collect(Collectors.toList());
        for (Map.Entry<String, List<FieldUsageShardResponse>> entry : sortedEntries) {
            builder.startObject(entry.getKey());
            builder.startArray("shards");
            for (FieldUsageShardResponse resp : entry.getValue()) {
                resp.toXContent(builder, params);
            }
            builder.endArray();
            builder.endObject();
        }
    }
}
