/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar;

import org.elasticsearch.xpack.esql.core.expression.Expression;
import org.elasticsearch.xpack.esql.core.expression.TypeResolutions;
import org.elasticsearch.xpack.esql.core.tree.Source;
import org.elasticsearch.xpack.esql.core.type.DataType;

import java.util.Arrays;

import static org.elasticsearch.xpack.esql.core.expression.TypeResolutions.isNumeric;

public abstract class UnaryScalarFunction extends EsqlScalarFunction {
    protected final Expression field;

    public UnaryScalarFunction(Source source, Expression field) {
        super(source, Arrays.asList(field));
        this.field = field;
    }

    @Override
    protected Expression.TypeResolution resolveType() {
        if (childrenResolved() == false) {
            return new Expression.TypeResolution("Unresolved children");
        }

        return isNumeric(field, sourceText(), TypeResolutions.ParamOrdinal.DEFAULT);
    }

    @Override
    public boolean foldable() {
        return field.foldable();
    }

    public final Expression field() {
        return field;
    }

    @Override
    public DataType dataType() {
        return field.dataType();
    }
}
