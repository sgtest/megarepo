/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations;

import org.elasticsearch.search.aggregations.support.ValuesSource;

import java.util.Optional;

/**
 * Collection of helper methods for what to throw in common aggregation error scenarios.
 */
public class AggregationErrors {
    private static String parentType;

    private AggregationErrors() {}

    /**
     * This error indicates that the aggregations path the user specified cannot be parsed.
     * It is a 400 class error and should not be retried.
     */
    public static IllegalArgumentException invalidPathElement(String element, String path) {
        return new IllegalArgumentException("Invalid path element [" + element + "] in path [" + path + "]");
    }

    /**
     * This error indicates that an aggregation is being used on a value type that it is not capable of processing, such as taking
     * the sum of a keyword. It is a 400 class error and should not be retried.
     *
     * This is a legacy version of this error; in general, we should aim to use the
     * {@link org.elasticsearch.search.aggregations.support.ValuesSourceType} version below
     *
     * @param valuesSource The values source we resolved from the query
     * @param name The name of the aggregation
     * @return an appropriate exception type
     */
    public static IllegalArgumentException unsupportedValuesSourceType(ValuesSource valuesSource, String name) {
        return new IllegalArgumentException(
            "ValuesSource type [" + valuesSource.toString() + "] is not supported for aggregation [" + name + "]"
        );
    }

    /**
     * This error indicates that a rate aggregation is being invoked without a single Date Histogram parent, as is required.  This is a
     * 400 class error and should not be retried.
     *
     * @param name the name of the rate aggregation
     * @return an appropriate exception
     */
    public static RuntimeException rateWithoutDateHistogram(String name) {
        return new IllegalArgumentException(
            "aggregation ["
                + name
                + "] does not have exactly one date_histogram value source; exactly one is required when using with rate aggregation"
        );
    }

    /**
     * This error indicates that the backing indices for a field have different, incompatible, types (e.g. IP and Keyword).  This
     * causes a failure at reduce time, and is not retryable (and thus should be a 400 class error)
     *
     * @param aggregationName - The name of the aggregation
     * @param position - optional, for multisource aggregations.  Indicates the position of the field causing the problem.
     * @return - an appropriate exception
     */
    public static RuntimeException reduceTypeMissmatch(String aggregationName, Optional<Integer> position) {
        String fieldString;
        if (position.isPresent()) {
            fieldString = "the field in position" + position.get().toString();
        } else {
            fieldString = "the field you gave";
        }
        return new IllegalArgumentException(
            "Merging/Reducing the aggregations failed when computing the aggregation ["
                + aggregationName
                + "] because "
                + fieldString
                + " in the aggregation query existed as two different "
                + "types in two different indices"
        );
    }

    public static RuntimeException valuesSourceDoesNotSupportScritps(String typeName) {
        return new IllegalArgumentException("value source of type [" + typeName + "] is not supported by scripts");
    }

    public static RuntimeException unsupportedScriptValue(String actual) {
        return new IllegalArgumentException("Unsupported script value [" + actual + "], expected a number, date, or boolean");
    }

    /**
     * Indicates that a multivalued field was found where we only support a single valued field
     * @return an appropriate exception
     */
    public static RuntimeException unsupportedMultivalue() {
        return new IllegalArgumentException(
            "Encountered more than one value for a "
                + "single document. Use a script to combine multiple values per doc into a single value."
        );
    }

    /**
     * Indicates an attempt to use date rounding on a non-date values source
     * @param typeName - name of the type we're attempting to round
     * @return an appropriate exception
     */
    public static RuntimeException unsupportedRounding(String typeName) {
        return new IllegalArgumentException("can't round a [" + typeName + "]");
    }
}
