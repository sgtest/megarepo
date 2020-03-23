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
import org.elasticsearch.painless.ir.NullSafeSubNode;
import org.elasticsearch.painless.symbol.ScriptRoot;

/**
 * Implements a field who's value is null if the prefix is null rather than throwing an NPE.
 */
public class PSubNullSafeField extends AStoreable {

    protected final AStoreable guarded;

    public PSubNullSafeField(Location location, AStoreable guarded) {
        super(location);
        this.guarded = guarded;
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, AStoreable.Input input) {
        Output output = new Output();

        if (input.write) {
            throw createError(new IllegalArgumentException("Can't write to null safe reference"));
        }

        Input guardedInput = new Input();
        guardedInput.read = input.read;
        Output guardedOutput = guarded.analyze(classNode, scriptRoot, scope, guardedInput);
        output.actual = guardedOutput.actual;

        if (output.actual.isPrimitive()) {
            throw new IllegalArgumentException("Result of null safe operator must be nullable");
        }

        NullSafeSubNode nullSafeSubNode = new NullSafeSubNode();

        nullSafeSubNode.setChildNode(guardedOutput.expressionNode);

        nullSafeSubNode.setLocation(location);
        nullSafeSubNode.setExpressionType(output.actual);

        output.expressionNode = nullSafeSubNode;

        return output;
    }

    @Override
    boolean isDefOptimized() {
        return false;
    }

    @Override
    public String toString() {
        return singleLineToString(guarded);
    }
}
