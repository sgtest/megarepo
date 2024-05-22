/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar;

import org.elasticsearch.xpack.esql.core.expression.Expression;
import org.elasticsearch.xpack.esql.core.expression.function.scalar.ScalarFunction;
import org.elasticsearch.xpack.esql.core.tree.Source;
import org.elasticsearch.xpack.esql.evaluator.mapper.EvaluatorMapper;

import java.util.List;

public abstract class EsqlScalarFunction extends ScalarFunction implements EvaluatorMapper {

    protected EsqlScalarFunction(Source source) {
        super(source);
    }

    protected EsqlScalarFunction(Source source, List<Expression> fields) {
        super(source, fields);
    }

    @Override
    public Object fold() {
        return EvaluatorMapper.super.fold();
    }

}
