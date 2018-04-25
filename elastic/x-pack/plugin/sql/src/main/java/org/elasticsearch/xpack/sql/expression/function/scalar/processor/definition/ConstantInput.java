/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition;

import org.elasticsearch.xpack.sql.execution.search.SqlSourceBuilder;
import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.runtime.ConstantProcessor;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.runtime.Processor;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;

public class ConstantInput extends LeafInput<Object> {

    public ConstantInput(Location location, Expression expression, Object context) {
        super(location, expression, context);
    }

    @Override
    protected NodeInfo<ConstantInput> info() {
        return NodeInfo.create(this, ConstantInput::new, expression(), context());
    }

    @Override
    public Processor asProcessor() {
        return new ConstantProcessor(context());
    }

    @Override
    public final boolean supportedByAggsOnlyQuery() {
        return true;
    }

    @Override
    public ProcessorDefinition resolveAttributes(AttributeResolver resolver) {
        return this;
    }

    @Override
    public final void collectFields(SqlSourceBuilder sourceBuilder) {
        // Nothing to collect
    }
}
