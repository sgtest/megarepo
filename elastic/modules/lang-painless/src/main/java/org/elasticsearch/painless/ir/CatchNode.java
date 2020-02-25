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
import org.elasticsearch.painless.symbol.ScopeTable.Variable;
import org.objectweb.asm.Label;
import org.objectweb.asm.Opcodes;

public class CatchNode extends StatementNode {

    /* ---- begin tree structure ---- */

    private DeclarationNode declarationNode;
    private BlockNode blockNode;

    public void setDeclarationNode(DeclarationNode declarationNode) {
        this.declarationNode = declarationNode;
    }

    public DeclarationNode getDeclarationNode() {
        return declarationNode;
    }

    public void setBlockNode(BlockNode blockNode) {
        this.blockNode = blockNode;
    }

    public BlockNode getBlockNode() {
        return blockNode;
    }

    /* ---- end tree structure ---- */

    Label begin = null;
    Label end = null;
    Label exception = null;

    @Override
    protected void write(ClassWriter classWriter, MethodWriter methodWriter, ScopeTable scopeTable) {
        methodWriter.writeStatementOffset(location);

        declarationNode.write(classWriter, methodWriter, scopeTable);
        Variable variable = scopeTable.getVariable(declarationNode.getName());

        Label jump = new Label();

        methodWriter.mark(jump);
        methodWriter.visitVarInsn(variable.getAsmType().getOpcode(Opcodes.ISTORE), variable.getSlot());

        if (blockNode != null) {
            blockNode.continueLabel = continueLabel;
            blockNode.breakLabel = breakLabel;
            blockNode.write(classWriter, methodWriter, scopeTable);
        }

        methodWriter.visitTryCatchBlock(begin, end, jump, variable.getAsmType().getInternalName());

        if (exception != null && (blockNode == null || blockNode.doAllEscape() == false)) {
            methodWriter.goTo(exception);
        }
    }
}
