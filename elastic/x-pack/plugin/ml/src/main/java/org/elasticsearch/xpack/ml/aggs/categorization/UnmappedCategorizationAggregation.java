/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.aggs.categorization;

import org.elasticsearch.search.aggregations.AggregationReduceContext;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.search.aggregations.InternalAggregations;

import java.util.List;
import java.util.Map;

class UnmappedCategorizationAggregation extends InternalCategorizationAggregation {
    protected UnmappedCategorizationAggregation(
        String name,
        int requiredSize,
        long minDocCount,
        int similarityThreshold,
        Map<String, Object> metadata
    ) {
        super(name, requiredSize, minDocCount, similarityThreshold, metadata);
    }

    @Override
    public InternalCategorizationAggregation create(List<Bucket> buckets) {
        return new UnmappedCategorizationAggregation(name, getRequiredSize(), getMinDocCount(), getSimilarityThreshold(), super.metadata);
    }

    @Override
    public Bucket createBucket(InternalAggregations aggregations, Bucket prototype) {
        throw new UnsupportedOperationException("not supported for UnmappedCategorizationAggregation");
    }

    @Override
    public InternalAggregation reduce(List<InternalAggregation> aggregations, AggregationReduceContext reduceContext) {
        return new UnmappedCategorizationAggregation(name, getRequiredSize(), getMinDocCount(), getSimilarityThreshold(), super.metadata);
    }

    @Override
    public boolean canLeadReduction() {
        return false;
    }

    @Override
    public List<Bucket> getBuckets() {
        return List.of();
    }
}
