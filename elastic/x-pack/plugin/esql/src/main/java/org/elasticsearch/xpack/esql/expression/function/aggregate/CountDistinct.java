/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.aggregate;

import org.elasticsearch.compute.aggregation.AggregatorFunctionSupplier;
import org.elasticsearch.compute.aggregation.CountDistinctBooleanAggregatorFunctionSupplier;
import org.elasticsearch.compute.aggregation.CountDistinctBytesRefAggregatorFunctionSupplier;
import org.elasticsearch.compute.aggregation.CountDistinctDoubleAggregatorFunctionSupplier;
import org.elasticsearch.compute.aggregation.CountDistinctIntAggregatorFunctionSupplier;
import org.elasticsearch.compute.aggregation.CountDistinctLongAggregatorFunctionSupplier;
import org.elasticsearch.xpack.esql.EsqlIllegalArgumentException;
import org.elasticsearch.xpack.esql.core.expression.Expression;
import org.elasticsearch.xpack.esql.core.expression.Literal;
import org.elasticsearch.xpack.esql.core.expression.function.OptionalArgument;
import org.elasticsearch.xpack.esql.core.tree.NodeInfo;
import org.elasticsearch.xpack.esql.core.tree.Source;
import org.elasticsearch.xpack.esql.core.type.DataType;
import org.elasticsearch.xpack.esql.core.type.DataTypes;
import org.elasticsearch.xpack.esql.expression.EsqlTypeResolutions;
import org.elasticsearch.xpack.esql.expression.SurrogateExpression;
import org.elasticsearch.xpack.esql.expression.function.FunctionInfo;
import org.elasticsearch.xpack.esql.expression.function.Param;
import org.elasticsearch.xpack.esql.expression.function.scalar.convert.ToLong;
import org.elasticsearch.xpack.esql.expression.function.scalar.multivalue.MvCount;
import org.elasticsearch.xpack.esql.expression.function.scalar.multivalue.MvDedupe;
import org.elasticsearch.xpack.esql.expression.function.scalar.nulls.Coalesce;
import org.elasticsearch.xpack.esql.planner.ToAggregator;

import java.util.List;

import static org.elasticsearch.xpack.esql.core.expression.TypeResolutions.ParamOrdinal.DEFAULT;
import static org.elasticsearch.xpack.esql.core.expression.TypeResolutions.ParamOrdinal.SECOND;
import static org.elasticsearch.xpack.esql.core.expression.TypeResolutions.isFoldable;
import static org.elasticsearch.xpack.esql.core.expression.TypeResolutions.isInteger;
import static org.elasticsearch.xpack.esql.core.expression.TypeResolutions.isType;

public class CountDistinct extends AggregateFunction implements OptionalArgument, ToAggregator, SurrogateExpression {
    private static final int DEFAULT_PRECISION = 3000;
    private final Expression precision;

    @FunctionInfo(returnType = "long", description = "Returns the approximate number of distinct values.", isAggregation = true)
    public CountDistinct(
        Source source,
        @Param(
            name = "field",
            type = { "boolean", "cartesian_point", "date", "double", "geo_point", "integer", "ip", "keyword", "long", "text", "version" },
            description = "Column or literal for which to count the number of distinct values."
        ) Expression field,
        @Param(optional = true, name = "precision", type = { "integer" }) Expression precision
    ) {
        super(source, field, precision != null ? List.of(precision) : List.of());
        this.precision = precision;
    }

    @Override
    protected NodeInfo<CountDistinct> info() {
        return NodeInfo.create(this, CountDistinct::new, field(), precision);
    }

    @Override
    public CountDistinct replaceChildren(List<Expression> newChildren) {
        return new CountDistinct(source(), newChildren.get(0), newChildren.size() > 1 ? newChildren.get(1) : null);
    }

    @Override
    public DataType dataType() {
        return DataTypes.LONG;
    }

    @Override
    protected TypeResolution resolveType() {
        if (childrenResolved() == false) {
            return new TypeResolution("Unresolved children");
        }

        TypeResolution resolution = EsqlTypeResolutions.isExact(field(), sourceText(), DEFAULT);
        if (resolution.unresolved()) {
            return resolution;
        }

        boolean resolved = resolution.resolved();
        resolution = isType(
            field(),
            dt -> resolved && dt != DataTypes.UNSIGNED_LONG,
            sourceText(),
            DEFAULT,
            "any exact type except unsigned_long or counter types"
        );
        if (resolution.unresolved() || precision == null) {
            return resolution;
        }
        return isInteger(precision, sourceText(), SECOND).and(isFoldable(precision, sourceText(), SECOND));
    }

    @Override
    public AggregatorFunctionSupplier supplier(List<Integer> inputChannels) {
        DataType type = field().dataType();
        int precision = this.precision == null ? DEFAULT_PRECISION : ((Number) this.precision.fold()).intValue();
        if (type == DataTypes.BOOLEAN) {
            // Booleans ignore the precision because there are only two possible values anyway
            return new CountDistinctBooleanAggregatorFunctionSupplier(inputChannels);
        }
        if (type == DataTypes.DATETIME || type == DataTypes.LONG) {
            return new CountDistinctLongAggregatorFunctionSupplier(inputChannels, precision);
        }
        if (type == DataTypes.INTEGER) {
            return new CountDistinctIntAggregatorFunctionSupplier(inputChannels, precision);
        }
        if (type == DataTypes.DOUBLE) {
            return new CountDistinctDoubleAggregatorFunctionSupplier(inputChannels, precision);
        }
        if (type == DataTypes.KEYWORD || type == DataTypes.IP || type == DataTypes.VERSION || type == DataTypes.TEXT) {
            return new CountDistinctBytesRefAggregatorFunctionSupplier(inputChannels, precision);
        }
        throw EsqlIllegalArgumentException.illegalDataType(type);
    }

    @Override
    public Expression surrogate() {
        var s = source();
        var field = field();

        return field.foldable()
            ? new ToLong(s, new Coalesce(s, new MvCount(s, new MvDedupe(s, field)), List.of(new Literal(s, 0, DataTypes.INTEGER))))
            : null;
    }
}
