/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.string;

import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.ann.Evaluator;
import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.xpack.esql.evaluator.mapper.EvaluatorMapper;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.function.scalar.ScalarFunction;
import org.elasticsearch.xpack.ql.expression.gen.script.ScriptTemplate;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;
import org.elasticsearch.xpack.ql.type.DataTypes;

import java.util.Arrays;
import java.util.List;
import java.util.function.Function;

import static org.elasticsearch.xpack.ql.expression.TypeResolutions.ParamOrdinal.FIRST;
import static org.elasticsearch.xpack.ql.expression.TypeResolutions.ParamOrdinal.SECOND;
import static org.elasticsearch.xpack.ql.expression.TypeResolutions.isString;

public class StartsWith extends ScalarFunction implements EvaluatorMapper {

    private final Expression str;
    private final Expression prefix;

    public StartsWith(Source source, Expression str, Expression prefix) {
        super(source, Arrays.asList(str, prefix));
        this.str = str;
        this.prefix = prefix;
    }

    @Override
    public DataType dataType() {
        return DataTypes.BOOLEAN;
    }

    @Override
    protected TypeResolution resolveType() {
        if (childrenResolved() == false) {
            return new TypeResolution("Unresolved children");
        }

        TypeResolution resolution = isString(str, sourceText(), FIRST);
        if (resolution.unresolved()) {
            return resolution;
        }
        return isString(prefix, sourceText(), SECOND);
    }

    @Override
    public boolean foldable() {
        return str.foldable() && prefix.foldable();
    }

    @Override
    public Object fold() {
        return EvaluatorMapper.super.fold();
    }

    @Evaluator
    static boolean process(BytesRef str, BytesRef prefix) {
        if (str.length < prefix.length) {
            return false;
        }
        return Arrays.equals(str.bytes, str.offset, str.offset + prefix.length, prefix.bytes, prefix.offset, prefix.offset + prefix.length);
    }

    @Override
    public Expression replaceChildren(List<Expression> newChildren) {
        return new StartsWith(source(), newChildren.get(0), newChildren.get(1));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, StartsWith::new, str, prefix);
    }

    @Override
    public ScriptTemplate asScript() {
        throw new UnsupportedOperationException("functions do not support scripting");
    }

    @Override
    public ExpressionEvaluator.Factory toEvaluator(Function<Expression, ExpressionEvaluator.Factory> toEvaluator) {
        var strEval = toEvaluator.apply(str);
        var prefixEval = toEvaluator.apply(prefix);
        return dvrCtx -> new StartsWithEvaluator(strEval.get(dvrCtx), prefixEval.get(dvrCtx), dvrCtx);
    }
}
