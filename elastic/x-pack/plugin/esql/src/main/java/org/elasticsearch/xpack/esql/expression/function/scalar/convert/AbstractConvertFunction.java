/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.convert;

import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.elasticsearch.common.TriFunction;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.data.Vector;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.xpack.esql.EsqlIllegalArgumentException;
import org.elasticsearch.xpack.esql.evaluator.mapper.EvaluatorMapper;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.esql.expression.function.scalar.UnaryScalarFunction;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;

import java.util.Locale;
import java.util.Map;
import java.util.function.Function;

import static org.elasticsearch.xpack.ql.expression.TypeResolutions.isType;

/**
 * Base class for functions that converts a field into a function-specific type.
 */
public abstract class AbstractConvertFunction extends UnaryScalarFunction implements EvaluatorMapper {

    protected AbstractConvertFunction(Source source, Expression field) {
        super(source, field);
    }

    /**
     * Build the evaluator given the evaluator a multivalued field.
     */
    protected ExpressionEvaluator.Factory evaluator(ExpressionEvaluator.Factory fieldEval) {
        DataType sourceType = field().dataType();
        var evaluator = evaluators().get(sourceType);
        if (evaluator == null) {
            throw EsqlIllegalArgumentException.illegalDataType(sourceType);
        }
        return dvrCtx -> evaluator.apply(fieldEval.get(dvrCtx), source(), dvrCtx);
    }

    @Override
    protected final TypeResolution resolveType() {
        if (childrenResolved() == false) {
            return new TypeResolution("Unresolved children");
        }
        return isType(
            field(),
            evaluators()::containsKey,
            sourceText(),
            null,
            evaluators().keySet().stream().map(dt -> dt.name().toLowerCase(Locale.ROOT)).sorted().toArray(String[]::new)
        );
    }

    protected abstract Map<DataType, TriFunction<ExpressionEvaluator, Source, DriverContext, ExpressionEvaluator>> evaluators();

    @Override
    public final Object fold() {
        return EvaluatorMapper.super.fold();
    }

    @Override
    public ExpressionEvaluator.Factory toEvaluator(Function<Expression, ExpressionEvaluator.Factory> toEvaluator) {
        return evaluator(toEvaluator.apply(field()));
    }

    public abstract static class AbstractEvaluator implements EvalOperator.ExpressionEvaluator {

        private static final Log logger = LogFactory.getLog(AbstractEvaluator.class);

        protected final DriverContext driverContext;
        private final EvalOperator.ExpressionEvaluator fieldEvaluator;
        private final Warnings warnings;

        protected AbstractEvaluator(DriverContext driverContext, EvalOperator.ExpressionEvaluator field, Source source) {
            this.driverContext = driverContext;
            this.fieldEvaluator = field;
            this.warnings = new Warnings(source);
        }

        protected abstract String name();

        /**
         * Called when evaluating a {@link Block} that contains null values.
         */
        protected abstract Block evalBlock(Block b);

        /**
         * Called when evaluating a {@link Block} that does not contain null values.
         */
        protected abstract Block evalVector(Vector v);

        public Block.Ref eval(Page page) {
            try (Block.Ref ref = fieldEvaluator.eval(page)) {
                if (ref.block().areAllValuesNull()) {
                    return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
                }
                Vector vector = ref.block().asVector();
                return Block.Ref.floating(vector == null ? evalBlock(ref.block()) : evalVector(vector));
            }
        }

        protected final void registerException(Exception exception) {
            logger.trace("conversion failure", exception);
            warnings.registerException(exception);
        }

        @Override
        public final String toString() {
            return name() + "Evaluator[field=" + fieldEvaluator + "]";
        }

        @Override
        public void close() {
            // TODO toString allocates - we should probably check breakers there too
            Releasables.closeExpectNoException(fieldEvaluator);
        }
    }
}
