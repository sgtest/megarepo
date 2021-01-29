/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */


package org.elasticsearch.xpack.eql.parser;

import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.Literal;
import org.elasticsearch.xpack.ql.expression.UnresolvedAttribute;
import org.elasticsearch.xpack.ql.expression.function.FunctionResolutionStrategy;
import org.elasticsearch.xpack.ql.expression.function.UnresolvedFunction;
import org.elasticsearch.xpack.ql.expression.predicate.logical.And;
import org.elasticsearch.xpack.ql.expression.predicate.logical.Not;
import org.elasticsearch.xpack.ql.expression.predicate.logical.Or;
import org.elasticsearch.xpack.ql.expression.predicate.operator.arithmetic.Mul;
import org.elasticsearch.xpack.ql.expression.predicate.operator.arithmetic.Neg;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.Equals;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.GreaterThan;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.GreaterThanOrEqual;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.In;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.LessThan;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.LessThanOrEqual;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.NotEquals;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataTypes;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static org.elasticsearch.xpack.eql.parser.AbstractBuilder.unquoteString;
import static org.elasticsearch.xpack.ql.TestUtils.UTC;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.Matchers.startsWith;

public class ExpressionTests extends ESTestCase {

    private final EqlParser parser = new EqlParser();

    private static Source source(String text) {
        return new Source(0, 0, text);
    }

    private Expression expr(String source) {
        return parser.createExpression(source);
    }

    private List<Expression> exprs(String... sources) {
        List<Expression> results = new ArrayList<>(sources.length);
        for (String s : sources) {
            results.add(expr(s));
        }
        return results;
    }

    public void testStrings() {
        assertEquals("hello\"world", unquoteString(source("\"hello\"world\"")));
        assertEquals("hello'world", unquoteString(source("\"hello'world\"")));
        assertEquals("hello\nworld", unquoteString(source("\"hello\\nworld\"")));
        assertEquals("hello\\\nworld", unquoteString(source("\"hello\\\\\\nworld\"")));
        assertEquals("hello\\\"world", unquoteString(source("\"hello\\\\\\\"world\"")));

        // test for unescaped strings: """...."""
        assertEquals("hello\"world", unquoteString(source("\"\"\"hello\"world\"\"\"")));
        assertEquals("hello\\\"world", unquoteString(source("\"\"\"hello\\\"world\"\"\"")));
        assertEquals("\"\"hello\"\\\"world\"\"\"", unquoteString(source("\"\"\"\"\"hello\"\\\"world\"\"\"\"\"\"")));
        assertEquals("hello'world", unquoteString(source("\"\"\"hello'world\"\"\"")));
        assertEquals("hello'world", unquoteString(source("\"\"\"hello\'world\"\"\"")));
        assertEquals("hello\\'world", unquoteString(source("\"\"\"hello\\\'world\"\"\"")));
        assertEquals("hello\\nworld", unquoteString(source("\"\"\"hello\\nworld\"\"\"")));
        assertEquals("hello\\\\nworld", unquoteString(source("\"\"\"hello\\\\nworld\"\"\"")));
        assertEquals("hello\\\\\\nworld", unquoteString(source("\"\"\"hello\\\\\\nworld\"\"\"")));
        assertEquals("hello\\\\\\\"world", unquoteString(source("\"\"\"hello\\\\\\\"world\"\"\"")));
        assertEquals("\"\\\"", unquoteString(source("\"\"\"\"\\\"\"\"\"")));
        assertEquals("\\\"\"\"", unquoteString(source("\"\"\"\\\"\"\"\"\"\"")));
        assertEquals("\"\\\"\"", unquoteString(source("\"\"\"\"\\\"\"\"\"\"")));
        assertEquals("\"\"\\\"", unquoteString(source("\"\"\"\"\"\\\"\"\"\"")));
        assertEquals("\"\"", unquoteString(source("\"\"\"\"\"\"\"\"")));
        assertEquals("\"\" \"\"", unquoteString(source("\"\"\"\"\" \"\"\"\"\"")));
        assertEquals("", unquoteString(source("\"\"\"\"\"\"")));
    }

    public void testLiterals() {
        assertEquals(Literal.TRUE, expr("true"));
        assertEquals(Literal.FALSE, expr("false"));
        assertEquals(Literal.NULL, expr("null"));
    }

    public void testSingleQuotedStringForbidden() {
        ParsingException e = expectThrows(ParsingException.class, () -> expr("'hello world'"));
        assertEquals("line 1:2: Use double quotes [\"] to define string literals, not single quotes [']",
                e.getMessage());
        e = expectThrows(ParsingException.class, () -> parser.createStatement("process where name=='hello world'"));
        assertEquals("line 1:22: Use double quotes [\"] to define string literals, not single quotes [']",
                e.getMessage());
    }

