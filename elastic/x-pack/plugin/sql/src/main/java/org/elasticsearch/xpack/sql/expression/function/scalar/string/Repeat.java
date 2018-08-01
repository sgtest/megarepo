/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.string;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinition;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinitions;
import org.elasticsearch.xpack.sql.expression.function.scalar.string.BinaryStringNumericProcessor.BinaryStringNumericOperation;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;

import java.util.function.BiFunction;

/**
 * Creates a string composed of a string repeated count times.
 */
public class Repeat extends BinaryStringNumericFunction {

    public Repeat(Location location, Expression left, Expression right) {
        super(location, left, right);
    }

    @Override
    protected BiFunction<String, Number, String> operation() {
        return BinaryStringNumericOperation.REPEAT;
    }

    @Override
    protected Repeat replaceChildren(Expression newLeft, Expression newRight) {
        return new Repeat(location(), newLeft, newRight);
    }

    @Override
    protected ProcessorDefinition makeProcessorDefinition() {
        return new BinaryStringNumericProcessorDefinition(location(), this,
                ProcessorDefinitions.toProcessorDefinition(left()),
                ProcessorDefinitions.toProcessorDefinition(right()),
                BinaryStringNumericOperation.REPEAT);
    }

    @Override
    protected NodeInfo<Repeat> info() {
        return NodeInfo.create(this, Repeat::new, left(), right());
    }

}
