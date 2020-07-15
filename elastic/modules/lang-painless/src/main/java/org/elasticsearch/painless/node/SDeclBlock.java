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
import org.elasticsearch.painless.phase.UserTreeVisitor;
import org.elasticsearch.painless.symbol.SemanticScope;

import java.util.Collections;
import java.util.List;

/**
 * Represents a series of declarations.
 */
public class SDeclBlock extends AStatement {

    private final List<SDeclaration> declarationNodes;

    public SDeclBlock(int identifier, Location location, List<SDeclaration> declarationNodes) {
        super(identifier, location);

        this.declarationNodes = Collections.unmodifiableList(declarationNodes);
    }

    public List<SDeclaration> getDeclarationNodes() {
        return declarationNodes;
    }

    @Override
    public <Input, Output> Output visit(UserTreeVisitor<Input, Output> userTreeVisitor, Input input) {
        return userTreeVisitor.visitDeclBlock(this, input);
    }

    @Override
    void analyze(SemanticScope semanticScope) {
        for (SDeclaration declaration : declarationNodes) {
            declaration.analyze(semanticScope);
        }
    }
}
