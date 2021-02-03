/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.indices.refresh;

import org.elasticsearch.action.support.DefaultShardOperationFailedException;
import org.elasticsearch.action.support.broadcast.BroadcastResponse;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;

/**
 * The response of a refresh action.
 */
public class RefreshResponse extends BroadcastResponse {

    private static final ConstructingObjectParser<RefreshResponse, Void> PARSER = new ConstructingObjectParser<>("refresh", true,
        arg -> {
            BroadcastResponse response = (BroadcastResponse) arg[0];
            return new RefreshResponse(response.getTotalShards(), response.getSuccessfulShards(), response.getFailedShards(),
                Arrays.asList(response.getShardFailures()));
        });

    static {
        declareBroadcastFields(PARSER);
    }

    RefreshResponse(StreamInput in) throws IOException {
        super(in);
    }

    RefreshResponse(int totalShards, int successfulShards, int failedShards, List<DefaultShardOperationFailedException> shardFailures) {
        super(totalShards, successfulShards, failedShards, shardFailures);
    }

    public static RefreshResponse fromXContent(XContentParser parser) {
        return PARSER.apply(parser, null);
    }
}
