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
import org.elasticsearch.painless.Scope.FunctionScope;
import org.elasticsearch.painless.ir.BlockNode;
import org.elasticsearch.painless.ir.ClassNode;
import org.elasticsearch.painless.ir.ConstantNode;
import org.elasticsearch.painless.ir.ExpressionNode;
import org.elasticsearch.painless.ir.FunctionNode;
import org.elasticsearch.painless.ir.NullNode;
import org.elasticsearch.painless.ir.ReturnNode;
import org.elasticsearch.painless.lookup.PainlessLookup;
import org.elasticsearch.painless.lookup.PainlessLookupUtility;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.lang.invoke.MethodType;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

import static java.util.Collections.emptyList;
import static org.elasticsearch.painless.Scope.newFunctionScope;

/**
 * Represents a user-defined function.
 */
public final class SFunction extends ANode {

    private final String rtnTypeStr;
    public final String name;
    private final List<String> paramTypeStrs;
    private final List<String> paramNameStrs;
    private final SBlock block;
    public final boolean isInternal;
    public final boolean isStatic;
    public final boolean synthetic;

    /**
     * If set to {@code true} default return values are inserted if
     * not all paths return a value.
     */
    public final boolean isAutoReturnEnabled;

    private int maxLoopCounter;

    Class<?> returnType;
    List<Class<?>> typeParameters;
    MethodType methodType;

    org.objectweb.asm.commons.Method method;

    private ScriptRoot scriptRoot;
    private boolean methodEscape;

    public SFunction(Location location, String rtnType, String name,
            List<String> paramTypes, List<String> paramNames,
            SBlock block, boolean isInternal, boolean isStatic, boolean synthetic, boolean isAutoReturnEnabled) {
        super(location);

        this.rtnTypeStr = Objects.requireNonNull(rtnType);
        this.name = Objects.requireNonNull(name);
        this.paramTypeStrs = Collections.unmodifiableList(paramTypes);
        this.paramNameStrs = Collections.unmodifiableList(paramNames);
        this.block = Objects.requireNonNull(block);
        this.isInternal = isInternal;
        this.synthetic = synthetic;
        this.isStatic = isStatic;
        this.isAutoReturnEnabled = isAutoReturnEnabled;
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
        }

        typeParameters = paramTypes;
        methodType = MethodType.methodType(PainlessLookupUtility.typeToJavaType(returnType), paramClasses);
        method = new org.objectweb.asm.commons.Method(name, MethodType.methodType(
                PainlessLookupUtility.typeToJavaType(returnType), paramClasses).toMethodDescriptorString());
    }

    void analyze(ScriptRoot scriptRoot) {
        this.scriptRoot = scriptRoot;
        FunctionScope functionScope = newFunctionScope(returnType);

        for (int index = 0; index < typeParameters.size(); ++index) {
            Class<?> typeParameter = typeParameters.get(index);
            String parameterName = paramNameStrs.get(index);
            functionScope.defineVariable(location, typeParameter, parameterName, false);
        }

        maxLoopCounter = scriptRoot.getCompilerSettings().getMaxLoopCounter();

        if (block.statements.isEmpty()) {
            throw createError(new IllegalArgumentException("Cannot generate an empty function [" + name + "]."));
        }

        block.lastSource = true;
        block.analyze(scriptRoot, functionScope.newLocalScope());
        methodEscape = block.methodEscape;

        if (methodEscape == false && isAutoReturnEnabled == false && returnType != void.class) {
            throw createError(new IllegalArgumentException("not all paths provide a return value " +
                    "for function [" + name + "] with [" + typeParameters.size() + "] parameters"));
        }

        // TODO: do not specialize for execute
        // TODO: https://github.com/elastic/elasticsearch/issues/51841
        if ("execute".equals(name)) {
            scriptRoot.setUsedVariables(functionScope.getUsedVariables());
        }
        // TODO: end
    }

    @Override
    public FunctionNode write(ClassNode classNode) {
        BlockNode blockNode = block.write(classNode);

        if (methodEscape == false) {
            ExpressionNode expressionNode;

            if (returnType == void.class) {
                expressionNode = null;
            } else if (isAutoReturnEnabled) {
                if (returnType.isPrimitive()) {
                    ConstantNode constantNode = new ConstantNode();
                    constantNode.setLocation(location);
                    constantNode.setExpressionType(returnType);

                    if (returnType == boolean.class) {
                        constantNode.setConstant(false);
                    } else if (returnType == byte.class
                            || returnType == char.class
                            || returnType == short.class
                            || returnType == int.class) {
                        constantNode.setConstant(0);
                    } else if (returnType == long.class) {
                        constantNode.setConstant(0L);
                    } else if (returnType == float.class) {
                        constantNode.setConstant(0f);
                    } else if (returnType == double.class) {
                        constantNode.setConstant(0d);
                    } else {
                        throw createError(new IllegalStateException("unexpected automatic return type " +
                                "[" + PainlessLookupUtility.typeToCanonicalTypeName(returnType) + "] " +
                                "for function [" + name + "] with [" + typeParameters.size() + "] parameters"));
                    }

                    expressionNode = constantNode;
                } else {
                    expressionNode = new NullNode();
                    expressionNode.setLocation(location);
                    expressionNode.setExpressionType(returnType);
                }
            } else {
                throw createError(new IllegalStateException("not all paths provide a return value " +
                        "for function [" + name + "] with [" + typeParameters.size() + "] parameters"));
            }

            ReturnNode returnNode = new ReturnNode();
            returnNode.setLocation(location);
            returnNode.setExpressionNode(expressionNode);

            blockNode.addStatementNode(returnNode);
        }

        FunctionNode functionNode = new FunctionNode();

        functionNode.setBlockNode(blockNode);

        functionNode.setLocation(location);
        functionNode.setScriptRoot(scriptRoot);
        functionNode.setName(name);
        functionNode.setReturnType(returnType);
        functionNode.getTypeParameters().addAll(typeParameters);
        functionNode.getParameterNames().addAll(paramNameStrs);
        functionNode.setStatic(isStatic);
        functionNode.setSynthetic(synthetic);
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
