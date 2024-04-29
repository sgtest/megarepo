/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.search;

import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.xcontent.ToXContentObject;
import org.elasticsearch.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.Base64;
import java.util.Objects;

public final class OpenPointInTimeResponse extends ActionResponse implements ToXContentObject {
    private final BytesReference pointInTimeId;

    public OpenPointInTimeResponse(BytesReference pointInTimeId) {
        this.pointInTimeId = Objects.requireNonNull(pointInTimeId, "Point in time parameter must be not null");
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeBytesReference(pointInTimeId);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field("id", Base64.getUrlEncoder().encodeToString(BytesReference.toBytes(pointInTimeId)));
        builder.endObject();
        return builder;
    }

    public BytesReference getPointInTimeId() {
        return pointInTimeId;
    }

}
