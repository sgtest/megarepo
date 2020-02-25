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

import org.elasticsearch.painless.ClassWriter;
import org.elasticsearch.painless.MethodWriter;
import org.elasticsearch.painless.symbol.ScopeTable;
import org.objectweb.asm.Label;
import org.objectweb.asm.Opcodes;

public class IfElseNode extends ConditionNode {

    /* ---- begin tree structure ---- */

    private BlockNode elseBlockNode;

    public void setElseBlockNode(BlockNode elseBlockNode) {
        this.elseBlockNode = elseBlockNode;
    }

    public BlockNode getElseBlockNode() {
        return elseBlockNode;
    }

    /* ---- end tree structure ---- */

    @Override
    protected void write(ClassWriter classWriter, MethodWriter methodWriter, ScopeTable scopeTable) {
        methodWriter.writeStatementOffset(location);

        Label fals = new Label();
        Label end = new Label();

        getConditionNode().write(classWriter, methodWriter, scopeTable);
        methodWriter.ifZCmp(Opcodes.IFEQ, fals);

        getBlockNode().continueLabel = continueLabel;
        getBlockNode().breakLabel = breakLabel;
        getBlockNode().write(classWriter, methodWriter, scopeTable.newScope());

        if (getBlockNode().doAllEscape() == false) {
            methodWriter.goTo(end);
        }

        methodWriter.mark(fals);

        elseBlockNode.continueLabel = continueLabel;
        elseBlockNode.breakLabel = breakLabel;
        elseBlockNode.write(classWriter, methodWriter, scopeTable.newScope());

        methodWriter.mark(end);
    }
}
