/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.analysis.analyzer;

import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.sql.analysis.index.EsIndex;
import org.elasticsearch.xpack.sql.analysis.index.IndexResolution;
import org.elasticsearch.xpack.sql.analysis.index.MappingException;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.Expressions;
import org.elasticsearch.xpack.sql.expression.FieldAttribute;
import org.elasticsearch.xpack.sql.expression.NamedExpression;
import org.elasticsearch.xpack.sql.expression.function.FunctionRegistry;
import org.elasticsearch.xpack.sql.parser.SqlParser;
import org.elasticsearch.xpack.sql.plan.logical.LogicalPlan;
import org.elasticsearch.xpack.sql.plan.logical.Project;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.type.EsField;
import org.elasticsearch.xpack.sql.type.TypesTests;

import java.util.List;
import java.util.Map;
import java.util.TimeZone;

import static org.elasticsearch.xpack.sql.type.DataType.BOOLEAN;
import static org.elasticsearch.xpack.sql.type.DataType.KEYWORD;
import static org.hamcrest.CoreMatchers.instanceOf;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.hasItems;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;

public class FieldAttributeTests extends ESTestCase {

    private SqlParser parser;
    private IndexResolution getIndexResult;
    private FunctionRegistry functionRegistry;
    private Analyzer analyzer;

    public FieldAttributeTests() {
        parser = new SqlParser();
        functionRegistry = new FunctionRegistry();

        Map<String, EsField> mapping = TypesTests.loadMapping("mapping-multi-field-variation.json");

        EsIndex test = new EsIndex("test", mapping);
        getIndexResult = IndexResolution.valid(test);
        analyzer = new Analyzer(functionRegistry, getIndexResult, TimeZone.getTimeZone("UTC"));
    }

    private LogicalPlan plan(String sql) {
        return analyzer.analyze(parser.createStatement(sql), true);
    }

    private FieldAttribute attribute(String fieldName) {
        // test multiple version of the attribute name
        // to make sure all match the same thing

        // NB: the equality is done on the same since each plan bumps the expression counter

        // unqualified
        FieldAttribute unqualified = parseQueryFor(fieldName);
        // unquoted qualifier
        FieldAttribute unquotedQualifier = parseQueryFor("test." + fieldName);
        assertEquals(unqualified.name(), unquotedQualifier.name());
        assertEquals(unqualified.qualifiedName(), unquotedQualifier.qualifiedName());
        // quoted qualifier
        FieldAttribute quotedQualifier = parseQueryFor("\"test\"." + fieldName);
        assertEquals(unqualified.name(), quotedQualifier.name());
        assertEquals(unqualified.qualifiedName(), quotedQualifier.qualifiedName());

        return randomFrom(unqualified, unquotedQualifier, quotedQualifier);
    }

    private FieldAttribute parseQueryFor(String fieldName) {
        LogicalPlan plan = plan("SELECT " + fieldName + " FROM test");
        assertThat(plan, instanceOf(Project.class));
        Project p = (Project) plan;
        List<? extends NamedExpression> projections = p.projections();
        assertThat(projections, hasSize(1));
        Attribute attribute = projections.get(0).toAttribute();
        assertThat(attribute, instanceOf(FieldAttribute.class));
        return (FieldAttribute) attribute;
    }

    private String error(String fieldName) {
        VerificationException ve = expectThrows(VerificationException.class, () -> plan("SELECT " + fieldName + " FROM test"));
        return ve.getMessage();
    }

    public void testRootField() {
        FieldAttribute attr = attribute("bool");
        assertThat(attr.name(), is("bool"));
        assertThat(attr.dataType(), is(BOOLEAN));
    }

    public void testDottedField() {
        FieldAttribute attr = attribute("some.dotted.field");
        assertThat(attr.path(), is("some.dotted"));
        assertThat(attr.name(), is("some.dotted.field"));
        assertThat(attr.dataType(), is(KEYWORD));
    }

