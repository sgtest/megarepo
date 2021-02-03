/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.search.aggregations;

/**
 * The aggregation context that is part of the search context.
 */
public class SearchContextAggregations {

    private final AggregatorFactories factories;
    private Aggregator[] aggregators;

    /**
     * Creates a new aggregation context with the parsed aggregator factories
     */
    public SearchContextAggregations(AggregatorFactories factories) {
        this.factories = factories;
    }

    public AggregatorFactories factories() {
        return factories;
    }

    public Aggregator[] aggregators() {
        return aggregators;
    }

    /**
     * Registers all the created aggregators (top level aggregators) for the search execution context.
     *
     * @param aggregators The top level aggregators of the search execution.
     */
    public void aggregators(Aggregator[] aggregators) {
        this.aggregators = aggregators;
    }
}
