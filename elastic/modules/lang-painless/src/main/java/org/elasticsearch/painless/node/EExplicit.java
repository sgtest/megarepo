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

import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.Scope;
import org.elasticsearch.painless.ir.ClassNode;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.util.Objects;

/**
 * Represents an explicit cast.
 */
public class EExplicit extends AExpression {

    protected final String type;
    protected final AExpression child;

    public EExplicit(Location location, String type, AExpression child) {
        super(location);

        this.type = Objects.requireNonNull(type);
        this.child = Objects.requireNonNull(child);
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, Input input) {
        Output output = new Output();

        output.actual = scriptRoot.getPainlessLookup().canonicalTypeNameToType(type);

        if (output.actual == null) {
            throw createError(new IllegalArgumentException("Not a type [" + type + "]."));
        }

        Input childInput = new Input();
        childInput.expected = output.actual;
        childInput.explicit = true;
        Output childOutput = child.analyze(classNode, scriptRoot, scope, childInput);
        child.cast(childInput, childOutput);

        output.expressionNode = child.cast(childOutput);

        return output;
    }
}
