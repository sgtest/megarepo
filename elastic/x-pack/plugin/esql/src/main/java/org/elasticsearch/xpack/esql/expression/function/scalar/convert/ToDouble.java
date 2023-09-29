/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.convert;

import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.TriFunction;
import org.elasticsearch.compute.ann.ConvertEvaluator;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;

import java.util.List;
import java.util.Map;

import static org.elasticsearch.xpack.ql.type.DataTypes.BOOLEAN;
import static org.elasticsearch.xpack.ql.type.DataTypes.DATETIME;
import static org.elasticsearch.xpack.ql.type.DataTypes.DOUBLE;
import static org.elasticsearch.xpack.ql.type.DataTypes.INTEGER;
import static org.elasticsearch.xpack.ql.type.DataTypes.KEYWORD;
import static org.elasticsearch.xpack.ql.type.DataTypes.LONG;
import static org.elasticsearch.xpack.ql.type.DataTypes.UNSIGNED_LONG;
import static org.elasticsearch.xpack.ql.util.NumericUtils.unsignedLongAsNumber;

public class ToDouble extends AbstractConvertFunction {

    private static final Map<
        DataType,
        TriFunction<EvalOperator.ExpressionEvaluator, Source, DriverContext, EvalOperator.ExpressionEvaluator>> EVALUATORS = Map.of(
            DOUBLE,
            (fieldEval, source, driverContext) -> fieldEval,
            BOOLEAN,
            ToDoubleFromBooleanEvaluator::new,
            DATETIME,
            ToDoubleFromLongEvaluator::new, // CastLongToDoubleEvaluator would be a candidate, but not MV'd
            KEYWORD,
            ToDoubleFromStringEvaluator::new,
            UNSIGNED_LONG,
            ToDoubleFromUnsignedLongEvaluator::new,
            LONG,
            ToDoubleFromLongEvaluator::new, // CastLongToDoubleEvaluator would be a candidate, but not MV'd
            INTEGER,
            ToDoubleFromIntEvaluator::new // CastIntToDoubleEvaluator would be a candidate, but not MV'd
        );

    public ToDouble(Source source, Expression field) {
        super(source, field);
    }

    @Override
    protected
        Map<DataType, TriFunction<EvalOperator.ExpressionEvaluator, Source, DriverContext, EvalOperator.ExpressionEvaluator>>
        evaluators() {
        return EVALUATORS;
    }

    @Override
    public DataType dataType() {
        return DOUBLE;
    }

    @Override
    public Expression replaceChildren(List<Expression> newChildren) {
        return new ToDouble(source(), newChildren.get(0));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, ToDouble::new, field());
    }

    @ConvertEvaluator(extraName = "FromBoolean")
    static double fromBoolean(boolean bool) {
        return bool ? 1d : 0d;
    }

    @ConvertEvaluator(extraName = "FromString")
    static double fromKeyword(BytesRef in) {
        return Double.parseDouble(in.utf8ToString());
    }

    @ConvertEvaluator(extraName = "FromUnsignedLong")
    static double fromUnsignedLong(long l) {
        return unsignedLongAsNumber(l).doubleValue();
    }

    @ConvertEvaluator(extraName = "FromLong")
    static double fromLong(long l) {
        return l;
    }

    @ConvertEvaluator(extraName = "FromInt")
    static double fromInt(int i) {
        return i;
    }
}
