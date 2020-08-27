/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.analytics.rate;

import org.elasticsearch.common.Rounding;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.support.ValuesSourceConfig;
import org.elasticsearch.search.internal.SearchContext;

import java.io.IOException;
import java.util.Map;

public interface RateAggregatorSupplier {
    Aggregator build(
        String name,
        ValuesSourceConfig valuesSourceConfig,
        Rounding.DateTimeUnit rateUnit,
        SearchContext context,
        Aggregator parent,
        Map<String, Object> metadata
    ) throws IOException;
}