    public void testDoubleQuotedString() {
        // "hello \" world"
        Expression parsed = expr("\"hello \\\" world!\"");
        Expression expected = new Literal(null, "hello \" world!", DataTypes.KEYWORD);
        assertEquals(expected, parsed);
    }

    public void testSingleQuotedUnescapedStringDisallowed() {
        ParsingException e = expectThrows(ParsingException.class, () -> expr("?'hello world'"));
        assertEquals("line 1:2: Use triple double quotes [\"\"\"] to define unescaped string literals, not [?']",
                e.getMessage());
        e = expectThrows(ParsingException.class, () -> parser.createStatement("process where name == ?'hello world'"));
        assertEquals("line 1:24: Use triple double quotes [\"\"\"] to define unescaped string literals, not [?']",
                e.getMessage());
    }

    public void testDoubleQuotedUnescapedStringForbidden() {
        ParsingException e = expectThrows(ParsingException.class, () -> expr("?\"hello world\""));
        assertEquals("line 1:2: Use triple double quotes [\"\"\"] to define unescaped string literals, not [?\"]",
                e.getMessage());
        e = expectThrows(ParsingException.class, () -> parser.createStatement("process where name == ?\"hello world\""));
        assertEquals("line 1:24: Use triple double quotes [\"\"\"] to define unescaped string literals, not [?\"]",
                e.getMessage());
    }

    public void testTripleDoubleQuotedUnescapedString() {
        // """hello world!"""" == """foobar""" => hello world! = foobar
        String str = "\"\"\"hello world!\"\"\" == \"\"\"foobar\"\"\"";
        String expectedStrLeft = "hello world!";
        String expectedStrRight = "foobar";
        Expression parsed = expr(str);
        assertEquals(Equals.class, parsed.getClass());
        Equals eq = (Equals) parsed;
        assertEquals(Literal.class, eq.left().getClass());
        assertEquals(expectedStrLeft, ((Literal) eq.left()).value());
        assertEquals(Literal.class, eq.right().getClass());
        assertEquals(expectedStrRight, ((Literal) eq.right()).value());

        // """""hello""world!"""" == """"foo"bar""""" => ""hello""world!" = "foo""bar""
        str = " \"\"\"\"\"hello\"\"world!\"\"\"\" == \"\"\"\"foo\"bar\"\"\"\"\" ";
        expectedStrLeft = "\"\"hello\"\"world!\"";
        expectedStrRight = "\"foo\"bar\"\"";
        parsed = expr(str);
        assertEquals(Equals.class, parsed.getClass());
        eq = (Equals) parsed;
        assertEquals(Literal.class, eq.left().getClass());
        assertEquals(expectedStrLeft, ((Literal) eq.left()).value());
        assertEquals(Literal.class, eq.right().getClass());
        assertEquals(expectedStrRight, ((Literal) eq.right()).value());

        // """""\""hello\\""\""world!\\""""" == """\\""\""foo""\\""\"bar""\\""\"""" =>
        // ""\""hello\\""\""world!\\"" == \\""\""foo""\\""\"bar""\\""\"
        str = " \"\"\"\"\"\\\"\"hello\\\\\"\"\\\"\"world!\\\\\"\"\"\"\"  ==  " +
                " \"\"\"\\\\\"\"\\\"\"foo\"\"\\\\\"\"\\\"bar\"\"\\\\\"\"\\\"\"\"\"  ";
        expectedStrLeft = "\"\"\\\"\"hello\\\\\"\"\\\"\"world!\\\\\"\"";
        expectedStrRight = "\\\\\"\"\\\"\"foo\"\"\\\\\"\"\\\"bar\"\"\\\\\"\"\\\"";
        parsed = expr(str);
        assertEquals(Equals.class, parsed.getClass());
        eq = (Equals) parsed;
        assertEquals(Literal.class, eq.left().getClass());
        assertEquals(expectedStrLeft, ((Literal) eq.left()).value());
        assertEquals(Literal.class, eq.right().getClass());
        assertEquals(expectedStrRight, ((Literal) eq.right()).value());

        // """"""hello world!""" == """foobar"""
        ParsingException e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("\"\"\"\"\"\"hello world!\"\"\" == \"\"\"foobar\"\"\""));
        assertThat(e.getMessage(), startsWith("line 1:7: mismatched input 'hello' expecting {<EOF>,"));

