/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.sql.execution.search;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregationBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.ql.execution.search.extractor.BucketExtractor;
import org.elasticsearch.xpack.ql.type.Schema;
import org.elasticsearch.xpack.ql.util.StringUtils;
import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.querydsl.agg.Aggs;
import org.elasticsearch.xpack.sql.session.Cursor;
import org.elasticsearch.xpack.sql.session.Rows;
import org.elasticsearch.xpack.sql.session.SqlConfiguration;

import java.io.IOException;
import java.util.Arrays;
import java.util.BitSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.BiFunction;
import java.util.function.Supplier;

import static org.elasticsearch.xpack.sql.execution.search.Querier.logSearchResponse;
import static org.elasticsearch.xpack.sql.execution.search.Querier.prepareRequest;

/**
 * Cursor for composite aggregation (GROUP BY).
 * Stores the query that gets updated/slides across requests.
 */
public class CompositeAggCursor implements Cursor {

    private static final Logger log = LogManager.getLogger(CompositeAggCursor.class);

    public static final String NAME = "c";

    private final String[] indices;
    private final SearchSourceBuilder nextQuery;
    private final List<BucketExtractor> extractors;
    private final BitSet mask;
    private final int limit;
    private final boolean includeFrozen;

    CompositeAggCursor(
        SearchSourceBuilder nextQuery,
        List<BucketExtractor> exts,
        BitSet mask,
        int remainingLimit,
        boolean includeFrozen,
        String... indices
    ) {
        this.indices = indices;
        this.nextQuery = nextQuery;
        this.extractors = exts;
        this.mask = mask;
        this.limit = remainingLimit;
        this.includeFrozen = includeFrozen;
    }

    public CompositeAggCursor(StreamInput in) throws IOException {
        indices = in.readStringArray();
        nextQuery = new SearchSourceBuilder(in);
        limit = in.readVInt();

        extractors = in.readNamedWriteableList(BucketExtractor.class);
        mask = BitSet.valueOf(in.readByteArray());
        includeFrozen = in.readBoolean();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeStringArray(indices);
        nextQuery.writeTo(out);
        out.writeVInt(limit);

        out.writeNamedWriteableList(extractors);
        out.writeByteArray(mask.toByteArray());
        out.writeBoolean(includeFrozen);
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    String[] indices() {
        return indices;
    }

    SearchSourceBuilder next() {
        return nextQuery;
    }

    BitSet mask() {
        return mask;
    }

    List<BucketExtractor> extractors() {
        return extractors;
    }

    int limit() {
        return limit;
    }

    boolean includeFrozen() {
        return includeFrozen;
    }

    @Override
    public void nextPage(SqlConfiguration cfg, Client client, ActionListener<Page> listener) {
        if (log.isTraceEnabled()) {
            log.trace("About to execute composite query {} on {}", StringUtils.toString(nextQuery), indices);
        }

        SearchRequest request = prepareRequest(nextQuery, cfg.requestTimeout(), includeFrozen, indices);

        client.search(request, new ActionListener.Delegating<>(listener) {
            @Override
            public void onResponse(SearchResponse response) {
                handle(
                    response,
                    request.source(),
                    makeRowSet(response),
                    makeCursor(),
                    () -> client.search(request, this),
                    delegate,
                    Schema.EMPTY
                );
            }
        });
    }

    protected Supplier<CompositeAggRowSet> makeRowSet(SearchResponse response) {
        return () -> new CompositeAggRowSet(extractors, mask, response, limit);
    }

    protected BiFunction<SearchSourceBuilder, CompositeAggRowSet, CompositeAggCursor> makeCursor() {
        return (q, r) -> new CompositeAggCursor(q, r.extractors(), r.mask(), r.remainingData(), includeFrozen, indices);
    }

    static void handle(
        SearchResponse response,
        SearchSourceBuilder source,
        Supplier<CompositeAggRowSet> makeRowSet,
        BiFunction<SearchSourceBuilder, CompositeAggRowSet, CompositeAggCursor> makeCursor,
        Runnable retry,
        ActionListener<Page> listener,
        Schema schema
    ) {

        if (log.isTraceEnabled()) {
            logSearchResponse(response, log);
        }
        // there are some results
        if (response.getAggregations().asList().isEmpty() == false) {
            // retry
            if (shouldRetryDueToEmptyPage(response)) {
                updateCompositeAfterKey(response, source);
                retry.run();
                return;
            }

            try {
                CompositeAggRowSet rowSet = makeRowSet.get();
                Map<String, Object> afterKey = rowSet.afterKey();

                if (afterKey != null) {
                    updateSourceAfterKey(afterKey, source);
                }

                Cursor next = rowSet.remainingData() == 0 ? Cursor.EMPTY : makeCursor.apply(source, rowSet);
                listener.onResponse(new Page(rowSet, next));
            } catch (Exception ex) {
                listener.onFailure(ex);
            }
        }
        // no results
        else {
            listener.onResponse(Page.last(Rows.empty(schema)));
        }
    }

    private static boolean shouldRetryDueToEmptyPage(SearchResponse response) {
        CompositeAggregation composite = getComposite(response);
        // if there are no buckets but a next page, go fetch it instead of sending an empty response to the client
        return composite != null
            && composite.getBuckets().isEmpty()
            && composite.afterKey() != null
            && composite.afterKey().isEmpty() == false;
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

    private static void updateCompositeAfterKey(SearchResponse r, SearchSourceBuilder search) {
        CompositeAggregation composite = getComposite(r);

        if (composite == null) {
            throw new SqlIllegalArgumentException("Invalid server response; no group-by detected");
        }

        updateSourceAfterKey(composite.afterKey(), search);
    }

    private static void updateSourceAfterKey(Map<String, Object> afterKey, SearchSourceBuilder search) {
        AggregationBuilder aggBuilder = search.aggregations().getAggregatorFactories().iterator().next();
        // update after-key with the new value
        if (aggBuilder instanceof CompositeAggregationBuilder comp) {
            comp.aggregateAfter(afterKey);
        } else {
            throw new SqlIllegalArgumentException("Invalid client request; expected a group-by but instead got {}", aggBuilder);
        }
    }

    @Override
    public void clear(Client client, ActionListener<Boolean> listener) {
        listener.onResponse(true);
    }

    @Override
    public int hashCode() {
        return Objects.hash(Arrays.hashCode(indices), nextQuery, extractors, limit, mask, includeFrozen);
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null || obj.getClass() != getClass()) {
            return false;
        }
        CompositeAggCursor other = (CompositeAggCursor) obj;
        return Arrays.equals(indices, other.indices)
            && Objects.equals(nextQuery, other.nextQuery)
            && Objects.equals(extractors, other.extractors)
            && Objects.equals(limit, other.limit)
            && Objects.equals(includeFrozen, other.includeFrozen);
    }

    @Override
    public String toString() {
        return "cursor for composite on index [" + Arrays.toString(indices) + "]";
    }
}
