/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.multivalue;

import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.xpack.esql.type.EsqlDataTypes;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;
import org.elasticsearch.xpack.ql.type.DataTypes;

import java.util.List;

import static org.elasticsearch.xpack.ql.expression.TypeResolutions.isType;

/**
 * Reduce a multivalued field to a single valued field containing the minimum value.
 */
public class MvCount extends AbstractMultivalueFunction {
    public MvCount(Source source, Expression field) {
        super(source, field);
    }

    @Override
    protected TypeResolution resolveFieldType() {
        return isType(field(), EsqlDataTypes::isRepresentable, sourceText(), null, "representable");
    }

    @Override
    public DataType dataType() {
        return DataTypes.INTEGER;
    }

    @Override
    protected ExpressionEvaluator.Factory evaluator(ExpressionEvaluator.Factory fieldEval) {
        return dvrCtx -> new Evaluator(dvrCtx, fieldEval.get(dvrCtx));
    }

    @Override
    public Expression replaceChildren(List<Expression> newChildren) {
        return new MvCount(source(), newChildren.get(0));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, MvCount::new, field());
    }

    private static class Evaluator extends AbstractEvaluator {
        private final DriverContext driverContext;

        protected Evaluator(DriverContext driverContext, EvalOperator.ExpressionEvaluator field) {
            super(field);
            this.driverContext = driverContext;
        }

        @Override
        protected String name() {
            return "MvCount";
        }

        @Override
        protected Block.Ref evalNullable(Block.Ref ref) {
            try (ref; IntBlock.Builder builder = IntBlock.newBlockBuilder(ref.block().getPositionCount(), driverContext.blockFactory())) {
                for (int p = 0; p < ref.block().getPositionCount(); p++) {
                    int valueCount = ref.block().getValueCount(p);
                    if (valueCount == 0) {
                        builder.appendNull();
                        continue;
                    }
                    builder.appendInt(valueCount);
                }
                return Block.Ref.floating(builder.build());
            }
        }

        @Override
        protected Block.Ref evalNotNullable(Block.Ref ref) {
            try (
                ref;
                IntVector.FixedBuilder builder = IntVector.newVectorFixedBuilder(
                    ref.block().getPositionCount(),
                    driverContext.blockFactory()
                )
            ) {
                for (int p = 0; p < ref.block().getPositionCount(); p++) {
                    builder.appendInt(ref.block().getValueCount(p));
                }
                return Block.Ref.floating(builder.build().asBlock());
            }
        }

        @Override
        protected Block.Ref evalSingleValuedNullable(Block.Ref ref) {
            return evalNullable(ref);
        }

        @Override
        protected Block.Ref evalSingleValuedNotNullable(Block.Ref ref) {
            try (ref) {
                return Block.Ref.floating(driverContext.blockFactory().newConstantIntBlockWith(1, ref.block().getPositionCount()));
            }
        }
    }
}
