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
import org.elasticsearch.painless.ir.DotSubShortcutNode;
import org.elasticsearch.painless.lookup.PainlessMethod;
import org.elasticsearch.painless.symbol.ScriptRoot;

/**
 * Represents a field load/store shortcut.  (Internal only.)
 */
public class PSubShortcut extends AExpression {

    protected final String value;
    protected final String type;
    protected final PainlessMethod getter;
    protected final PainlessMethod setter;

    PSubShortcut(Location location, String value, String type, PainlessMethod getter, PainlessMethod setter) {
        super(location);

        this.value = value;
        this.type = type;
        this.getter = getter;
        this.setter = setter;
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, Input input) {
        Output output = new Output();

        if (getter != null && (getter.returnType == void.class || !getter.typeParameters.isEmpty())) {
            throw createError(new IllegalArgumentException(
                "Illegal get shortcut on field [" + value + "] for type [" + type + "]."));
        }

        if (setter != null && (setter.returnType != void.class || setter.typeParameters.size() != 1)) {
            throw createError(new IllegalArgumentException(
                "Illegal set shortcut on field [" + value + "] for type [" + type + "]."));
        }

        if (getter != null && setter != null && setter.typeParameters.get(0) != getter.returnType) {
            throw createError(new IllegalArgumentException("Shortcut argument types must match."));
        }

        if ((getter != null || setter != null) && (input.read == false || getter != null) && (input.write == false || setter != null)) {
            output.actual = setter != null ? setter.typeParameters.get(0) : getter.returnType;
        } else {
            throw createError(new IllegalArgumentException("Illegal shortcut on field [" + value + "] for type [" + type + "]."));
        }

        DotSubShortcutNode dotSubShortcutNode = new DotSubShortcutNode();

        dotSubShortcutNode.setLocation(location);
        dotSubShortcutNode.setExpressionType(output.actual);
        dotSubShortcutNode.setGetter(getter);
        dotSubShortcutNode.setSetter(setter);

        output.expressionNode = dotSubShortcutNode;

        return output;
    }
}
