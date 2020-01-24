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

import org.elasticsearch.painless.AnalyzerCaster;
import org.elasticsearch.painless.Locals;
import org.elasticsearch.painless.Locals.Variable;
import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.ir.ForEachSubIterableNode;
import org.elasticsearch.painless.lookup.PainlessCast;
import org.elasticsearch.painless.lookup.PainlessLookupUtility;
import org.elasticsearch.painless.lookup.PainlessMethod;
import org.elasticsearch.painless.lookup.def;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.util.Iterator;
import java.util.Objects;
import java.util.Set;

import static org.elasticsearch.painless.lookup.PainlessLookupUtility.typeToCanonicalTypeName;

/**
 * Represents a for-each loop for iterables.
 */
final class SSubEachIterable extends AStatement {

    private AExpression expression;
    private final SBlock block;
    private final Variable variable;

    private PainlessCast cast = null;
    private Variable iterator = null;
    private PainlessMethod method = null;

    SSubEachIterable(Location location, Variable variable, AExpression expression, SBlock block) {
        super(location);

        this.variable = Objects.requireNonNull(variable);
        this.expression = Objects.requireNonNull(expression);
        this.block = block;
    }

    @Override
    void extractVariables(Set<String> variables) {
        throw createError(new IllegalStateException("Illegal tree structure."));
    }

    @Override
    void analyze(ScriptRoot scriptRoot, Locals locals) {
        // We must store the iterator as a variable for securing a slot on the stack, and
        // also add the location offset to make the name unique in case of nested for each loops.
        iterator = locals.addVariable(location, Iterator.class, "#itr" + location.getOffset(), true);

        if (expression.actual == def.class) {
            method = null;
        } else {
            method = scriptRoot.getPainlessLookup().lookupPainlessMethod(expression.actual, false, "iterator", 0);

            if (method == null) {
                    throw createError(new IllegalArgumentException(
                            "method [" + typeToCanonicalTypeName(expression.actual) + ", iterator/0] not found"));
            }
        }

        cast = AnalyzerCaster.getLegalCast(location, def.class, variable.clazz, true, true);
    }

    @Override
    ForEachSubIterableNode write() {
        ForEachSubIterableNode forEachSubIterableNode = new ForEachSubIterableNode();

        forEachSubIterableNode.setConditionNode(expression.write());
        forEachSubIterableNode.setBlockNode(block.write());

        forEachSubIterableNode.setLocation(location);
        forEachSubIterableNode.setVariable(variable);
        forEachSubIterableNode.setCast(cast);
        forEachSubIterableNode.setIterator(iterator);
        forEachSubIterableNode.setMethod(method);
        forEachSubIterableNode.setLoopCounter(loopCounter);
        forEachSubIterableNode.setContinuous(false);

        return forEachSubIterableNode;
    }

    @Override
    public String toString() {
        return singleLineToString(PainlessLookupUtility.typeToCanonicalTypeName(variable.clazz), variable.name, expression, block);
    }
}
