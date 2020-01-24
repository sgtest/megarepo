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
import org.elasticsearch.painless.Locals.Parameter;
import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.ir.FunctionNode;
import org.elasticsearch.painless.ir.StatementNode;
import org.elasticsearch.painless.lookup.PainlessLookup;
import org.elasticsearch.painless.lookup.PainlessLookupUtility;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.lang.invoke.MethodType;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Objects;
import java.util.Set;

import static java.util.Collections.emptyList;

/**
 * Represents a user-defined function.
 */
public final class SFunction extends AStatement {

    private final String rtnTypeStr;
    public final String name;
    private final List<String> paramTypeStrs;
    private final List<String> paramNameStrs;
    private final SBlock block;
    public final boolean synthetic;

    private int maxLoopCounter;

    Class<?> returnType;
    List<Class<?>> typeParameters;
    MethodType methodType;

    org.objectweb.asm.commons.Method method;
    List<Parameter> parameters = new ArrayList<>();

    public SFunction(Location location, String rtnType, String name,
                     List<String> paramTypes, List<String> paramNames, SBlock block,
                     boolean synthetic) {
        super(location);

        this.rtnTypeStr = Objects.requireNonNull(rtnType);
        this.name = Objects.requireNonNull(name);
        this.paramTypeStrs = Collections.unmodifiableList(paramTypes);
        this.paramNameStrs = Collections.unmodifiableList(paramNames);
        this.block = Objects.requireNonNull(block);
        this.synthetic = synthetic;
    }

    @Override
    void extractVariables(Set<String> variables) {
        // we reset the list for function scope
        // note this is not stored for this node
        // but still required for lambdas
        block.extractVariables(new HashSet<>());
    }

    void generateSignature(PainlessLookup painlessLookup) {
        returnType = painlessLookup.canonicalTypeNameToType(rtnTypeStr);

        if (returnType == null) {
            throw createError(new IllegalArgumentException("Illegal return type [" + rtnTypeStr + "] for function [" + name + "]."));
        }

        if (paramTypeStrs.size() != paramNameStrs.size()) {
            throw createError(new IllegalStateException("Illegal tree structure."));
        }

        Class<?>[] paramClasses = new Class<?>[this.paramTypeStrs.size()];
        List<Class<?>> paramTypes = new ArrayList<>();

        for (int param = 0; param < this.paramTypeStrs.size(); ++param) {
            Class<?> paramType = painlessLookup.canonicalTypeNameToType(this.paramTypeStrs.get(param));

            if (paramType == null) {
                throw createError(new IllegalArgumentException(
                    "Illegal parameter type [" + this.paramTypeStrs.get(param) + "] for function [" + name + "]."));
            }

            paramClasses[param] = PainlessLookupUtility.typeToJavaType(paramType);
            paramTypes.add(paramType);
            parameters.add(new Parameter(location, paramNameStrs.get(param), paramType));
        }

        typeParameters = paramTypes;
        methodType = MethodType.methodType(PainlessLookupUtility.typeToJavaType(returnType), paramClasses);
        method = new org.objectweb.asm.commons.Method(name, MethodType.methodType(
                PainlessLookupUtility.typeToJavaType(returnType), paramClasses).toMethodDescriptorString());
    }

    @Override
    void analyze(ScriptRoot scriptRoot, Locals locals) {
        maxLoopCounter = scriptRoot.getCompilerSettings().getMaxLoopCounter();

        if (block.statements.isEmpty()) {
            throw createError(new IllegalArgumentException("Cannot generate an empty function [" + name + "]."));
        }

        locals = Locals.newLocalScope(locals);

        block.lastSource = true;
        block.analyze(scriptRoot, locals);
        methodEscape = block.methodEscape;

        if (!methodEscape && returnType != void.class) {
            throw createError(new IllegalArgumentException("Not all paths provide a return value for method [" + name + "]."));
        }

        if (maxLoopCounter > 0) {
            loopCounter = locals.getVariable(null, Locals.LOOP);
        }
    }

    @Override
    public StatementNode write() {
        throw new UnsupportedOperationException();
    }

    FunctionNode writeFunction() {
        FunctionNode functionNode = new FunctionNode();

        functionNode.setBlockNode(block.write());

        functionNode.setLocation(location);
        functionNode.setName(name);
        functionNode.setReturnType(returnType);
        functionNode.getTypeParameters().addAll(typeParameters);
        functionNode.setSynthetic(synthetic);
        functionNode.setMethodEscape(methodEscape);
        functionNode.setLoopCounter(loopCounter);
        functionNode.setMaxLoopCounter(maxLoopCounter);

        return functionNode;
    }

    @Override
    public String toString() {
        List<Object> description = new ArrayList<>();
        description.add(rtnTypeStr);
        description.add(name);
        if (false == (paramTypeStrs.isEmpty() && paramNameStrs.isEmpty())) {
            description.add(joinWithName("Args", pairwiseToString(paramTypeStrs, paramNameStrs), emptyList()));
        }
        return multilineToString(description, block.statements);
    }
}
