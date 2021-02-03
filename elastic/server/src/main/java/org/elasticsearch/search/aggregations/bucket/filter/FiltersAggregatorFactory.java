/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations.bucket.filter;

import org.apache.lucene.search.Query;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.AggregatorFactory;
import org.elasticsearch.search.aggregations.CardinalityUpperBound;
import org.elasticsearch.search.aggregations.bucket.filter.FiltersAggregator.KeyedFilter;
import org.elasticsearch.search.aggregations.support.AggregationContext;

import java.io.IOException;
import java.util.List;
import java.util.Map;

public class FiltersAggregatorFactory extends AggregatorFactory {

    private final String[] keys;
    private final Query[] filters;
    private final boolean keyed;
    private final boolean otherBucket;
    private final String otherBucketKey;

    public FiltersAggregatorFactory(String name, List<KeyedFilter> filters, boolean keyed, boolean otherBucket,
                                    String otherBucketKey, AggregationContext context, AggregatorFactory parent,
                                    AggregatorFactories.Builder subFactories, Map<String, Object> metadata) throws IOException {
        super(name, context, parent, subFactories, metadata);
        this.keyed = keyed;
        this.otherBucket = otherBucket;
        this.otherBucketKey = otherBucketKey;
        keys = new String[filters.size()];
        this.filters = new Query[filters.size()];
        for (int i = 0; i < filters.size(); ++i) {
            KeyedFilter keyedFilter = filters.get(i);
            this.keys[i] = keyedFilter.key();
            this.filters[i] = context.buildQuery(keyedFilter.filter());
        }
    }

    @Override
    public Aggregator createInternal(Aggregator parent,
                                        CardinalityUpperBound cardinality,
                                        Map<String, Object> metadata) throws IOException {
        return FiltersAggregator.build(name, factories, keys, filters, keyed,
            otherBucket ? otherBucketKey : null, context, parent, cardinality, metadata);
    }
}
