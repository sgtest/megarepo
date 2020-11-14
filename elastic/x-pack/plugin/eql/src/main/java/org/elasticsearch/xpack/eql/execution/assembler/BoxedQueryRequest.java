/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.eql.execution.assembler;

import org.elasticsearch.index.query.RangeQueryBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.eql.execution.search.Ordinal;
import org.elasticsearch.xpack.eql.execution.search.QueryRequest;
import org.elasticsearch.xpack.eql.execution.search.RuntimeUtils;

import static org.elasticsearch.index.query.QueryBuilders.rangeQuery;

/**
 * Ranged or boxed query. Provides a beginning or end to the current query.
 * The query moves between them through search_after.
 *
 * Note that the range is not set at once on purpose since each query tends to have
 * its own number of results separate from the others.
 * As such, each query starts from where it left off to reach the current in-progress window
 * as oppose to always operating with the exact same window.
 */
public class BoxedQueryRequest implements QueryRequest {

    private final RangeQueryBuilder timestampRange;
    private final SearchSourceBuilder searchSource;

    private Ordinal from, to;
    private Ordinal after;

    public BoxedQueryRequest(QueryRequest original, String timestamp) {
        searchSource = original.searchSource();
        // setup range queries and preserve their reference to simplify the update
        timestampRange = rangeQuery(timestamp).timeZone("UTC").format("epoch_millis");
        RuntimeUtils.addFilter(timestampRange, searchSource);
    }

    @Override
    public SearchSourceBuilder searchSource() {
        return searchSource;
    }

    @Override
    public void nextAfter(Ordinal ordinal) {
        after = ordinal;
        // and leave only search_after
        searchSource.searchAfter(ordinal.toArray());
    }

    /**
     * Sets the lower boundary for the query (inclusive).
     * Can be removed (when the query in unbounded) through null.
     */
    public BoxedQueryRequest from(Ordinal begin) {
        from = begin;
        timestampRange.gte(begin != null ? begin.timestamp() : null);
        return this;
    }

    /**
     * Sets the upper boundary for the query (inclusive).
     * Can be removed through null.
     */
    public BoxedQueryRequest to(Ordinal end) {
        to = end;
        timestampRange.lte(end != null ? end.timestamp() : null);
        return this;
    }

    public Ordinal after() {
        return after;
    }

    public Ordinal from() {
        return from;
    }

    public Ordinal to() {
        return to;
    }

    @Override
    public String toString() {
        return "( " + string(from) + " >-" + string(after) + "-> " + string(to) + "]";
    }

    private static String string(Ordinal o) {
        return o != null ? o.toString() : "<none>";
    }
}