        // """""\"hello world!"""""" == """foobar"""
        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("\"\"\"\"\"\\\"hello world!\"\"\"\"\"\" == \"\"\"foobar\"\"\""));
        assertThat(e.getMessage(), startsWith("line 1:25: mismatched input '\" == \"' expecting {<EOF>,"));

        // """""\"hello world!""\"""" == """"""foobar"""
        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("\"\"\"\"\"\\\"hello world!\"\"\\\"\"\"\" == \"\"\"\"\"\"foobar\"\"\""));
        assertThat(e.getMessage(), startsWith("line 1:37: mismatched input 'foobar' expecting {<EOF>,"));

        // """""\"hello world!""\"""" == """""\"foobar\"\""""""
        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("\"\"\"\"\"\\\"hello world!\"\"\\\"\"\"\" == \"\"\"\"\"\\\"foobar\\\"\\\"\"\"\"\"\""));
        assertEquals("line 1:52: token recognition error at: '\"'", e.getMessage());
    }

    public void testNumbers() {
        assertEquals(new Literal(null, 8589934592L, DataTypes.LONG), expr("8589934592"));
        assertEquals(new Literal(null, 5, DataTypes.INTEGER), expr("5"));
        assertEquals(new Literal(null, 5e14, DataTypes.DOUBLE), expr("5e14"));
        assertEquals(new Literal(null, 5.2, DataTypes.DOUBLE), expr("5.2"));

        Expression parsed = expr("-5.2");
        Expression expected = new Neg(null, new Literal(null, 5.2, DataTypes.DOUBLE));
        assertEquals(expected, parsed);
    }

    public void testBackQuotedIdentifier() {
        String quote = "`";
        String qualifier = "table";
        String name = "@timestamp";
        Expression exp = expr(quote + qualifier + quote + "." + quote + name + quote);
        assertThat(exp, instanceOf(UnresolvedAttribute.class));
        UnresolvedAttribute ua = (UnresolvedAttribute) exp;
        assertThat(ua.name(), equalTo(qualifier + "." + name));
        assertThat(ua.qualifiedName(), equalTo(qualifier + "." + name));
        assertThat(ua.qualifier(), is(nullValue()));
    }

    public void testBackQuotedIdentifierWithEscapedBackQuote() {
        String quote = "`";
        String qualifier = "``test``table``";
        String expectedQualifier = "`test`table`";
        String name = "@timestamp";
        Expression exp = expr(quote + qualifier + quote + "." + quote + name + quote);
        assertThat(exp, instanceOf(UnresolvedAttribute.class));
        UnresolvedAttribute ua = (UnresolvedAttribute) exp;
        assertThat(ua.name(), equalTo(expectedQualifier + "." + name));
        assertThat(ua.qualifiedName(), equalTo(expectedQualifier + "." + name));
        assertThat(ua.qualifier(), is(nullValue()));

        quote = "`";
        qualifier = "``test_table";
        expectedQualifier = "`test_table";
        name = "@timestamp";
        exp = expr(quote + qualifier + quote + "." + quote + name + quote);
        assertThat(exp, instanceOf(UnresolvedAttribute.class));
        ua = (UnresolvedAttribute) exp;
        assertThat(ua.name(), equalTo(expectedQualifier + "." + name));
        assertThat(ua.qualifiedName(), equalTo(expectedQualifier + "." + name));
        assertThat(ua.qualifier(), is(nullValue()));
    }

    public void testBackQuotedIdentifierWithUnescapedBackQuotes() {
        ParsingException e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("`wrong_identifier == true"));
        assertEquals("line 1:1: token recognition error at: '`wrong_identifier == true'", e.getMessage());

        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("``wrong_identifier == true"));
        assertThat(e.getMessage(), startsWith("line 1:3: mismatched input 'wrong_identifier' expecting {<EOF>, "));

        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("``wrong_identifier` == true"));
        assertEquals("line 1:19: token recognition error at: '` == true'", e.getMessage());

        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("`wrong`identifier` == true"));
        assertEquals("line 1:18: token recognition error at: '` == true'", e.getMessage());

        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("wrong_identifier` == true"));
        assertEquals("line 1:17: token recognition error at: '` == true'", e.getMessage());

        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("wrong_identifier`` == true"));
        assertThat(e.getMessage(), startsWith("line 1:17: mismatched input '``' expecting {<EOF>,"));

        e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr("`wrong_identifier`` == true"));
        assertEquals("line 1:19: token recognition error at: '` == true'", e.getMessage());
    }

    public void testIdentifierForEventTypeDisallowed() {
        ParsingException e = expectThrows(ParsingException.class, "Expected syntax error",
                () -> parser.createStatement("`identifier` where foo == true"));
        assertEquals("line 1:1: no viable alternative at input '`identifier`'", e.getMessage());
    }

    public void testFunctions() {
        List<Expression> arguments = Arrays.asList(
            new UnresolvedAttribute(null, "some.field"),
            new Literal(null, "test string", DataTypes.KEYWORD)
        );
        FunctionResolutionStrategy resolutionStrategy = FunctionResolutionStrategy.DEFAULT;
        Expression expected = new UnresolvedFunction(null, "concat", resolutionStrategy, arguments);

        assertEquals(expected, expr("concat(some.field, \"test string\")"));
    }

    public void testComparison() {
        String fieldText = "field";
        String valueText = "2.0";

        Expression field = expr(fieldText);
        Expression value = expr(valueText);

        assertEquals(new Equals(null, field, value, UTC), expr(fieldText + "==" + valueText));
        assertEquals(new NotEquals(null, field, value, UTC), expr(fieldText + "!=" + valueText));
        assertEquals(new LessThanOrEqual(null, field, value, UTC), expr(fieldText + "<=" + valueText));
        assertEquals(new GreaterThanOrEqual(null, field, value, UTC), expr(fieldText + ">=" + valueText));
        assertEquals(new GreaterThan(null, field, value, UTC), expr(fieldText + ">" + valueText));
        assertEquals(new LessThan(null, field, value, UTC), expr(fieldText + "<" + valueText));

        expectThrows(ParsingException.class, "Expected syntax error",
                () -> expr(fieldText + "=" + valueText));
    }

    public void testBoolean() {
        String leftText = "process_name == \"net.exe\"";
        String rightText = "command_line == \"* localgroup*\"";

        Expression lhs = expr(leftText);
        Expression rhs = expr(rightText);

        Expression booleanAnd = expr(leftText + " and " + rightText);
        assertEquals(new And(null, lhs, rhs), booleanAnd);

        Expression booleanOr = expr(leftText + " or " + rightText);
        assertEquals(new Or(null, lhs, rhs), booleanOr);
    }

    public void testInSet() {
        assertEquals(
            expr("name in (1)"),
            new In(null, expr("name"), exprs("1"))
        );

        assertEquals(
            expr("name in (2, 1)"),
            new In(null, expr("name"), exprs("2", "1"))
        );
        assertEquals(
            expr("name in (\"net.exe\")"),
            new In(null, expr("name"), exprs("\"net.exe\""))
        );

        assertEquals(
            expr("name in (\"net.exe\", \"whoami.exe\", \"hostname.exe\")"),
            new In(null, expr("name"), exprs("\"net.exe\"", "\"whoami.exe\"", "\"hostname.exe\""))
        );
    }

    public void testInSetDuplicates() {
        assertEquals(
            expr("name in (1, 1)"),
            new In(null, expr("name"), exprs("1", "1"))
        );

        assertEquals(
            expr("name in (\"net.exe\", \"net.exe\")"),
            new In(null, expr("name"), exprs("\"net.exe\"", "\"net.exe\""))
        );
    }

    public void testNotInSet() {
        assertEquals(
            expr("name not in (\"net.exe\", \"whoami.exe\", \"hostname.exe\")"),
            new Not(null, new In(null,
                expr("name"),
                exprs("\"net.exe\"", "\"whoami.exe\"", "\"hostname.exe\"")))
        );
    }

    public void testInEmptySet() {
        expectThrows(ParsingException.class, "Expected syntax error",
            () -> expr("name in ()"));
    }

    public void testComplexComparison() {
        String comparison;
        if (randomBoolean()) {
            comparison = "1 * -2 <= -3 * 4";
        } else {
            comparison = "(1 * -2) <= (-3 * 4)";
        }

        Mul left = new Mul(null,
                new Literal(null, 1, DataTypes.INTEGER),
                new Neg(null, new Literal(null, 2, DataTypes.INTEGER)));
        Mul right = new Mul(null,
                new Neg(null, new Literal(null, 3, DataTypes.INTEGER)),
                new Literal(null, 4, DataTypes.INTEGER));

        assertEquals(new LessThanOrEqual(null, left, right, UTC), expr(comparison));
    }

    public void testChainedComparisonsDisallowed() {
        int noComparisions = randomIntBetween(2, 20);
        String firstComparator = "";
        String secondComparator = "";
        StringBuilder sb = new StringBuilder("a ");
        for (int i = 0 ; i < noComparisions; i++) {
            String comparator = randomFrom("==", "!=", "<", "<=", ">", ">=");
            sb.append(comparator).append(" a ");

            if (i == 0) {
                firstComparator = comparator;
            } else if (i == 1) {
                secondComparator = comparator;
            }
        }
        ParsingException e = expectThrows(ParsingException.class, () -> expr(sb.toString()));
        assertEquals("line 1:" + (6 + firstComparator.length()) + ": mismatched input '" + secondComparator +
                "' expecting {<EOF>, 'and', 'in', 'in~', 'not', 'or', ':', '+', '-', '*', '/', '%', '.', '['}",
            e.getMessage());
    }
}
