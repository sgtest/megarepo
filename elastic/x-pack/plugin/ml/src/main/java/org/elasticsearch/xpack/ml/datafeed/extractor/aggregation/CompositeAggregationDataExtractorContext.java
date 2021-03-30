/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.datafeed.extractor.aggregation;

import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregationBuilder;

import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;

class CompositeAggregationDataExtractorContext {

    final String jobId;
    final String timeField;
    final Set<String> fields;
    final String[] indices;
    final QueryBuilder query;
    final CompositeAggregationBuilder compositeAggregationBuilder;
    final long start;
    final long end;
    final boolean includeDocCount;
    final Map<String, String> headers;
    final IndicesOptions indicesOptions;
    final Map<String, Object> runtimeMappings;
    final String compositeAggDateHistogramGroupSourceName;

    CompositeAggregationDataExtractorContext(String jobId,
                                             String timeField,
                                             Set<String> fields,
                                             List<String> indices,
                                             QueryBuilder query,
                                             CompositeAggregationBuilder compositeAggregationBuilder,
                                             String compositeAggDateHistogramGroupSourceName,
                                             long start,
                                             long end,
                                             boolean includeDocCount,
                                             Map<String, String> headers,
                                             IndicesOptions indicesOptions,
                                             Map<String, Object> runtimeMappings) {
        this.jobId = Objects.requireNonNull(jobId);
        this.timeField = Objects.requireNonNull(timeField);
        this.fields = Objects.requireNonNull(fields);
        this.indices = indices.toArray(new String[0]);
        this.query = Objects.requireNonNull(query);
        this.compositeAggregationBuilder = Objects.requireNonNull(compositeAggregationBuilder);
        this.compositeAggDateHistogramGroupSourceName = Objects.requireNonNull(compositeAggDateHistogramGroupSourceName);
        this.start = start;
        this.end = end;
        this.includeDocCount = includeDocCount;
        this.headers = headers;
        this.indicesOptions = Objects.requireNonNull(indicesOptions);
        this.runtimeMappings = Objects.requireNonNull(runtimeMappings);
    }
}
