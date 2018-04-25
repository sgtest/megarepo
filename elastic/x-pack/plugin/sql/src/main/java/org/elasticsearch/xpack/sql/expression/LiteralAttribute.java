/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression;

import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ConstantInput;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinition;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;
import org.elasticsearch.xpack.sql.type.DataType;

public class LiteralAttribute extends TypedAttribute {

    private final Literal literal;

    public LiteralAttribute(Literal literal) {
        this(literal.location(), String.valueOf(literal.fold()), null, false, null, false, literal.dataType(), literal);
    }

    public LiteralAttribute(Location location, String name, String qualifier, boolean nullable, ExpressionId id, boolean synthetic,
            DataType dataType, Literal literal) {
        super(location, name, dataType, qualifier, nullable, id, synthetic);
        this.literal = literal;
    }

    public Literal literal() {
        return literal;
    }

    @Override
    protected NodeInfo<LiteralAttribute> info() {
        return NodeInfo.create(this, LiteralAttribute::new,
            name(), qualifier(), nullable(), id(), synthetic(), dataType(), literal);
    }

    @Override
    protected LiteralAttribute clone(Location location, String name, String qualifier, boolean nullable,
                                     ExpressionId id, boolean synthetic) {
        return new LiteralAttribute(location, name, qualifier, nullable, id, synthetic, dataType(), literal);
    }

    public ProcessorDefinition asProcessorDefinition() {
        return new ConstantInput(location(), literal, literal.value());
    }

    @Override
    protected String label() {
        return "c";
    }
}
