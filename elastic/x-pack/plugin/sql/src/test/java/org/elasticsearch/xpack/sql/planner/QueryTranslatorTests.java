/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.sql.planner;

import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.time.DateFormatter;
import org.elasticsearch.index.query.ExistsQueryBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.filter.FilterAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.AbstractPercentilesAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.AvgAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.CardinalityAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.PercentileRanksAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.PercentilesAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.PercentilesConfig;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.ql.QlIllegalArgumentException;
import org.elasticsearch.xpack.ql.execution.search.FieldExtraction;
import org.elasticsearch.xpack.ql.expression.Alias;
import org.elasticsearch.xpack.ql.expression.Attribute;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.FieldAttribute;
import org.elasticsearch.xpack.ql.expression.Literal;
import org.elasticsearch.xpack.ql.expression.ReferenceAttribute;
import org.elasticsearch.xpack.ql.expression.function.FunctionDefinition;
import org.elasticsearch.xpack.ql.expression.function.aggregate.AggregateFunction;
import org.elasticsearch.xpack.ql.expression.function.aggregate.Count;
import org.elasticsearch.xpack.ql.expression.gen.script.ScriptTemplate;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.Equals;
import org.elasticsearch.xpack.ql.expression.predicate.operator.comparison.GreaterThan;
import org.elasticsearch.xpack.ql.index.EsIndex;
import org.elasticsearch.xpack.ql.index.IndexResolution;
import org.elasticsearch.xpack.ql.plan.logical.Aggregate;
import org.elasticsearch.xpack.ql.plan.logical.Filter;
import org.elasticsearch.xpack.ql.plan.logical.LogicalPlan;
import org.elasticsearch.xpack.ql.plan.logical.Project;
import org.elasticsearch.xpack.ql.querydsl.query.BoolQuery;
import org.elasticsearch.xpack.ql.querydsl.query.ExistsQuery;
import org.elasticsearch.xpack.ql.querydsl.query.GeoDistanceQuery;
import org.elasticsearch.xpack.ql.querydsl.query.NotQuery;
import org.elasticsearch.xpack.ql.querydsl.query.PrefixQuery;
import org.elasticsearch.xpack.ql.querydsl.query.Query;
import org.elasticsearch.xpack.ql.querydsl.query.RangeQuery;
import org.elasticsearch.xpack.ql.querydsl.query.RegexQuery;
import org.elasticsearch.xpack.ql.querydsl.query.ScriptQuery;
import org.elasticsearch.xpack.ql.querydsl.query.TermQuery;
import org.elasticsearch.xpack.ql.querydsl.query.TermsQuery;
import org.elasticsearch.xpack.ql.querydsl.query.WildcardQuery;
import org.elasticsearch.xpack.ql.type.EsField;
import org.elasticsearch.xpack.sql.analysis.analyzer.Analyzer;
import org.elasticsearch.xpack.sql.analysis.analyzer.Verifier;
import org.elasticsearch.xpack.sql.expression.function.SqlFunctionRegistry;
import org.elasticsearch.xpack.sql.expression.function.aggregate.ExtendedStatsEnclosed;
import org.elasticsearch.xpack.sql.expression.function.aggregate.MatrixStatsEnclosed;
import org.elasticsearch.xpack.sql.expression.function.aggregate.Percentile;
import org.elasticsearch.xpack.sql.expression.function.aggregate.PercentileRank;
import org.elasticsearch.xpack.sql.expression.function.aggregate.TopHits;
import org.elasticsearch.xpack.sql.expression.function.grouping.Histogram;
import org.elasticsearch.xpack.sql.expression.function.scalar.Cast;
import org.elasticsearch.xpack.sql.expression.function.scalar.datetime.DateTimeProcessor.DateTimeExtractor;
import org.elasticsearch.xpack.sql.expression.function.scalar.math.MathProcessor.MathOperation;
import org.elasticsearch.xpack.sql.expression.function.scalar.math.Round;
import org.elasticsearch.xpack.sql.expression.function.scalar.string.LTrim;
import org.elasticsearch.xpack.sql.expression.function.scalar.string.RTrim;
import org.elasticsearch.xpack.sql.expression.function.scalar.string.Trim;
import org.elasticsearch.xpack.sql.expression.function.scalar.string.UnaryStringFunction;
import org.elasticsearch.xpack.sql.optimizer.Optimizer;
import org.elasticsearch.xpack.sql.parser.SqlParser;
import org.elasticsearch.xpack.sql.plan.physical.EsQueryExec;
import org.elasticsearch.xpack.sql.plan.physical.PhysicalPlan;
import org.elasticsearch.xpack.sql.planner.QueryFolder.FoldAggregate.GroupingContext;
import org.elasticsearch.xpack.sql.planner.QueryTranslator.QueryTranslation;
import org.elasticsearch.xpack.sql.proto.SqlTypedParamValue;
import org.elasticsearch.xpack.sql.querydsl.agg.AggFilter;
import org.elasticsearch.xpack.sql.querydsl.agg.GroupByDateHistogram;
import org.elasticsearch.xpack.sql.querydsl.container.MetricAggRef;
import org.elasticsearch.xpack.sql.stats.Metrics;
import org.elasticsearch.xpack.sql.types.SqlTypesTests;
import org.elasticsearch.xpack.sql.util.DateUtils;
import org.junit.BeforeClass;

import java.time.ZoneId;
import java.time.ZonedDateTime;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.function.BiConsumer;
import java.util.function.Function;
import java.util.stream.Collectors;
import java.util.stream.IntStream;
import java.util.stream.Stream;

import static java.util.Arrays.asList;
import static org.elasticsearch.xpack.ql.expression.Literal.TRUE;
import static org.elasticsearch.xpack.ql.type.DataTypes.BOOLEAN;
import static org.elasticsearch.xpack.ql.type.DataTypes.DATETIME;
import static org.elasticsearch.xpack.ql.type.DataTypes.DOUBLE;
import static org.elasticsearch.xpack.ql.type.DataTypes.INTEGER;
import static org.elasticsearch.xpack.ql.type.DataTypes.KEYWORD;
import static org.elasticsearch.xpack.ql.type.DataTypes.LONG;
import static org.elasticsearch.xpack.ql.type.DataTypes.TEXT;
import static org.elasticsearch.xpack.sql.SqlTestUtils.TEST_CFG;
import static org.elasticsearch.xpack.sql.SqlTestUtils.literal;
import static org.elasticsearch.xpack.sql.expression.function.scalar.math.MathProcessor.MathOperation.E;
import static org.elasticsearch.xpack.sql.expression.function.scalar.math.MathProcessor.MathOperation.PI;
import static org.elasticsearch.xpack.sql.planner.QueryTranslator.DATE_FORMAT;
import static org.elasticsearch.xpack.sql.planner.QueryTranslator.TIME_FORMAT;
import static org.elasticsearch.xpack.sql.type.SqlDataTypes.DATE;
import static org.hamcrest.CoreMatchers.containsString;
import static org.hamcrest.Matchers.endsWith;
import static org.hamcrest.Matchers.everyItem;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.startsWith;

public class QueryTranslatorTests extends ESTestCase {

    private static class TestContext {
        private SqlFunctionRegistry sqlFunctionRegistry;
        private SqlParser parser;
        private Analyzer analyzer;
        private Optimizer optimizer;
        private Planner planner;

        TestContext(String mappingFile) {
            parser = new SqlParser();
            sqlFunctionRegistry = new SqlFunctionRegistry();

            Map<String, EsField> mapping = SqlTypesTests.loadMapping(mappingFile);
            EsIndex test = new EsIndex("test", mapping);
            IndexResolution getIndexResult = IndexResolution.valid(test);
            analyzer = new Analyzer(TEST_CFG, sqlFunctionRegistry, getIndexResult, new Verifier(new Metrics()));
            optimizer = new Optimizer();
            planner = new Planner();
        }

        public LogicalPlan plan(String sql) {
            return plan(sql, DateUtils.UTC);
        }

        public LogicalPlan plan(String sql, ZoneId zoneId) {
            return analyzer.analyze(parser.createStatement(sql, zoneId), true);
        }

        private PhysicalPlan optimizeAndPlan(String sql) {
            return optimizeAndPlan(plan(sql));
        }

        private PhysicalPlan optimizeAndPlan(LogicalPlan plan) {
            return planner.plan(optimizer.optimize(plan),true);
        }

        private LogicalPlan parameterizedSql(String sql, SqlTypedParamValue... params) {
            return analyzer.analyze(parser.createStatement(sql, asList(params), DateUtils.UTC), true);
        }
    }

    private static TestContext defaultTestContext;

    @BeforeClass
    public static void init() {
        defaultTestContext = new TestContext("mapping-multi-field-variation.json");
    }

    private LogicalPlan plan(String sql) {
        return defaultTestContext.plan(sql, DateUtils.UTC);
    }

    private LogicalPlan plan(String sql, ZoneId zoneId) {
        return defaultTestContext.plan(sql, zoneId);
    }

    private PhysicalPlan optimizeAndPlan(String sql) {
        return defaultTestContext.optimizeAndPlan(sql);
    }

    private PhysicalPlan optimizeAndPlan(LogicalPlan plan) {
        return defaultTestContext.optimizeAndPlan(plan);
    }

    private QueryTranslation translate(Expression condition) {
        return QueryTranslator.toQuery(condition, false);
    }

    private QueryTranslation translateWithAggs(Expression condition) {
        return QueryTranslator.toQuery(condition, true);
    }

    private LogicalPlan parameterizedSql(String sql, SqlTypedParamValue... params) {
        return defaultTestContext.parameterizedSql(sql, params);
    }

