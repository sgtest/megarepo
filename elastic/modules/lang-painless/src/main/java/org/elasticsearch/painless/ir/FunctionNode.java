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

package org.elasticsearch.painless.ir;

import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.phase.IRTreeVisitor;

import java.util.ArrayList;
import java.util.List;

public class FunctionNode extends IRNode {

    /* ---- begin tree structure ---- */

    private BlockNode blockNode;

    public void setBlockNode(BlockNode blockNode) {
        this.blockNode = blockNode;
    }

    public BlockNode getBlockNode() {
        return blockNode;
    }

    /* ---- end tree structure, begin node data ---- */

    protected String name;
    Class<?> returnType;
    List<Class<?>> typeParameters = new ArrayList<>();
    List<String> parameterNames = new ArrayList<>();
    protected boolean isStatic;
    protected boolean hasVarArgs;
    protected boolean isSynthetic;
    protected int maxLoopCounter;

    public void setName(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }

    public void setReturnType(Class<?> returnType) {
        this.returnType = returnType;
    }

    public Class<?> getReturnType() {
        return returnType;
    }

    public void addTypeParameter(Class<?> typeParameter) {
        typeParameters.add(typeParameter);
    }

    public List<Class<?>> getTypeParameters() {
        return typeParameters;
    }

    public void addParameterName(String parameterName) {
        parameterNames.add(parameterName);
    }

    public List<String> getParameterNames() {
        return parameterNames;
    }

    public void setStatic(boolean isStatic) {
        this.isStatic = isStatic;
    }

    public boolean isStatic() {
        return isStatic;
    }

    public void setVarArgs(boolean hasVarArgs) {
        this.hasVarArgs = hasVarArgs;
    }

    public boolean hasVarArgs() {
        return hasVarArgs;
    }

    public void setSynthetic(boolean isSythetic) {
        this.isSynthetic = isSythetic;
    }

    public boolean isSynthetic() {
        return isSynthetic;
    }

    public void setMaxLoopCounter(int maxLoopCounter) {
        this.maxLoopCounter = maxLoopCounter;
    }

    public int getMaxLoopCounter() {
        return maxLoopCounter;
    }

    /* ---- end node data, begin visitor ---- */

    @Override
    public <Scope> void visit(IRTreeVisitor<Scope> irTreeVisitor, Scope scope) {
        irTreeVisitor.visitFunction(this, scope);
    }

    @Override
    public <Scope> void visitChildren(IRTreeVisitor<Scope> irTreeVisitor, Scope scope) {
        getBlockNode().visit(irTreeVisitor, scope);
    }

    /* ---- end visitor ---- */

    public FunctionNode(Location location) {
        super(location);
    }

}
