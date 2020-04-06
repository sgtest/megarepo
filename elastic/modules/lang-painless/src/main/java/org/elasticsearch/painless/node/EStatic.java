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
import org.elasticsearch.painless.ir.StaticNode;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.util.Objects;

/**
 * Represents a static type target.
 */
public class EStatic extends AExpression {

    protected final String type;

    public EStatic(Location location, String type) {
        super(location);

        this.type = Objects.requireNonNull(type);
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, Input input) {
        if (input.read == false) {
            throw createError(new IllegalArgumentException("not a statement: static type [" + type + "] not used"));
        }

        Output output = new Output();
        output.actual = scriptRoot.getPainlessLookup().canonicalTypeNameToType(type);

        if (output.actual == null) {
            throw createError(new IllegalArgumentException("Not a type [" + type + "]."));
        }

        StaticNode staticNode = new StaticNode();

        staticNode.setLocation(location);
        staticNode.setExpressionType(output.actual);

        output.expressionNode = staticNode;

        return output;
    }
}
