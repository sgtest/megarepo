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
 * Returns the rightmost count characters of a string.
 */
public class Right extends BinaryStringNumericFunction {

    public Right(Location location, Expression left, Expression right) {
        super(location, left, right);
    }

    @Override
    protected BiFunction<String, Number, String> operation() {
        return BinaryStringNumericOperation.RIGHT;
    }

    @Override
    protected Right replaceChildren(Expression newLeft, Expression newRight) {
        return new Right(location(), newLeft, newRight);
    }

    @Override
    protected ProcessorDefinition makeProcessorDefinition() {
        return new BinaryStringNumericProcessorDefinition(location(), this,
                ProcessorDefinitions.toProcessorDefinition(left()),
                ProcessorDefinitions.toProcessorDefinition(right()),
                BinaryStringNumericOperation.RIGHT);
    }

    @Override
    protected NodeInfo<Right> info() {
        return NodeInfo.create(this, Right::new, left(), right());
    }

}
