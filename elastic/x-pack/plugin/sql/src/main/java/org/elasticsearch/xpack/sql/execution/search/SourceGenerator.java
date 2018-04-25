/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.execution.search;

import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.ConstantScoreQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregationBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.fetch.StoredFieldsContext;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;
import org.elasticsearch.search.sort.FieldSortBuilder;
import org.elasticsearch.search.sort.NestedSortBuilder;
import org.elasticsearch.search.sort.ScriptSortBuilder.ScriptSortType;
import org.elasticsearch.search.sort.SortBuilder;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.FieldAttribute;
import org.elasticsearch.xpack.sql.querydsl.container.AttributeSort;
import org.elasticsearch.xpack.sql.querydsl.container.QueryContainer;
import org.elasticsearch.xpack.sql.querydsl.container.ScoreSort;
import org.elasticsearch.xpack.sql.querydsl.container.ScriptSort;
import org.elasticsearch.xpack.sql.querydsl.container.Sort;

import java.util.List;

import static java.util.Collections.singletonList;
import static org.elasticsearch.search.sort.SortBuilders.fieldSort;
import static org.elasticsearch.search.sort.SortBuilders.scoreSort;
import static org.elasticsearch.search.sort.SortBuilders.scriptSort;

public abstract class SourceGenerator {

    private static final List<String> NO_STORED_FIELD = singletonList(StoredFieldsContext._NONE_);

    public static SearchSourceBuilder sourceBuilder(QueryContainer container, QueryBuilder filter, Integer size) {
        final SearchSourceBuilder source = new SearchSourceBuilder();
        // add the source
        if (container.query() == null) {
            if (filter != null) {
                source.query(new ConstantScoreQueryBuilder(filter));
            }
        } else {
            if (filter != null) {
                source.query(new BoolQueryBuilder().must(container.query().asBuilder()).filter(filter));
            } else {
                source.query(container.query().asBuilder());
            }
        }

        SqlSourceBuilder sortBuilder = new SqlSourceBuilder();
        // Iterate through all the columns requested, collecting the fields that
        // need to be retrieved from the result documents

        // NB: the sortBuilder takes care of eliminating duplicates
        container.columns().forEach(cr -> cr.collectFields(sortBuilder));
        sortBuilder.build(source);
        optimize(sortBuilder, source);

        // add the aggs (if present)
        AggregationBuilder aggBuilder = container.aggs().asAggBuilder();

        if (aggBuilder != null) {
            source.aggregation(aggBuilder);
        }

        sorting(container, source);

        // set page size
        if (size != null) {
            int sz = container.limit() > 0 ? Math.min(container.limit(), size) : size;

            if (source.size() == -1) {
                source.size(sz);
            }
            if (aggBuilder instanceof CompositeAggregationBuilder) {
                ((CompositeAggregationBuilder) aggBuilder).size(sz);
            }
        }

        optimize(container, source);

        return source;
    }

    private static void sorting(QueryContainer container, SearchSourceBuilder source) {
        if (source.aggregations() != null && source.aggregations().count() > 0) {
            // Aggs can't be sorted using search sorting. That sorting is handled elsewhere.
            return;
        }
        if (container.sort() == null || container.sort().isEmpty()) {
            // if no sorting is specified, use the _doc one
            source.sort("_doc");
            return;
        }
        for (Sort sortable : container.sort()) {
            SortBuilder<?> sortBuilder = null;

            if (sortable instanceof AttributeSort) {
                AttributeSort as = (AttributeSort) sortable;
                Attribute attr = as.attribute();

                // sorting only works on not-analyzed fields - look for a multi-field replacement
                if (attr instanceof FieldAttribute) {
                    FieldAttribute fa = (FieldAttribute) attr;
                    fa = fa.isInexact() ? fa.exactAttribute() : fa;

                    sortBuilder = fieldSort(fa.name());
                    if (fa.isNested()) {
                        FieldSortBuilder fieldSort = fieldSort(fa.name());
                        NestedSortBuilder newSort = new NestedSortBuilder(fa.nestedParent().name());
                        NestedSortBuilder nestedSort = fieldSort.getNestedSort();

                        if (nestedSort == null) {
                            fieldSort.setNestedSort(newSort);
                        } else {
                            for (; nestedSort.getNestedSort() != null; nestedSort = nestedSort.getNestedSort()) {
                            }
                            nestedSort.setNestedSort(newSort);
                        }

                        nestedSort = newSort;

                        if (container.query() != null) {
                            container.query().enrichNestedSort(nestedSort);
                        }
                        sortBuilder = fieldSort;
                    }
                }
            } else if (sortable instanceof ScriptSort) {
                ScriptSort ss = (ScriptSort) sortable;
                sortBuilder = scriptSort(ss.script().toPainless(),
                        ss.script().outputType().isNumeric() ? ScriptSortType.NUMBER : ScriptSortType.STRING);
            } else if (sortable instanceof ScoreSort) {
                sortBuilder = scoreSort();
            }

            if (sortBuilder != null) {
                sortBuilder.order(sortable.direction().asOrder());
                source.sort(sortBuilder);
            }
        }
    }

    private static void optimize(SqlSourceBuilder sqlSource, SearchSourceBuilder builder) {
        if (sqlSource.sourceFields.isEmpty()) {
            disableSource(builder);
        }
    }

    private static void optimize(QueryContainer query, SearchSourceBuilder builder) {
        // if only aggs are needed, don't retrieve any docs and remove scoring
        if (query.isAggsOnly()) {
            builder.size(0);
            builder.trackScores(false);
            // disable source fetching (only doc values are used)
            disableSource(builder);
        }
    }

    private static void disableSource(SearchSourceBuilder builder) {
        builder.fetchSource(FetchSourceContext.DO_NOT_FETCH_SOURCE);
        if (builder.storedFields() == null) {
            builder.storedFields(NO_STORED_FIELD);
        }
    }
}