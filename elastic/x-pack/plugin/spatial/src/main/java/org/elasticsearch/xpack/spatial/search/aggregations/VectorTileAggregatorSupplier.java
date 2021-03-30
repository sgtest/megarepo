/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.spatial.search.aggregations;

import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.support.AggregationContext;
import org.elasticsearch.search.aggregations.support.ValuesSourceConfig;

import java.io.IOException;
import java.util.Map;

@FunctionalInterface
public interface VectorTileAggregatorSupplier {

    AbstractVectorTileAggregator build(
        String name,
        ValuesSourceConfig valuesSourceConfig,
        int z,
        int x,
        int y,
        AggregationContext context,
        Aggregator parent,
        Map<String, Object> metadata
    ) throws IOException;
}