    public void testTermEqualityAnalyzer() {
        LogicalPlan p = plan("SELECT some.string FROM test WHERE some.string = 'value'");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof TermQuery);
        TermQuery tq = (TermQuery) query;
        assertEquals("some.string.typical", tq.term());
        assertEquals("value", tq.value());
    }

    public void testAliasAndGroupByResolution(){
        LogicalPlan p = plan("SELECT COUNT(*) AS c FROM test WHERE ABS(int) > 0 GROUP BY int");
        assertTrue(p instanceof Aggregate);
        var pc = ((Aggregate) p).child();
        assertTrue(pc instanceof Filter);
        Expression condition = ((Filter) pc).condition();
        assertEquals("GREATERTHAN", ((GreaterThan) condition).functionName());
        List<Expression> groupings = ((Aggregate) p).groupings();
        assertTrue(groupings.get(0).resolved());
        var agg = ((Aggregate) p).aggregates();
        assertEquals("c", (agg.get(0)).name());
        assertEquals("COUNT", ((Count) ((Alias) agg.get(0)).child()).functionName());
    }
    public void testLiteralWithGroupBy(){
        LogicalPlan p = plan("SELECT 1 as t, 2 FROM test GROUP BY int");
        assertTrue(p instanceof Aggregate);
        List<Expression> groupings = ((Aggregate) p).groupings();
        assertEquals(1, groupings.size());
        assertTrue(groupings.get(0).resolved());
        assertTrue(groupings.get(0) instanceof FieldAttribute);
        var aggs = ((Aggregate) p).aggregates();
        assertEquals(2, aggs.size());
        assertEquals("t", (aggs.get(0)).name());
        assertTrue(((Alias) aggs.get(0)).child() instanceof Literal);
        assertEquals("1", ((Alias) aggs.get(0)).child().toString());
        assertEquals("2", ((Alias) aggs.get(1)).child().toString());
    }

    public void testTermEqualityNotAnalyzed() {
        LogicalPlan p = plan("SELECT some.string FROM test WHERE int = 5");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof TermQuery);
        TermQuery tq = (TermQuery) query;
        assertEquals("int", tq.term());
        assertEquals(5, tq.value());
    }

    public void testTermEqualityForDate() {
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date = 5");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof TermQuery);
        TermQuery tq = (TermQuery) query;
        assertEquals("date", tq.term());
        assertEquals(5, tq.value());
    }

    public void testTermEqualityForDateWithLiteralDate() {
        ZoneId zoneId = randomZone();
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date = CAST('2019-08-08T12:34:56' AS DATETIME)", zoneId);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof RangeQuery);
        RangeQuery rq = (RangeQuery) query;
        assertEquals("date", rq.field());
        assertEquals("2019-08-08T12:34:56.000Z", rq.upper());
        assertEquals("2019-08-08T12:34:56.000Z", rq.lower());
        assertTrue(rq.includeLower());
        assertTrue(rq.includeUpper());
        assertEquals(DATE_FORMAT, rq.format());
        assertEquals(zoneId, rq.zoneId());
    }

    public void testTermEqualityForDateWithLiteralTime() {
        ZoneId zoneId = randomZone();
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date = CAST('12:34:56' AS TIME)", zoneId);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof RangeQuery);
        RangeQuery rq = (RangeQuery) query;
        assertEquals("date", rq.field());
        assertEquals("12:34:56.000", rq.upper());
        assertEquals("12:34:56.000", rq.lower());
        assertTrue(rq.includeLower());
        assertTrue(rq.includeUpper());
        assertEquals(TIME_FORMAT, rq.format());
        assertEquals(zoneId, rq.zoneId());
    }

    public void testComparisonAgainstColumns() {
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date > int");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QlIllegalArgumentException ex = expectThrows(QlIllegalArgumentException.class, () -> translate(condition));
        assertEquals("Line 1:43: Comparisons against fields are not (currently) supported; offender [int] in [>]", ex.getMessage());
    }

    public void testDateRange() {
        ZoneId zoneId = randomZone();
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date > 1969-05-13", zoneId);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof RangeQuery);
        RangeQuery rq = (RangeQuery) query;
        assertEquals("date", rq.field());
        assertEquals(1951, rq.lower());
        assertEquals(zoneId, rq.zoneId());
    }

    public void testDateRangeLiteral() {
        ZoneId zoneId = randomZone();
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date > '1969-05-13'", zoneId);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof RangeQuery);
        RangeQuery rq = (RangeQuery) query;
        assertEquals("date", rq.field());
        assertEquals("1969-05-13", rq.lower());
        assertEquals(zoneId, rq.zoneId());
    }

    public void testDateRangeCast() {
        ZoneId zoneId = randomZone();
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date > CAST('1969-05-13T12:34:56Z' AS DATETIME)", zoneId);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof RangeQuery);
        RangeQuery rq = (RangeQuery) query;
        assertEquals("date", rq.field());
        assertEquals("1969-05-13T12:34:56.000Z", rq.lower());
        assertEquals(zoneId, rq.zoneId());
    }

    public void testDateRangeWithCurrentTimestamp() {
        Integer nanoPrecision = randomPrecision();
        testDateRangeWithCurrentFunctions(
                functionWithPrecision("CURRENT_TIMESTAMP", nanoPrecision),
                DATE_FORMAT,
                nanoPrecision,
                TEST_CFG.now()
        );
        testDateRangeWithCurrentFunctions_AndRangeOptimization(
                functionWithPrecision("CURRENT_TIMESTAMP", nanoPrecision),
                DATE_FORMAT,
                nanoPrecision,
                TEST_CFG.now().minusDays(1L).minusSeconds(1L),
                TEST_CFG.now().plusDays(1L).plusSeconds(1L)
        );
    }

    public void testDateRangeWithCurrentDate() {
        testDateRangeWithCurrentFunctions("CURRENT_DATE()", DATE_FORMAT, null, DateUtils.asDateOnly(TEST_CFG.now()));
        testDateRangeWithCurrentFunctions_AndRangeOptimization(
                "CURRENT_DATE()",
                DATE_FORMAT,
                null,
                DateUtils.asDateOnly(TEST_CFG.now().minusDays(1L)).minusSeconds(1),
                DateUtils.asDateOnly(TEST_CFG.now().plusDays(1L)).plusSeconds(1)
        );
    }

    public void testDateRangeWithToday() {
        testDateRangeWithCurrentFunctions("TODAY()", DATE_FORMAT, null, DateUtils.asDateOnly(TEST_CFG.now()));
        testDateRangeWithCurrentFunctions_AndRangeOptimization(
                "TODAY()",
                DATE_FORMAT,
                null,
                DateUtils.asDateOnly(TEST_CFG.now().minusDays(1L)).minusSeconds(1),
                DateUtils.asDateOnly(TEST_CFG.now().plusDays(1L)).plusSeconds(1)
        );
    }

    public void testDateRangeWithNow() {
        Integer nanoPrecision = randomPrecision();
        testDateRangeWithCurrentFunctions(
                functionWithPrecision("NOW", nanoPrecision),
                DATE_FORMAT,
                nanoPrecision,
                TEST_CFG.now()
        );
        testDateRangeWithCurrentFunctions_AndRangeOptimization(
                functionWithPrecision("NOW", nanoPrecision),
                DATE_FORMAT,
                nanoPrecision,
                TEST_CFG.now().minusDays(1L).minusSeconds(1L),
                TEST_CFG.now().plusDays(1L).plusSeconds(1L)
        );
    }

    public void testDateRangeWithCurrentTime() {
        Integer nanoPrecision = randomPrecision();
        testDateRangeWithCurrentFunctions(
                functionWithPrecision("CURRENT_TIME", nanoPrecision),
                TIME_FORMAT,
                nanoPrecision,
                TEST_CFG.now()
        );
        testDateRangeWithCurrentFunctions_AndRangeOptimization(
                functionWithPrecision("CURRENT_TIME", nanoPrecision),
                TIME_FORMAT,
                nanoPrecision,
                TEST_CFG.now().minusDays(1L).minusSeconds(1L),
                TEST_CFG.now().plusDays(1L).plusSeconds(1L)
        );
    }

    private Integer randomPrecision() {
        return randomFrom(new Integer[] {null, 1, 2, 3, 4, 5, 6, 7, 8, 9});
    }

    private String functionWithPrecision(String function, Integer precision) {
        return function + "(" + (precision == null ? "" : precision.toString()) + ")";
    }

    private void testDateRangeWithCurrentFunctions(String function, String pattern, Integer nanoPrecision, ZonedDateTime now) {
        String operator = randomFrom(new String[] {">", ">=", "<", "<=", "=", "!="});
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date" + operator + function);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = QueryTranslator.toQuery(condition, false);
        Query query = translation.query;
        RangeQuery rq;

        if (operator.equals("!=")) {
            assertTrue(query instanceof NotQuery);
            NotQuery nq = (NotQuery) query;
            assertTrue(nq.child() instanceof RangeQuery);
            rq = (RangeQuery) nq.child();
        } else {
            assertTrue(query instanceof RangeQuery);
            rq = (RangeQuery) query;
        }
        assertEquals("date", rq.field());

        if (operator.contains("<") || operator.equals("=") || operator.equals("!=")) {
            assertEquals(DateFormatter.forPattern(pattern).format(now.withNano(DateUtils.getNanoPrecision(
                    nanoPrecision == null ? null : literal(nanoPrecision), now.getNano()))), rq.upper());
        }
        if (operator.contains(">") || operator.equals("=") || operator.equals("!=")) {
            assertEquals(DateFormatter.forPattern(pattern).format(now.withNano(DateUtils.getNanoPrecision(
                    nanoPrecision == null ? null : literal(nanoPrecision), now.getNano()))), rq.lower());
        }

        assertEquals(operator.equals("=") || operator.equals("!=") || operator.equals("<="), rq.includeUpper());
        assertEquals(operator.equals("=") || operator.equals("!=") || operator.equals(">="), rq.includeLower());
        assertEquals(pattern, rq.format());
    }

    private void testDateRangeWithCurrentFunctions_AndRangeOptimization(
            String function,
            String pattern,
            Integer nanoPrecision,
            ZonedDateTime lowerValue,
            ZonedDateTime upperValue
    ) {
        String lowerOperator = randomFrom(new String[] {"<", "<="});
        String upperOperator = randomFrom(new String[] {">", ">="});
        // use both date-only interval (1 DAY) and time-only interval (1 second) to cover CURRENT_TIMESTAMP and TODAY scenarios
        String interval = "(INTERVAL 1 DAY + INTERVAL 1 SECOND)";

        PhysicalPlan p = optimizeAndPlan("SELECT some.string FROM test WHERE date" + lowerOperator + function + " + " + interval
                + " AND date " + upperOperator + function + " - " + interval);
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("test.some.string", eqe.output().get(0).qualifiedName());
        assertEquals(TEXT, eqe.output().get(0).dataType());

        Query query = eqe.queryContainer().query();
        // the range queries optimization should create a single "range" query with "from" and "to" populated with the values
        // in the two branches of the AND condition
        assertTrue(query instanceof RangeQuery);
        RangeQuery rq = (RangeQuery) query;
        assertEquals("date", rq.field());

        assertEquals(DateFormatter.forPattern(pattern)
                .format(upperValue.withNano(DateUtils.getNanoPrecision(
                        nanoPrecision == null ? null : literal(nanoPrecision), upperValue.getNano()))), rq.upper());
        assertEquals(DateFormatter.forPattern(pattern)
                .format(lowerValue.withNano(DateUtils.getNanoPrecision(
                        nanoPrecision == null ? null : literal(nanoPrecision), lowerValue.getNano()))), rq.lower());

        assertEquals(lowerOperator.equals("<="), rq.includeUpper());
        assertEquals(upperOperator.equals(">="), rq.includeLower());
        assertEquals(pattern, rq.format());
    }

    public void testDateRangeWithESDateMath() {
        ZoneId zoneId = randomZone();
        String operator = randomFrom(">", ">=", "<", "<=", "=", "!=");
        String dateMath = randomFrom("now", "now/d", "now/h", "now-2h", "now+2h", "now-5d", "now+5d");
        LogicalPlan p = plan("SELECT some.string FROM test WHERE date" + operator + "'" + dateMath + "'", zoneId);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);
        Query query = translation.query;

        if ("=".equals(operator) || "!=".equals(operator)) {
            TermQuery tq;
            if ("=".equals(operator)) {
                assertTrue(query instanceof TermQuery);
                tq = (TermQuery) query;
            } else {
                assertTrue(query instanceof NotQuery);
                NotQuery nq = (NotQuery) query;
                assertTrue(nq.child() instanceof TermQuery);
                tq = (TermQuery) nq.child();
            }
            assertEquals("date", tq.term());
        } else {
            assertTrue(query instanceof RangeQuery);
            RangeQuery rq = (RangeQuery) query;
            assertEquals("date", rq.field());

            if (operator.contains("<")) {
                assertEquals(dateMath, rq.upper());
            }
            if (operator.contains(">")) {
                assertEquals(dateMath, rq.lower());
            }

            assertEquals("<=".equals(operator), rq.includeUpper());
            assertEquals(">=".equals(operator), rq.includeLower());
            assertNull(rq.format());
            assertEquals(zoneId, rq.zoneId());
        }
    }

    public void testTranslateDateAdd_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE DATE_ADD('quarter',int, date) > '2018-09-04'::date");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalSqlScriptUtils.dateAdd(" +
                "params.v0,InternalQlScriptUtils.docValue(doc,params.v1),InternalQlScriptUtils.docValue(doc,params.v2)," +
                "params.v3),InternalSqlScriptUtils.asDateTime(params.v4)))",
            sc.script().toString());
        assertEquals("[{v=quarter}, {v=int}, {v=date}, {v=Z}, {v=2018-09-04T00:00:00.000Z}]", sc.script().params().toString());
    }

    public void testTranslateDateDiff_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE DATE_DIFF('week',date, date) > '2018-09-04'::date");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalSqlScriptUtils.dateDiff(" +
                "params.v0,InternalQlScriptUtils.docValue(doc,params.v1),InternalQlScriptUtils.docValue(doc,params.v2)," +
                "params.v3),InternalSqlScriptUtils.asDateTime(params.v4)))",
            sc.script().toString());
        assertEquals("[{v=week}, {v=date}, {v=date}, {v=Z}, {v=2018-09-04T00:00:00.000Z}]", sc.script().params().toString());
    }

    public void testTranslateDateTrunc_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE DATE_TRUNC('month', date) > '2018-09-04'::date");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalSqlScriptUtils.dateTrunc(" +
                "params.v0,InternalQlScriptUtils.docValue(doc,params.v1),params.v2),InternalSqlScriptUtils.asDateTime(params.v3)))",
            sc.script().toString());
        assertEquals("[{v=month}, {v=date}, {v=Z}, {v=2018-09-04T00:00:00.000Z}]", sc.script().params().toString());
    }

    public void testTranslateDatePart_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE DATE_PART('month', date) > '2018-09-04'::date");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalSqlScriptUtils.datePart(" +
                "params.v0,InternalQlScriptUtils.docValue(doc,params.v1),params.v2),InternalSqlScriptUtils.asDateTime(params.v3)))",
            sc.script().toString());
        assertEquals("[{v=month}, {v=date}, {v=Z}, {v=2018-09-04T00:00:00.000Z}]", sc.script().params().toString());
    }

    public void testTranslateDateTimeFormat_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE DATETIME_FORMAT(date, 'YYYY_MM_dd') = '2018_09_04'");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.eq(InternalSqlScriptUtils.dateTimeFormat(" +
                        "InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2),params.v3))",
                sc.script().toString());
        assertEquals("[{v=date}, {v=YYYY_MM_dd}, {v=Z}, {v=2018_09_04}]", sc.script().params().toString());
    }

    public void testTranslateDateTimeParse_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE DATETIME_PARSE(keyword, 'uuuu_MM_dd') = '2018-09-04'::date");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.eq(InternalSqlScriptUtils.dateTimeParse(" +
                "InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2),InternalSqlScriptUtils.asDateTime(params.v3)))",
            sc.script().toString()
        );
        assertEquals("[{v=keyword}, {v=uuuu_MM_dd}, {v=Z}, {v=2018-09-04T00:00:00.000Z}]", sc.script().params().toString());
    }

    public void testTranslateFormat_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE FORMAT(date, 'YYYY_MM_dd') = '2018_09_04'");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = QueryTranslator.toQuery(condition, false);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.eq(InternalSqlScriptUtils.format(" +
                "InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2),params.v3))",
            sc.script().toString());
        assertEquals("[{v=date}, {v=YYYY_MM_dd}, {v=Z}, {v=2018_09_04}]", sc.script().params().toString());
    }

    public void testLikeOnInexact() {
        LogicalPlan p = plan("SELECT * FROM test WHERE some.string LIKE '%a%'");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation qt = translate(condition);
        assertEquals(WildcardQuery.class, qt.query.getClass());
        WildcardQuery qsq = ((WildcardQuery) qt.query);
        assertEquals("some.string.typical", qsq.field());
    }

    public void testRLikeOnInexact() {
        LogicalPlan p = plan("SELECT * FROM test WHERE some.string RLIKE '.*a.*'");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation qt = translate(condition);
        assertEquals(RegexQuery.class, qt.query.getClass());
        RegexQuery qsq = ((RegexQuery) qt.query);
        assertEquals("some.string.typical", qsq.field());
    }

    public void testLikeWithScalars() {
        LogicalPlan p = plan("SELECT LTRIM(keyword) lt FROM test WHERE LTRIM(keyword) like '%a%'");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation qt = translate(condition);
        assertTrue(qt.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) qt.query;
        assertEquals(
            "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.regex("
                +
                "InternalSqlScriptUtils.ltrim(InternalQlScriptUtils.docValue(doc,params.v0)),params.v1))",
            sc.script().toString());
        assertEquals("[{v=keyword}, {v=^.*a.*$}]", sc.script().params().toString());
    }

    public void testRLikeWithScalars() {
        LogicalPlan p = plan("SELECT LTRIM(keyword) lt FROM test WHERE LTRIM(keyword) RLIKE '.*a.*'");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation qt = translate(condition);
        assertTrue(qt.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) qt.query;
        assertEquals(
            "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.regex("
                +
                "InternalSqlScriptUtils.ltrim(InternalQlScriptUtils.docValue(doc,params.v0)),params.v1))",
            sc.script().toString());
        assertEquals("[{v=keyword}, {v=.*a.*}]", sc.script().params().toString());
    }

    public void testDifferentLikeAndNotLikePatterns() {
        LogicalPlan p = plan("SELECT keyword k FROM test WHERE k LIKE 'X%' AND k NOT LIKE 'Y%'");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);

        Expression condition = ((Filter) p).condition();
        QueryTranslation qt = translate(condition);
        assertEquals(BoolQuery.class, qt.query.getClass());
        BoolQuery bq = ((BoolQuery) qt.query);
        assertTrue(bq.isAnd());
        assertTrue(bq.left() instanceof WildcardQuery);
        assertTrue(bq.right() instanceof NotQuery);

        NotQuery nq = (NotQuery) bq.right();
        assertTrue(nq.child() instanceof WildcardQuery);
        WildcardQuery lqsq = (WildcardQuery) bq.left();
        WildcardQuery rqsq = (WildcardQuery) nq.child();

        assertEquals("X*", lqsq.query());
        assertEquals("keyword", lqsq.field());
        assertEquals("Y*", rqsq.query());
        assertEquals("keyword", rqsq.field());
    }

    public void testRLikePatterns() {
        String[] patterns = new String[] {"(...)+", "abab(ab)?", "(ab){1,2}", "(ab){3}", "aabb|bbaa", "a+b+|b+a+", "aa(cc|bb)",
                "a{4,6}b{4,6}", ".{3}.{3}", "aaa*bbb*", "a+.+", "a.c.e", "[^abc\\-]"};
        for (int i = 0; i < 5; i++) {
            assertDifferentRLikeAndNotRLikePatterns(randomFrom(patterns), randomFrom(patterns));
        }
    }

    private void assertDifferentRLikeAndNotRLikePatterns(String firstPattern, String secondPattern) {
        LogicalPlan p = plan("SELECT keyword k FROM test WHERE k RLIKE '" + firstPattern + "' AND k NOT RLIKE '" + secondPattern + "'");
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);

        Expression condition = ((Filter) p).condition();
        QueryTranslation qt = translate(condition);
        assertEquals(BoolQuery.class, qt.query.getClass());
        BoolQuery bq = ((BoolQuery) qt.query);
        assertTrue(bq.isAnd());
        assertTrue(bq.left() instanceof RegexQuery);
        assertTrue(bq.right() instanceof NotQuery);

        NotQuery nq = (NotQuery) bq.right();
        assertTrue(nq.child() instanceof RegexQuery);
        RegexQuery lqsq = (RegexQuery) bq.left();
        RegexQuery rqsq = (RegexQuery) nq.child();

        assertEquals(firstPattern, lqsq.regex());
        assertEquals("keyword", lqsq.field());
        assertEquals(secondPattern, rqsq.regex());
        assertEquals("keyword", rqsq.field());
    }

    public void testLikeRLikeAsPainlessScripts() {
        LogicalPlan p = plan("SELECT count(*), CASE WHEN keyword LIKE '%foo%' THEN 1 WHEN keyword RLIKE '.*bar.*' THEN 2 " +
                             "ELSE 3 END AS t FROM test GROUP BY t");
        assertTrue(p instanceof Aggregate);
        Expression condition = ((Aggregate) p).groupings().get(0);
        assertFalse(condition.foldable());
        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(((Aggregate) p).groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals(
            "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.regex(InternalQlScriptUtils.docValue(doc,params.v0),"
                + "params.v1)) ? params.v2 : InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.regex(InternalQlScriptUtils."
                +
                "docValue(doc,params.v3),params.v4)) ? params.v5 : params.v6",
                scriptTemplate.toString());
        assertEquals("[{v=keyword}, {v=^.*foo.*$}, {v=1}, {v=keyword}, {v=.*bar.*}, {v=2}, {v=3}]",
                scriptTemplate.params().toString());
    }

    public void testStartsWithUsesPrefixQuery() {
        LogicalPlan p = plan("SELECT keyword FROM test WHERE STARTS_WITH(keyword, 'x') OR STARTS_WITH(keyword, 'y')");

        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());

        QueryTranslation translation = translate(condition);
        assertTrue(translation.query instanceof BoolQuery);
        BoolQuery bq = (BoolQuery) translation.query;

        assertFalse(bq.isAnd());
        assertTrue(bq.left() instanceof PrefixQuery);
        assertTrue(bq.right() instanceof PrefixQuery);

        PrefixQuery pqr = (PrefixQuery) bq.right();
        assertEquals("keyword", pqr.field());
        assertEquals("y", pqr.query());

        PrefixQuery pql = (PrefixQuery) bq.left();
        assertEquals("keyword", pql.field());
        assertEquals("x", pql.query());
    }

    public void testStartsWithUsesPrefixQueryAndScript() {
        LogicalPlan p = plan("SELECT keyword FROM test WHERE STARTS_WITH(keyword, 'x') AND STARTS_WITH(keyword, 'xy') "
            + "AND STARTS_WITH(LCASE(keyword), 'xyz')");

        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());

        QueryTranslation translation = translate(condition);
        assertTrue(translation.query instanceof BoolQuery);
        BoolQuery bq = (BoolQuery) translation.query;

        assertTrue(bq.isAnd());
        assertTrue(bq.left() instanceof BoolQuery);
        assertTrue(bq.right() instanceof ScriptQuery);

        BoolQuery bbq = (BoolQuery) bq.left();
        assertTrue(bbq.isAnd());
        PrefixQuery pqr = (PrefixQuery) bbq.right();
        assertEquals("keyword", pqr.field());
        assertEquals("xy", pqr.query());

        PrefixQuery pql = (PrefixQuery) bbq.left();
        assertEquals("keyword", pql.field());
        assertEquals("x", pql.query());

        ScriptQuery sq = (ScriptQuery) bq.right();
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.startsWith("
                + "InternalSqlScriptUtils.lcase(InternalQlScriptUtils.docValue(doc,params.v0)), "
                + "params.v1, params.v2))",
            sq.script().toString());
        assertEquals("[{v=keyword}, {v=xyz}, {v=false}]", sq.script().params().toString());
    }

    @SuppressWarnings("unchecked")
    public void testTrim_WhereClause_Painless() {
        Class<? extends UnaryStringFunction> trimFunction = randomFrom(Trim.class, LTrim.class, RTrim.class);
        String trimFunctionName = trimFunction.getSimpleName().toUpperCase(Locale.ROOT);
        LogicalPlan p = plan("SELECT " + trimFunctionName + "(keyword) trimmed FROM test WHERE " + trimFunctionName + "(keyword) = 'foo'");

        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation qt = translate(condition);
        assertTrue(qt.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) qt.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.eq(InternalSqlScriptUtils." +
                trimFunctionName.toLowerCase(Locale.ROOT) + "(InternalQlScriptUtils.docValue(doc,params.v0)),params.v1))",
            sc.script().toString());
        assertEquals("[{v=keyword}, {v=foo}]", sc.script().params().toString());
    }

    @SuppressWarnings("unchecked")
    public void testTrim_GroupBy_Painless() {
        Class<? extends UnaryStringFunction> trimFunction = randomFrom(Trim.class, LTrim.class, RTrim.class);
        String trimFunctionName = trimFunction.getSimpleName().toUpperCase(Locale.ROOT);
        LogicalPlan p = plan("SELECT " + trimFunctionName + "(keyword) trimmed, count(*) FROM test GROUP BY " +
            trimFunctionName + "(keyword)");

        assertEquals(Aggregate.class, p.getClass());
        Aggregate agg = (Aggregate) p;
        assertEquals(1, agg.groupings().size());
        assertEquals(2, agg.aggregates().size());
        assertEquals(trimFunction, agg.groupings().get(0).getClass());
        assertEquals(trimFunction, ((Alias) agg.aggregates().get(0)).child().getClass());
        assertEquals(Count.class,((Alias) agg.aggregates().get(1)).child().getClass());

        UnaryStringFunction trim = (UnaryStringFunction) agg.groupings().get(0);
        assertEquals(1, trim.children().size());

        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(agg.groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals("InternalSqlScriptUtils." + trimFunctionName.toLowerCase(Locale.ROOT) +
                "(InternalQlScriptUtils.docValue(doc,params.v0))",
            scriptTemplate.toString());
        assertEquals("[{v=keyword}]", scriptTemplate.params().toString());
    }

    public void testTranslateNotExpression_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT * FROM test WHERE NOT(POSITION('x', keyword) = 0)");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.not(" +
                "InternalQlScriptUtils.eq(InternalSqlScriptUtils.position(" +
                "params.v0,InternalQlScriptUtils.docValue(doc,params.v1)),params.v2)))",
            sc.script().toString());
        assertEquals("[{v=x}, {v=keyword}, {v=0}]", sc.script().params().toString());
    }

    public void testTranslateIsNullExpression_WhereClause() {
        LogicalPlan p = plan("SELECT * FROM test WHERE keyword IS NULL");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertTrue(translation.query instanceof NotQuery);
        NotQuery tq = (NotQuery) translation.query;
        assertTrue(tq.child() instanceof ExistsQuery);
        ExistsQuery eq = (ExistsQuery) tq.child();
        assertEquals("{\"exists\":{\"field\":\"keyword\",\"boost\":1.0}}",
            eq.asBuilder().toString().replaceAll("\\s+", ""));
    }

    public void testTranslateIsNullExpression_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT * FROM test WHERE POSITION('x', keyword) IS NULL");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.isNull(" +
                "InternalSqlScriptUtils.position(params.v0,InternalQlScriptUtils.docValue(doc,params.v1))))",
            sc.script().toString());
        assertEquals("[{v=x}, {v=keyword}]", sc.script().params().toString());
    }

    public void testTranslateIsNotNullExpression_WhereClause() {
        LogicalPlan p = plan("SELECT * FROM test WHERE keyword IS NOT NULL");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertTrue(translation.query instanceof ExistsQuery);
        ExistsQuery eq = (ExistsQuery) translation.query;
        assertEquals("{\"exists\":{\"field\":\"keyword\",\"boost\":1.0}}",
            eq.asBuilder().toString().replaceAll("\\s+", ""));
    }

    public void testTranslateIsNotNullExpression_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT * FROM test WHERE POSITION('x', keyword) IS NOT NULL");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.isNotNull(" +
                "InternalSqlScriptUtils.position(params.v0,InternalQlScriptUtils.docValue(doc,params.v1))))",
            sc.script().toString());
        assertEquals("[{v=x}, {v=keyword}]", sc.script().params().toString());
    }

    public void testTranslateIsNullExpression_HavingClause_Painless() {
        LogicalPlan p = plan("SELECT keyword, max(int) FROM test GROUP BY keyword HAVING max(int) IS NULL");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals(
                "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.isNull(" +
                    "InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0)))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=max(int)"));
    }

    public void testTranslateIsNotNullExpression_HavingClause_Painless() {
        LogicalPlan p = plan("SELECT keyword, max(int) FROM test GROUP BY keyword HAVING max(int) IS NOT NULL");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals(
                "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.isNotNull(" +
                    "InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0)))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=max(int)"));
    }



    public void testTranslateCoalesceExpression_WhereGroupByAndHaving_Painless() {
        PhysicalPlan p = optimizeAndPlan("SELECT COALESCE(int, 2) AS c, COALESCE(max(date), '2020-01-01'::date) as m FROM test " +
                                         "WHERE c > 10 GROUP BY c HAVING m > '2020-01-01'::date");
        assertTrue(p instanceof EsQueryExec);
        EsQueryExec esQExec = (EsQueryExec) p;
        assertEquals(2, esQExec.output().size());
        AggregationBuilder aggBuilder = esQExec.queryContainer().aggs().asAggBuilder();
        assertEquals(1, aggBuilder.getSubAggregations().size());
        assertEquals(1, aggBuilder.getPipelineAggregations().size());
        String aggName = aggBuilder.getSubAggregations().iterator().next().getName();
        String havingName = aggBuilder.getPipelineAggregations().iterator().next().getName();
        assertThat(aggBuilder.toString(), containsString("{\"terms\":{\"script\":{\"source\":\"" +
                "InternalSqlScriptUtils.coalesce([InternalQlScriptUtils.docValue(doc,params.v0),params.v1])\",\"lang\":\"painless\"" +
                ",\"params\":{\"v0\":\"int\",\"v1\":2}},\"missing_bucket\":true,\"value_type\":\"long\",\"order\":\"asc\"}}}]}," +
                "\"aggregations\":{\"" + aggName + "\":{\"max\":{\"field\":\"date\"}},\"" + havingName + "\":" +
                "{\"bucket_selector\":{\"buckets_path\":{\"a0\":\"" + aggName + "\"},\"script\":{\"source\":\"" +
                "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalSqlScriptUtils.coalesce(" +
                "[InternalSqlScriptUtils.asDateTime(params.a0),InternalSqlScriptUtils.asDateTime(params.v0)])," +
                "InternalSqlScriptUtils.asDateTime(params.v1)))\"," +
                "\"lang\":\"painless\",\"params\":{\"v0\":\"2020-01-01T00:00:00.000Z\",\"v1\":\"2020-01-01T00:00:00.000Z"));
        assertTrue(esQExec.queryContainer().query() instanceof ScriptQuery);
        ScriptQuery sq = (ScriptQuery) esQExec.queryContainer().query();
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(" +
                "InternalSqlScriptUtils.coalesce([InternalQlScriptUtils.docValue(doc,params.v0),params.v1]),params.v2))",
                sq.script().toString());
        assertEquals("[{v=int}, {v=2}, {v=10}]", sq.script().params().toString());
    }

    public void testTranslateAggFloat_HavingClause_NoCasting_Painless() {
        LogicalPlan p = plan("SELECT keyword, max(float) FROM test GROUP BY keyword HAVING max(float) IS NOT NULL");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals(
            "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.isNotNull(params.a0))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=max(float)"));
    }

    public void testTranslateInExpression_WhereClause() {
        LogicalPlan p = plan("SELECT * FROM test WHERE keyword IN ('foo', 'bar', 'lala', 'foo', concat('la', 'la'))");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof TermsQuery);
        TermsQuery tq = (TermsQuery) query;
        assertEquals("{\"terms\":{\"keyword\":[\"foo\",\"bar\",\"lala\"],\"boost\":1.0}}",
            tq.asBuilder().toString().replaceAll("\\s", ""));
    }

    public void testTranslateInExpression_WhereClause_TextFieldWithKeyword() {
        LogicalPlan p = plan("SELECT * FROM test WHERE some.string IN ('foo', 'bar', 'lala', 'foo', concat('la', 'la'))");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof TermsQuery);
        TermsQuery tq = (TermsQuery) query;
        assertEquals("{\"terms\":{\"some.string.typical\":[\"foo\",\"bar\",\"lala\"],\"boost\":1.0}}",
            tq.asBuilder().toString().replaceAll("\\s", ""));
    }

    public void testTranslateInExpression_WhereClauseAndNullHandling() {
        LogicalPlan p = plan("SELECT * FROM test WHERE keyword IN ('foo', null, 'lala', null, 'foo', concat('la', 'la'))");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        Query query = translation.query;
        assertTrue(query instanceof TermsQuery);
        TermsQuery tq = (TermsQuery) query;
        assertEquals("{\"terms\":{\"keyword\":[\"foo\",\"lala\"],\"boost\":1.0}}",
            tq.asBuilder().toString().replaceAll("\\s", ""));
    }

    public void testTranslateInExpression_WhereClause_Datetime() {
        ZoneId zoneId = randomZone();
        String[] dates = {"2002-02-02T02:02:02.222Z", "2003-03-03T03:03:03.333Z"};
        LogicalPlan p = plan("SELECT * FROM test WHERE date IN ('" + dates[0] + "'::datetime, '" + dates[1] + "'::datetime)", zoneId);
        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        QueryTranslation translation = translate(condition);

        Query query = translation.query;
        assertTrue(query instanceof BoolQuery);
        BoolQuery bq = (BoolQuery) query;
        assertFalse(bq.isAnd());
        assertTrue(bq.left() instanceof RangeQuery);
        assertTrue(bq.right() instanceof RangeQuery);
        List<Tuple<String, RangeQuery>> tuples = asList(new Tuple<>(dates[0], (RangeQuery)bq.left()),
            new Tuple<>(dates[1], (RangeQuery) bq.right()));

        for (Tuple<String, RangeQuery> t: tuples) {
            String date = t.v1();
            RangeQuery rq = t.v2();

            assertEquals("date", rq.field());
            assertEquals(date, rq.upper().toString());
            assertEquals(date, rq.lower().toString());
            assertEquals(zoneId, rq.zoneId());
            assertTrue(rq.includeLower());
            assertTrue(rq.includeUpper());
            assertEquals(DATE_FORMAT, rq.format());
        }
    }

    public void testTranslateInExpression_WhereClause_Painless() {
        LogicalPlan p = plan("SELECT int FROM test WHERE POWER(int, 2) IN (10, null, 20, 30 - 10)");
        assertTrue(p instanceof Project);
        assertTrue(p.children().get(0) instanceof Filter);
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.in(" +
                "InternalSqlScriptUtils.power(InternalQlScriptUtils.docValue(doc,params.v0),params.v1), params.v2))",
            sc.script().toString());
        assertEquals("[{v=int}, {v=2}, {v=[10.0, null, 20.0]}]", sc.script().params().toString());
    }

    public void testTranslateInExpression_HavingClause_Painless() {
        LogicalPlan p = plan("SELECT keyword, max(int) FROM test GROUP BY keyword HAVING max(int) IN (10, 20, 30 - 10)");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.in(" +
                "InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0), params.v1))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=max(int)"));
        assertThat(aggFilter.scriptTemplate().params().toString(), endsWith(", {v=[10, 20]}]"));
    }

    public void testTranslateInExpression_HavingClause_PainlessOneArg() {
        LogicalPlan p = plan("SELECT keyword, max(int) FROM test GROUP BY keyword HAVING max(int) IN (10, 30 - 20)");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.in(" +
                "InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0), params.v1))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=max(int)"));
        assertThat(aggFilter.scriptTemplate().params().toString(), endsWith(", {v=[10]}]"));

    }

    public void testTranslateInExpression_HavingClause_PainlessAndNullHandling() {
        LogicalPlan p = plan("SELECT keyword, max(int) FROM test GROUP BY keyword HAVING max(int) IN (10, null, 20, 30, null, 30 - 10)");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.in(" +
                "InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0), params.v1))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=max(int)"));
        assertThat(aggFilter.scriptTemplate().params().toString(), endsWith(", {v=[10, null, 20, 30]}]"));
    }

    public void testTranslateMathFunction_HavingClause_Painless() {
        MathOperation operation =
            (MathOperation) randomFrom(Stream.of(MathOperation.values()).filter(o -> o != PI && o != E).toArray());

        LogicalPlan p = plan("SELECT keyword, max(int) FROM test GROUP BY keyword HAVING " +
            operation.name() + "(max(int)) > 10");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalSqlScriptUtils." +
                operation.name().toLowerCase(Locale.ROOT) +
                "(InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0)),params.v1))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=max(int)"));
        assertThat(aggFilter.scriptTemplate().params().toString(), endsWith(", {v=10}]"));
    }

    public void testTranslateRoundWithOneParameter() {
        LogicalPlan p = plan("SELECT ROUND(YEAR(date)) FROM test GROUP BY ROUND(YEAR(date))");

        assertTrue(p instanceof Aggregate);
        assertEquals(1, ((Aggregate) p).groupings().size());
        assertEquals(1, ((Aggregate) p).aggregates().size());
        assertTrue(((Aggregate) p).groupings().get(0) instanceof Round);
        assertTrue(((Alias) (((Aggregate) p).aggregates().get(0))).child() instanceof Round);

        Round groupingRound = (Round) ((Aggregate) p).groupings().get(0);
        assertEquals(1, groupingRound.children().size());

        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(((Aggregate) p).groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals(
                "InternalSqlScriptUtils.round(InternalSqlScriptUtils.dateTimeExtract(InternalQlScriptUtils.docValue(doc,params.v0), "
                + "params.v1, params.v2),params.v3)",
            scriptTemplate.toString());
        assertEquals("[{v=date}, {v=Z}, {v=YEAR}, {v=null}]", scriptTemplate.params().toString());
    }

    public void testTranslateRoundWithTwoParameters() {
        LogicalPlan p = plan("SELECT ROUND(YEAR(date), -2) FROM test GROUP BY ROUND(YEAR(date), -2)");

        assertTrue(p instanceof Aggregate);
        assertEquals(1, ((Aggregate) p).groupings().size());
        assertEquals(1, ((Aggregate) p).aggregates().size());
        assertTrue(((Aggregate) p).groupings().get(0) instanceof Round);
        assertTrue(((Aggregate) p).aggregates().get(0) instanceof Alias);
        assertTrue(((Alias) (((Aggregate) p).aggregates().get(0))).child() instanceof Round);

        Round groupingRound = (Round) ((Aggregate) p).groupings().get(0);
        assertEquals(2, groupingRound.children().size());
        assertTrue(groupingRound.children().get(1) instanceof Literal);
        assertEquals(-2, ((Literal) groupingRound.children().get(1)).value());

        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(((Aggregate) p).groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals(
                "InternalSqlScriptUtils.round(InternalSqlScriptUtils.dateTimeExtract(InternalQlScriptUtils.docValue(doc,params.v0), "
                + "params.v1, params.v2),params.v3)",
            scriptTemplate.toString());
        assertEquals("[{v=date}, {v=Z}, {v=YEAR}, {v=-2}]", scriptTemplate.params().toString());
    }

    public void testGroupByAndHavingWithFunctionOnTopOfAggregation() {
        LogicalPlan p = plan("SELECT keyword, MAX(int) FROM test GROUP BY 1 HAVING ABS(MAX(int)) > 10");
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalSqlScriptUtils.abs" +
                "(InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0)),params.v1))",
            aggFilter.scriptTemplate().toString());
        assertThat(aggFilter.scriptTemplate().params().toString(), startsWith("[{a=MAX(int)"));
        assertThat(aggFilter.scriptTemplate().params().toString(), endsWith(", {v=10}]"));
    }

    public void testTranslateStAsWktForPoints() {
        LogicalPlan p = plan("SELECT ST_AsWKT(point) FROM test WHERE ST_AsWKT(point) = 'point (10 20)'");
        assertThat(p, instanceOf(Project.class));
        assertThat(p.children().get(0), instanceOf(Filter.class));
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.eq(" +
                "InternalSqlScriptUtils.stAswkt(InternalSqlScriptUtils.geoDocValue(doc,params.v0))," +
                "params.v1)" +
                ")",
            aggFilter.scriptTemplate().toString());
        assertEquals("[{v=point}, {v=point (10 20)}]", aggFilter.scriptTemplate().params().toString());
    }

    public void testTranslateStWktToSql() {
        LogicalPlan p = plan("SELECT shape FROM test WHERE ST_WKTToSQL(keyword) = ST_WKTToSQL('point (10 20)')");
        assertThat(p, instanceOf(Project.class));
        assertThat(p.children().get(0), instanceOf(Filter.class));
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translateWithAggs(condition);
        assertNull(translation.query);
        AggFilter aggFilter = translation.aggFilter;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(" +
                "InternalQlScriptUtils.eq(InternalSqlScriptUtils.stWktToSql(" +
                "InternalQlScriptUtils.docValue(doc,params.v0)),InternalSqlScriptUtils.stWktToSql(params.v1)))",
            aggFilter.scriptTemplate().toString());
        assertEquals("[{v=keyword}, {v=POINT (10.0 20.0)}]", aggFilter.scriptTemplate().params().toString());
    }

    public void testTranslateStDistanceToScript() {
        String operator = randomFrom(">", ">=");
        String operatorFunction = operator.equalsIgnoreCase(">") ? "gt" : "gte";
        LogicalPlan p = plan("SELECT shape FROM test WHERE ST_Distance(point, ST_WKTToSQL('point (10 20)')) " + operator + " 20");
        assertThat(p, instanceOf(Project.class));
        assertThat(p.children().get(0), instanceOf(Filter.class));
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof ScriptQuery);
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(" +
                "InternalQlScriptUtils." + operatorFunction + "(" +
                "InternalSqlScriptUtils.stDistance(" +
                "InternalSqlScriptUtils.geoDocValue(doc,params.v0),InternalSqlScriptUtils.stWktToSql(params.v1)),params.v2))",
            sc.script().toString());
        assertEquals("[{v=point}, {v=POINT (10.0 20.0)}, {v=20}]", sc.script().params().toString());
    }

    public void testTranslateStDistanceToQuery() {
        String operator = randomFrom("<", "<=");
        LogicalPlan p = plan("SELECT shape FROM test WHERE ST_Distance(point, ST_WKTToSQL('point (10 20)')) " + operator + " 25");
        assertThat(p, instanceOf(Project.class));
        assertThat(p.children().get(0), instanceOf(Filter.class));
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertTrue(translation.query instanceof GeoDistanceQuery);
        GeoDistanceQuery gq = (GeoDistanceQuery) translation.query;
        assertEquals("point", gq.field());
        assertEquals(20.0, gq.lat(), 0.00001);
        assertEquals(10.0, gq.lon(), 0.00001);
        assertEquals(25.0, gq.distance(), 0.00001);
        EsQueryExec eqe = (EsQueryExec) optimizeAndPlan(p);
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""),
            containsString("{\"geo_distance\":{\"point\":[10.0,20.0],\"distance\":25.0," +
                "\"distance_type\":\"arc\",\"validation_method\":\"STRICT\","));
    }

    public void testTranslateStXY() {
        String dim = randomFrom("X", "Y");
        LogicalPlan p = plan("SELECT ST_AsWKT(point) FROM test WHERE ST_" + dim + "(point) = 10");
        assertThat(p, instanceOf(Project.class));
        assertThat(p.children().get(0), instanceOf(Filter.class));
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertThat(translation.query, instanceOf(ScriptQuery.class));
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.eq(InternalSqlScriptUtils.st" + dim + "(" +
                "InternalSqlScriptUtils.geoDocValue(doc,params.v0)),params.v1))",
            sc.script().toString());
        assertEquals("[{v=point}, {v=10}]", sc.script().params().toString());
    }

    public void testTranslateStGeometryType() {
        LogicalPlan p = plan("SELECT ST_AsWKT(point) FROM test WHERE ST_GEOMETRYTYPE(point) = 'POINT'");
        assertThat(p, instanceOf(Project.class));
        assertThat(p.children().get(0), instanceOf(Filter.class));
        Expression condition = ((Filter) p.children().get(0)).condition();
        assertFalse(condition.foldable());
        QueryTranslation translation = translate(condition);
        assertNull(translation.aggFilter);
        assertThat(translation.query, instanceOf(ScriptQuery.class));
        ScriptQuery sc = (ScriptQuery) translation.query;
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.eq(InternalSqlScriptUtils.stGeometryType(" +
                "InternalSqlScriptUtils.geoDocValue(doc,params.v0)),params.v1))",
            sc.script().toString());
        assertEquals("[{v=point}, {v=POINT}]", sc.script().params().toString());
    }

    public void testTranslateCoalesce_GroupBy_Painless() {
        LogicalPlan p = plan("SELECT COALESCE(int, 10) FROM test GROUP BY 1");
        assertTrue(p instanceof Aggregate);
        Expression condition = ((Aggregate) p).groupings().get(0);
        assertFalse(condition.foldable());
        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(((Aggregate) p).groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals(
                "InternalSqlScriptUtils.coalesce([InternalQlScriptUtils.docValue(doc,params.v0),params.v1])",
            scriptTemplate.toString());
        assertEquals("[{v=int}, {v=10}]", scriptTemplate.params().toString());
    }

    public void testTranslateNullIf_GroupBy_Painless() {
        LogicalPlan p = plan("SELECT NULLIF(int, 10) FROM test GROUP BY 1");
        assertTrue(p instanceof Aggregate);
        Expression condition = ((Aggregate) p).groupings().get(0);
        assertFalse(condition.foldable());
        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(((Aggregate) p).groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals(
                "InternalSqlScriptUtils.nullif(InternalQlScriptUtils.docValue(doc,params.v0),params.v1)",
            scriptTemplate.toString());
        assertEquals("[{v=int}, {v=10}]", scriptTemplate.params().toString());
    }

    public void testTranslateCase_GroupBy_Painless() {
        LogicalPlan p = plan("SELECT CASE WHEN int > 10 THEN 'foo' WHEN int > 20 THEN 'bar' ELSE 'default' END FROM test GROUP BY 1");
        assertTrue(p instanceof Aggregate);
        Expression condition = ((Aggregate) p).groupings().get(0);
        assertFalse(condition.foldable());
        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(((Aggregate) p).groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalQlScriptUtils.docValue(doc,params.v0)," +
                "params.v1)) ? params.v2 : InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalQlScriptUtils.docValue(" +
                "doc,params.v3),params.v4)) ? params.v5 : params.v6",
            scriptTemplate.toString());
        assertEquals("[{v=int}, {v=10}, {v=foo}, {v=int}, {v=20}, {v=bar}, {v=default}]", scriptTemplate.params().toString());
    }

    public void testTranslateIif_GroupBy_Painless() {
        LogicalPlan p = plan("SELECT IIF(int > 20, 'foo', 'bar') FROM test GROUP BY 1");
        assertTrue(p instanceof Aggregate);
        Expression condition = ((Aggregate) p).groupings().get(0);
        assertFalse(condition.foldable());
        GroupingContext groupingContext = QueryFolder.FoldAggregate.groupBy(((Aggregate) p).groupings());
        assertNotNull(groupingContext);
        ScriptTemplate scriptTemplate = groupingContext.tail.script();
        assertEquals("InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(InternalQlScriptUtils.docValue(doc,params.v0)," +
                "params.v1)) ? params.v2 : params.v3",
            scriptTemplate.toString());
        assertEquals("[{v=int}, {v=20}, {v=foo}, {v=bar}]", scriptTemplate.params().toString());
    }

    public void testGroupByDateHistogram() {
        LogicalPlan p = plan("SELECT MAX(int) FROM test GROUP BY HISTOGRAM(int, 1000)");
        assertTrue(p instanceof Aggregate);
        Aggregate a = (Aggregate) p;
        List<Expression> groupings = a.groupings();
        assertEquals(1, groupings.size());
        Expression exp = groupings.get(0);
        assertEquals(Histogram.class, exp.getClass());
        Histogram h = (Histogram) exp;
        assertEquals(1000, h.interval().fold());
        Expression field = h.field();
        assertEquals(FieldAttribute.class, field.getClass());
        assertEquals(INTEGER, field.dataType());
    }

    public void testGroupByHistogram() {
        LogicalPlan p = plan("SELECT MAX(int) FROM test GROUP BY HISTOGRAM(date, INTERVAL 2 YEARS)");
        assertTrue(p instanceof Aggregate);
        Aggregate a = (Aggregate) p;
        List<Expression> groupings = a.groupings();
        assertEquals(1, groupings.size());
        Expression exp = groupings.get(0);
        assertEquals(Histogram.class, exp.getClass());
        Histogram h = (Histogram) exp;
        assertEquals("+2-0", h.interval().fold().toString());
        Expression field = h.field();
        assertEquals(FieldAttribute.class, field.getClass());
        assertEquals(DATETIME, field.dataType());
    }

    public void testGroupByHistogramQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT MAX(int) FROM test GROUP BY HISTOGRAM(date, INTERVAL 2 YEARS)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("MAX(int)", eqe.output().get(0).qualifiedName());
        assertEquals(INTEGER, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
            containsString("\"date_histogram\":{\"field\":\"date\",\"missing_bucket\":true,\"order\":\"asc\","
                    + "\"fixed_interval\":\"62208000000ms\",\"time_zone\":\"Z\"}}}]}"));
    }

    public void testGroupByHistogramWithScalarsQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT MAX(int), HISTOGRAM(date, INTERVAL 5 YEARS - INTERVAL 6 MONTHS) AS h " +
                                         "FROM test GROUP BY h");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(2, eqe.output().size());
        assertEquals("MAX(int)", eqe.output().get(0).qualifiedName());
        assertEquals(INTEGER, eqe.output().get(0).dataType());
        assertEquals("h", eqe.output().get(1).qualifiedName());
        assertEquals(DATETIME, eqe.output().get(1).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                containsString("\"date_histogram\":{\"field\":\"date\",\"missing_bucket\":true," +
                        "\"order\":\"asc\",\"fixed_interval\":\"139968000000ms\",\"time_zone\":\"Z\"}}}]}"));
    }

    public void testGroupByYearQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT YEAR(date) FROM test GROUP BY YEAR(date)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("YEAR(date)", eqe.output().get(0).qualifiedName());
        assertEquals(INTEGER, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
            endsWith("\"date_histogram\":{\"field\":\"date\",\"missing_bucket\":true,\"order\":\"asc\","
                    + "\"calendar_interval\":\"1y\",\"time_zone\":\"Z\"}}}]}}}"));
    }

    public void testGroupByOneMonthHistogramQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT HISTOGRAM(date, INTERVAL 1 MONTH) AS h FROM test GROUP BY h");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("h", eqe.output().get(0).qualifiedName());
        assertEquals(DATETIME, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
            endsWith("\"date_histogram\":{\"field\":\"date\",\"missing_bucket\":true,\"order\":\"asc\","
                    + "\"calendar_interval\":\"1M\",\"time_zone\":\"Z\"}}}]}}}"));
    }

    public void testGroupByMoreMonthsHistogramQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT HISTOGRAM(date, INTERVAL 5 MONTH) AS h FROM test GROUP BY h");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("h", eqe.output().get(0).qualifiedName());
        assertEquals(DATETIME, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
            endsWith("\"date_histogram\":{\"field\":\"date\",\"missing_bucket\":true,\"order\":\"asc\","
                    + "\"fixed_interval\":\"12960000000ms\",\"time_zone\":\"Z\"}}}]}}}"));
    }

    public void testGroupByOneDayHistogramQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT HISTOGRAM(date, INTERVAL 1 DAY) AS h FROM test GROUP BY h");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("h", eqe.output().get(0).qualifiedName());
        assertEquals(DATETIME, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
            endsWith("\"date_histogram\":{\"field\":\"date\",\"missing_bucket\":true,\"order\":\"asc\","
                    + "\"calendar_interval\":\"1d\",\"time_zone\":\"Z\"}}}]}}}"));
    }

    public void testGroupByMoreDaysHistogramQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT HISTOGRAM(date, INTERVAL '1 5' DAY TO HOUR) AS h FROM test GROUP BY h");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("h", eqe.output().get(0).qualifiedName());
        assertEquals(DATETIME, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
            endsWith("\"date_histogram\":{\"field\":\"date\",\"missing_bucket\":true,\"order\":\"asc\","
                    + "\"fixed_interval\":\"104400000ms\",\"time_zone\":\"Z\"}}}]}}}"));
    }

    public void testGroupByMoreDaysHistogram_WithFunction_QueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT HISTOGRAM(date + INTERVAL 5 DAYS, INTERVAL 1 DAY) AS h FROM test GROUP BY h");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("h", eqe.output().get(0).qualifiedName());
        assertEquals(DATETIME, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"date_histogram\":{\"script\":{\"source\":\"InternalSqlScriptUtils.add(" +
                        "InternalQlScriptUtils.docValue(doc,params.v0),InternalSqlScriptUtils.intervalDayTime(params.v1,params.v2))\"," +
                        "\"lang\":\"painless\",\"params\":{\"v0\":\"date\",\"v1\":\"PT120H\",\"v2\":\"INTERVAL_DAY\"}}," +
                        "\"missing_bucket\":true,\"value_type\":\"long\",\"order\":\"asc\"," +
                        "\"calendar_interval\":\"1d\",\"time_zone\":\"Z\"}}}]}}}"));
    }

    public void testGroupByYearAndScalarsQueryTranslator() {
        PhysicalPlan p = optimizeAndPlan("SELECT YEAR(CAST(date + INTERVAL 5 months AS DATE)) FROM test GROUP BY 1");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("YEAR(CAST(date + INTERVAL 5 months AS DATE))", eqe.output().get(0).qualifiedName());
        assertEquals(INTEGER, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"date_histogram\":{\"script\":{\"source\":\"InternalSqlScriptUtils.cast(" +
                        "InternalSqlScriptUtils.add(InternalQlScriptUtils.docValue(doc,params.v0)," +
                        "InternalSqlScriptUtils.intervalYearMonth(params.v1,params.v2)),params.v3)\"," +
                        "\"lang\":\"painless\",\"params\":{\"v0\":\"date\",\"v1\":\"P5M\",\"v2\":\"INTERVAL_MONTH\"," +
                        "\"v3\":\"DATE\"}},\"missing_bucket\":true,\"value_type\":\"long\",\"order\":\"asc\"," +
                        "\"calendar_interval\":\"1y\",\"time_zone\":\"Z\"}}}]}}}"));
    }

    public void testOrderByYear() {
        PhysicalPlan p = optimizeAndPlan("SELECT YEAR(date) FROM test ORDER BY 1");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.output().size());
        assertEquals("YEAR(date)", eqe.output().get(0).qualifiedName());
        assertEquals(INTEGER, eqe.output().get(0).dataType());
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""),
                endsWith("\"sort\":[{\"_script\":{\"script\":{\"source\":\"InternalQlScriptUtils.nullSafeSortNumeric("
                        +
                        "InternalSqlScriptUtils.dateTimeExtract(InternalQlScriptUtils.docValue(doc,params.v0)," +
                        "params.v1,params.v2))\",\"lang\":\"painless\",\"params\":{\"v0\":\"date\",\"v1\":\"Z\"," +
                        "\"v2\":\"YEAR\"}},\"type\":\"number\",\"order\":\"asc\"}}]}"));
    }

    public void testGroupByHistogramWithDate() {
        LogicalPlan p = plan("SELECT MAX(int) FROM test GROUP BY HISTOGRAM(CAST(date AS DATE), INTERVAL 2 MONTHS)");
        assertTrue(p instanceof Aggregate);
        Aggregate a = (Aggregate) p;
        List<Expression> groupings = a.groupings();
        assertEquals(1, groupings.size());
        Expression exp = groupings.get(0);
        assertEquals(Histogram.class, exp.getClass());
        Histogram h = (Histogram) exp;
        assertEquals("+0-2", h.interval().fold().toString());
        Expression field = h.field();
        assertEquals(Cast.class, field.getClass());
        assertEquals(DATE, field.dataType());
    }

    public void testGroupByHistogramWithDateAndSmallInterval() {
        PhysicalPlan p = optimizeAndPlan("SELECT MAX(int) FROM test GROUP BY " +
            "HISTOGRAM(CAST(date AS DATE), INTERVAL 5 MINUTES)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(1, eqe.queryContainer().aggs().groups().size());
        assertEquals(GroupByDateHistogram.class, eqe.queryContainer().aggs().groups().get(0).getClass());
        assertEquals(86400000L, ((GroupByDateHistogram) eqe.queryContainer().aggs().groups().get(0)).fixedInterval());
    }

    public void testGroupByHistogramWithDateTruncateIntervalToDayMultiples() {
        {
            PhysicalPlan p = optimizeAndPlan("SELECT MAX(int) FROM test GROUP BY " +
                "HISTOGRAM(CAST(date AS DATE), INTERVAL '2 3:04' DAY TO MINUTE)");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.queryContainer().aggs().groups().size());
            assertEquals(GroupByDateHistogram.class, eqe.queryContainer().aggs().groups().get(0).getClass());
            assertEquals(172800000L, ((GroupByDateHistogram) eqe.queryContainer().aggs().groups().get(0)).fixedInterval());
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT MAX(int) FROM test GROUP BY " +
                "HISTOGRAM(CAST(date AS DATE), INTERVAL 4409 MINUTES)");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.queryContainer().aggs().groups().size());
            assertEquals(GroupByDateHistogram.class, eqe.queryContainer().aggs().groups().get(0).getClass());
            assertEquals(259200000L, ((GroupByDateHistogram) eqe.queryContainer().aggs().groups().get(0)).fixedInterval());
        }
    }

    public void testCountAndCountDistinctFolding() {
        PhysicalPlan p = optimizeAndPlan("SELECT COUNT(DISTINCT keyword) dkey, COUNT(keyword) key FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec ee = (EsQueryExec) p;
        assertEquals(2, ee.output().size());
        assertThat(ee.output().get(0).toString(), startsWith("dkey{r}"));
        assertThat(ee.output().get(1).toString(), startsWith("key{r}"));

        Collection<AggregationBuilder> subAggs = ee.queryContainer().aggs().asAggBuilder().getSubAggregations();
        assertEquals(2, subAggs.size());
        assertTrue(subAggs.toArray()[0] instanceof CardinalityAggregationBuilder);
        assertTrue(subAggs.toArray()[1] instanceof FilterAggregationBuilder);

        CardinalityAggregationBuilder cardinalityKeyword = (CardinalityAggregationBuilder) subAggs.toArray()[0];
        assertEquals("keyword", cardinalityKeyword.field());

        FilterAggregationBuilder existsKeyword = (FilterAggregationBuilder) subAggs.toArray()[1];
        assertTrue(existsKeyword.getFilter() instanceof ExistsQueryBuilder);
        assertEquals("keyword", ((ExistsQueryBuilder) existsKeyword.getFilter()).fieldName());

        assertThat(ee.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("{\"filter\":{\"exists\":{\"field\":\"keyword\",\"boost\":1.0}}}}}}"));
    }

    public void testAllCountVariantsWithHavingGenerateCorrectAggregations() {
        PhysicalPlan p = optimizeAndPlan("SELECT AVG(int), COUNT(keyword) ln, COUNT(distinct keyword) dln, COUNT(some.dotted.field) fn,"
                + "COUNT(distinct some.dotted.field) dfn, COUNT(*) ccc FROM test GROUP BY bool "
                + "HAVING dln > 3 AND ln > 32 AND dfn > 1 AND fn > 2 AND ccc > 5 AND AVG(int) > 50000");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec ee = (EsQueryExec) p;
        assertEquals(6, ee.output().size());
        assertThat(ee.output().get(0).toString(), startsWith("AVG(int){r}"));
        assertThat(ee.output().get(1).toString(), startsWith("ln{r}"));
        assertThat(ee.output().get(2).toString(), startsWith("dln{r}"));
        assertThat(ee.output().get(3).toString(), startsWith("fn{r}"));
        assertThat(ee.output().get(4).toString(), startsWith("dfn{r}"));
        assertThat(ee.output().get(5).toString(), startsWith("ccc{r}"));

        Collection<AggregationBuilder> subAggs = ee.queryContainer().aggs().asAggBuilder().getSubAggregations();
        assertEquals(5, subAggs.size());
        assertTrue(subAggs.toArray()[0] instanceof AvgAggregationBuilder);
        assertTrue(subAggs.toArray()[1] instanceof FilterAggregationBuilder);
        assertTrue(subAggs.toArray()[2] instanceof CardinalityAggregationBuilder);
        assertTrue(subAggs.toArray()[3] instanceof FilterAggregationBuilder);
        assertTrue(subAggs.toArray()[4] instanceof CardinalityAggregationBuilder);

        AvgAggregationBuilder avgInt = (AvgAggregationBuilder) subAggs.toArray()[0];
        assertEquals("int", avgInt.field());

        FilterAggregationBuilder existsKeyword = (FilterAggregationBuilder) subAggs.toArray()[1];
        assertTrue(existsKeyword.getFilter() instanceof ExistsQueryBuilder);
        assertEquals("keyword", ((ExistsQueryBuilder) existsKeyword.getFilter()).fieldName());

        CardinalityAggregationBuilder cardinalityKeyword = (CardinalityAggregationBuilder) subAggs.toArray()[2];
        assertEquals("keyword", cardinalityKeyword.field());

        FilterAggregationBuilder existsDottedField = (FilterAggregationBuilder) subAggs.toArray()[3];
        assertTrue(existsDottedField.getFilter() instanceof ExistsQueryBuilder);
        assertEquals("some.dotted.field", ((ExistsQueryBuilder) existsDottedField.getFilter()).fieldName());

        CardinalityAggregationBuilder cardinalityDottedField = (CardinalityAggregationBuilder) subAggs.toArray()[4];
        assertEquals("some.dotted.field", cardinalityDottedField.field());

        assertThat(ee.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("{\"buckets_path\":{"
                        + "\"a0\":\"" + cardinalityKeyword.getName() + "\","
                        + "\"a1\":\"" + existsKeyword.getName() + "._count\","
                        + "\"a2\":\"" + cardinalityDottedField.getName() + "\","
                        + "\"a3\":\"" + existsDottedField.getName() + "._count\","
                        + "\"a4\":\"_count\","
                        + "\"a5\":\"" + avgInt.getName() + "\"},"
                        + "\"script\":{\"source\":\""
                        + "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.and("
                        +   "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.and("
                        +     "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.and("
                        +       "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.and("
                        +         "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.and("
                        +           "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(params.a0,params.v0)),"
                        +           "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(params.a1,params.v1)))),"
                        +         "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(params.a2,params.v2)))),"
                        +       "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(params.a3,params.v3)))),"
                        +     "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(params.a4,params.v4)))),"
                        +   "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(params.a5,params.v5))))\","
                        + "\"lang\":\"painless\",\"params\":{\"v0\":3,\"v1\":32,\"v2\":1,\"v3\":2,\"v4\":5,\"v5\":50000}},"
                        + "\"gap_policy\":\"skip\"}}}}}"));
    }

    public void testGroupByCastScalar() {
        PhysicalPlan p = optimizeAndPlan("SELECT CAST(ABS(EXTRACT(YEAR FROM date)) AS BIGINT) FROM test " +
            "GROUP BY CAST(ABS(EXTRACT(YEAR FROM date)) AS BIGINT) ORDER BY CAST(ABS(EXTRACT(YEAR FROM date)) AS BIGINT) NULLS FIRST");
        assertEquals(EsQueryExec.class, p.getClass());
        assertEquals(1, p.output().size());
        assertEquals("CAST(ABS(EXTRACT(YEAR FROM date)) AS BIGINT)", p.output().get(0).qualifiedName());
        assertEquals(LONG, p.output().get(0).dataType());
        assertThat(
            ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                .replaceAll("\\s+", ""),
            endsWith("{\"source\":\"InternalSqlScriptUtils.cast(InternalSqlScriptUtils.abs(InternalSqlScriptUtils.dateTimeExtract" +
                        "(InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)),params.v3)\",\"lang\":\"painless\"," +
                "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"YEAR\",\"v3\":\"LONG\"}},\"missing_bucket\":true," +
                "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
        );
    }

    public void testGroupByCastScalarWithAlias() {
        PhysicalPlan p = optimizeAndPlan("SELECT CAST(ABS(EXTRACT(YEAR FROM date)) AS BIGINT) as \"cast\"  FROM test " +
            "GROUP BY \"cast\" ORDER BY \"cast\" NULLS FIRST");
        assertEquals(EsQueryExec.class, p.getClass());
        assertEquals(1, p.output().size());
        assertEquals("cast", p.output().get(0).qualifiedName());
        assertEquals(LONG, p.output().get(0).dataType());
        assertThat(
            ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                .replaceAll("\\s+", ""),
            endsWith("{\"source\":\"InternalSqlScriptUtils.cast(InternalSqlScriptUtils.abs(InternalSqlScriptUtils.dateTimeExtract" +
                        "(InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)),params.v3)\",\"lang\":\"painless\"," +
                "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"YEAR\",\"v3\":\"LONG\"}},\"missing_bucket\":true," +
                "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
        );
    }

    public void testGroupByCastScalarWithNumericRef() {
        PhysicalPlan p = optimizeAndPlan("SELECT CAST(ABS(EXTRACT(YEAR FROM date)) AS BIGINT) FROM test " +
            "GROUP BY 1 ORDER BY 1 NULLS FIRST");
        assertEquals(EsQueryExec.class, p.getClass());
        assertEquals(1, p.output().size());
        assertEquals("CAST(ABS(EXTRACT(YEAR FROM date)) AS BIGINT)", p.output().get(0).qualifiedName());
        assertEquals(LONG, p.output().get(0).dataType());
        assertThat(
            ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                .replaceAll("\\s+", ""),
            endsWith("{\"source\":\"InternalSqlScriptUtils.cast(InternalSqlScriptUtils.abs(InternalSqlScriptUtils.dateTimeExtract" +
                        "(InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)),params.v3)\",\"lang\":\"painless\"," +
                "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"YEAR\",\"v3\":\"LONG\"}},\"missing_bucket\":true," +
                "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
        );
    }

    public void testGroupByConvertScalar() {
        {
            PhysicalPlan p = optimizeAndPlan("SELECT CONVERT(ABS(EXTRACT(YEAR FROM date)), SQL_BIGINT) FROM test " +
                "GROUP BY CONVERT(ABS(EXTRACT(YEAR FROM date)), SQL_BIGINT) ORDER BY CONVERT(ABS(EXTRACT(YEAR FROM date)), SQL_BIGINT) " +
                "NULLS FIRST");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("CONVERT(ABS(EXTRACT(YEAR FROM date)), SQL_BIGINT)", p.output().get(0).qualifiedName());
            assertEquals(LONG, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                endsWith("{\"source\":\"InternalSqlScriptUtils.cast(InternalSqlScriptUtils.abs(InternalSqlScriptUtils.dateTimeExtract" +
                            "(InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)),params.v3)\",\"lang\":\"painless\"," +
                    "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"YEAR\",\"v3\":\"LONG\"}},\"missing_bucket\":true," +
                    "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
            );
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT EXTRACT(HOUR FROM CONVERT(date, SQL_TIMESTAMP)) FROM test GROUP BY " +
                "EXTRACT(HOUR FROM CONVERT(date, SQL_TIMESTAMP))");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("EXTRACT(HOUR FROM CONVERT(date, SQL_TIMESTAMP))", p.output().get(0).qualifiedName());
            assertEquals(INTEGER, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                endsWith("{\"source\":\"InternalSqlScriptUtils.dateTimeExtract(" +
                            "InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)\",\"lang\":\"painless\"," +
                    "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"HOUR_OF_DAY\"}},\"missing_bucket\":true," +
                    "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
            );
        }
    }

    public void testGroupByConvertScalarWithAlias() {
        {
            PhysicalPlan p = optimizeAndPlan("SELECT CONVERT(ABS(EXTRACT(YEAR FROM date)), SQL_BIGINT) as \"convert\" FROM test " +
                "GROUP BY \"convert\" ORDER BY \"convert\" NULLS FIRST");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("convert", p.output().get(0).qualifiedName());
            assertEquals(LONG, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                endsWith("{\"source\":\"InternalSqlScriptUtils.cast(InternalSqlScriptUtils.abs(InternalSqlScriptUtils.dateTimeExtract" +
                            "(InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)),params.v3)\",\"lang\":\"painless\"," +
                    "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"YEAR\",\"v3\":\"LONG\"}},\"missing_bucket\":true," +
                    "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
            );
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT EXTRACT(MINUTE FROM CONVERT(date, SQL_TIMESTAMP)) x FROM test GROUP BY x");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("x", p.output().get(0).qualifiedName());
            assertEquals(INTEGER, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                endsWith("{\"source\":\"InternalSqlScriptUtils.dateTimeExtract(" +
                            "InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)\",\"lang\":\"painless\"," +
                    "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"MINUTE_OF_HOUR\"}}," +
                    "\"missing_bucket\":true,\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
            );
        }
    }

    public void testGroupByConvertScalarWithNumericRef() {
        PhysicalPlan p = optimizeAndPlan("SELECT CONVERT(ABS(EXTRACT(YEAR FROM date)), SQL_BIGINT) FROM test " +
            "GROUP BY 1 ORDER BY 1 NULLS FIRST");
        assertEquals(EsQueryExec.class, p.getClass());
        assertEquals(1, p.output().size());
        assertEquals("CONVERT(ABS(EXTRACT(YEAR FROM date)), SQL_BIGINT)", p.output().get(0).qualifiedName());
        assertEquals(LONG, p.output().get(0).dataType());
        assertThat(
            ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                .replaceAll("\\s+", ""),
            endsWith("{\"source\":\"InternalSqlScriptUtils.cast(InternalSqlScriptUtils.abs(InternalSqlScriptUtils.dateTimeExtract" +
                        "(InternalQlScriptUtils.docValue(doc,params.v0),params.v1,params.v2)),params.v3)\",\"lang\":\"painless\"," +
                "\"params\":{\"v0\":\"date\",\"v1\":\"Z\",\"v2\":\"YEAR\",\"v3\":\"LONG\"}},\"missing_bucket\":true," +
                "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
        );
    }

    public void testGroupByConstantScalar() {
        PhysicalPlan p = optimizeAndPlan("SELECT PI() * int FROM test WHERE PI() * int > 5.0 GROUP BY PI() * int " +
            "ORDER BY PI() * int LIMIT 10");
        assertEquals(EsQueryExec.class, p.getClass());
        assertEquals(1, p.output().size());
        assertEquals("PI() * int", p.output().get(0).qualifiedName());
        assertEquals(DOUBLE, p.output().get(0).dataType());
        assertThat(
            ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                .replaceAll("\\s+", ""),
                endsWith("{\"script\":{\"source\":\"InternalSqlScriptUtils.mul(InternalQlScriptUtils.docValue(doc,params.v0),params.v1)\","
                        +
                "\"lang\":\"painless\",\"params\":{\"v0\":\"int\",\"v1\":3.141592653589793}},\"missing_bucket\":true," +
                "\"value_type\":\"double\",\"order\":\"asc\"}}}]}}}")
        );
    }


    public void testGroupByConstantScalarWithAlias() {
        {
            PhysicalPlan p = optimizeAndPlan("SELECT PI() * int AS \"value\"  FROM test GROUP BY \"value\" ORDER BY \"value\" LIMIT 10");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("value", p.output().get(0).qualifiedName());
            assertEquals(DOUBLE, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                    endsWith("{\"script\":{\"source\":\"InternalSqlScriptUtils.mul(InternalQlScriptUtils.docValue(doc,params.v0),params.v1)"
                            +
                    "\",\"lang\":\"painless\",\"params\":{\"v0\":\"int\",\"v1\":3.141592653589793}},\"missing_bucket\":true," +
                    "\"value_type\":\"double\",\"order\":\"asc\"}}}]}}}")
            );
        }
        {
            PhysicalPlan p = optimizeAndPlan("select (3 < int) as multi_language, count(*) from test group by multi_language");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(2, p.output().size());
            assertEquals("multi_language", p.output().get(0).qualifiedName());
            assertEquals(BOOLEAN, p.output().get(0).dataType());
            assertEquals("count(*)", p.output().get(1).qualifiedName());
            assertEquals(LONG, p.output().get(1).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                    endsWith("{\"source\":\"InternalQlScriptUtils.gt(InternalQlScriptUtils.docValue(doc,params.v0),params.v1)\"," +
                    "\"lang\":\"painless\",\"params\":{\"v0\":\"int\",\"v1\":3}}," +
                    "\"missing_bucket\":true,\"value_type\":\"boolean\",\"order\":\"asc\"}}}]}}}")
            );
        }
    }


    public void testGroupByConstantScalarWithNumericRef() {
        {
            PhysicalPlan p = optimizeAndPlan("SELECT PI() * int FROM test GROUP BY 1 ORDER BY 1 LIMIT 10");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("PI() * int", p.output().get(0).qualifiedName());
            assertEquals(DOUBLE, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                    endsWith("{\"script\":{\"source\":\"InternalSqlScriptUtils.mul(InternalQlScriptUtils.docValue(doc,params.v0),params.v1)"
                            +
                    "\",\"lang\":\"painless\",\"params\":{\"v0\":\"int\",\"v1\":3.141592653589793}},\"missing_bucket\":true," +
                    "\"value_type\":\"double\",\"order\":\"asc\"}}}]}}}")
            );
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT PI() * int FROM test GROUP BY 1");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("PI() * int", p.output().get(0).qualifiedName());
            assertEquals(DOUBLE, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                    endsWith("{\"source\":\"InternalSqlScriptUtils.mul(InternalQlScriptUtils.docValue(doc,params.v0),params.v1)\"," +
                    "\"lang\":\"painless\",\"params\":{\"v0\":\"int\",\"v1\":3.141592653589793}}," +
                    "\"missing_bucket\":true,\"value_type\":\"double\",\"order\":\"asc\"}}}]}}}")
            );
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT date + 1 * INTERVAL '1' DAY FROM test GROUP BY 1");
            assertEquals(EsQueryExec.class, p.getClass());
            assertEquals(1, p.output().size());
            assertEquals("date + 1 * INTERVAL '1' DAY", p.output().get(0).qualifiedName());
            assertEquals(DATETIME, p.output().get(0).dataType());
            assertThat(
                ((EsQueryExec) p).queryContainer().aggs().asAggBuilder().toString()
                    .replaceAll("\\s+", ""),
                    endsWith("{\"source\":\"InternalSqlScriptUtils.add(InternalQlScriptUtils.docValue(doc,params.v0)," +
                            "InternalSqlScriptUtils.intervalDayTime(params.v1,params.v2))\"," +
                    "\"lang\":\"painless\",\"params\":{\"v0\":\"date\",\"v1\":\"PT24H\",\"v2\":\"INTERVAL_DAY\"}}," +
                    "\"missing_bucket\":true,\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}")
            );
        }
    }

    public void testOrderByWithCastWithMissingRefs() {
        PhysicalPlan p = optimizeAndPlan("SELECT keyword FROM test ORDER BY date::TIME, int LIMIT 5");
        assertEquals(EsQueryExec.class, p.getClass());
        assertEquals(1, p.output().size());
        assertEquals("test.keyword", p.output().get(0).qualifiedName());
        assertEquals(KEYWORD, p.output().get(0).dataType());
        assertThat(
            ((EsQueryExec) p).queryContainer().toString()
                .replaceAll("\\s+", ""),
            endsWith("\"sort\":[{\"_script\":{\"script\":{\"source\":\"InternalQlScriptUtils.nullSafeSortString(InternalSqlScriptUtils" +
                        ".cast(InternalQlScriptUtils.docValue(doc,params.v0),params.v1))\",\"lang\":\"painless\"," +
                "\"params\":{\"v0\":\"date\",\"v1\":\"TIME\"}},\"type\":\"string\",\"order\":\"asc\"}},{\"int\":{\"order\":\"asc\"," +
                "\"missing\":\"_last\",\"unmapped_type\":\"integer\"}}]}")
        );
    }

    public void testTopHitsAggregationWithOneArg() {
        {
            PhysicalPlan p = optimizeAndPlan("SELECT FIRST(keyword) FROM test");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.output().size());
            assertEquals("FIRST(keyword)", eqe.output().get(0).qualifiedName());
            assertEquals(KEYWORD, eqe.output().get(0).dataType());
            assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"top_hits\":{\"from\":0,\"size\":1,\"version\":false,\"seq_no_primary_term\":false," +
                        "\"explain\":false,\"docvalue_fields\":[{\"field\":\"keyword\"}]," +
                        "\"sort\":[{\"keyword\":{\"order\":\"asc\",\"missing\":\"_last\",\"unmapped_type\":\"keyword\"}}]}}}}}"));
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT MIN(keyword) FROM test");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.output().size());
            assertEquals("MIN(keyword)", eqe.output().get(0).qualifiedName());
            assertEquals(KEYWORD, eqe.output().get(0).dataType());
            assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"top_hits\":{\"from\":0,\"size\":1,\"version\":false,\"seq_no_primary_term\":false," +
                    "\"explain\":false,\"docvalue_fields\":[{\"field\":\"keyword\"}]," +
                    "\"sort\":[{\"keyword\":{\"order\":\"asc\",\"missing\":\"_last\",\"unmapped_type\":\"keyword\"}}]}}}}}"));
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT LAST(date) FROM test");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.output().size());
            assertEquals("LAST(date)", eqe.output().get(0).qualifiedName());
            assertEquals(DATETIME, eqe.output().get(0).dataType());
            assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"top_hits\":{\"from\":0,\"size\":1,\"version\":false,\"seq_no_primary_term\":false," +
                    "\"explain\":false,\"docvalue_fields\":[{\"field\":\"date\",\"format\":\"strict_date_optional_time_nanos\"}]," +
                    "\"sort\":[{\"date\":{\"order\":\"desc\",\"missing\":\"_last\",\"unmapped_type\":\"date\"}}]}}}}}"));
        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT MAX(keyword) FROM test");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.output().size());
            assertEquals("MAX(keyword)", eqe.output().get(0).qualifiedName());
            assertEquals(KEYWORD, eqe.output().get(0).dataType());
            assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"top_hits\":{\"from\":0,\"size\":1,\"version\":false,\"seq_no_primary_term\":false," +
                    "\"explain\":false,\"docvalue_fields\":[{\"field\":\"keyword\"}]," +
                    "\"sort\":[{\"keyword\":{\"order\":\"desc\",\"missing\":\"_last\",\"unmapped_type\":\"keyword\"}}]}}}}}"));
        }
    }

    public void testTopHitsAggregationWithTwoArgs() {
        {
            PhysicalPlan p = optimizeAndPlan("SELECT FIRST(keyword, int) FROM test");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.output().size());
            assertEquals("FIRST(keyword, int)", eqe.output().get(0).qualifiedName());
            assertEquals(KEYWORD, eqe.output().get(0).dataType());
            assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"top_hits\":{\"from\":0,\"size\":1,\"version\":false,\"seq_no_primary_term\":false," +
                    "\"explain\":false,\"docvalue_fields\":[{\"field\":\"keyword\"}]," +
                    "\"sort\":[{\"int\":{\"order\":\"asc\",\"missing\":\"_last\",\"unmapped_type\":\"integer\"}}," +
                    "{\"keyword\":{\"order\":\"asc\",\"missing\":\"_last\",\"unmapped_type\":\"keyword\"}}]}}}}}"));

        }
        {
            PhysicalPlan p = optimizeAndPlan("SELECT LAST(date, int) FROM test");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.output().size());
            assertEquals("LAST(date, int)", eqe.output().get(0).qualifiedName());
            assertEquals(DATETIME, eqe.output().get(0).dataType());
            assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"top_hits\":{\"from\":0,\"size\":1,\"version\":false,\"seq_no_primary_term\":false," +
                    "\"explain\":false,\"docvalue_fields\":[{\"field\":\"date\",\"format\":\"strict_date_optional_time_nanos\"}]," +
                    "\"sort\":[{\"int\":{\"order\":\"desc\",\"missing\":\"_last\",\"unmapped_type\":\"integer\"}}," +
                    "{\"date\":{\"order\":\"desc\",\"missing\":\"_last\",\"unmapped_type\":\"date\"}}]}}}}}"));
        }
    }

    public void testExtendedStatsAggsStddevAndVar() {
        final Map<String, String> metricToAgg =  Map.of(
            "STDDEV_POP", "std_deviation",
            "STDDEV_SAMP", "std_deviation_sampling",
            "VAR_POP", "variance",
            "VAR_SAMP", "variance_sampling"
        );
        for (String funcName: metricToAgg.keySet()) {
            PhysicalPlan p = optimizeAndPlan("SELECT " + funcName + "(int) FROM test");
            assertEquals(EsQueryExec.class, p.getClass());
            EsQueryExec eqe = (EsQueryExec) p;
            assertEquals(1, eqe.output().size());

            assertEquals(funcName + "(int)", eqe.output().get(0).qualifiedName());
            assertEquals(DOUBLE, eqe.output().get(0).dataType());

            FieldExtraction fe = eqe.queryContainer().fields().get(0).v1();
            assertEquals(MetricAggRef.class, fe.getClass());
            assertEquals(((MetricAggRef) fe).property(), metricToAgg.get(funcName));

            String aggName = eqe.queryContainer().aggs().asAggBuilder().getSubAggregations().iterator().next().getName();
            assertThat(eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                endsWith("\"aggregations\":{\"" + aggName + "\":{\"extended_stats\":{\"field\":\"int\",\"sigma\":2.0}}}}}"));
        }

    }

    public void testGlobalCountInImplicitGroupByForcesTrackHits() {
        PhysicalPlan p = optimizeAndPlan("SELECT COUNT(*) FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertTrue("Should be tracking hits", eqe.queryContainer().shouldTrackHits());
    }

    public void testGlobalCountAllInImplicitGroupByForcesTrackHits() {
        PhysicalPlan p = optimizeAndPlan("SELECT COUNT(ALL *) FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertTrue("Should be tracking hits", eqe.queryContainer().shouldTrackHits());
    }

    public void testGlobalCountInSpecificGroupByDoesNotForceTrackHits() {
        PhysicalPlan p = optimizeAndPlan("SELECT COUNT(*) FROM test GROUP BY int");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertFalse("Should NOT be tracking hits", eqe.queryContainer().shouldTrackHits());
    }

    public void testFieldAllCountDoesNotTrackHits() {
        PhysicalPlan p = optimizeAndPlan("SELECT COUNT(ALL int) FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertFalse("Should NOT be tracking hits", eqe.queryContainer().shouldTrackHits());
    }

    public void testFieldCountDoesNotTrackHits() {
        PhysicalPlan p = optimizeAndPlan("SELECT COUNT(int) FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertFalse("Should NOT be tracking hits", eqe.queryContainer().shouldTrackHits());
    }

    public void testDistinctCountDoesNotTrackHits() {
        PhysicalPlan p = optimizeAndPlan("SELECT COUNT(DISTINCT int) FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertFalse("Should NOT be tracking hits", eqe.queryContainer().shouldTrackHits());
    }

    public void testNoCountDoesNotTrackHits() {
        PhysicalPlan p = optimizeAndPlan("SELECT int FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertFalse("Should NOT be tracking hits", eqe.queryContainer().shouldTrackHits());
    }

    public void testZonedDateTimeInScripts() {
        PhysicalPlan p = optimizeAndPlan(
                "SELECT date FROM test WHERE date - INTERVAL 999999999 YEAR > CAST('2019-03-11T12:34:56.000Z' AS DATETIME)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString(
                "\"script\":{\"script\":{\"source\":\"InternalQlScriptUtils.nullSafeFilter("
                        + "InternalQlScriptUtils.gt(InternalSqlScriptUtils.sub(InternalQlScriptUtils.docValue(doc,params.v0),"
                        + "InternalSqlScriptUtils.intervalYearMonth(params.v1,params.v2)),InternalSqlScriptUtils.asDateTime(params.v3)))\","
                + "\"lang\":\"painless\","
                + "\"params\":{\"v0\":\"date\",\"v1\":\"P999999999Y\",\"v2\":\"INTERVAL_YEAR\",\"v3\":\"2019-03-11T12:34:56.000Z\"}},"));
    }

    public void testChronoFieldBasedDateTimeFunctionsWithMathIntervalAndGroupBy() {
        DateTimeExtractor randomFunction = randomValueOtherThan(DateTimeExtractor.YEAR, () -> randomFrom(DateTimeExtractor.values()));
        PhysicalPlan p = optimizeAndPlan(
                "SELECT "
                + randomFunction.name()
                + "(date + INTERVAL 1 YEAR) FROM test GROUP BY " + randomFunction.name() + "(date + INTERVAL 1 YEAR)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString(
                "{\"terms\":{\"script\":{\"source\":\"InternalSqlScriptUtils.dateTimeExtract("
                        + "InternalSqlScriptUtils.add(InternalQlScriptUtils.docValue(doc,params.v0),"
                        + "InternalSqlScriptUtils.intervalYearMonth(params.v1,params.v2)),params.v3,params.v4)\","
                + "\"lang\":\"painless\",\"params\":{\"v0\":\"date\",\"v1\":\"P1Y\",\"v2\":\"INTERVAL_YEAR\","
                + "\"v3\":\"Z\",\"v4\":\"" + randomFunction.name() + "\"}},\"missing_bucket\":true,"
                + "\"value_type\":\"long\",\"order\":\"asc\"}}}]}}}}"));
    }

    public void testDateTimeFunctionsWithMathIntervalAndGroupBy() {
        String[] functions = new String[] {"DAY_NAME", "MONTH_NAME", "DAY_OF_WEEK", "WEEK_OF_YEAR", "QUARTER"};
        String[] scriptMethods = new String[] {"dayName", "monthName", "dayOfWeek", "weekOfYear", "quarter"};
        int pos = randomIntBetween(0, functions.length - 1);
        PhysicalPlan p = optimizeAndPlan(
                "SELECT "
                + functions[pos]
                + "(date + INTERVAL 1 YEAR) FROM test GROUP BY " + functions[pos] + "(date + INTERVAL 1 YEAR)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString(
                "{\"terms\":{\"script\":{\"source\":\"InternalSqlScriptUtils." + scriptMethods[pos]
                        + "(InternalSqlScriptUtils.add(InternalQlScriptUtils.docValue(doc,params.v0),"
                        + "InternalSqlScriptUtils.intervalYearMonth(params.v1,params.v2)),params.v3)\",\"lang\":\"painless\","
                + "\"params\":{\"v0\":\"date\",\"v1\":\"P1Y\",\"v2\":\"INTERVAL_YEAR\",\"v3\":\"Z\"}},\"missing_bucket\":true,"));
    }


    public void testHavingWithLiteralImplicitGrouping() {
        PhysicalPlan p = optimizeAndPlan("SELECT 1 FROM test HAVING COUNT(*) > 0");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertTrue("Should be tracking hits", eqe.queryContainer().shouldTrackHits());
        assertEquals(1, eqe.output().size());
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString("\"size\":0"));
    }

    public void testHavingWithColumnImplicitGrouping() {
        PhysicalPlan p = optimizeAndPlan("SELECT MAX(int) FROM test HAVING COUNT(*) > 0");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertTrue("Should be tracking hits", eqe.queryContainer().shouldTrackHits());
        assertEquals(1, eqe.output().size());
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString(
                "\"script\":{\"source\":\"InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(params.a0,params.v0))\","
                + "\"lang\":\"painless\",\"params\":{\"v0\":0}}"));
    }

    public void testScriptsInsideAggregateFunctions() {
        for (FunctionDefinition fd : defaultTestContext.sqlFunctionRegistry.listFunctions()) {
            if (AggregateFunction.class.isAssignableFrom(fd.clazz()) && (MatrixStatsEnclosed.class.isAssignableFrom(fd.clazz()) == false)) {
                String aggFunction = fd.name() + "(ABS((int * 10) / 3) + 1";
                if (fd.clazz() == Percentile.class || fd.clazz() == PercentileRank.class) {
                    aggFunction += ", 50";
                }
                aggFunction += ")";
                PhysicalPlan p = optimizeAndPlan("SELECT " + aggFunction + " FROM test");
                assertEquals(EsQueryExec.class, p.getClass());
                EsQueryExec eqe = (EsQueryExec) p;
                assertEquals(1, eqe.output().size());
                assertEquals(aggFunction, eqe.output().get(0).qualifiedName());
                if (fd.clazz() == Count.class) {
                    assertThat(
                        eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                        containsString(
                            ":{\"script\":{\"source\":\"InternalQlScriptUtils.isNotNull(InternalSqlScriptUtils.add("
                                + "InternalSqlScriptUtils.abs(InternalSqlScriptUtils.div(InternalSqlScriptUtils.mul("
                                + "InternalQlScriptUtils.docValue(doc,params.v0),params.v1),params.v2)),params.v3))\","
                                + "\"lang\":\"painless\",\"params\":{\"v0\":\"int\",\"v1\":10,\"v2\":3,\"v3\":1}}"
                        )
                    );
                } else {
                    assertThat(
                        eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                        containsString(
                            ":{\"script\":{\"source\":\"InternalSqlScriptUtils.add(InternalSqlScriptUtils.abs("
                                + "InternalSqlScriptUtils.div(InternalSqlScriptUtils.mul(InternalQlScriptUtils.docValue("
                                + "doc,params.v0),params.v1),params.v2)),params.v3)\",\"lang\":\"painless\",\"params\":{"
                                + "\"v0\":\"int\",\"v1\":10,\"v2\":3,\"v3\":1}}"
                        )
                    );
                }
            }
        }
    }

    public void testScriptsInsideAggregateFunctions_WithHaving() {
        for (FunctionDefinition fd : defaultTestContext.sqlFunctionRegistry.listFunctions()) {
            if (AggregateFunction.class.isAssignableFrom(fd.clazz())
                    && (MatrixStatsEnclosed.class.isAssignableFrom(fd.clazz()) == false)
                    // First/Last don't support having: https://github.com/elastic/elasticsearch/issues/37938
                    && (TopHits.class.isAssignableFrom(fd.clazz()) == false)) {
                String aggFunction = fd.name() + "(ABS((int * 10) / 3) + 1";
                if (fd.clazz() == Percentile.class || fd.clazz() == PercentileRank.class) {
                    aggFunction += ", 50";
                }
                aggFunction += ")";
                LogicalPlan p = plan("SELECT " + aggFunction + ", keyword FROM test " + "GROUP BY keyword HAVING " + aggFunction + " > 20");
                assertTrue(p instanceof Filter);
                assertTrue(((Filter) p).child() instanceof Aggregate);
                List<Attribute> outputs = ((Filter) p).child().output();
                assertEquals(2, outputs.size());
                assertEquals(aggFunction, outputs.get(0).qualifiedName());
                assertEquals("test.keyword", outputs.get(1).qualifiedName());

                Expression condition = ((Filter) p).condition();
                assertFalse(condition.foldable());
                QueryTranslation translation = translateWithAggs(condition);
                assertNull(translation.query);
                AggFilter aggFilter = translation.aggFilter;
                String fn = fd.name();
                String typeName = "MAX".equals(fn) || "MIN".equals(fn) ? "INTEGER" : "SUM".equals(fn) ? "LONG" : "";
                String safeCast = "InternalQlScriptUtils.nullSafeCastNumeric(params.a0,params.v0)";
                String param1st = typeName.length() == 0 ? "params.a0" : safeCast;
                String param2nd = typeName.length() == 0 ? "params.v0" : "params.v1";
                assertEquals(
                    "InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt(" + param1st + "," + param2nd + "))",
                    aggFilter.scriptTemplate().toString()
                );
                String params = "[{a=" + aggFunction + "}," + (typeName.length() == 0 ? "" : " {v=" + typeName + "},") + " {v=20}]";
                assertEquals(params, aggFilter.scriptTemplate().params().toString());
            }
        }
    }

    public void testScriptsInsideAggregateFunctions_ConvertedToStats() {
        String aggFunctionArgs1 = "MIN(ABS((int * 10) / 3) + 1)";
        String aggFunctionArgs2 = "MAX(ABS((int * 10) / 3) + 1)";
        PhysicalPlan p = optimizeAndPlan("SELECT " + aggFunctionArgs1 + ", " + aggFunctionArgs2 + " FROM test");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        assertEquals(2, eqe.output().size());
        assertEquals(aggFunctionArgs1, eqe.output().get(0).qualifiedName());
        assertEquals(aggFunctionArgs2, eqe.output().get(1).qualifiedName());
        assertThat(
            eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
            containsString(
                "{\"stats\":{\"script\":{\"source\":\"InternalSqlScriptUtils.add(InternalSqlScriptUtils.abs("
                    + "InternalSqlScriptUtils.div(InternalSqlScriptUtils.mul(InternalQlScriptUtils.docValue("
                    + "doc,params.v0),params.v1),params.v2)),params.v3)\",\"lang\":\"painless\",\"params\":{"
                    + "\"v0\":\"int\",\"v1\":10,\"v2\":3,\"v3\":1}}"
            )
        );
    }

    public void testScriptsInsideAggregateFunctions_ExtendedStats() {
        for (FunctionDefinition fd : defaultTestContext.sqlFunctionRegistry.listFunctions()) {
            if (ExtendedStatsEnclosed.class.isAssignableFrom(fd.clazz())) {
                String aggFunction = fd.name() + "(ABS((int * 10) / 3) + 1)";
                PhysicalPlan p = optimizeAndPlan("SELECT " + aggFunction + " FROM test");
                assertEquals(EsQueryExec.class, p.getClass());
                EsQueryExec eqe = (EsQueryExec) p;
                assertEquals(1, eqe.output().size());
                assertEquals(aggFunction, eqe.output().get(0).qualifiedName());
                assertThat(
                    eqe.queryContainer().aggs().asAggBuilder().toString().replaceAll("\\s+", ""),
                    containsString(
                        "{\"extended_stats\":{\"script\":{\"source\":\"InternalSqlScriptUtils.add(InternalSqlScriptUtils.abs("
                            + "InternalSqlScriptUtils.div(InternalSqlScriptUtils.mul(InternalQlScriptUtils.docValue("
                            + "doc,params.v0),params.v1),params.v2)),params.v3)\",\"lang\":\"painless\",\"params\":{"
                            + "\"v0\":\"int\",\"v1\":10,\"v2\":3,\"v3\":1}}"
                    )
                );
            }
        }
    }

    public void testScriptsInsideAggregateFunctions_WithDatetimeField() {
        PhysicalPlan p = optimizeAndPlan("SELECT MAX(date) FROM test HAVING MAX(date) > CAST('2020-05-03T12:34:56.000Z' AS DATETIME)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        AggregationBuilder aggBuilder = eqe.queryContainer().aggs().asAggBuilder();
        assertEquals(1, aggBuilder.getSubAggregations().size());
        assertEquals(1, aggBuilder.getPipelineAggregations().size());
        String aggName = aggBuilder.getSubAggregations().iterator().next().getName();
        String havingName = aggBuilder.getPipelineAggregations().iterator().next().getName();
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString(
            "\"aggregations\":{\"" + aggName + "\":{\"max\":{\"field\":\"date\"}},\"" + havingName + "\":{\"bucket_selector\":"
            + "{\"buckets_path\":{\"a0\":\"" + aggName + "\"},\"script\":{\"source\":\"InternalQlScriptUtils.nullSafeFilter("
            + "InternalQlScriptUtils.gt(InternalSqlScriptUtils.asDateTime(params.a0),InternalSqlScriptUtils.asDateTime(params.v0)))\","
            + "\"lang\":\"painless\",\"params\":{\"v0\":\"2020-05-03T12:34:56.000Z\"}},\"gap_policy\":\"skip\"}}}}}}"));
    }

    public void testScriptsInsideAggregateFunctions_WithDateField_AndExtendedStats() {
        PhysicalPlan p = optimizeAndPlan("SELECT MIN(CAST(date AS DATE)), MAX(CAST(date AS DATE)) FROM test HAVING "
            + "MIN(CAST(date AS DATE)) > CAST('2020-05-03T12:34:56.000Z' AS DATE)");
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec eqe = (EsQueryExec) p;
        AggregationBuilder aggBuilder = eqe.queryContainer().aggs().asAggBuilder();
        assertEquals(1, aggBuilder.getSubAggregations().size());
        assertEquals(1, aggBuilder.getPipelineAggregations().size());
        String aggName = aggBuilder.getSubAggregations().iterator().next().getName();
        String havingName = aggBuilder.getPipelineAggregations().iterator().next().getName();
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString(
            "\"aggregations\":{\"" + aggName + "\":{\"stats\":{\"script\":{\"source\":\"InternalSqlScriptUtils.cast("
            + "InternalQlScriptUtils.docValue(doc,params.v0),params.v1)\",\"lang\":\"painless\",\"params\":"
            + "{\"v0\":\"date\",\"v1\":\"DATE\"}}}},\"" + havingName + "\":{\"bucket_selector\":{\"buckets_path\":"
            + "{\"a0\":\"" + aggName + ".min\"},\"script\":{\"source\":\"InternalQlScriptUtils.nullSafeFilter(InternalQlScriptUtils.gt("
            + "InternalSqlScriptUtils.asDateTime(params.a0),InternalSqlScriptUtils.asDateTime(params.v0)))\",\"lang\":\"painless\","
            + "\"params\":{\"v0\":\"2020-05-03T00:00:00.000Z\"}},\"gap_policy\":\"skip\"}}}}}}"));
    }

    public void testFoldingWithParamsWithoutIndex() {
        PhysicalPlan p = optimizeAndPlan(parameterizedSql("SELECT ?, ?, ? FROM test",
            new SqlTypedParamValue("integer", 100),
            new SqlTypedParamValue("integer", 100),
            new SqlTypedParamValue("integer", 200)));
        assertThat(p.output(), everyItem(instanceOf(ReferenceAttribute.class)));
        assertThat(p.output().get(0).toString(), startsWith("?{r}#"));
        assertThat(p.output().get(1).toString(), startsWith("?{r}#"));
        assertThat(p.output().get(2).toString(), startsWith("?{r}#"));
        assertNotEquals(p.output().get(1).id(), p.output().get(2).id());
    }

    public void testSameAliasForParamAndField() {
        PhysicalPlan p = optimizeAndPlan(parameterizedSql("SELECT ?, int as \"?\" FROM test",
            new SqlTypedParamValue("integer", 100)));
        assertThat(p.output(), everyItem(instanceOf(ReferenceAttribute.class)));
        assertThat(p.output().get(0).toString(), startsWith("?{r}#"));
        assertThat(p.output().get(1).toString(), startsWith("?{r}#"));
        assertNotEquals(p.output().get(0).id(), p.output().get(1).id());
    }

    public void testSameAliasOnSameField() {
        PhysicalPlan p = optimizeAndPlan(parameterizedSql("SELECT int as \"int\", int as \"int\" FROM test"));
        assertThat(p.output(), everyItem(instanceOf(ReferenceAttribute.class)));
        assertThat(p.output().get(0).toString(), startsWith("int{r}#"));
        assertThat(p.output().get(1).toString(), startsWith("int{r}#"));
    }

    public void testFoldingWithMixedParamsWithoutAlias() {
        PhysicalPlan p = optimizeAndPlan(parameterizedSql("SELECT ?, ? FROM test",
            new SqlTypedParamValue("integer", 100),
            new SqlTypedParamValue("text", "200")));
        assertThat(p.output(), everyItem(instanceOf(ReferenceAttribute.class)));
        assertThat(p.output().get(0).toString(), startsWith("?{r}#"));
        assertThat(p.output().get(1).toString(), startsWith("?{r}#"));
    }

    public void testSameExpressionWithoutAlias() {
        PhysicalPlan physicalPlan = optimizeAndPlan("SELECT 100, 100 FROM test");
        assertEquals(EsQueryExec.class, physicalPlan.getClass());
        EsQueryExec eqe = (EsQueryExec) physicalPlan;
        assertEquals(2, eqe.output().size());
        assertThat(eqe.output().get(0).toString(), startsWith("100{r}#"));
        assertThat(eqe.output().get(1).toString(), startsWith("100{r}#"));
        // these two should be semantically different reference attributes
        assertNotEquals(eqe.output().get(0).id(), eqe.output().get(1).id());
    }

    public void testInOutOfRangeValues() {
        QlIllegalArgumentException ex = expectThrows(QlIllegalArgumentException.class,
            () -> optimizeAndPlan("SELECT int FROM test WHERE int IN (1, 2, 3, " + Long.MAX_VALUE + ", 5, 6, 7)"));
        assertThat(ex.getMessage(), is("[" + Long.MAX_VALUE + "] out of [integer] range"));
    }

    public void testInInRangeValues() {
        TestContext testContext = new TestContext("mapping-numeric.json");
        PhysicalPlan p = testContext.optimizeAndPlan("SELECT long FROM test WHERE long IN (1, 2, 3, " + Long.MAX_VALUE + ", 5, 6, 7)");
        assertEquals(EsQueryExec.class, p.getClass());
    }

    public void testEqualsAndInOnTheSameField() {
        PhysicalPlan physicalPlan = optimizeAndPlan("SELECT int FROM test WHERE int in (1, 2) OR int = 3 OR int = 2");
        assertEquals(EsQueryExec.class, physicalPlan.getClass());
        EsQueryExec eqe = (EsQueryExec) physicalPlan;
        assertEquals(1, eqe.output().size());
        assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString("\"terms\":{\"int\":[1,2,3],"));
    }

    @SuppressWarnings({"rawtypes"})
    private static List<AbstractPercentilesAggregationBuilder> percentilesAggsByField(PhysicalPlan p, int fieldCount) {
        assertEquals(EsQueryExec.class, p.getClass());
        EsQueryExec ee = (EsQueryExec) p;
        AggregationBuilder aggregationBuilder = ee.queryContainer().aggs().asAggBuilder();
        assertEquals(fieldCount, ee.output().size());
        assertEquals(ReferenceAttribute.class, ee.output().get(0).getClass());
        assertEquals(fieldCount, ee.queryContainer().fields().size());
        assertThat(fieldCount, greaterThanOrEqualTo(ee.queryContainer().aggs().asAggBuilder().getSubAggregations().size()));
        Map<String, AggregationBuilder> aggsByName =
            aggregationBuilder.getSubAggregations().stream().collect(Collectors.toMap(AggregationBuilder::getName, ab -> ab));
        return IntStream.range(0, fieldCount).mapToObj(i -> {
            String percentileAggName = ((MetricAggRef) ee.queryContainer().fields().get(i).v1()).name();
            return (AbstractPercentilesAggregationBuilder) aggsByName.get(percentileAggName);
        }).collect(Collectors.toList());
    }

    @SuppressWarnings({"rawtypes"})
    public void testPercentileMethodParametersSameAsDefault() {
        BiConsumer<String, Function<AbstractPercentilesAggregationBuilder, double[]>> test = (fnName, pctOrValFn) -> {
            final int fieldCount = 5;
            final String sql = ("SELECT " +
                // 0-3: these all should fold into the same aggregation
                "   PERCENTILE(int, 50, 'tdigest', 79.8 + 20.2), " +
                "   PERCENTILE(int, 40 + 10, 'tdigest', null), " +
                "   PERCENTILE(int, 50, 'tdigest'), " +
                "   PERCENTILE(int, 50), " +
                // 4: this has a different method parameter
                // just to make sure we don't fold everything to default
                "   PERCENTILE(int, 50, 'tdigest', 22) "
                + "FROM test").replaceAll("PERCENTILE", fnName);

            List<AbstractPercentilesAggregationBuilder> aggs = percentilesAggsByField(optimizeAndPlan(sql), fieldCount);

            // 0-3
            assertEquals(aggs.get(0), aggs.get(1));
            assertEquals(aggs.get(0), aggs.get(2));
            assertEquals(aggs.get(0), aggs.get(3));
            assertEquals(new PercentilesConfig.TDigest(), aggs.get(0).percentilesConfig());
            assertArrayEquals(new double[] { 50 }, pctOrValFn.apply(aggs.get(0)), 0);

            // 4
            assertEquals(new PercentilesConfig.TDigest(22), aggs.get(4).percentilesConfig());
            assertArrayEquals(new double[] { 50 }, pctOrValFn.apply(aggs.get(4)), 0);
        };

        test.accept("PERCENTILE", p -> ((PercentilesAggregationBuilder)p).percentiles());
        test.accept("PERCENTILE_RANK", p -> ((PercentileRanksAggregationBuilder)p).values());
    }

    @SuppressWarnings({"rawtypes"})
    public void testPercentileOptimization() {
        BiConsumer<String, Function<AbstractPercentilesAggregationBuilder, double[]>> test = (fnName, pctOrValFn) -> {
            final int fieldCount = 5;
            final String sql = ("SELECT " +
                // 0-1: fold into the same aggregation
                "   PERCENTILE(int, 50, 'tdigest'), " +
                "   PERCENTILE(int, 60, 'tdigest'), " +

                // 2-3: fold into one aggregation
                "   PERCENTILE(int, 50, 'hdr'), " +
                "   PERCENTILE(int, 60, 'hdr', 3), " +

                // 4: folds into a separate aggregation
                "   PERCENTILE(int, 60, 'hdr', 4)" +
                "FROM test").replaceAll("PERCENTILE", fnName);

            List<AbstractPercentilesAggregationBuilder> aggs = percentilesAggsByField(optimizeAndPlan(sql), fieldCount);

            // 0-1
            assertEquals(aggs.get(0), aggs.get(1));
            assertEquals(new PercentilesConfig.TDigest(), aggs.get(0).percentilesConfig());
            assertArrayEquals(new double[]{50, 60}, pctOrValFn.apply(aggs.get(0)), 0);

            // 2-3
            assertEquals(aggs.get(2), aggs.get(3));
            assertEquals(new PercentilesConfig.Hdr(), aggs.get(2).percentilesConfig());
            assertArrayEquals(new double[]{50, 60}, pctOrValFn.apply(aggs.get(2)), 0);

            // 4
            assertEquals(new PercentilesConfig.Hdr(4), aggs.get(4).percentilesConfig());
            assertArrayEquals(new double[]{60}, pctOrValFn.apply(aggs.get(4)), 0);
        };

        test.accept("PERCENTILE", p -> ((PercentilesAggregationBuilder)p).percentiles());
        test.accept("PERCENTILE_RANK", p -> ((PercentileRanksAggregationBuilder)p).values());
    }

    // Tests the workaround for the SUM(all zeros) = NULL issue raised in https://github.com/elastic/elasticsearch/issues/45251 and
    // should be removed as soon as root cause is fixed and the sum aggregation results can differentiate between SUM(all zeroes)
    // and SUM(all nulls)
    public void testReplaceSumWithStats() {
        List<String> testCases = asList(
            "SELECT keyword, SUM(int) FROM test GROUP BY keyword",
            "SELECT SUM(int) FROM test",
            "SELECT * FROM (SELECT some.string, keyword, int FROM test) PIVOT (SUM(int) FOR keyword IN ('a', 'b'))");
        for (String testCase : testCases) {
            PhysicalPlan physicalPlan = optimizeAndPlan(testCase);
            assertEquals(EsQueryExec.class, physicalPlan.getClass());
            EsQueryExec eqe = (EsQueryExec) physicalPlan;
            assertThat(eqe.queryContainer().toString().replaceAll("\\s+", ""), containsString("{\"stats\":{\"field\":\"int\"}}"));
        }
    }

    public void testAddMissingEqualsToBoolField() {
        LogicalPlan p = plan("SELECT bool FROM test WHERE bool");
        assertTrue(p instanceof Project);

        p = ((Project) p).child();
        assertTrue(p instanceof Filter);

        Expression condition = ((Filter) p).condition();
        assertTrue(condition instanceof Equals);
        Equals eq = (Equals) condition;

        assertTrue(eq.left() instanceof FieldAttribute);
        assertEquals("bool", ((FieldAttribute) eq.left()).name());

        assertTrue(eq.right() instanceof Literal);
        assertEquals(TRUE, eq.right());
    }

    public void testAddMissingEqualsToNestedBoolField() {
        LogicalPlan p = plan("SELECT bool FROM test " +
            "WHERE int > 1 and (bool or int < 2) or (int = 3 and bool) or (int = 4 and bool = false) or bool");
        LogicalPlan expectedPlan = plan("SELECT bool FROM test " +
            "WHERE int > 1 and (bool = true or int < 2) or (int = 3 and bool = true) or (int = 4 and bool = false) or bool = true");

        assertTrue(p instanceof Project);
        p = ((Project) p).child();
        assertTrue(p instanceof Filter);
        Expression condition = ((Filter) p).condition();

        Expression expectedCondition = ((Filter) ((Project) expectedPlan).child()).condition();

        List<Expression> expectedFields = expectedCondition.collect(x -> x instanceof FieldAttribute);
        Set<Expression> expectedBools = expectedFields.stream()
            .filter(x -> ((FieldAttribute) x).name().equals("bool")).collect(Collectors.toSet());
        assertEquals(1, expectedBools.size());
        Set<Expression> expectedInts = expectedFields.stream()
            .filter(x -> ((FieldAttribute) x).name().equals("int")).collect(Collectors.toSet());
        assertEquals(1, expectedInts.size());

        condition = condition
            .transformDown(FieldAttribute.class, x -> x.name().equals("bool") ? (FieldAttribute) expectedBools.toArray()[0] : x)
            .transformDown(FieldAttribute.class, x -> x.name().equals("int") ? (FieldAttribute) expectedInts.toArray()[0] : x);

        assertEquals(expectedCondition, condition);
    }

    public void testSubqueryBasicSelect() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT int FROM " +
            "( SELECT int FROM test )");
    }

    public void testSubquerySelectOnFieldAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test )");
    }

    public void testSubqueryGroupByNoAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT int FROM " +
            "( SELECT int FROM test ) " +
            "GROUP BY int");
    }

    public void testSubqueryGroupByOnFieldAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) " +
            "GROUP BY i");
    }

    public void testSubqueryFilterOrderByAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) " +
            "WHERE i IS NOT NULL " +
            "ORDER BY i");
    }

    public void testSubqueryGroupByFilterAndOrderByByAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) " +
            "WHERE i IS NOT NULL " +
            "GROUP BY i " +
            "ORDER BY i");
    }

    public void testSubqueryFilterByAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) " +
            "WHERE i > 10");
    }

    public void testSubqueryOrderByAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) " +
            "ORDER BY i");
    }

    public void testSubqueryWithAliasBasicSelect() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT int FROM " +
            "( SELECT int FROM test ) AS s");
    }

    public void testSubqueryWithAliasBasicQualifiedSelect() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT s.int FROM " +
            "( SELECT int FROM test ) AS s");
    }

    public void testSubqueryWithAliasSelectOnFieldAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) AS s");
    }

    public void testSubqueryWithAliasQualifiedSelectOnFieldAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT s.i FROM " +
            "( SELECT int AS i FROM test ) AS s");
    }

    public void testSubqueryWithAliasGroupBy() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) AS s " +
            "GROUP BY s.i");
    }

    public void testSubqueryWithAliasFilterByAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) AS s " +
            "WHERE s.i > 10");
    }

    public void testSubqueryWithAliasOrderByAlias() throws Exception {
        PhysicalPlan p = optimizeAndPlan("SELECT i FROM " +
            "( SELECT int AS i FROM test ) AS s " +
            "ORDER BY s.i > 10");
    }

    public void testMultiLevelSubqueriesOrderByByAlias() {
        optimizeAndPlan("SELECT i AS j FROM ( SELECT int AS i FROM test) ORDER BY j");
        optimizeAndPlan("SELECT j AS k FROM (SELECT i AS j FROM ( SELECT int AS i FROM test)) ORDER BY k");
    }

    public void testSubqueryGroupByOrderByAliasedExpression() {
        optimizeAndPlan("SELECT int_group AS g, min_date AS d " +
            "FROM (" +
            "    SELECT int % 2 AS int_group, MIN(date) AS min_date " +
            "    FROM test WHERE date > '1970-01-01'::datetime " +
            "    GROUP BY int_group" +
            ") " +
            "ORDER BY d DESC");
    }

    public void testSubqueryGroupByOrderByAliasedAggFunction() {
        optimizeAndPlan("SELECT int_group AS g, min_date AS d " +
            "FROM (" +
            "    SELECT int % 2 AS int_group, MIN(date) AS min_date " +
            "    FROM test WHERE date > '1970-01-01'::datetime " +
            "    GROUP BY int_group " +
            ")" +
            "ORDER BY g DESC");
    }

    public void testMultiLevelSubquerySelectStar() {
        optimizeAndPlan("SELECT * FROM (SELECT * FROM ( SELECT * FROM test ))");
        optimizeAndPlan("SELECT * FROM (SELECT * FROM ( SELECT * FROM test ) b) c");
    }

    public void testMultiLevelSubqueryGroupBy() {
        optimizeAndPlan("SELECT i AS j FROM ( SELECT int AS i FROM test) GROUP BY j");
        optimizeAndPlan("SELECT j AS k FROM (SELECT i AS j FROM ( SELECT int AS i FROM test)) GROUP BY k");
    }

    public void testSubqueryGroupByFilterAndOrderByByRealiased() {
        optimizeAndPlan("SELECT g as h FROM (SELECT date AS f, int AS g FROM test) WHERE h IS NOT NULL GROUP BY h ORDER BY h ASC");
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/69758")
    public void testFilterAfterGroupBy() {
        optimizeAndPlan("SELECT j AS k FROM (SELECT i AS j FROM ( SELECT int AS i FROM test) GROUP BY j) WHERE j < 5");
    }
}
