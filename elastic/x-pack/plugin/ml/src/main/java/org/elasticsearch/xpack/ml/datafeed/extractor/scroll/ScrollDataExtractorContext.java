/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.datafeed.extractor.scroll;

import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;

import java.util.List;
import java.util.Map;
import java.util.Objects;

class ScrollDataExtractorContext {

    final String jobId;
    final ExtractedFields extractedFields;
    final String[] indices;
    final String[] types;
    final QueryBuilder query;
    final List<SearchSourceBuilder.ScriptField> scriptFields;
    final int scrollSize;
    final long start;
    final long end;
    final Map<String, String> headers;

    ScrollDataExtractorContext(String jobId, ExtractedFields extractedFields, List<String> indices, List<String> types,
                                      QueryBuilder query, List<SearchSourceBuilder.ScriptField> scriptFields, int scrollSize,
                                      long start, long end, Map<String, String> headers) {
        this.jobId = Objects.requireNonNull(jobId);
        this.extractedFields = Objects.requireNonNull(extractedFields);
        this.indices = indices.toArray(new String[indices.size()]);
        this.types = types.toArray(new String[types.size()]);
        this.query = Objects.requireNonNull(query);
        this.scriptFields = Objects.requireNonNull(scriptFields);
        this.scrollSize = scrollSize;
        this.start = start;
        this.end = end;
        this.headers = headers;
    }
}
