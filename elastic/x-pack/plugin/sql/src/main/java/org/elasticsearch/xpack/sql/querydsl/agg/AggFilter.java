/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.querydsl.agg;

import java.util.Collection;
import java.util.Map;
import java.util.Objects;

import org.elasticsearch.script.Script;
import org.elasticsearch.search.aggregations.PipelineAggregationBuilder;
import org.elasticsearch.xpack.sql.expression.function.scalar.script.ScriptTemplate;
import org.elasticsearch.xpack.sql.util.Check;

import static org.elasticsearch.search.aggregations.pipeline.PipelineAggregatorBuilders.bucketSelector;

public class AggFilter extends PipelineAgg {

    private final ScriptTemplate scriptTemplate;
    private final Map<String, String> aggPaths;

    public AggFilter(String name, ScriptTemplate scriptTemplate) {
        super(name);
        Check.isTrue(scriptTemplate != null, "a valid script is required");
        this.scriptTemplate = scriptTemplate;
        this.aggPaths = scriptTemplate.aggPaths();
    }

    public Map<String, String> aggPaths() {
        return aggPaths;
    }

    public Collection<String> aggRefs() {
        return scriptTemplate.aggRefs();
    }

    public ScriptTemplate scriptTemplate() {
        return scriptTemplate;
    }

    @Override
    PipelineAggregationBuilder toBuilder() {
        Script script = scriptTemplate.toPainless();
        return bucketSelector(name(), aggPaths, script);
    }

    @Override
    public int hashCode() {
        return Objects.hash(name(), scriptTemplate);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        AggFilter other = (AggFilter) obj;
        return Objects.equals(name(), other.name())
                && Objects.equals(scriptTemplate(), other.scriptTemplate());
    }

    @Override
    public String toString() {
        return scriptTemplate.toString();
    }
}
