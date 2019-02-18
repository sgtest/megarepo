/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.transforms.pivot;

import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public final class Aggregations {
    private Aggregations() {}

    /**
     * Supported aggregation by dataframe and corresponding meta information.
     *
     * aggregationType - the name of the aggregation as returned by
     * {@link org.elasticsearch.search.aggregations.BaseAggregationBuilder#getType()}}
     *
     * targetMapping - the field type for the output, if null, the source type should be used
     *
     */
    enum AggregationType {
        AVG("avg", "double"),
        CARDINALITY("cardinality", "long"),
        VALUE_COUNT("value_count", "long"),
        MAX("max", null),
        MIN("min", null),
        SUM("sum", null);

        private final String aggregationType;
        private final String targetMapping;

        AggregationType(String name, String targetMapping) {
            this.aggregationType = name;
            this.targetMapping = targetMapping;
        }

        public String getName() {
            return aggregationType;
        }

        public String getTargetMapping() {
            return targetMapping;
        }
    }

    private static Set<String> aggregationSupported = Stream.of(AggregationType.values()).map(AggregationType::name)
            .collect(Collectors.toSet());

    public static boolean isSupportedByDataframe(String aggregationType) {
        return aggregationSupported.contains(aggregationType.toUpperCase(Locale.ROOT));
    }

    public static String resolveTargetMapping(String aggregationType, String sourceType) {
        AggregationType agg = AggregationType.valueOf(aggregationType.toUpperCase(Locale.ROOT));
        return agg.getTargetMapping() == null ? sourceType : agg.getTargetMapping();
    }
}
