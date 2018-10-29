/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.math;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.Expressions;
import org.elasticsearch.xpack.sql.expression.Expressions.ParamOrdinal;
import org.elasticsearch.xpack.sql.expression.function.scalar.BinaryScalarFunction;
import org.elasticsearch.xpack.sql.expression.function.scalar.math.BinaryMathProcessor.BinaryMathOperation;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.Pipe;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.type.DataType;

import java.util.Objects;

public abstract class BinaryNumericFunction extends BinaryScalarFunction {

    private final BinaryMathOperation operation;

    BinaryNumericFunction(Location location, Expression left, Expression right, BinaryMathOperation operation) {
        super(location, left, right);
        this.operation = operation;
    }

    @Override
    public DataType dataType() {
        return DataType.DOUBLE;
    }

    @Override
    protected TypeResolution resolveType() {
        if (!childrenResolved()) {
            return new TypeResolution("Unresolved children");
        }

        TypeResolution resolution = Expressions.typeMustBeNumeric(left(), functionName(), ParamOrdinal.FIRST);
        if (resolution.unresolved()) {
            return resolution;

        }
        return Expressions.typeMustBeNumeric(right(), functionName(), ParamOrdinal.SECOND);
    }

    @Override
    public Object fold() {
        return operation.apply((Number) left().fold(), (Number) right().fold());
    }

    @Override
    protected Pipe makePipe() {
        return new BinaryMathPipe(location(), this, Expressions.pipe(left()), Expressions.pipe(right()), operation);
    }

    @Override
    public int hashCode() {
        return Objects.hash(left(), right(), operation);
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null || obj.getClass() != getClass()) {
            return false;
        }
        BinaryNumericFunction other = (BinaryNumericFunction) obj;
        return Objects.equals(other.left(), left())
            && Objects.equals(other.right(), right())
                && Objects.equals(other.operation, operation);
    }
}
