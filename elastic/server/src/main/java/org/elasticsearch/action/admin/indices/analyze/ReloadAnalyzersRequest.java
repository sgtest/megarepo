/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.indices.analyze;

import org.elasticsearch.action.support.broadcast.BroadcastRequest;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;

import java.io.IOException;
import java.util.Arrays;
import java.util.Objects;

/**
 * Request for reloading index search analyzers
 */
public class ReloadAnalyzersRequest extends BroadcastRequest<ReloadAnalyzersRequest> {
    private final String resource;

    /**
     * Constructs a request for reloading index search analyzers
     * @param resource changed resource to reload analyzers from, @null if not applicable
     * @param indices the indices to reload analyzers for
     */
    public ReloadAnalyzersRequest(String resource, String... indices) {
        super(indices);
        this.resource = resource;
    }

    public ReloadAnalyzersRequest(StreamInput in) throws IOException {
        super(in);
        this.resource = in.readOptionalString();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeOptionalString(resource);
    }

    public String resource() {
        return resource;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        ReloadAnalyzersRequest that = (ReloadAnalyzersRequest) o;
        return Objects.equals(indicesOptions(), that.indicesOptions())
            && Arrays.equals(indices, that.indices)
            && Objects.equals(resource, that.resource);
    }

    @Override
    public int hashCode() {
        return Objects.hash(indicesOptions(), Arrays.hashCode(indices), resource);
    }

}
