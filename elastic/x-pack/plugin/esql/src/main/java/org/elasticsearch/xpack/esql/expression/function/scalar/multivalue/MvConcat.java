/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.multivalue;

import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.BytesRefBuilder;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.xpack.esql.evaluator.mapper.EvaluatorMapper;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.TypeResolutions;
import org.elasticsearch.xpack.ql.expression.function.scalar.BinaryScalarFunction;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;
import org.elasticsearch.xpack.ql.type.DataTypes;

import java.util.function.Function;

import static org.elasticsearch.xpack.ql.expression.TypeResolutions.isString;

/**
 * Reduce a multivalued string field to a single valued field by concatenating all values.
 */
public class MvConcat extends BinaryScalarFunction implements EvaluatorMapper {
    public MvConcat(Source source, Expression field, Expression delim) {
        super(source, field, delim);
    }

    @Override
    protected TypeResolution resolveType() {
        if (childrenResolved() == false) {
            return new TypeResolution("Unresolved children");
        }

        TypeResolution resolution = isString(left(), sourceText(), TypeResolutions.ParamOrdinal.FIRST);
        if (resolution.unresolved()) {
            return resolution;
        }

        return isString(right(), sourceText(), TypeResolutions.ParamOrdinal.SECOND);
    }

    @Override
    public DataType dataType() {
        return DataTypes.KEYWORD;
    }

    @Override
    public ExpressionEvaluator.Factory toEvaluator(Function<Expression, ExpressionEvaluator.Factory> toEvaluator) {
        var fieldEval = toEvaluator.apply(left());
        var delimEval = toEvaluator.apply(right());
        return dvrCtx -> new MvConcatEvaluator(dvrCtx, fieldEval.get(dvrCtx), delimEval.get(dvrCtx));
    }

    @Override
    public Object fold() {
        return EvaluatorMapper.super.fold();
    }

    @Override
    protected BinaryScalarFunction replaceChildren(Expression newLeft, Expression newRight) {
        return new MvConcat(source(), newLeft, newRight);
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, MvConcat::new, left(), right());
    }

    /**
     * Evaluator for {@link MvConcat}. Not generated and doesn't extend from
     * {@link AbstractMultivalueFunction.AbstractEvaluator} because it's just
     * too different from all the other mv operators:
     * <ul>
     *     <li>It takes an extra parameter - the delimiter</li>
     *     <li>That extra parameter makes it much more likely to be {@code null}</li>
     *     <li>The actual joining process needs init step per row - {@link BytesRefBuilder#clear()}</li>
     * </ul>
     */
    private class MvConcatEvaluator implements EvalOperator.ExpressionEvaluator {
        private final DriverContext context;
        private final EvalOperator.ExpressionEvaluator field;
        private final EvalOperator.ExpressionEvaluator delim;

        MvConcatEvaluator(DriverContext context, EvalOperator.ExpressionEvaluator field, EvalOperator.ExpressionEvaluator delim) {
            this.context = context;
            this.field = field;
            this.delim = delim;
        }

        @Override
        public final Block.Ref eval(Page page) {
            try (Block.Ref fieldRef = field.eval(page); Block.Ref delimRef = delim.eval(page)) {
                if (fieldRef.block().areAllValuesNull() || delimRef.block().areAllValuesNull()) {
                    return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), context.blockFactory()));
                }
                BytesRefBlock fieldVal = (BytesRefBlock) fieldRef.block();
                BytesRefBlock delimVal = (BytesRefBlock) delimRef.block();

                int positionCount = page.getPositionCount();
                try (BytesRefBlock.Builder builder = BytesRefBlock.newBlockBuilder(positionCount, context.blockFactory())) {
                    BytesRefBuilder work = new BytesRefBuilder(); // TODO BreakingBytesRefBuilder so we don't blow past circuit breakers
                    BytesRef fieldScratch = new BytesRef();
                    BytesRef delimScratch = new BytesRef();
                    for (int p = 0; p < positionCount; p++) {
                        int fieldValueCount = fieldVal.getValueCount(p);
                        if (fieldValueCount == 0) {
                            builder.appendNull();
                            continue;
                        }
                        if (delimVal.getValueCount(p) != 1) {
                            builder.appendNull();
                            continue;
                        }
                        int first = fieldVal.getFirstValueIndex(p);
                        if (fieldValueCount == 1) {
                            builder.appendBytesRef(fieldVal.getBytesRef(first, fieldScratch));
                            continue;
                        }
                        int end = first + fieldValueCount;
                        BytesRef delim = delimVal.getBytesRef(delimVal.getFirstValueIndex(p), delimScratch);
                        work.clear();
                        work.append(fieldVal.getBytesRef(first, fieldScratch));
                        for (int i = first + 1; i < end; i++) {
                            work.append(delim);
                            work.append(fieldVal.getBytesRef(i, fieldScratch));
                        }
                        builder.appendBytesRef(work.get());
                    }
                    return Block.Ref.floating(builder.build());
                }
            }
        }

        @Override
        public final String toString() {
            return "MvConcat[field=" + field + ", delim=" + delim + "]";
        }

        @Override
        public void close() {}
    }
}
