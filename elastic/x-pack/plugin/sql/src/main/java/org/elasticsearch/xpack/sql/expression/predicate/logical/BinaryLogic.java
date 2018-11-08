/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.predicate.logical;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.Expressions;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.Pipe;
import org.elasticsearch.xpack.sql.expression.predicate.BinaryOperator;
import org.elasticsearch.xpack.sql.expression.predicate.logical.BinaryLogicProcessor.BinaryLogicOperation;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.type.DataType;

public abstract class BinaryLogic extends BinaryOperator<Boolean, Boolean, Boolean, BinaryLogicOperation> {

    protected BinaryLogic(Location location, Expression left, Expression right, BinaryLogicOperation operation) {
        super(location, left, right, operation);
    }

    @Override
    public DataType dataType() {
        return DataType.BOOLEAN;
    }

    @Override
    protected TypeResolution resolveInputType(Expression e, Expressions.ParamOrdinal paramOrdinal) {
        return Expressions.typeMustBeBoolean(e, functionName(), paramOrdinal);
    }

    @Override
    protected Pipe makePipe() {
        return new BinaryLogicPipe(location(), this, Expressions.pipe(left()), Expressions.pipe(right()), function());
    }

    @Override
    public boolean nullable() {
        // Cannot fold null due to 3vl, constant folding will do any possible folding.
        return false;
    }
}
