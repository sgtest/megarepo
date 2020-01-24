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

import org.elasticsearch.painless.Locals;
import org.elasticsearch.painless.Locals.Variable;
import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.ir.DeclarationNode;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.util.Objects;
import java.util.Set;

/**
 * Represents a single variable declaration.
 */
public final class SDeclaration extends AStatement {

    private final DType type;
    private final String name;
    private AExpression expression;

    Variable variable = null;

    public SDeclaration(Location location, DType type, String name, AExpression expression) {
        super(location);

        this.type = Objects.requireNonNull(type);
        this.name = Objects.requireNonNull(name);
        this.expression = expression;
    }

    @Override
    void extractVariables(Set<String> variables) {
        variables.add(name);

        if (expression != null) {
            expression.extractVariables(variables);
        }
    }

    @Override
    void analyze(ScriptRoot scriptRoot, Locals locals) {
        DResolvedType resolvedType = type.resolveType(scriptRoot.getPainlessLookup());

        if (expression != null) {
            expression.expected = resolvedType.getType();
            expression.analyze(scriptRoot, locals);
            expression = expression.cast(scriptRoot, locals);
        }

        variable = locals.addVariable(location, resolvedType.getType(), name, false);
    }

    @Override
    DeclarationNode write() {
        DeclarationNode declarationNode = new DeclarationNode();

        declarationNode.setExpressionNode(expression == null ? null : expression.write());

        declarationNode.setLocation(location);
        declarationNode.setVariable(variable);

        return declarationNode;
    }

    @Override
    public String toString() {
        if (expression == null) {
            return singleLineToString(type, name);
        }
        return singleLineToString(type, name, expression);
    }
}
