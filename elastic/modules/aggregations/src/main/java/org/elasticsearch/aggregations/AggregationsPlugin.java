/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.aggregations;

import org.elasticsearch.aggregations.bucket.adjacency.AdjacencyMatrixAggregationBuilder;
import org.elasticsearch.aggregations.bucket.adjacency.InternalAdjacencyMatrix;
import org.elasticsearch.aggregations.bucket.histogram.AutoDateHistogramAggregationBuilder;
import org.elasticsearch.aggregations.bucket.histogram.InternalAutoDateHistogram;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.SearchPlugin;

import java.util.List;

public class AggregationsPlugin extends Plugin implements SearchPlugin {

    @Override
    public List<AggregationSpec> getAggregations() {
        return List.of(
            new AggregationSpec(
                AdjacencyMatrixAggregationBuilder.NAME,
                AdjacencyMatrixAggregationBuilder::new,
                AdjacencyMatrixAggregationBuilder::parse
            ).addResultReader(InternalAdjacencyMatrix::new),
            new AggregationSpec(
                AutoDateHistogramAggregationBuilder.NAME,
                AutoDateHistogramAggregationBuilder::new,
                AutoDateHistogramAggregationBuilder.PARSER
            ).addResultReader(InternalAutoDateHistogram::new)
                .setAggregatorRegistrar(AutoDateHistogramAggregationBuilder::registerAggregators)
        );
    }
}
