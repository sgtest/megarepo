/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.painless.node;

import org.elasticsearch.painless.AnalyzerCaster;
import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.Scope;
import org.elasticsearch.painless.ir.ClassNode;
import org.elasticsearch.painless.ir.ElvisNode;
import org.elasticsearch.painless.symbol.ScriptRoot;

import static java.util.Objects.requireNonNull;

/**
 * The Elvis operator ({@code ?:}), a null coalescing operator. Binary operator that evaluates the first expression and return it if it is
 * non null. If the first expression is null then it evaluates the second expression and returns it.
 */
public class EElvis extends AExpression {

    protected AExpression lhs;
    protected AExpression rhs;

    public EElvis(Location location, AExpression lhs, AExpression rhs) {
        super(location);

        this.lhs = requireNonNull(lhs);
        this.rhs = requireNonNull(rhs);
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, Input input) {
        Output output = new Output();

        if (input.expected != null && input.expected.isPrimitive()) {
            throw createError(new IllegalArgumentException("Elvis operator cannot return primitives"));
        }

        Input leftInput = new Input();
        leftInput.expected = input.expected;
        leftInput.explicit = input.explicit;
        leftInput.internal = input.internal;
        Output leftOutput = lhs.analyze(classNode, scriptRoot, scope, leftInput);

        Input rightInput = new Input();
        rightInput.expected = input.expected;
        rightInput.explicit = input.explicit;
        rightInput.internal = input.internal;
        Output rightOutput = rhs.analyze(classNode, scriptRoot, scope, rightInput);

        output.actual = input.expected;

        if (lhs instanceof ENull) {
            throw createError(new IllegalArgumentException("Extraneous elvis operator. LHS is null."));
        }
        if (lhs instanceof EBoolean
                || lhs instanceof ENumeric
                || lhs instanceof EDecimal
                || lhs instanceof EString
                || lhs instanceof EConstant) {
            throw createError(new IllegalArgumentException("Extraneous elvis operator. LHS is a constant."));
        }
        if (leftOutput.actual.isPrimitive()) {
            throw createError(new IllegalArgumentException("Extraneous elvis operator. LHS is a primitive."));
        }
        if (rhs instanceof ENull) {
            throw createError(new IllegalArgumentException("Extraneous elvis operator. RHS is null."));
        }

        if (input.expected == null) {
            Class<?> promote = AnalyzerCaster.promoteConditional(leftOutput.actual, rightOutput.actual);

            leftInput.expected = promote;
            rightInput.expected = promote;
            output.actual = promote;
        }

        lhs.cast(leftInput, leftOutput);
        rhs.cast(rightInput, rightOutput);

        ElvisNode elvisNode = new ElvisNode();

        elvisNode.setLeftNode(lhs.cast(leftOutput));
        elvisNode.setRightNode(rhs.cast(rightOutput));

        elvisNode.setLocation(location);
        elvisNode.setExpressionType(output.actual);

        output.expressionNode = elvisNode;

        return output;
    }
}
