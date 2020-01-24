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
import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.ir.CatchNode;
import org.elasticsearch.painless.lookup.PainlessLookupUtility;
import org.elasticsearch.painless.symbol.ScriptRoot;
import org.objectweb.asm.Label;

import java.util.Objects;
import java.util.Set;

/**
 * Represents a catch block as part of a try-catch block.
 */
public final class SCatch extends AStatement {

    private final DType baseException;
    private final SDeclaration declaration;
    private final SBlock block;

    Label begin = null;
    Label end = null;
    Label exception = null;

    public SCatch(Location location, DType baseException, SDeclaration declaration, SBlock block) {
        super(location);

        this.baseException = Objects.requireNonNull(baseException);
        this.declaration = Objects.requireNonNull(declaration);
        this.block = block;
    }

    @Override
    void extractVariables(Set<String> variables) {
        declaration.extractVariables(variables);

        if (block != null) {
            block.extractVariables(variables);
        }
    }

    @Override
    void analyze(ScriptRoot scriptRoot, Locals locals) {
        declaration.analyze(scriptRoot, locals);

        Class<?> baseType = baseException.resolveType(scriptRoot.getPainlessLookup()).getType();
        Class<?> type = declaration.variable.clazz;

        if (baseType.isAssignableFrom(type) == false) {
            throw createError(new ClassCastException(
                    "cannot cast from [" + PainlessLookupUtility.typeToCanonicalTypeName(type) + "] " +
                    "to [" + PainlessLookupUtility.typeToCanonicalTypeName(baseType) + "]"));
        }

        if (block != null) {
            block.lastSource = lastSource;
            block.inLoop = inLoop;
            block.lastLoop = lastLoop;
            block.analyze(scriptRoot, locals);

            methodEscape = block.methodEscape;
            loopEscape = block.loopEscape;
            allEscape = block.allEscape;
            anyContinue = block.anyContinue;
            anyBreak = block.anyBreak;
            statementCount = block.statementCount;
        }
    }

    @Override
    CatchNode write() {
        CatchNode catchNode = new CatchNode();

        catchNode.setDeclarationNode(declaration.write());
        catchNode.setBlockNode(block == null ? null : block.write());

        catchNode.setLocation(location);

        return catchNode;
    }

    @Override
    public String toString() {
        return singleLineToString(baseException, declaration, block);
    }
}
