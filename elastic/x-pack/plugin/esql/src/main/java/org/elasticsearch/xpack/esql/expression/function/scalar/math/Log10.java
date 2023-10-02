/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.math;

import org.elasticsearch.compute.ann.Evaluator;
import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.xpack.esql.EsqlIllegalArgumentException;
import org.elasticsearch.xpack.esql.evaluator.mapper.EvaluatorMapper;
import org.elasticsearch.xpack.esql.expression.function.Param;
import org.elasticsearch.xpack.esql.expression.function.scalar.UnaryScalarFunction;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;
import org.elasticsearch.xpack.ql.type.DataTypes;
import org.elasticsearch.xpack.ql.util.NumericUtils;

import java.util.List;
import java.util.function.Function;

import static org.elasticsearch.xpack.ql.expression.TypeResolutions.ParamOrdinal.DEFAULT;
import static org.elasticsearch.xpack.ql.expression.TypeResolutions.isNumeric;

public class Log10 extends UnaryScalarFunction implements EvaluatorMapper {
    public Log10(Source source, @Param(name = "n", type = { "integer", "long", "double", "unsigned_long" }) Expression n) {
        super(source, n);
    }

    @Override
    public ExpressionEvaluator.Factory toEvaluator(Function<Expression, ExpressionEvaluator.Factory> toEvaluator) {
        var field = toEvaluator.apply(field());
        var fieldType = field().dataType();

        if (fieldType == DataTypes.DOUBLE) {
            return dvrCtx -> new Log10DoubleEvaluator(source(), field.get(dvrCtx), dvrCtx);
        }
        if (fieldType == DataTypes.INTEGER) {
            return dvrCtx -> new Log10IntEvaluator(source(), field.get(dvrCtx), dvrCtx);
        }
        if (fieldType == DataTypes.LONG) {
            return dvrCtx -> new Log10LongEvaluator(source(), field.get(dvrCtx), dvrCtx);
        }
        if (fieldType == DataTypes.UNSIGNED_LONG) {
            return dvrCtx -> new Log10UnsignedLongEvaluator(source(), field.get(dvrCtx), dvrCtx);
        }

        throw EsqlIllegalArgumentException.illegalDataType(fieldType);
    }

    @Evaluator(extraName = "Double", warnExceptions = ArithmeticException.class)
    static double process(double val) {
        if (val <= 0d) {
            throw new ArithmeticException("Log of non-positive number");
        }
        return Math.log10(val);
    }

    @Evaluator(extraName = "Long", warnExceptions = ArithmeticException.class)
    static double process(long val) {
        if (val <= 0L) {
            throw new ArithmeticException("Log of non-positive number");
        }
        return Math.log10(val);
    }

    @Evaluator(extraName = "UnsignedLong", warnExceptions = ArithmeticException.class)
    static double processUnsignedLong(long val) {
        if (val == NumericUtils.ZERO_AS_UNSIGNED_LONG) {
            throw new ArithmeticException("Log of non-positive number");
        }
        return Math.log10(NumericUtils.unsignedLongToDouble(val));
    }

    @Evaluator(extraName = "Int", warnExceptions = ArithmeticException.class)
    static double process(int val) {
        if (val <= 0) {
            throw new ArithmeticException("Log of non-positive number");
        }
        return Math.log10(val);
    }

    @Override
    public final Expression replaceChildren(List<Expression> newChildren) {
        return new Log10(source(), newChildren.get(0));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, Log10::new, field());
    }

    @Override
    public DataType dataType() {
        return DataTypes.DOUBLE;
    }

    @Override
    public Object fold() {
        return EvaluatorMapper.super.fold();
    }

    @Override
    protected TypeResolution resolveType() {
        if (childrenResolved() == false) {
            return new Expression.TypeResolution("Unresolved children");
        }

        return isNumeric(field, sourceText(), DEFAULT);
    }
}
