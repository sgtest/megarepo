/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.math;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.math.BinaryMathProcessor.BinaryMathOperation;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinition;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinitions;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;

import java.util.function.BiFunction;

public class Power extends BinaryNumericFunction {

    public Power(Location location, Expression left, Expression right) {
        super(location, left, right);
    }

    @Override
    protected BiFunction<Number, Number, Number> operation() {
        return BinaryMathOperation.POWER;
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, Power::new, left(), right());
    }

    @Override
    protected Power replaceChildren(Expression newLeft, Expression newRight) {
        return new Power(location(), newLeft, newRight);
    }

    @Override
    protected ProcessorDefinition makeProcessorDefinition() {
        return new BinaryMathProcessorDefinition(location(), this,
                ProcessorDefinitions.toProcessorDefinition(left()),
                ProcessorDefinitions.toProcessorDefinition(right()),
                BinaryMathOperation.POWER);
    }

    @Override
    protected String mathFunction() {
        return "pow";
    }
}