    public void testExactKeyword() {
        FieldAttribute attr = attribute("some.string");
        assertThat(attr.path(), is("some"));
        assertThat(attr.name(), is("some.string"));
        assertThat(attr.dataType(), is(DataType.TEXT));
        assertThat(attr.isInexact(), is(true));
        FieldAttribute exact = attr.exactAttribute();
        assertThat(exact.isInexact(), is(false));
        assertThat(exact.name(), is("some.string.typical"));
        assertThat(exact.dataType(), is(KEYWORD));
    }

    public void testAmbiguousExactKeyword() {
        FieldAttribute attr = attribute("some.ambiguous");
        assertThat(attr.path(), is("some"));
        assertThat(attr.name(), is("some.ambiguous"));
        assertThat(attr.dataType(), is(DataType.TEXT));
        assertThat(attr.isInexact(), is(true));
        MappingException me = expectThrows(MappingException.class, () -> attr.exactAttribute());
        assertThat(me.getMessage(),
                is("Multiple exact keyword candidates available for [ambiguous]; specify which one to use"));
    }

    public void testNormalizedKeyword() {
        FieldAttribute attr = attribute("some.string.normalized");
        assertThat(attr.path(), is("some.string"));
        assertThat(attr.name(), is("some.string.normalized"));
        assertThat(attr.dataType(), is(KEYWORD));
        assertThat(attr.isInexact(), is(true));
    }

    public void testDottedFieldPath() {
        assertThat(error("some"), is("Found 1 problem(s)\nline 1:8: Cannot use field [some] type [object] only its subfields"));
    }

    public void testDottedFieldPathDeeper() {
        assertThat(error("some.dotted"),
                is("Found 1 problem(s)\nline 1:8: Cannot use field [some.dotted] type [object] only its subfields"));
    }

    public void testDottedFieldPathTypo() {
        assertThat(error("some.dotted.fild"),
                is("Found 1 problem(s)\nline 1:8: Unknown column [some.dotted.fild], did you mean [some.dotted.field]?"));
    }

    public void testStarExpansionExcludesObjectAndUnsupportedTypes() {
        LogicalPlan plan = plan("SELECT * FROM test");
        List<? extends NamedExpression> list = ((Project) plan).projections();
        assertThat(list, hasSize(8));
        List<String> names = Expressions.names(list);
        assertThat(names, not(hasItem("some")));
        assertThat(names, not(hasItem("some.dotted")));
        assertThat(names, not(hasItem("unsupported")));
        assertThat(names, hasItems("bool", "text", "keyword", "int"));
    }

    public void testFieldAmbiguity() {
        Map<String, EsField> mapping = TypesTests.loadMapping("mapping-dotted-field.json");

        EsIndex index = new EsIndex("test", mapping);
        getIndexResult = IndexResolution.valid(index);
        analyzer = new Analyzer(functionRegistry, getIndexResult, TimeZone.getTimeZone("UTC"));

        VerificationException ex = expectThrows(VerificationException.class, () -> plan("SELECT test.bar FROM test"));
        assertEquals(
                "Found 1 problem(s)\nline 1:8: Reference [test.bar] is ambiguous (to disambiguate use quotes or qualifiers); "
                        + "matches any of [\"test\".\"bar\", \"test\".\"test.bar\"]",
                ex.getMessage());

        ex = expectThrows(VerificationException.class, () -> plan("SELECT test.test FROM test"));
        assertEquals(
                "Found 1 problem(s)\nline 1:8: Reference [test.test] is ambiguous (to disambiguate use quotes or qualifiers); "
                        + "matches any of [\"test\".\"test\", \"test\".\"test.test\"]",
                ex.getMessage());

        LogicalPlan plan = plan("SELECT test.test FROM test AS x");
        assertThat(plan, instanceOf(Project.class));

        plan = plan("SELECT \"test\".test.test FROM test");
        assertThat(plan, instanceOf(Project.class));

        Project p = (Project) plan;
        List<? extends NamedExpression> projections = p.projections();
        assertThat(projections, hasSize(1));
        Attribute attribute = projections.get(0).toAttribute();
        assertThat(attribute, instanceOf(FieldAttribute.class));
        assertThat(attribute.qualifier(), is("test"));
        assertThat(attribute.name(), is("test.test"));
    }
}