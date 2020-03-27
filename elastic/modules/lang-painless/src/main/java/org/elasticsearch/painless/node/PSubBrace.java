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
import org.elasticsearch.painless.ir.BraceSubNode;
import org.elasticsearch.painless.ir.ClassNode;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.util.Objects;

/**
 * Represents an array load/store.
 */
public class PSubBrace extends AStoreable {

    protected final Class<?> clazz;
    protected final AExpression index;

    PSubBrace(Location location, Class<?> clazz, AExpression index) {
        super(location);

        this.clazz = Objects.requireNonNull(clazz);
        this.index = Objects.requireNonNull(index);
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, AStoreable.Input input) {
        Output output = new Output();

        Input indexInput = new Input();
        indexInput.expected = int.class;
        Output indexOutput = index.analyze(classNode, scriptRoot, scope, indexInput);
        index.cast(indexInput, indexOutput);

        output.actual = clazz.getComponentType();

        BraceSubNode braceSubNode = new BraceSubNode();

        braceSubNode.setChildNode(index.cast(indexOutput));

        braceSubNode.setLocation(location);
        braceSubNode.setExpressionType(output.actual);

        output.expressionNode = braceSubNode;

        return output;
    }

    @Override
    boolean isDefOptimized() {
        return false;
    }
}
