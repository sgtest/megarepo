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

import org.elasticsearch.painless.FunctionRef;
import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.Scope;
import org.elasticsearch.painless.ir.ClassNode;
import org.elasticsearch.painless.ir.FuncRefNode;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.util.Collections;
import java.util.List;
import java.util.Objects;

/**
 * Represents a function reference.
 */
public class EFunctionRef extends AExpression implements ILambda {

    protected final String type;
    protected final String call;

    // TODO: #54015
    private String defPointer;

    public EFunctionRef(Location location, String type, String call) {
        super(location);

        this.type = Objects.requireNonNull(type);
        this.call = Objects.requireNonNull(call);
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, Input input) {
        FunctionRef ref;

        Output output = new Output();

        if (input.expected == null) {
            ref = null;
            output.actual = String.class;
            defPointer = "S" + type + "." + call + ",0";
        } else {
            defPointer = null;
            ref = FunctionRef.create(
                    scriptRoot.getPainlessLookup(), scriptRoot.getFunctionTable(), location, input.expected, type, call, 0);
            output.actual = input.expected;
        }

        FuncRefNode funcRefNode = new FuncRefNode();

        funcRefNode.setLocation(location);
        funcRefNode.setExpressionType(output.actual);
        funcRefNode.setFuncRef(ref);

        output.expressionNode = funcRefNode;

        return output;
    }

    @Override
    public String getPointer() {
        return defPointer;
    }

    @Override
    public List<Class<?>> getCaptures() {
        return Collections.emptyList();
    }
}
