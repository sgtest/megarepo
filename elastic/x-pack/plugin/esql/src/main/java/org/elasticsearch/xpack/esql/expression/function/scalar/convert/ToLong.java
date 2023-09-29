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

import static org.elasticsearch.xpack.ql.type.DataTypeConverter.safeDoubleToLong;
import static org.elasticsearch.xpack.ql.type.DataTypeConverter.safeToLong;
import static org.elasticsearch.xpack.ql.type.DataTypes.BOOLEAN;
import static org.elasticsearch.xpack.ql.type.DataTypes.DATETIME;
import static org.elasticsearch.xpack.ql.type.DataTypes.DOUBLE;
import static org.elasticsearch.xpack.ql.type.DataTypes.INTEGER;
import static org.elasticsearch.xpack.ql.type.DataTypes.KEYWORD;
import static org.elasticsearch.xpack.ql.type.DataTypes.LONG;
import static org.elasticsearch.xpack.ql.type.DataTypes.UNSIGNED_LONG;
import static org.elasticsearch.xpack.ql.util.NumericUtils.unsignedLongAsNumber;

public class ToLong extends AbstractConvertFunction {

    private static final Map<
        DataType,
        TriFunction<EvalOperator.ExpressionEvaluator, Source, DriverContext, EvalOperator.ExpressionEvaluator>> EVALUATORS = Map.of(
            LONG,
            (fieldEval, source, driverContext) -> fieldEval,
            DATETIME,
            (fieldEval, source, driverContext) -> fieldEval,
            BOOLEAN,
            ToLongFromBooleanEvaluator::new,
            KEYWORD,
            ToLongFromStringEvaluator::new,
            DOUBLE,
            ToLongFromDoubleEvaluator::new,
            UNSIGNED_LONG,
            ToLongFromUnsignedLongEvaluator::new,
            INTEGER,
            ToLongFromIntEvaluator::new // CastIntToLongEvaluator would be a candidate, but not MV'd
        );

    public ToLong(Source source, Expression field) {
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
        return LONG;
    }

    @Override
    public Expression replaceChildren(List<Expression> newChildren) {
        return new ToLong(source(), newChildren.get(0));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, ToLong::new, field());
    }

    @ConvertEvaluator(extraName = "FromBoolean")
    static long fromBoolean(boolean bool) {
        return bool ? 1L : 0L;
    }

    @ConvertEvaluator(extraName = "FromString")
    static long fromKeyword(BytesRef in) {
        String asString = in.utf8ToString();
        try {
            return Long.parseLong(asString);
        } catch (NumberFormatException nfe) {
            try {
                return fromDouble(Double.parseDouble(asString));
            } catch (Exception e) {
                throw nfe;
            }
        }
    }

    @ConvertEvaluator(extraName = "FromDouble")
    static long fromDouble(double dbl) {
        return safeDoubleToLong(dbl);
    }

    @ConvertEvaluator(extraName = "FromUnsignedLong")
    static long fromUnsignedLong(long ul) {
        return safeToLong(unsignedLongAsNumber(ul));
    }

    @ConvertEvaluator(extraName = "FromInt")
    static long fromInt(int i) {
        return i;
    }
}
