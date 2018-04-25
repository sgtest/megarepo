/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.script.ScriptTemplate;
import org.elasticsearch.xpack.sql.tree.Location;

import java.util.Arrays;
import java.util.List;

public abstract class BinaryScalarFunction extends ScalarFunction {

    private final Expression left, right;

    protected BinaryScalarFunction(Location location, Expression left, Expression right) {
        super(location, Arrays.asList(left, right));
        this.left = left;
        this.right = right;
    }

    @Override
    public final BinaryScalarFunction replaceChildren(List<Expression> newChildren) {
        if (newChildren.size() != 2) {
            throw new IllegalArgumentException("expected [2] children but received [" + newChildren.size() + "]");
        }
        return replaceChildren(newChildren.get(0), newChildren.get(1));
    }

    protected abstract BinaryScalarFunction replaceChildren(Expression newLeft, Expression newRight);

    public Expression left() {
        return left;
    }

    public Expression right() {
        return right;
    }

    @Override
    public boolean foldable() {
        return left.foldable() && right.foldable();
    }

    @Override
    public ScriptTemplate asScript() {
        ScriptTemplate leftScript = asScript(left());
        ScriptTemplate rightScript = asScript(right());

        return asScriptFrom(leftScript, rightScript);
    }

    protected abstract ScriptTemplate asScriptFrom(ScriptTemplate leftScript, ScriptTemplate rightScript);
}
