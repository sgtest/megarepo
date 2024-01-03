/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.math;

import org.elasticsearch.compute.ann.Evaluator;
import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.xpack.esql.expression.function.FunctionInfo;
import org.elasticsearch.xpack.esql.expression.function.Param;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;

import java.util.List;
import java.util.function.Function;

public class IsInfinite extends RationalUnaryPredicate {

    @FunctionInfo(
        returnType = "boolean",
        description = "Returns true if the specified floating-point value is infinitely large in magnitude."
    )
    public IsInfinite(Source source, @Param(name = "n", type = { "double" }, description = "A floating-point value") Expression field) {
        super(source, field);
    }

    @Override
    public ExpressionEvaluator.Factory toEvaluator(Function<Expression, ExpressionEvaluator.Factory> toEvaluator) {
        return new IsInfiniteEvaluator.Factory(source(), toEvaluator.apply(field()));
    }

    @Evaluator
    static boolean process(double val) {
        return Double.isInfinite(val);
    }

    @Override
    public final Expression replaceChildren(List<Expression> newChildren) {
        return new IsInfinite(source(), newChildren.get(0));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, IsInfinite::new, field());
    }
}
