/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.string;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinition;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinitions;
import org.elasticsearch.xpack.sql.expression.function.scalar.string.BinaryStringStringProcessor.BinaryStringStringOperation;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;

import java.util.function.BiFunction;

/**
 * Returns the position of the first character expression in the second character expression, if not found it returns 0.
 */
public class Position extends BinaryStringStringFunction {

    public Position(Location location, Expression left, Expression right) {
        super(location, left, right);
    }

    @Override
    protected BiFunction<String, String, Number> operation() {
        return BinaryStringStringOperation.POSITION;
    }

    @Override
    protected Position replaceChildren(Expression newLeft, Expression newRight) {
        return new Position(location(), newLeft, newRight);
    }

    @Override
    protected ProcessorDefinition makeProcessorDefinition() {
        return new BinaryStringStringProcessorDefinition(location(), this,
                ProcessorDefinitions.toProcessorDefinition(left()),
                ProcessorDefinitions.toProcessorDefinition(right()),
                BinaryStringStringOperation.POSITION);
    }

    @Override
    protected NodeInfo<Position> info() {
        return NodeInfo.create(this, Position::new, left(), right());
    }

}
