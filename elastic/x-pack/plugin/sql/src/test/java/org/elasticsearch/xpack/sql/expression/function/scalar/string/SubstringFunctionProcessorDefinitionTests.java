/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.sql.expression.function.scalar.string;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.FunctionTestUtils.Combinations;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinition;
import org.elasticsearch.xpack.sql.tree.AbstractNodeTestCase;
import org.elasticsearch.xpack.sql.tree.Location;

import java.util.ArrayList;
import java.util.BitSet;
import java.util.List;
import java.util.Objects;
import java.util.function.Function;

import static org.elasticsearch.xpack.sql.expression.function.scalar.FunctionTestUtils.randomIntLiteral;
import static org.elasticsearch.xpack.sql.expression.function.scalar.FunctionTestUtils.randomStringLiteral;
import static org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinitions.toProcessorDefinition;
import static org.elasticsearch.xpack.sql.tree.LocationTests.randomLocation;

public class SubstringFunctionProcessorDefinitionTests 
    extends AbstractNodeTestCase<SubstringFunctionProcessorDefinition, ProcessorDefinition> {

    @Override
    protected SubstringFunctionProcessorDefinition randomInstance() {
        return randomSubstringFunctionProcessorDefinition();
    }
    
    private Expression randomSubstringFunctionExpression() {        
        return randomSubstringFunctionProcessorDefinition().expression();
    }
    
    public static SubstringFunctionProcessorDefinition randomSubstringFunctionProcessorDefinition() {
        return (SubstringFunctionProcessorDefinition) (new Substring(randomLocation(), 
                            randomStringLiteral(), 
                            randomIntLiteral(),
                            randomIntLiteral())
                .makeProcessorDefinition());
    }

    @Override
    public void testTransform() {
        // test transforming only the properties (location, expression), 
        // skipping the children (the two parameters of the binary function) which are tested separately
        SubstringFunctionProcessorDefinition b1 = randomInstance();
        Expression newExpression = randomValueOtherThan(b1.expression(), () -> randomSubstringFunctionExpression());
        SubstringFunctionProcessorDefinition newB = new SubstringFunctionProcessorDefinition(
                b1.location(), 
                newExpression,
                b1.source(), 
                b1.start(),
                b1.length());
        assertEquals(newB, b1.transformPropertiesOnly(v -> Objects.equals(v, b1.expression()) ? newExpression : v, Expression.class));
        
        SubstringFunctionProcessorDefinition b2 = randomInstance();
        Location newLoc = randomValueOtherThan(b2.location(), () -> randomLocation());
        newB = new SubstringFunctionProcessorDefinition(
                newLoc, 
                b2.expression(),
                b2.source(), 
                b2.start(),
                b2.length());
        assertEquals(newB, 
                b2.transformPropertiesOnly(v -> Objects.equals(v, b2.location()) ? newLoc : v, Location.class));
    }

    @Override
    public void testReplaceChildren() {
        SubstringFunctionProcessorDefinition b = randomInstance();
        ProcessorDefinition newSource = toProcessorDefinition((Expression) randomValueOtherThan(b.source(), () -> randomStringLiteral()));
        ProcessorDefinition newStart = toProcessorDefinition((Expression) randomValueOtherThan(b.start(), () -> randomIntLiteral()));
        ProcessorDefinition newLength = toProcessorDefinition((Expression) randomValueOtherThan(b.length(), () -> randomIntLiteral()));
        SubstringFunctionProcessorDefinition newB = 
                new SubstringFunctionProcessorDefinition(b.location(), b.expression(), b.source(), b.start(), b.length());
        SubstringFunctionProcessorDefinition transformed = null;
        
        // generate all the combinations of possible children modifications and test all of them
        for(int i = 1; i < 4; i++) {
            for(BitSet comb : new Combinations(3, i)) {
                transformed = (SubstringFunctionProcessorDefinition) newB.replaceChildren(
                        comb.get(0) ? newSource : b.source(),
                        comb.get(1) ? newStart : b.start(),
                        comb.get(2) ? newLength : b.length());
                assertEquals(transformed.source(), comb.get(0) ? newSource : b.source());
                assertEquals(transformed.start(), comb.get(1) ? newStart : b.start());
                assertEquals(transformed.length(), comb.get(2) ? newLength : b.length());
                assertEquals(transformed.expression(), b.expression());
                assertEquals(transformed.location(), b.location());
            }
        }
    }

    @Override
    protected SubstringFunctionProcessorDefinition mutate(SubstringFunctionProcessorDefinition instance) {
        List<Function<SubstringFunctionProcessorDefinition, SubstringFunctionProcessorDefinition>> randoms = new ArrayList<>();
        
        for(int i = 1; i < 4; i++) {
            for(BitSet comb : new Combinations(3, i)) {
                randoms.add(f -> new SubstringFunctionProcessorDefinition(
                        f.location(),
                        f.expression(), 
                        comb.get(0) ? toProcessorDefinition((Expression) randomValueOtherThan(f.source(),
                                () -> randomStringLiteral())) : f.source(),
                        comb.get(1) ? toProcessorDefinition((Expression) randomValueOtherThan(f.start(),
                                () -> randomIntLiteral())) : f.start(),
                        comb.get(2) ? toProcessorDefinition((Expression) randomValueOtherThan(f.length(),
                                () -> randomIntLiteral())): f.length()));
            }
        }
        
        return randomFrom(randoms).apply(instance);
    }

    @Override
    protected SubstringFunctionProcessorDefinition copy(SubstringFunctionProcessorDefinition instance) {
        return new SubstringFunctionProcessorDefinition(instance.location(),
                instance.expression(),
                instance.source(),
                instance.start(),
                instance.length());
    }
}
