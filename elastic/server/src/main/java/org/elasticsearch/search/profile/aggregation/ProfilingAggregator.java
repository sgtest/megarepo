/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.profile.aggregation;

import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.ScoreMode;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.search.aggregations.LeafBucketCollector;
import org.elasticsearch.search.aggregations.support.AggregationPath.PathElement;
import org.elasticsearch.search.profile.Timer;
import org.elasticsearch.search.sort.SortOrder;

import java.io.IOException;
import java.util.Iterator;

public class ProfilingAggregator extends Aggregator {

    private final Aggregator delegate;
    private final AggregationProfiler profiler;
    private AggregationProfileBreakdown profileBreakdown;

    public ProfilingAggregator(Aggregator delegate, AggregationProfiler profiler) throws IOException {
        this.profiler = profiler;
        this.delegate = delegate;
    }

    @Override
    public void close() {
        delegate.close();
    }

    @Override
    public ScoreMode scoreMode() {
        return delegate.scoreMode();
    }

    @Override
    public String name() {
        return delegate.name();
    }

    @Override
    public Aggregator parent() {
        return delegate.parent();
    }

    @Override
    public Aggregator subAggregator(String name) {
        return delegate.subAggregator(name);
    }

    @Override
    public Aggregator resolveSortPath(PathElement next, Iterator<PathElement> path) {
        return delegate.resolveSortPath(next, path);
    }

    @Override
    public BucketComparator bucketComparator(String key, SortOrder order) {
        return delegate.bucketComparator(key, order);
    }

    @Override
    public InternalAggregation[] buildAggregations(long[] owningBucketOrds) throws IOException {
        Timer timer = profileBreakdown.getTimer(AggregationTimingType.BUILD_AGGREGATION);
        timer.start();
        try {
            return delegate.buildAggregations(owningBucketOrds);
        } finally {
            timer.stop();
            delegate.collectDebugInfo(profileBreakdown::addDebugInfo);
        }
    }

    @Override
    public InternalAggregation buildEmptyAggregation() {
        return delegate.buildEmptyAggregation();
    }

    @Override
    public LeafBucketCollector getLeafCollector(LeafReaderContext ctx) throws IOException {
        Timer timer = profileBreakdown.getTimer(AggregationTimingType.BUILD_LEAF_COLLECTOR);
        timer.start();
        try {
            return new ProfilingLeafBucketCollector(delegate.getLeafCollector(ctx), profileBreakdown);
        } finally {
            timer.stop();
        }
    }

    @Override
    public void preCollection() throws IOException {
        this.profileBreakdown = profiler.getQueryBreakdown(delegate);
        Timer timer = profileBreakdown.getTimer(AggregationTimingType.INITIALIZE);
        timer.start();
        try {
            delegate.preCollection();
        } finally {
            timer.stop();
        }
        profiler.pollLastElement();
    }

    @Override
    public void postCollection() throws IOException {
        Timer timer = profileBreakdown.getTimer(AggregationTimingType.POST_COLLECTION);
        timer.start();
        try {
            delegate.postCollection();
        } finally {
            timer.stop();
        }
    }

    @Override
    public String toString() {
        return delegate.toString();
    }

    @Override
    public Aggregator[] subAggregators() {
        return delegate.subAggregators();
    }

    public static Aggregator unwrap(Aggregator agg) {
        if (agg instanceof ProfilingAggregator) {
            return ((ProfilingAggregator) agg).delegate;
        }
        return agg;
    }
}
