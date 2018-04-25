/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.math;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.math.MathProcessor.MathOperation;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;

import java.util.Locale;

import static java.lang.String.format;

/**
 * <a href="https://en.wikipedia.org/wiki/Trigonometric_functions#Cosecant,_secant,_and_cotangent">Cotangent</a>
 * function.
 */
public class Cot extends MathFunction {
    public Cot(Location location, Expression field) {
        super(location, field);
    }

    @Override
    protected NodeInfo<Cot> info() {
        return NodeInfo.create(this, Cot::new, field());
    }

    @Override
    protected Cot replaceChild(Expression newChild) {
        return new Cot(location(), newChild);
    }

    @Override
    protected String formatScript(String template) {
        return super.formatScript(format(Locale.ROOT, "1.0 / Math.tan(%s)", template));
    }

    @Override
    protected MathOperation operation() {
        return MathOperation.COT;
    }
}
