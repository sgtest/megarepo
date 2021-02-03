/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.sql.expression.function.scalar.string;

import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.gen.pipeline.BinaryPipe;
import org.elasticsearch.xpack.ql.expression.gen.pipeline.Pipe;
import org.elasticsearch.xpack.ql.tree.AbstractNodeTestCase;
import org.elasticsearch.xpack.ql.tree.Source;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.function.Function;

import static org.elasticsearch.xpack.ql.expression.Expressions.pipe;
import static org.elasticsearch.xpack.ql.expression.function.scalar.FunctionTestUtils.randomStringLiteral;
import static org.elasticsearch.xpack.ql.tree.SourceTests.randomSource;

public class ConcatFunctionPipeTests extends AbstractNodeTestCase<ConcatFunctionPipe, Pipe> {

    @Override
    protected ConcatFunctionPipe randomInstance() {
        return randomConcatFunctionPipe();
    }

    private Expression randomConcatFunctionExpression() {
        return randomConcatFunctionPipe().expression();
    }

    public static ConcatFunctionPipe randomConcatFunctionPipe() {
        return (ConcatFunctionPipe) new Concat(
                randomSource(),
                randomStringLiteral(),
                randomStringLiteral())
                .makePipe();
    }

    @Override
    public void testTransform() {
        // test transforming only the properties (source, expression),
        // skipping the children (the two parameters of the binary function) which are tested separately
        ConcatFunctionPipe b1 = randomInstance();

        Expression newExpression = randomValueOtherThan(b1.expression(), () -> randomConcatFunctionExpression());
        ConcatFunctionPipe newB = new ConcatFunctionPipe(
                b1.source(),
                newExpression,
                b1.left(),
                b1.right());
        assertEquals(newB, b1.transformPropertiesOnly(Expression.class, v -> Objects.equals(v, b1.expression()) ? newExpression : v));

        ConcatFunctionPipe b2 = randomInstance();
        Source newLoc = randomValueOtherThan(b2.source(), () -> randomSource());
        newB = new ConcatFunctionPipe(
            newLoc,
            b2.expression(),
            b2.left(),
            b2.right());
        assertEquals(newB,
            b2.transformPropertiesOnly(Source.class, v -> Objects.equals(v, b2.source()) ? newLoc : v));
    }

    @Override
    public void testReplaceChildren() {
        ConcatFunctionPipe b = randomInstance();
        Pipe newLeft = randomValueOtherThan(b.left(), () -> pipe(randomStringLiteral()));
        Pipe newRight = randomValueOtherThan(b.right(), () -> pipe(randomStringLiteral()));
        ConcatFunctionPipe newB =
                new ConcatFunctionPipe(b.source(), b.expression(), b.left(), b.right());
        BinaryPipe transformed = newB.replaceChildren(newLeft, b.right());

        assertEquals(transformed.left(), newLeft);
        assertEquals(transformed.source(), b.source());
        assertEquals(transformed.expression(), b.expression());
        assertEquals(transformed.right(), b.right());

        transformed = newB.replaceChildren(b.left(), newRight);
        assertEquals(transformed.left(), b.left());
        assertEquals(transformed.source(), b.source());
        assertEquals(transformed.expression(), b.expression());
        assertEquals(transformed.right(), newRight);

        transformed = newB.replaceChildren(newLeft, newRight);
        assertEquals(transformed.left(), newLeft);
        assertEquals(transformed.source(), b.source());
        assertEquals(transformed.expression(), b.expression());
        assertEquals(transformed.right(), newRight);
    }

    @Override
    protected ConcatFunctionPipe mutate(ConcatFunctionPipe instance) {
        List<Function<ConcatFunctionPipe, ConcatFunctionPipe>> randoms = new ArrayList<>();
        randoms.add(f -> new ConcatFunctionPipe(f.source(),
                f.expression(),
                randomValueOtherThan(f.left(), () -> pipe(randomStringLiteral())),
                f.right()));
        randoms.add(f -> new ConcatFunctionPipe(f.source(),
                f.expression(),
                f.left(),
                randomValueOtherThan(f.right(), () -> pipe(randomStringLiteral()))));
        randoms.add(f -> new ConcatFunctionPipe(f.source(),
                f.expression(),
                randomValueOtherThan(f.left(), () -> pipe(randomStringLiteral())),
                randomValueOtherThan(f.right(), () -> pipe(randomStringLiteral()))));

        return randomFrom(randoms).apply(instance);
    }

    @Override
    protected ConcatFunctionPipe copy(ConcatFunctionPipe instance) {
        return new ConcatFunctionPipe(instance.source(),
                instance.expression(),
                instance.left(),
                instance.right());
    }
}
