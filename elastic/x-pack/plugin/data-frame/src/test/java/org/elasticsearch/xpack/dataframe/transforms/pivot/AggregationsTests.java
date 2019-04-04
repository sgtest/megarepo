/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.transforms.pivot;

import org.elasticsearch.test.ESTestCase;

public class AggregationsTests extends ESTestCase {
    public void testResolveTargetMapping() {

        // avg
        assertEquals("double", Aggregations.resolveTargetMapping("avg", "int"));
        assertEquals("double", Aggregations.resolveTargetMapping("avg", "double"));

        // cardinality
        assertEquals("long", Aggregations.resolveTargetMapping("cardinality", "int"));
        assertEquals("long", Aggregations.resolveTargetMapping("cardinality", "double"));

        // value_count
        assertEquals("long", Aggregations.resolveTargetMapping("value_count", "int"));
        assertEquals("long", Aggregations.resolveTargetMapping("value_count", "double"));

        // max
        assertEquals("int", Aggregations.resolveTargetMapping("max", "int"));
        assertEquals("double", Aggregations.resolveTargetMapping("max", "double"));
        assertEquals("half_float", Aggregations.resolveTargetMapping("max", "half_float"));

        // min
        assertEquals("int", Aggregations.resolveTargetMapping("min", "int"));
        assertEquals("double", Aggregations.resolveTargetMapping("min", "double"));
        assertEquals("half_float", Aggregations.resolveTargetMapping("min", "half_float"));

        // sum
        assertEquals("int", Aggregations.resolveTargetMapping("sum", "int"));
        assertEquals("double", Aggregations.resolveTargetMapping("sum", "double"));
        assertEquals("half_float", Aggregations.resolveTargetMapping("sum", "half_float"));

        // scripted_metric
        assertEquals("_dynamic", Aggregations.resolveTargetMapping("scripted_metric", null));
        assertEquals("_dynamic", Aggregations.resolveTargetMapping("scripted_metric", "int"));
    }
}
