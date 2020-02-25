/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.eql.execution.search;

import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.fetch.StoredFieldsContext;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;
import org.elasticsearch.xpack.eql.querydsl.container.QueryContainer;
import org.elasticsearch.xpack.ql.execution.search.QlSourceBuilder;

import java.util.List;

import static java.util.Collections.singletonList;
import static org.elasticsearch.index.query.QueryBuilders.boolQuery;

public abstract class SourceGenerator {

    private SourceGenerator() {}

    private static final List<String> NO_STORED_FIELD = singletonList(StoredFieldsContext._NONE_);

    public static SearchSourceBuilder sourceBuilder(QueryContainer container, QueryBuilder filter, Integer size) {
        QueryBuilder finalQuery = null;
        // add the source
        if (container.query() != null) {
            if (filter != null) {
                finalQuery = boolQuery().must(container.query().asBuilder()).filter(filter);
            } else {
                finalQuery = container.query().asBuilder();
            }
        } else {
            if (filter != null) {
                finalQuery = boolQuery().filter(filter);
            }
        }

        final SearchSourceBuilder source = new SearchSourceBuilder();
        source.query(finalQuery);

        QlSourceBuilder sortBuilder = new QlSourceBuilder();
        // Iterate through all the columns requested, collecting the fields that
        // need to be retrieved from the result documents

        // NB: the sortBuilder takes care of eliminating duplicates
        container.fields().forEach(f -> f.v1().collectFields(sortBuilder));
        sortBuilder.build(source);
        optimize(sortBuilder, source);

        return source;
    }

    private static void optimize(QlSourceBuilder qlSource, SearchSourceBuilder builder) {
        if (qlSource.noSource()) {
            disableSource(builder);
        }
    }

    private static void optimize(QueryContainer query, SearchSourceBuilder builder) {
        if (query.shouldTrackHits()) {
            builder.trackTotalHits(true);
        }
    }

    private static void disableSource(SearchSourceBuilder builder) {
        builder.fetchSource(FetchSourceContext.DO_NOT_FETCH_SOURCE);
        if (builder.storedFields() == null) {
            builder.storedFields(NO_STORED_FIELD);
        }
    }
}
