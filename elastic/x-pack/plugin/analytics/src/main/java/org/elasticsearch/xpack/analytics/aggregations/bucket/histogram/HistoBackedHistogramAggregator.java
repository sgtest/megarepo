/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.analytics.aggregations.bucket.histogram;

import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.index.fielddata.HistogramValue;
import org.elasticsearch.index.fielddata.HistogramValues;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.BucketOrder;
import org.elasticsearch.search.aggregations.CardinalityUpperBound;
import org.elasticsearch.search.aggregations.LeafBucketCollector;
import org.elasticsearch.search.aggregations.LeafBucketCollectorBase;
import org.elasticsearch.search.aggregations.bucket.histogram.AbstractHistogramAggregator;
import org.elasticsearch.search.aggregations.bucket.histogram.DoubleBounds;
import org.elasticsearch.search.aggregations.support.ValuesSourceConfig;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.xpack.analytics.aggregations.support.HistogramValuesSource;

import java.io.IOException;
import java.util.Map;

public class HistoBackedHistogramAggregator extends AbstractHistogramAggregator {

    private final HistogramValuesSource.Histogram valuesSource;

    public HistoBackedHistogramAggregator(
        String name,
        AggregatorFactories factories,
        double interval,
        double offset,
        BucketOrder order,
        boolean keyed,
        long minDocCount,
        double minBound,
        double maxBound,
        DoubleBounds hardBounds,
        ValuesSourceConfig valuesSourceConfig,
        SearchContext context,
        Aggregator parent,
        CardinalityUpperBound cardinalityUpperBound,
        Map<String, Object> metadata) throws IOException {
        super(name, factories, interval, offset, order, keyed, minDocCount, minBound, maxBound, hardBounds,
            valuesSourceConfig.format(), context, parent, cardinalityUpperBound, metadata);

        // TODO: Stop using null here
        this.valuesSource = valuesSourceConfig.hasValues() ? (HistogramValuesSource.Histogram) valuesSourceConfig.getValuesSource() : null;

        // Sub aggregations are not allowed when running histogram agg over histograms
        if (subAggregators().length > 0) {
            throw new IllegalArgumentException("Histogram aggregation on histogram fields does not support sub-aggregations");
        }
    }

    @Override
    public LeafBucketCollector getLeafCollector(LeafReaderContext ctx,
            final LeafBucketCollector sub) throws IOException {
        if (valuesSource == null) {
            return LeafBucketCollector.NO_OP_COLLECTOR;
        }

        final HistogramValues values = valuesSource.getHistogramValues(ctx);
        return new LeafBucketCollectorBase(sub, values) {
            @Override
            public void collect(int doc, long owningBucketOrd) throws IOException {
                if (values.advanceExact(doc)) {
                    final HistogramValue sketch = values.histogram();

                    double previousKey = Double.NEGATIVE_INFINITY;
                    while (sketch.next()) {
                        final double value = sketch.value();
                        final int count = sketch.count();

                        double key = Math.floor((value - offset) / interval);
                        assert key >= previousKey;
                        long bucketOrd = bucketOrds.add(owningBucketOrd, Double.doubleToLongBits(key));
                        if (bucketOrd < 0) { // already seen
                            bucketOrd = -1 - bucketOrd;
                            collectExistingBucket(sub, doc, bucketOrd);
                        } else {
                            collectBucket(sub, doc, bucketOrd);
                        }
                        // We have added the document already. We should increment doc_count by count - 1
                        // so that we have added it count times.
                        incrementBucketDocCount(bucketOrd, count - 1);
                        previousKey = key;
                    }
                }
            }
        };
    }
}
