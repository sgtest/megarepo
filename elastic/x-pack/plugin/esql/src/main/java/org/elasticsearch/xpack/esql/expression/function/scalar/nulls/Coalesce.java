/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.nulls;

import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.ElementType;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.core.Releasable;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.xpack.esql.evaluator.mapper.EvaluatorMapper;
import org.elasticsearch.xpack.esql.planner.LocalExecutionPlanner;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.Expressions;
import org.elasticsearch.xpack.ql.expression.Nullability;
import org.elasticsearch.xpack.ql.expression.TypeResolutions;
import org.elasticsearch.xpack.ql.expression.function.OptionalArgument;
import org.elasticsearch.xpack.ql.expression.function.scalar.ScalarFunction;
import org.elasticsearch.xpack.ql.expression.gen.script.ScriptTemplate;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;

import java.util.List;
import java.util.function.Function;
import java.util.stream.IntStream;
import java.util.stream.Stream;

import static org.elasticsearch.xpack.ql.type.DataTypes.NULL;

/**
 * Function returning the first non-null value.
 */
public class Coalesce extends ScalarFunction implements EvaluatorMapper, OptionalArgument {
    private DataType dataType;

    public Coalesce(Source source, Expression first, List<Expression> rest) {
        super(source, Stream.concat(Stream.of(first), rest.stream()).toList());
    }

    @Override
    public DataType dataType() {
        if (dataType == null) {
            resolveType();
        }
        return dataType;
    }

    @Override
    protected TypeResolution resolveType() {
        if (childrenResolved() == false) {
            return new TypeResolution("Unresolved children");
        }

        for (int position = 0; position < children().size(); position++) {
            if (dataType == null || dataType == NULL) {
                dataType = children().get(position).dataType();
                continue;
            }
            TypeResolution resolution = TypeResolutions.isType(
                children().get(position),
                t -> t == dataType,
                sourceText(),
                TypeResolutions.ParamOrdinal.fromIndex(position),
                dataType.typeName()
            );
            if (resolution.unresolved()) {
                return resolution;
            }
        }
        return TypeResolution.TYPE_RESOLVED;
    }

    @Override
    public Nullability nullable() {
        // If any of the children aren't nullable then this isn't.
        for (Expression c : children()) {
            if (c.nullable() == Nullability.FALSE) {
                return Nullability.FALSE;
            }
        }
        /*
         * Otherwise let's call this one "unknown". If we returned TRUE here
         * an optimizer rule would replace this with null if any of our children
         * fold to null. We don't want that at all.
         */
        return Nullability.UNKNOWN;
    }

    @Override
    public ScriptTemplate asScript() {
        throw new UnsupportedOperationException("functions do not support scripting");
    }

    @Override
    public Expression replaceChildren(List<Expression> newChildren) {
        return new Coalesce(source(), newChildren.get(0), newChildren.subList(1, newChildren.size()));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, Coalesce::new, children().get(0), children().subList(1, children().size()));
    }

    @Override
    public boolean foldable() {
        return Expressions.foldable(children());
    }

    @Override
    public Object fold() {
        return EvaluatorMapper.super.fold();
    }

    @Override
    public ExpressionEvaluator.Factory toEvaluator(Function<Expression, ExpressionEvaluator.Factory> toEvaluator) {
        return dvrCxt -> new CoalesceEvaluator(
            dvrCxt,
            LocalExecutionPlanner.toElementType(dataType()),
            children().stream().map(toEvaluator).map(x -> x.get(dvrCxt)).toList()
        );
    }

    private record CoalesceEvaluator(DriverContext driverContext, ElementType resultType, List<EvalOperator.ExpressionEvaluator> evaluators)
        implements
            EvalOperator.ExpressionEvaluator {
        @Override
        public Block.Ref eval(Page page) {
            /*
             * We have to evaluate lazily so any errors or warnings that would be
             * produced by the right hand side are avoided. And so if anything
             * on the right hand side is slow we skip it.
             *
             * And it'd be good if that lazy evaluation were fast. But this
             * implementation isn't. It's fairly simple - running position at
             * a time - but it's not at all fast.
             */
            int positionCount = page.getPositionCount();
            try (Block.Builder result = resultType.newBlockBuilder(positionCount, driverContext.blockFactory())) {
                position: for (int p = 0; p < positionCount; p++) {
                    int[] positions = new int[] { p };
                    Page limited = new Page(
                        1,
                        IntStream.range(0, page.getBlockCount()).mapToObj(b -> page.getBlock(b).filter(positions)).toArray(Block[]::new)
                    );
                    try (Releasable ignored = limited::releaseBlocks) {
                        for (EvalOperator.ExpressionEvaluator eval : evaluators) {
                            try (Block.Ref ref = eval.eval(limited)) {
                                if (false == ref.block().isNull(0)) {
                                    result.copyFrom(ref.block(), 0, 1);
                                    continue position;
                                }
                            }
                        }
                        result.appendNull();
                    }
                }
                return Block.Ref.floating(result.build());
            }
        }

        @Override
        public String toString() {
            return "CoalesceEvaluator[values=" + evaluators + ']';
        }

        @Override
        public void close() {
            Releasables.closeExpectNoException(() -> Releasables.close(evaluators));
        }
    }
}
