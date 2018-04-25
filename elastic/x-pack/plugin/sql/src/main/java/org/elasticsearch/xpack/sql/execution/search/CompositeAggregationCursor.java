/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.execution.search;

import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregationBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.execution.search.extractor.BucketExtractor;
import org.elasticsearch.xpack.sql.querydsl.agg.Aggs;
import org.elasticsearch.xpack.sql.session.Configuration;
import org.elasticsearch.xpack.sql.session.Cursor;
import org.elasticsearch.xpack.sql.session.RowSet;
import org.elasticsearch.xpack.sql.util.StringUtils;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * Cursor for composite aggregation (GROUP BY).
 * Stores the query that gets updated/slides across requests.
 */
public class CompositeAggregationCursor implements Cursor {

    private final Logger log = Loggers.getLogger(getClass());

    public static final String NAME = "c";

    private final String[] indices;
    private final byte[] nextQuery;
    private final List<BucketExtractor> extractors;
    private final int limit;

    CompositeAggregationCursor(byte[] next, List<BucketExtractor> exts, int remainingLimit, String... indices) {
        this.indices = indices;
        this.nextQuery = next;
        this.extractors = exts;
        this.limit = remainingLimit;
    }

    public CompositeAggregationCursor(StreamInput in) throws IOException {
        indices = in.readStringArray();
        nextQuery = in.readByteArray();
        limit = in.readVInt();

        extractors = in.readNamedWriteableList(BucketExtractor.class);
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeStringArray(indices);
        out.writeByteArray(nextQuery);
        out.writeVInt(limit);

        out.writeNamedWriteableList(extractors);
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    String[] indices() {
        return indices;
    }

    byte[] next() {
        return nextQuery;
    }

    List<BucketExtractor> extractors() {
        return extractors;
    }

    int limit() {
        return limit;
    }

    @Override
    public void nextPage(Configuration cfg, Client client, NamedWriteableRegistry registry, ActionListener<RowSet> listener) {
        SearchSourceBuilder q;
        try {
            q = deserializeQuery(registry, nextQuery);
        } catch (Exception ex) {
            listener.onFailure(ex);
            return;
        }

        SearchSourceBuilder query = q;
        if (log.isTraceEnabled()) {
            log.trace("About to execute composite query {} on {}", StringUtils.toString(query), indices);
        }

        SearchRequest search = Querier.prepareRequest(client, query, cfg.pageTimeout(), indices);

        client.search(search, ActionListener.wrap(r -> {
            updateCompositeAfterKey(r, query);
            CompositeAggsRowSet rowSet = new CompositeAggsRowSet(extractors, r, limit,
                    serializeQuery(query), indices);
            listener.onResponse(rowSet);
        }, listener::onFailure));
    }

    static CompositeAggregation getComposite(SearchResponse response) {
        Aggregation agg = response.getAggregations().get(Aggs.ROOT_GROUP_NAME);
        if (agg == null) {
            return null;
        }

        if (agg instanceof CompositeAggregation) {
            return (CompositeAggregation) agg;
        }

        throw new SqlIllegalArgumentException("Unrecognized root group found; {}", agg.getClass());
    }

    static void updateCompositeAfterKey(SearchResponse r, SearchSourceBuilder next) {
        CompositeAggregation composite = getComposite(r);

        if (composite == null) {
            throw new SqlIllegalArgumentException("Invalid server response; no group-by detected");
        }

        Map<String, Object> afterKey = composite.afterKey();
        // a null after-key means done
        if (afterKey != null) {
            AggregationBuilder aggBuilder = next.aggregations().getAggregatorFactories().get(0);
            // update after-key with the new value
            if (aggBuilder instanceof CompositeAggregationBuilder) {
                CompositeAggregationBuilder comp = (CompositeAggregationBuilder) aggBuilder;
                comp.aggregateAfter(afterKey);
            } else {
                throw new SqlIllegalArgumentException("Invalid client request; expected a group-by but instead got {}", aggBuilder);
            }
        }
    }

    /**
     * Deserializes the search source from a byte array.
     */
    static SearchSourceBuilder deserializeQuery(NamedWriteableRegistry registry, byte[] source) throws IOException {
        try (NamedWriteableAwareStreamInput in = new NamedWriteableAwareStreamInput(StreamInput.wrap(source), registry)) {
            return new SearchSourceBuilder(in);
        }
    }

    /**
     * Serializes the search source to a byte array.
     */
    static byte[] serializeQuery(SearchSourceBuilder source) throws IOException {
        if (source == null) {
            return new byte[0];
        }

        try (BytesStreamOutput out = new BytesStreamOutput()) {
            source.writeTo(out);
            return BytesReference.toBytes(out.bytes());
        }
    }


    @Override
    public void clear(Configuration cfg, Client client, ActionListener<Boolean> listener) {
        listener.onResponse(true);
    }

    @Override
    public int hashCode() {
        return Objects.hash(Arrays.hashCode(indices), Arrays.hashCode(nextQuery), extractors, limit);
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null || obj.getClass() != getClass()) {
            return false;
        }
        CompositeAggregationCursor other = (CompositeAggregationCursor) obj;
        return Arrays.equals(indices, other.indices)
                && Arrays.equals(nextQuery, other.nextQuery)
                && Objects.equals(extractors, other.extractors)
                && Objects.equals(limit, other.limit);
    }

    @Override
    public String toString() {
        return "cursor for composite on index [" + Arrays.toString(indices) + "]";
    }
}