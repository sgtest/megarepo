/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.nulls;

import com.carrotsearch.randomizedtesting.annotations.Name;
import com.carrotsearch.randomizedtesting.annotations.ParametersFactory;

import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BooleanBlock;
import org.elasticsearch.xpack.esql.expression.function.TestCaseSupplier;
import org.elasticsearch.xpack.esql.expression.function.scalar.AbstractScalarFunctionTestCase;
import org.elasticsearch.xpack.esql.type.EsqlDataTypes;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.Literal;
import org.elasticsearch.xpack.ql.expression.predicate.nulls.IsNull;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;
import org.elasticsearch.xpack.ql.type.DataTypes;
import org.hamcrest.Matcher;

import java.util.List;
import java.util.function.Supplier;

import static org.hamcrest.Matchers.equalTo;

public class IsNullTests extends AbstractScalarFunctionTestCase {
    public IsNullTests(@Name("TestCase") Supplier<TestCaseSupplier.TestCase> testCaseSupplier) {
        this.testCase = testCaseSupplier.get();
    }

    @ParametersFactory
    public static Iterable<Object[]> parameters() {
        return parameterSuppliersFromTypedData(List.of(new TestCaseSupplier("Keyword is Null", () -> {
            return new TestCaseSupplier.TestCase(
                List.of(new TestCaseSupplier.TypedData(new BytesRef("cat"), DataTypes.KEYWORD, "exp")),
                "IsNullEvaluator[field=Attribute[channel=0]]",
                DataTypes.BOOLEAN,
                equalTo(false)
            );
        })));
    }

    @Override
    protected DataType expectedType(List<DataType> argTypes) {
        return DataTypes.BOOLEAN;
    }

    @Override
    protected void assertSimpleWithNulls(List<Object> data, Block value, int nullBlock) {
        assertTrue(((BooleanBlock) value).asVector().getBoolean(0));
    }

    @Override
    protected List<ArgumentSpec> argSpec() {
        return List.of(required(EsqlDataTypes.types().toArray(DataType[]::new)));
    }

    @Override
    protected Expression build(Source source, List<Expression> args) {
        return new IsNull(Source.EMPTY, args.get(0));
    }

    public void testAllTypes() {
        for (DataType type : EsqlDataTypes.types()) {
            if (DataTypes.isPrimitive(type) == false) {
                continue;
            }
            Literal lit = randomLiteral(EsqlDataTypes.widenSmallNumericTypes(type));
            assertThat(new IsNull(Source.EMPTY, lit).fold(), equalTo(lit.value() == null));
            assertThat(new IsNull(Source.EMPTY, new Literal(Source.EMPTY, null, type)).fold(), equalTo(true));
        }
    }

    @Override
    protected Matcher<Object> allNullsMatcher() {
        return equalTo(true);
    }
}
