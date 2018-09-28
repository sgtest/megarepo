/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.math;


import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.Literal;
import org.elasticsearch.xpack.sql.expression.function.scalar.math.MathProcessor.MathOperation;
import org.elasticsearch.xpack.sql.expression.gen.script.Params;
import org.elasticsearch.xpack.sql.expression.gen.script.ScriptTemplate;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.util.StringUtils;

public class E extends MathFunction {

    private static final ScriptTemplate TEMPLATE = new ScriptTemplate("Math.E", Params.EMPTY, DataType.DOUBLE);

    public E(Location location) {
        super(location, new Literal(location, "E", Math.E, DataType.DOUBLE));
    }

    @Override
    protected NodeInfo<E> info() {
        return NodeInfo.create(this);
    }

    @Override
    protected E replaceChild(Expression field) {
        throw new UnsupportedOperationException("this node doesn't have any children");
    }

    @Override
    public Object fold() {
        return Math.E;
    }

    @Override
    protected String functionArgs() {
        return StringUtils.EMPTY;
    }

    @Override
    public ScriptTemplate asScript() {
        return TEMPLATE;
    }

    @Override
    protected MathOperation operation() {
        return MathOperation.E;
    }
}
