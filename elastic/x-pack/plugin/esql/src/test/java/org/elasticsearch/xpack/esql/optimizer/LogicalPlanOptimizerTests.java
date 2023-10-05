/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.optimizer;

import org.elasticsearch.common.lucene.BytesRefs;
import org.elasticsearch.compute.aggregation.QuantileStates;
import org.elasticsearch.compute.data.BlockFactory;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.esql.EsqlTestUtils;
import org.elasticsearch.xpack.esql.analysis.Analyzer;
import org.elasticsearch.xpack.esql.analysis.AnalyzerContext;
import org.elasticsearch.xpack.esql.analysis.AnalyzerTestUtils;
import org.elasticsearch.xpack.esql.analysis.EnrichResolution;
import org.elasticsearch.xpack.esql.enrich.EnrichPolicyResolution;
import org.elasticsearch.xpack.esql.evaluator.predicate.operator.comparison.Equals;
import org.elasticsearch.xpack.esql.evaluator.predicate.operator.comparison.GreaterThan;
import org.elasticsearch.xpack.esql.evaluator.predicate.operator.comparison.GreaterThanOrEqual;
import org.elasticsearch.xpack.esql.evaluator.predicate.operator.comparison.LessThan;
import org.elasticsearch.xpack.esql.evaluator.predicate.operator.regex.RLike;
import org.elasticsearch.xpack.esql.evaluator.predicate.operator.regex.WildcardLike;
import org.elasticsearch.xpack.esql.expression.Order;
import org.elasticsearch.xpack.esql.expression.function.EsqlFunctionRegistry;
import org.elasticsearch.xpack.esql.expression.function.aggregate.Count;
import org.elasticsearch.xpack.esql.expression.function.aggregate.Min;
import org.elasticsearch.xpack.esql.expression.function.aggregate.Percentile;
import org.elasticsearch.xpack.esql.expression.function.aggregate.Sum;
import org.elasticsearch.xpack.esql.expression.function.scalar.date.DateFormat;
import org.elasticsearch.xpack.esql.expression.function.scalar.date.DateParse;
import org.elasticsearch.xpack.esql.expression.function.scalar.date.DateTrunc;
import org.elasticsearch.xpack.esql.expression.function.scalar.math.Pow;
import org.elasticsearch.xpack.esql.expression.function.scalar.math.Round;
import org.elasticsearch.xpack.esql.expression.function.scalar.string.Substring;
import org.elasticsearch.xpack.esql.expression.predicate.operator.arithmetic.Add;
import org.elasticsearch.xpack.esql.expression.predicate.operator.arithmetic.Mul;
import org.elasticsearch.xpack.esql.expression.predicate.operator.comparison.In;
import org.elasticsearch.xpack.esql.parser.EsqlParser;
import org.elasticsearch.xpack.esql.plan.logical.Dissect;
import org.elasticsearch.xpack.esql.plan.logical.Enrich;
import org.elasticsearch.xpack.esql.plan.logical.Eval;
import org.elasticsearch.xpack.esql.plan.logical.Grok;
import org.elasticsearch.xpack.esql.plan.logical.TopN;
import org.elasticsearch.xpack.esql.plan.logical.local.EsqlProject;
import org.elasticsearch.xpack.esql.plan.logical.local.LocalRelation;
import org.elasticsearch.xpack.esql.plan.logical.local.LocalSupplier;
import org.elasticsearch.xpack.ql.expression.Alias;
import org.elasticsearch.xpack.ql.expression.Attribute;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.Expressions;
import org.elasticsearch.xpack.ql.expression.FieldAttribute;
import org.elasticsearch.xpack.ql.expression.Literal;
import org.elasticsearch.xpack.ql.expression.NamedExpression;
import org.elasticsearch.xpack.ql.expression.Nullability;
import org.elasticsearch.xpack.ql.expression.ReferenceAttribute;
import org.elasticsearch.xpack.ql.expression.function.aggregate.AggregateFunction;
import org.elasticsearch.xpack.ql.expression.predicate.logical.And;
import org.elasticsearch.xpack.ql.expression.predicate.logical.Or;
import org.elasticsearch.xpack.ql.expression.predicate.nulls.IsNotNull;
import org.elasticsearch.xpack.ql.expression.predicate.regex.RLikePattern;
import org.elasticsearch.xpack.ql.expression.predicate.regex.WildcardPattern;
import org.elasticsearch.xpack.ql.index.EsIndex;
import org.elasticsearch.xpack.ql.index.IndexResolution;
import org.elasticsearch.xpack.ql.plan.logical.Aggregate;
import org.elasticsearch.xpack.ql.plan.logical.EsRelation;
import org.elasticsearch.xpack.ql.plan.logical.Filter;
import org.elasticsearch.xpack.ql.plan.logical.Limit;
import org.elasticsearch.xpack.ql.plan.logical.LogicalPlan;
import org.elasticsearch.xpack.ql.plan.logical.OrderBy;
import org.elasticsearch.xpack.ql.plan.logical.Project;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataType;
import org.elasticsearch.xpack.ql.type.DataTypes;
import org.elasticsearch.xpack.ql.type.EsField;
import org.junit.BeforeClass;

import java.util.List;
import java.util.Map;
import java.util.Set;

import static java.util.Collections.emptyList;
import static java.util.Collections.emptyMap;
import static java.util.Collections.singletonList;
import static org.elasticsearch.xpack.esql.EsqlTestUtils.L;
import static org.elasticsearch.xpack.esql.EsqlTestUtils.TEST_VERIFIER;
import static org.elasticsearch.xpack.esql.EsqlTestUtils.as;
import static org.elasticsearch.xpack.esql.EsqlTestUtils.emptySource;
import static org.elasticsearch.xpack.esql.EsqlTestUtils.loadMapping;
import static org.elasticsearch.xpack.esql.EsqlTestUtils.localSource;
import static org.elasticsearch.xpack.ql.TestUtils.relation;
import static org.elasticsearch.xpack.ql.tree.Source.EMPTY;
import static org.elasticsearch.xpack.ql.type.DataTypes.INTEGER;
import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.emptyArray;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.startsWith;

//@TestLogging(value = "org.elasticsearch.xpack.esql:TRACE", reason = "debug")
public class LogicalPlanOptimizerTests extends ESTestCase {

    private static final Literal ONE = L(1);
    private static final Literal TWO = L(2);
    private static final Literal THREE = L(3);

    private static EsqlParser parser;
    private static Analyzer analyzer;
    private static LogicalPlanOptimizer logicalOptimizer;
    private static Map<String, EsField> mapping;

    @BeforeClass
    public static void init() {
        parser = new EsqlParser();

        mapping = loadMapping("mapping-basic.json");
        EsIndex test = new EsIndex("test", mapping);
        IndexResolution getIndexResult = IndexResolution.valid(test);

        logicalOptimizer = new LogicalPlanOptimizer();
        EnrichPolicyResolution policy = AnalyzerTestUtils.loadEnrichPolicyResolution(
            "languages_idx",
            "id",
            "languages_idx",
            "mapping-languages.json"
        );
        analyzer = new Analyzer(
            new AnalyzerContext(
                EsqlTestUtils.TEST_CFG,
                new EsqlFunctionRegistry(),
                getIndexResult,
                new EnrichResolution(Set.of(policy), Set.of("languages_idx", "something"))
            ),
            TEST_VERIFIER
        );
    }

    public void testEmptyProjections() {
        var plan = plan("""
            from test
            | keep salary
            | drop salary
            """);

        var relation = as(plan, LocalRelation.class);
        assertThat(relation.output(), is(empty()));
        assertThat(relation.supplier().get(), emptyArray());
    }

    public void testEmptyProjectionInStat() {
        var plan = plan("""
            from test
            | stats c = count(salary)
            | drop c
            """);

        var relation = as(plan, LocalRelation.class);
        assertThat(relation.output(), is(empty()));
        assertThat(relation.supplier().get(), emptyArray());
    }

    public void testCombineProjections() {
        var plan = plan("""
            from test
            | keep emp_no, *name, salary
            | keep last_name
            """);

        var keep = as(plan, Project.class);
        assertThat(Expressions.names(keep.projections()), contains("last_name"));
        var limit = as(keep.child(), Limit.class);
        var relation = as(limit.child(), EsRelation.class);
    }

    public void testCombineProjectionWithFilterInBetween() {
        var plan = plan("""
            from test
            | keep *name, salary
            | where salary > 10
            | keep last_name
            """);

        var keep = as(plan, Project.class);
        assertThat(Expressions.names(keep.projections()), contains("last_name"));
    }

    public void testCombineProjectionWhilePreservingAlias() {
        var plan = plan("""
            from test
            | rename first_name as x
            | keep x, salary
            | where salary > 10
            | rename x as y
            | keep y
            """);

        var keep = as(plan, Project.class);
        assertThat(Expressions.names(keep.projections()), contains("y"));
        var p = keep.projections().get(0);
        var alias = as(p, Alias.class);
        assertThat(Expressions.name(alias.child()), containsString("first_name"));
    }

    public void testCombineProjectionWithAggregation() {
        var plan = plan("""
            from test
            | stats s = sum(salary) by last_name, first_name
            | keep s, last_name, first_name
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        assertThat(Expressions.names(agg.aggregates()), contains("s", "last_name", "first_name"));
        assertThat(Expressions.names(agg.groupings()), contains("last_name", "first_name"));
    }

    public void testQlComparisonOptimizationsApply() {
        var plan = plan("""
            from test
            | where (1 + 4) < salary
            """);

        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        // The core QL optimizations rotate constants to the right.
        var condition = as(filter.condition(), GreaterThan.class);
        assertThat(Expressions.name(condition.left()), equalTo("salary"));
        assertThat(Expressions.name(condition.right()), equalTo("1 + 4"));
        var con = as(condition.right(), Literal.class);
        assertThat(con.value(), equalTo(5));
    }

    public void testCombineProjectionWithPruning() {
        var plan = plan("""
            from test
            | rename first_name as x
            | keep x, salary, last_name
            | stats count(salary) by x
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        assertThat(Expressions.names(agg.aggregates()), contains("count(salary)", "x"));
        assertThat(Expressions.names(agg.groupings()), contains("x"));
        var alias = as(agg.aggregates().get(1), Alias.class);
        var field = as(alias.child(), FieldAttribute.class);
        assertThat(field.name(), is("first_name"));
        var group = as(agg.groupings().get(0), Attribute.class);
        assertThat(group, is(alias.toAttribute()));
        var from = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * EsqlProject[[x{r}#3, y{r}#6]]
     * \_Eval[[emp_no{f}#9 + 2[INTEGER] AS x, salary{f}#14 + 3[INTEGER] AS y]]
     *   \_Limit[10000[INTEGER]]
     *     \_EsRelation[test][_meta_field{f}#15, emp_no{f}#9, first_name{f}#10, g..]
     */
    public void testCombineEvals() {
        var plan = plan("""
            from test
            | eval x = emp_no + 2
            | eval y = salary + 3
            | keep x, y
            """);

        var project = as(plan, Project.class);
        var eval = as(project.child(), Eval.class);
        assertThat(Expressions.names(eval.fields()), contains("x", "y"));
        var limit = as(eval.child(), Limit.class);
        var source = as(limit.child(), EsRelation.class);
    }

    public void testCombineLimits() {
        var limitValues = new int[] { randomIntBetween(10, 99), randomIntBetween(100, 1000) };
        var firstLimit = randomBoolean() ? 0 : 1;
        var secondLimit = firstLimit == 0 ? 1 : 0;
        var oneLimit = new Limit(EMPTY, L(limitValues[firstLimit]), emptySource());
        var anotherLimit = new Limit(EMPTY, L(limitValues[secondLimit]), oneLimit);
        assertEquals(
            new Limit(EMPTY, L(Math.min(limitValues[0], limitValues[1])), emptySource()),
            new LogicalPlanOptimizer.PushDownAndCombineLimits().rule(anotherLimit)
        );
    }

    public void testMultipleCombineLimits() {
        var numberOfLimits = randomIntBetween(3, 10);
        var minimum = randomIntBetween(10, 99);
        var limitWithMinimum = randomIntBetween(0, numberOfLimits - 1);

        var fa = getFieldAttribute("a", INTEGER);
        var relation = localSource(BlockFactory.getNonBreakingInstance(), singletonList(fa), singletonList(1));
        LogicalPlan plan = relation;

        for (int i = 0; i < numberOfLimits; i++) {
            var value = i == limitWithMinimum ? minimum : randomIntBetween(100, 1000);
            plan = new Limit(EMPTY, L(value), plan);
        }
        assertEquals(new Limit(EMPTY, L(minimum), relation), new LogicalPlanOptimizer().optimize(plan));
    }

    public static GreaterThan greaterThanOf(Expression left, Expression right) {
        return new GreaterThan(EMPTY, left, right, randomZone());
    }

    public static LessThan lessThanOf(Expression left, Expression right) {
        return new LessThan(EMPTY, left, right, randomZone());
    }

    public static GreaterThanOrEqual greaterThanOrEqualOf(Expression left, Expression right) {
        return new GreaterThanOrEqual(EMPTY, left, right, randomZone());
    }

    public void testCombineFilters() {
        EsRelation relation = relation();
        GreaterThan conditionA = greaterThanOf(getFieldAttribute("a"), ONE);
        LessThan conditionB = lessThanOf(getFieldAttribute("b"), TWO);

        Filter fa = new Filter(EMPTY, relation, conditionA);
        Filter fb = new Filter(EMPTY, fa, conditionB);

        assertEquals(
            new Filter(EMPTY, relation, new And(EMPTY, conditionA, conditionB)),
            new LogicalPlanOptimizer.PushDownAndCombineFilters().apply(fb)
        );
    }

    public void testCombineFiltersLikeRLike() {
        EsRelation relation = relation();
        RLike conditionA = rlike(getFieldAttribute("a"), "foo");
        WildcardLike conditionB = wildcardLike(getFieldAttribute("b"), "bar");

        Filter fa = new Filter(EMPTY, relation, conditionA);
        Filter fb = new Filter(EMPTY, fa, conditionB);

        assertEquals(
            new Filter(EMPTY, relation, new And(EMPTY, conditionA, conditionB)),
            new LogicalPlanOptimizer.PushDownAndCombineFilters().apply(fb)
        );
    }

    public void testPushDownFilter() {
        EsRelation relation = relation();
        GreaterThan conditionA = greaterThanOf(getFieldAttribute("a"), ONE);
        LessThan conditionB = lessThanOf(getFieldAttribute("b"), TWO);

        Filter fa = new Filter(EMPTY, relation, conditionA);
        List<FieldAttribute> projections = singletonList(getFieldAttribute("b"));
        EsqlProject keep = new EsqlProject(EMPTY, fa, projections);
        Filter fb = new Filter(EMPTY, keep, conditionB);

        Filter combinedFilter = new Filter(EMPTY, relation, new And(EMPTY, conditionA, conditionB));
        assertEquals(new EsqlProject(EMPTY, combinedFilter, projections), new LogicalPlanOptimizer.PushDownAndCombineFilters().apply(fb));
    }

    public void testPushDownLikeRlikeFilter() {
        EsRelation relation = relation();
        org.elasticsearch.xpack.ql.expression.predicate.regex.RLike conditionA = rlike(getFieldAttribute("a"), "foo");
        WildcardLike conditionB = wildcardLike(getFieldAttribute("b"), "bar");

        Filter fa = new Filter(EMPTY, relation, conditionA);
        List<FieldAttribute> projections = singletonList(getFieldAttribute("b"));
        EsqlProject keep = new EsqlProject(EMPTY, fa, projections);
        Filter fb = new Filter(EMPTY, keep, conditionB);

        Filter combinedFilter = new Filter(EMPTY, relation, new And(EMPTY, conditionA, conditionB));
        assertEquals(new EsqlProject(EMPTY, combinedFilter, projections), new LogicalPlanOptimizer.PushDownAndCombineFilters().apply(fb));
    }

    // from ... | where a > 1 | stats count(1) by b | where count(1) >= 3 and b < 2
    // => ... | where a > 1 and b < 2 | stats count(1) by b | where count(1) >= 3
    public void testSelectivelyPushDownFilterPastFunctionAgg() {
        EsRelation relation = relation();
        GreaterThan conditionA = greaterThanOf(getFieldAttribute("a"), ONE);
        LessThan conditionB = lessThanOf(getFieldAttribute("b"), TWO);
        GreaterThanOrEqual aggregateCondition = greaterThanOrEqualOf(new Count(EMPTY, ONE), THREE);

        Filter fa = new Filter(EMPTY, relation, conditionA);
        // invalid aggregate but that's fine cause its properties are not used by this rule
        Aggregate aggregate = new Aggregate(EMPTY, fa, singletonList(getFieldAttribute("b")), emptyList());
        Filter fb = new Filter(EMPTY, aggregate, new And(EMPTY, aggregateCondition, conditionB));

        // expected
        Filter expected = new Filter(
            EMPTY,
            new Aggregate(
                EMPTY,
                new Filter(EMPTY, relation, new And(EMPTY, conditionA, conditionB)),
                singletonList(getFieldAttribute("b")),
                emptyList()
            ),
            aggregateCondition
        );
        assertEquals(expected, new LogicalPlanOptimizer.PushDownAndCombineFilters().apply(fb));
    }

    public void testSelectivelyPushDownFilterPastRefAgg() {
        // expected plan: "from test | where emp_no > 1 and emp_no < 3 | stats x = count(1) by emp_no | where x > 7"
        LogicalPlan plan = optimizedPlan("""
            from test
            | where emp_no > 1
            | stats x = count(1) by emp_no
            | where x + 2 > 9
            | where emp_no < 3""");
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        assertTrue(filter.condition() instanceof GreaterThan);
        var gt = (GreaterThan) filter.condition();
        assertTrue(gt.left() instanceof ReferenceAttribute);
        var refAttr = (ReferenceAttribute) gt.left();
        assertEquals("x", refAttr.name());
        assertEquals(L(7), gt.right());

        var agg = as(filter.child(), Aggregate.class);

        filter = as(agg.child(), Filter.class);
        assertTrue(filter.condition() instanceof And);
        var and = (And) filter.condition();
        assertTrue(and.left() instanceof GreaterThan);
        gt = (GreaterThan) and.left();
        assertTrue(gt.left() instanceof FieldAttribute);
        assertEquals("emp_no", ((FieldAttribute) gt.left()).name());
        assertTrue(and.right() instanceof LessThan);
        var lt = (LessThan) and.right();
        assertTrue(lt.left() instanceof FieldAttribute);
        assertEquals("emp_no", ((FieldAttribute) lt.left()).name());

        assertTrue(filter.child() instanceof EsRelation);
    }

    public void testNoPushDownOrFilterPastAgg() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | stats x = count(1) by emp_no
            | where emp_no < 3 or x > 9""");
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        assertTrue(filter.condition() instanceof Or);
        var or = (Or) filter.condition();
        assertTrue(or.left() instanceof LessThan);
        assertTrue(or.right() instanceof GreaterThan);

        var stats = as(filter.child(), Aggregate.class);
        assertTrue(stats.child() instanceof EsRelation);
    }

    public void testSelectivePushDownComplexFilterPastAgg() {
        // expected plan: from test | emp_no > 0 | stats x = count(1) by emp_no | where emp_no < 3 or x > 9
        LogicalPlan plan = optimizedPlan("""
            from test
            | stats x = count(1) by emp_no
            | where (emp_no < 3 or x > 9) and emp_no > 0""");
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        assertTrue(filter.condition() instanceof Or);
        var or = (Or) filter.condition();
        assertTrue(or.left() instanceof LessThan);
        assertTrue(or.right() instanceof GreaterThan);

        var stats = as(filter.child(), Aggregate.class);
        filter = as(stats.child(), Filter.class);
        assertTrue(filter.condition() instanceof GreaterThan);
        var gt = (GreaterThan) filter.condition();
        assertTrue(gt.left() instanceof FieldAttribute);
        assertEquals("emp_no", ((FieldAttribute) gt.left()).name());
        assertEquals(L(0), gt.right());

        assertTrue(filter.child() instanceof EsRelation);
    }

    public void testSelectivelyPushDownFilterPastEval() {
        // expected plan: "from test | where emp_no > 1 and emp_no < 3 | eval x = emp_no + 1 | where x < 7"
        LogicalPlan plan = optimizedPlan("""
            from test
            | where emp_no > 1
            | eval x = emp_no + 1
            | where x + 2 < 9
            | where emp_no < 3""");
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        assertTrue(filter.condition() instanceof LessThan);
        var lt = (LessThan) filter.condition();
        assertTrue(lt.left() instanceof ReferenceAttribute);
        var refAttr = (ReferenceAttribute) lt.left();
        assertEquals("x", refAttr.name());
        assertEquals(L(7), lt.right());

        var eval = as(filter.child(), Eval.class);
        assertEquals(1, eval.fields().size());
        assertTrue(eval.fields().get(0) instanceof Alias);
        assertEquals("x", (eval.fields().get(0)).name());

        filter = as(eval.child(), Filter.class);
        assertTrue(filter.condition() instanceof And);
        var and = (And) filter.condition();
        assertTrue(and.left() instanceof GreaterThan);
        var gt = (GreaterThan) and.left();
        assertTrue(gt.left() instanceof FieldAttribute);
        assertEquals("emp_no", ((FieldAttribute) gt.left()).name());
        assertTrue(and.right() instanceof LessThan);
        lt = (LessThan) and.right();
        assertTrue(lt.left() instanceof FieldAttribute);
        assertEquals("emp_no", ((FieldAttribute) lt.left()).name());

        assertTrue(filter.child() instanceof EsRelation);
    }

    public void testNoPushDownOrFilterPastLimit() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | limit 3
            | where emp_no < 3 or salary > 9""");
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        assertTrue(filter.condition() instanceof Or);
        var or = (Or) filter.condition();
        assertTrue(or.left() instanceof LessThan);
        assertTrue(or.right() instanceof GreaterThan);

        var limit2 = as(filter.child(), Limit.class);
        assertTrue(limit2.child() instanceof EsRelation);
    }

    public void testPushDownFilterPastProject() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | rename emp_no as x
            | keep x
            | where x > 10""");

        var keep = as(plan, Project.class);
        var limit = as(keep.child(), Limit.class);
        var filter = as(limit.child(), Filter.class);
        var attr = filter.condition().collect(Attribute.class::isInstance).stream().findFirst().get();
        assertThat(as(attr, FieldAttribute.class).name(), is("emp_no"));
    }

    public void testPushDownEvalPastProject() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | rename emp_no as x
            | keep x
            | eval y = x * 2""");

        var keep = as(plan, Project.class);
        var eval = as(keep.child(), Eval.class);
        assertThat(
            eval.fields(),
            contains(
                new Alias(
                    EMPTY,
                    "y",
                    new Mul(EMPTY, new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")), new Literal(EMPTY, 2, INTEGER))
                )
            )
        );
    }

    public void testPushDownDissectPastProject() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | rename first_name as x
            | keep x
            | dissect x "%{y}"
            """);

        var keep = as(plan, Project.class);
        var dissect = as(keep.child(), Dissect.class);
        assertThat(dissect.extractedFields(), contains(new ReferenceAttribute(Source.EMPTY, "y", DataTypes.KEYWORD)));
    }

    public void testPushDownGrokPastProject() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | rename first_name as x
            | keep x
            | grok x "%{WORD:y}"
            """);

        var keep = as(plan, Project.class);
        var grok = as(keep.child(), Grok.class);
        assertThat(grok.extractedFields(), contains(new ReferenceAttribute(Source.EMPTY, "y", DataTypes.KEYWORD)));
    }

    public void testPushDownFilterPastProjectUsingEval() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | eval y = emp_no + 1
            | rename y as x
            | where x > 10""");

        var keep = as(plan, Project.class);
        var limit = as(keep.child(), Limit.class);
        var filter = as(limit.child(), Filter.class);
        var attr = filter.condition().collect(Attribute.class::isInstance).stream().findFirst().get();
        assertThat(as(attr, ReferenceAttribute.class).name(), is("y"));
        var eval = as(filter.child(), Eval.class);
        as(eval.child(), EsRelation.class);
    }

    public void testPushDownFilterPastProjectUsingDissect() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | dissect first_name "%{y}"
            | rename y as x
            | keep x
            | where x == "foo"
            """);

        var keep = as(plan, Project.class);
        var limit = as(keep.child(), Limit.class);
        var filter = as(limit.child(), Filter.class);
        var attr = filter.condition().collect(Attribute.class::isInstance).stream().findFirst().get();
        assertThat(as(attr, ReferenceAttribute.class).name(), is("y"));
        var dissect = as(filter.child(), Dissect.class);
        as(dissect.child(), EsRelation.class);
    }

    public void testPushDownFilterPastProjectUsingGrok() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | grok first_name "%{WORD:y}"
            | rename y as x
            | keep x
            | where x == "foo"
            """);

        var keep = as(plan, Project.class);
        var limit = as(keep.child(), Limit.class);
        var filter = as(limit.child(), Filter.class);
        var attr = filter.condition().collect(Attribute.class::isInstance).stream().findFirst().get();
        assertThat(as(attr, ReferenceAttribute.class).name(), is("y"));
        var grok = as(filter.child(), Grok.class);
        as(grok.child(), EsRelation.class);
    }

    public void testPushDownLimitPastEval() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | eval x = emp_no + 100
            | limit 10""");

        var eval = as(plan, Eval.class);
        as(eval.child(), Limit.class);
    }

    public void testPushDownLimitPastDissect() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | dissect first_name "%{y}"
            | limit 10""");

        var dissect = as(plan, Dissect.class);
        as(dissect.child(), Limit.class);
    }

    public void testPushDownLimitPastGrok() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | grok first_name "%{WORD:y}"
            | limit 10""");

        var grok = as(plan, Grok.class);
        as(grok.child(), Limit.class);
    }

    public void testPushDownLimitPastProject() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | rename emp_no as a
            | keep a
            | limit 10""");

        var keep = as(plan, Project.class);
        as(keep.child(), Limit.class);
    }

    public void testDontPushDownLimitPastFilter() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | limit 100
            | where emp_no > 10
            | limit 10""");

        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);
        as(filter.child(), Limit.class);
    }

    public void testEliminateHigherLimitDueToDescendantLimit() throws Exception {
        LogicalPlan plan = optimizedPlan("""
            from test
            | limit 10
            | sort emp_no
            | where emp_no > 10
            | eval c = emp_no + 2
            | limit 100""");

        var topN = as(plan, TopN.class);
        var eval = as(topN.child(), Eval.class);
        var filter = as(eval.child(), Filter.class);
        as(filter.child(), Limit.class);
    }

    public void testDoNotEliminateHigherLimitDueToDescendantLimit() throws Exception {
        LogicalPlan plan = optimizedPlan("""
            from test
            | limit 10
            | where emp_no > 10
            | stats c = count(emp_no) by emp_no
            | limit 100""");

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var filter = as(agg.child(), Filter.class);
        as(filter.child(), Limit.class);
    }

    public void testBasicNullFolding() {
        FoldNull rule = new FoldNull();
        assertNullLiteral(rule.rule(new Add(EMPTY, L(randomInt()), Literal.NULL)));
        assertNullLiteral(rule.rule(new Round(EMPTY, Literal.NULL, null)));
        assertNullLiteral(rule.rule(new Pow(EMPTY, Literal.NULL, Literal.NULL)));
        assertNullLiteral(rule.rule(new DateFormat(EMPTY, Literal.NULL, Literal.NULL, null)));
        assertNullLiteral(rule.rule(new DateParse(EMPTY, Literal.NULL, Literal.NULL)));
        assertNullLiteral(rule.rule(new DateTrunc(EMPTY, Literal.NULL, Literal.NULL)));
        assertNullLiteral(rule.rule(new Substring(EMPTY, Literal.NULL, Literal.NULL, Literal.NULL)));
    }

    public void testPruneSortBeforeStats() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | where emp_no > 10
            | stats x = sum(salary) by first_name""");

        var limit = as(plan, Limit.class);
        var stats = as(limit.child(), Aggregate.class);
        var filter = as(stats.child(), Filter.class);
        as(filter.child(), EsRelation.class);
    }

    public void testDontPruneSortWithLimitBeforeStats() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | limit 100
            | stats x = sum(salary) by first_name""");

        var limit = as(plan, Limit.class);
        var stats = as(limit.child(), Aggregate.class);
        var topN = as(stats.child(), TopN.class);
        as(topN.child(), EsRelation.class);
    }

    public void testCombineOrderBy() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | sort salary""");

        var topN = as(plan, TopN.class);
        assertThat(orderNames(topN), contains("salary", "emp_no"));
        as(topN.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughEval() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | eval x = salary + 1
            | sort x""");

        var topN = as(plan, TopN.class);
        assertThat(orderNames(topN), contains("x", "emp_no"));
        var eval = as(topN.child(), Eval.class);
        as(eval.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughEvalWithTwoDefs() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | eval x = salary + 1, y = salary + 2
            | eval z = x * y
            | sort z""");

        var topN = as(plan, TopN.class);
        assertThat(orderNames(topN), contains("z", "emp_no"));
        var eval = as(topN.child(), Eval.class);
        assertThat(Expressions.names(eval.fields()), contains("x", "y", "z"));
        as(eval.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughDissect() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | dissect first_name "%{x}"
            | sort x""");

        var topN = as(plan, TopN.class);
        assertThat(orderNames(topN), contains("x", "emp_no"));
        var dissect = as(topN.child(), Dissect.class);
        as(dissect.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughGrok() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | grok first_name "%{WORD:x}"
            | sort x""");

        var topN = as(plan, TopN.class);
        assertThat(orderNames(topN), contains("x", "emp_no"));
        var grok = as(topN.child(), Grok.class);
        as(grok.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughProject() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | keep salary, emp_no
            | sort salary""");

        var keep = as(plan, Project.class);
        var topN = as(keep.child(), TopN.class);
        assertThat(orderNames(topN), contains("salary", "emp_no"));
        as(topN.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughProjectAndEval() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | rename emp_no as en
            | keep salary, en
            | eval e = en * 2
            | sort salary""");

        var keep = as(plan, Project.class);
        var topN = as(keep.child(), TopN.class);
        assertThat(orderNames(topN), contains("salary", "emp_no"));
        var eval = as(topN.child(), Eval.class);
        assertThat(Expressions.names(eval.fields()), contains("e"));
        as(eval.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughProjectWithAlias() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | rename salary as l
            | keep l, emp_no
            | sort l""");

        var keep = as(plan, Project.class);
        var topN = as(keep.child(), TopN.class);
        assertThat(orderNames(topN), contains("salary", "emp_no"));
        as(topN.child(), EsRelation.class);
    }

    public void testCombineOrderByThroughFilter() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | where emp_no > 10
            | sort salary""");

        var topN = as(plan, TopN.class);
        assertThat(orderNames(topN), contains("salary", "emp_no"));
        var filter = as(topN.child(), Filter.class);
        as(filter.child(), EsRelation.class);
    }

    private static List<String> orderNames(TopN topN) {
        return topN.order().stream().map(o -> as(o.child(), NamedExpression.class).name()).toList();
    }

    public void testCombineLimitWithOrderByThroughFilterAndEval() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort salary
            | eval x = emp_no / 2
            | where x > 20
            | sort x
            | limit 10""");

        var topN = as(plan, TopN.class);
        var filter = as(topN.child(), Filter.class);
        var eval = as(filter.child(), Eval.class);
        as(eval.child(), EsRelation.class);
    }

    public void testCombineMultipleOrderByAndLimits() {
        // expected plan:
        // from test
        // | sort salary, emp_no
        // | limit 100
        // | where salary > 1
        // | sort emp_no, first_name
        // | keep l = salary, emp_no, first_name
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no
            | rename salary as l
            | keep l, emp_no, first_name
            | sort l
            | limit 100
            | sort first_name
            | where l > 1
            | sort emp_no""");

        var keep = as(plan, Project.class);
        var topN = as(keep.child(), TopN.class);
        assertThat(orderNames(topN), contains("emp_no", "first_name"));
        var filter = as(topN.child(), Filter.class);
        var topN2 = as(filter.child(), TopN.class);
        assertThat(orderNames(topN2), contains("salary", "emp_no"));
        as(topN2.child(), EsRelation.class);
    }

    public void testDontPruneSameFieldDifferentDirectionSortClauses() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort salary nulls last, emp_no desc nulls first
            | where salary > 2
            | eval e = emp_no * 2
            | keep salary, emp_no, e
            | sort e, emp_no, salary desc, emp_no desc""");

        var keep = as(plan, Project.class);
        var topN = as(keep.child(), TopN.class);
        assertThat(
            topN.order(),
            contains(
                new Order(
                    EMPTY,
                    new ReferenceAttribute(EMPTY, "e", INTEGER, null, Nullability.TRUE, null, false),
                    Order.OrderDirection.ASC,
                    Order.NullsPosition.LAST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")),
                    Order.OrderDirection.ASC,
                    Order.NullsPosition.LAST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "salary", mapping.get("salary")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.FIRST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.FIRST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "salary", mapping.get("salary")),
                    Order.OrderDirection.ASC,
                    Order.NullsPosition.LAST
                )
            )
        );
        assertThat(topN.child().collect(OrderBy.class::isInstance), is(emptyList()));
    }

    public void testPruneRedundantSortClauses() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort salary desc nulls last, emp_no desc nulls first
            | where salary > 2
            | eval e = emp_no * 2
            | keep salary, emp_no, e
            | sort e, emp_no desc, salary desc, emp_no desc nulls last""");

        var project = as(plan, Project.class);
        var topN = as(project.child(), TopN.class);
        assertThat(
            topN.order(),
            contains(
                new Order(
                    EMPTY,
                    new ReferenceAttribute(EMPTY, "e", INTEGER, null, Nullability.TRUE, null, false),
                    Order.OrderDirection.ASC,
                    Order.NullsPosition.LAST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.FIRST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "salary", mapping.get("salary")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.FIRST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.LAST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "salary", mapping.get("salary")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.LAST
                )
            )
        );
        assertThat(topN.child().collect(OrderBy.class::isInstance), is(emptyList()));
    }

    public void testDontPruneSameFieldDifferentDirectionSortClauses_UsingAlias() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no desc
            | rename emp_no as e
            | keep e
            | sort e""");

        var keep = as(plan, Project.class);
        var topN = as(keep.child(), TopN.class);
        assertThat(
            topN.order(),
            contains(
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")),
                    Order.OrderDirection.ASC,
                    Order.NullsPosition.LAST
                ),
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.FIRST
                )
            )
        );
    }

    public void testPruneRedundantSortClausesUsingAlias() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | sort emp_no desc
            | rename emp_no as e
            | keep e
            | sort e desc""");

        var project = as(plan, Project.class);
        var topN = as(project.child(), TopN.class);
        assertThat(
            topN.order(),
            contains(
                new Order(
                    EMPTY,
                    new FieldAttribute(EMPTY, "emp_no", mapping.get("emp_no")),
                    Order.OrderDirection.DESC,
                    Order.NullsPosition.FIRST
                )
            )
        );
    }

    public void testSimplifyLikeNoWildcard() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where first_name like "foo"
            """);
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        assertTrue(filter.condition() instanceof Equals);
        Equals equals = as(filter.condition(), Equals.class);
        assertEquals(BytesRefs.toBytesRef("foo"), equals.right().fold());
        assertTrue(filter.child() instanceof EsRelation);
    }

    public void testSimplifyLikeMatchAll() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where first_name like "*"
            """);
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        as(filter.condition(), IsNotNull.class);
        assertTrue(filter.child() instanceof EsRelation);
    }

    public void testSimplifyRLikeNoWildcard() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where first_name rlike "foo"
            """);
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        assertTrue(filter.condition() instanceof Equals);
        Equals equals = as(filter.condition(), Equals.class);
        assertEquals(BytesRefs.toBytesRef("foo"), equals.right().fold());
        assertTrue(filter.child() instanceof EsRelation);
    }

    public void testSimplifyRLikeMatchAll() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where first_name rlike ".*"
            """);
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);

        var isNotNull = as(filter.condition(), IsNotNull.class);
        assertTrue(filter.child() instanceof EsRelation);
    }

    public void testFoldNullInToLocalRelation() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where null in (first_name, ".*")
            """);
        assertThat(plan, instanceOf(LocalRelation.class));
    }

    public void testFoldNullListInToLocalRelation() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where first_name in (null, null)
            """);
        assertThat(plan, instanceOf(LocalRelation.class));
    }

    public void testFoldInKeyword() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where "foo" in ("bar", "baz")
            """);
        assertThat(plan, instanceOf(LocalRelation.class));

        plan = optimizedPlan("""
            from test
            | where "foo" in ("bar", "foo", "baz")
            """);
        var limit = as(plan, Limit.class);
        as(limit.child(), EsRelation.class);
    }

    public void testFoldInIP() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where to_ip("1.1.1.1") in (to_ip("1.1.1.2"), to_ip("1.1.1.2"))
            """);
        assertThat(plan, instanceOf(LocalRelation.class));

        plan = optimizedPlan("""
            from test
            | where to_ip("1.1.1.1") in (to_ip("1.1.1.1"), to_ip("1.1.1.2"))
            """);
        var limit = as(plan, Limit.class);
        as(limit.child(), EsRelation.class);
    }

    public void testFoldInVersion() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where to_version("1.2.3") in (to_version("1"), to_version("1.2.4"))
            """);
        assertThat(plan, instanceOf(LocalRelation.class));

        plan = optimizedPlan("""
            from test
            | where to_version("1.2.3") in (to_version("1"), to_version("1.2.3"))
            """);
        var limit = as(plan, Limit.class);
        as(limit.child(), EsRelation.class);
    }

    public void testFoldInNumerics() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | where 3 in (4.0, 5, 2147483648)
            """);
        assertThat(plan, instanceOf(LocalRelation.class));

        plan = optimizedPlan("""
            from test
            | where 3 in (4.0, 3.0, to_long(3))
            """);
        var limit = as(plan, Limit.class);
        as(limit.child(), EsRelation.class);
    }

    public void testFoldInEval() {
        var plan = optimizedPlan("""
            from test
            | eval a = 1, b = a + 1, c = b + a
            | where c > 10
            """);

        var local = as(plan, LocalRelation.class);
        assertThat(local.supplier(), is(LocalSupplier.EMPTY));
    }

    public void testFoldFromRow() {
        var plan = optimizedPlan("""
              row a = 1, b = 2, c = 3
            | where c > 10
            """);

        as(plan, LocalRelation.class);
    }

    public void testFoldFromRowInEval() {
        var plan = optimizedPlan("""
              row a = 1, b = 2, c = 3
            | eval x = c
            | where x > 10
            """);

        as(plan, LocalRelation.class);
    }

    public void testInvalidFoldDueToReplacement() {
        var plan = optimizedPlan("""
              from test
            | eval x = 1
            | eval x = emp_no
            | where x > 10
            | keep x
            """);

        var project = as(plan, Project.class);
        assertThat(Expressions.names(project.projections()), contains("x"));
        var child = aliased(project.projections().get(0), FieldAttribute.class);
        assertThat(Expressions.name(child), is("emp_no"));
        var limit = as(project.child(), Limit.class);
        var filter = as(limit.child(), Filter.class);
        var source = as(filter.child(), EsRelation.class);
    }

    public void testEnrich() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | eval x = to_string(languages)
            | enrich languages_idx on x
            """);
        var enrich = as(plan, Enrich.class);
        assertTrue(enrich.policyName().resolved());
        assertThat(enrich.policyName().fold(), is(BytesRefs.toBytesRef("languages_idx")));
        var eval = as(enrich.child(), Eval.class);
        var limit = as(eval.child(), Limit.class);
        as(limit.child(), EsRelation.class);
    }

    public void testPushDownEnrichPastProject() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | eval a = to_string(languages)
            | rename a as x
            | keep x
            | enrich languages_idx on x
            """);

        var keep = as(plan, Project.class);
        as(keep.child(), Enrich.class);
    }

    public void testTopNEnrich() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | rename languages as x
            | eval x = to_string(x)
            | keep x
            | enrich languages_idx on x
            | sort language_name
            """);

        var keep = as(plan, Project.class);
        var topN = as(keep.child(), TopN.class);
        as(topN.child(), Enrich.class);
    }

    public void testEnrichNotNullFilter() {
        LogicalPlan plan = optimizedPlan("""
            from test
            | eval x = to_string(languages)
            | enrich languages_idx on x
            | where language_name is not null
            | limit 10
            """);
        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);
        var enrich = as(filter.child(), Enrich.class);
        assertTrue(enrich.policyName().resolved());
        assertThat(enrich.policyName().fold(), is(BytesRefs.toBytesRef("languages_idx")));
        var eval = as(enrich.child(), Eval.class);
        as(eval.child(), EsRelation.class);
    }

    /**
     * Expects
     * EsqlProject[[a{r}#3, last_name{f}#9]]
     * \_Eval[[__a_SUM_123{r}#12 / __a_COUNT_150{r}#13 AS a]]
     *   \_Limit[10000[INTEGER]]
     *     \_Aggregate[[last_name{f}#9],[SUM(salary{f}#10) AS __a_SUM_123, COUNT(salary{f}#10) AS __a_COUNT_150, last_nam
     * e{f}#9]]
     *       \_EsRelation[test][_meta_field{f}#11, emp_no{f}#5, first_name{f}#6, !g..]
     */
    public void testSimpleAvgReplacement() {
        var plan = plan("""
              from test
            | stats a = avg(salary) by last_name
            """);

        var project = as(plan, Project.class);
        assertThat(Expressions.names(project.projections()), contains("a", "last_name"));
        var eval = as(project.child(), Eval.class);
        var f = eval.fields();
        assertThat(f, hasSize(1));
        assertThat(f.get(0).name(), is("a"));
        var limit = as(eval.child(), Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        var a = as(aggs.get(0), Alias.class);
        assertThat(a.name(), startsWith("__a_SUM@"));
        var sum = as(a.child(), Sum.class);

        a = as(aggs.get(1), Alias.class);
        assertThat(a.name(), startsWith("__a_COUNT@"));
        var count = as(a.child(), Count.class);

        assertThat(Expressions.names(agg.groupings()), contains("last_name"));
    }

    /**
     * Expects
     * EsqlProject[[a{r}#3, c{r}#6, s{r}#9, last_name{f}#15]]
     * \_Eval[[s{r}#9 / c{r}#6 AS a]]
     *   \_Limit[10000[INTEGER]]
     *     \_Aggregate[[last_name{f}#15],[COUNT(salary{f}#16) AS c, SUM(salary{f}#16) AS s, last_name{f}#15]]
     *       \_EsRelation[test][_meta_field{f}#17, emp_no{f}#11, first_name{f}#12, ..]
     */
    public void testClashingAggAvgReplacement() {
        var plan = plan("""
            from test
            | stats a = avg(salary), c = count(salary), s = sum(salary) by last_name
            """);

        assertThat(Expressions.names(plan.output()), contains("a", "c", "s", "last_name"));
        var project = as(plan, EsqlProject.class);
        var eval = as(project.child(), Eval.class);
        var f = eval.fields();
        assertThat(f, hasSize(1));
        assertThat(f.get(0).name(), is("a"));
        var limit = as(eval.child(), Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("c", "s", "last_name"));
    }

    /**
     * Expects
     * EsqlProject[[a{r}#3, c{r}#6, s{r}#9, last_name{f}#15]]
     * \_Eval[[s{r}#9 / __a_COUNT@xxx{r}#18 AS a]]
     *   \_Limit[10000[INTEGER]]
     *     \_Aggregate[[last_name{f}#15],[COUNT(salary{f}#16) AS __a_COUNT@xxx, COUNT(languages{f}#14) AS c, SUM(salary{f}#16) AS
     *  s, last_name{f}#15]]
     *       \_EsRelation[test][_meta_field{f}#17, emp_no{f}#11, first_name{f}#12, ..]
     */
    public void testSemiClashingAvgReplacement() {
        var plan = plan("""
            from test
            | stats a = avg(salary), c = count(languages), s = sum(salary) by last_name
            """);

        var project = as(plan, Project.class);
        assertThat(Expressions.names(project.projections()), contains("a", "c", "s", "last_name"));
        var eval = as(project.child(), Eval.class);
        var f = eval.fields();
        assertThat(f, hasSize(1));
        assertThat(f.get(0).name(), is("a"));
        var limit = as(eval.child(), Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        var a = as(aggs.get(0), Alias.class);
        assertThat(a.name(), startsWith("__a_COUNT@"));
        var sum = as(a.child(), Count.class);

        a = as(aggs.get(1), Alias.class);
        assertThat(a.name(), is("c"));
        var count = as(a.child(), Count.class);

        a = as(aggs.get(2), Alias.class);
        assertThat(a.name(), is("s"));
    }

    /**
     * Expected
     * Limit[10000[INTEGER]]
     * \_Aggregate[[last_name{f}#9],[PERCENTILE(salary{f}#10,50[INTEGER]) AS m, last_name{f}#9]]
     *   \_EsRelation[test][_meta_field{f}#11, emp_no{f}#5, first_name{f}#6, !g..]
     */
    public void testMedianReplacement() {
        var plan = plan("""
              from test
            | stats m = median(salary) by last_name
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        assertThat(Expressions.names(agg.aggregates()), contains("m", "last_name"));
        var aggs = agg.aggregates();
        var a = as(aggs.get(0), Alias.class);
        var per = as(a.child(), Percentile.class);
        var literal = as(per.percentile(), Literal.class);
        assertThat((int) QuantileStates.MEDIAN, is(literal.fold()));

        assertThat(Expressions.names(agg.groupings()), contains("last_name"));
    }

    public void testSplittingInWithFoldableValue() {
        FieldAttribute fa = getFieldAttribute("foo");
        In in = new In(EMPTY, ONE, List.of(TWO, THREE, fa, L(null)));
        Or expected = new Or(EMPTY, new In(EMPTY, ONE, List.of(TWO, THREE)), new In(EMPTY, ONE, List.of(fa, L(null))));
        assertThat(new LogicalPlanOptimizer.SplitInWithFoldableValue().rule(in), equalTo(expected));
    }

    public void testReplaceFilterWithExact() {
        var plan = plan("""
              from test
            | where job == "foo"
            """);

        var limit = as(plan, Limit.class);
        var filter = as(limit.child(), Filter.class);
        Equals equals = as(filter.condition(), Equals.class);
        FieldAttribute left = as(equals.left(), FieldAttribute.class);
        assertThat(left.name(), equalTo("job"));
    }

    public void testReplaceExpressionWithExact() {
        var plan = plan("""
              from test
            | eval x = job
            """);

        var eval = as(plan, Eval.class);
        var alias = as(eval.fields().get(0), Alias.class);
        var field = as(alias.child(), FieldAttribute.class);
        assertThat(field.name(), equalTo("job"));
    }

    public void testReplaceSortWithExact() {
        var plan = plan("""
              from test
            | sort job
            """);

        var topN = as(plan, TopN.class);
        assertThat(topN.order().size(), equalTo(1));
        var sortField = as(topN.order().get(0).child(), FieldAttribute.class);
        assertThat(sortField.name(), equalTo("job"));
    }

    public void testPruneUnusedEval() {
        var plan = plan("""
              from test
            | eval garbage = salary + 3
            | keep salary
            """);

        var keep = as(plan, Project.class);
        var limit = as(keep.child(), Limit.class);
        var source = as(limit.child(), EsRelation.class);
    }

    public void testPruneChainedEval() {
        var plan = plan("""
              from test
            | eval garbage_a = salary + 3
            | eval garbage_b = emp_no / garbage_a, garbage_c = garbage_a
            | eval garbage_x = 1 - garbage_b/garbage_c
            | keep salary
            """);
        var keep = as(plan, Project.class);
        var limit = as(keep.child(), Limit.class);
        var source = as(limit.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[],[COUNT(salary{f}#1345) AS c]]
     *   \_EsRelation[test][_meta_field{f}#1346, emp_no{f}#1340, first_name{f}#..]
     */
    public void testPruneEvalDueToStats() {
        var plan = plan("""
              from test
            | eval garbage_a = salary + 3, x = salary
            | eval garbage_b = x + 3
            | stats c = count(x)
            """);

        var limit = as(plan, Limit.class);
        var aggregate = as(limit.child(), Aggregate.class);
        var aggs = aggregate.aggregates();
        assertThat(Expressions.names(aggs), contains("c"));
        aggFieldName(aggs.get(0), Count.class, "salary");
        var source = as(aggregate.child(), EsRelation.class);
    }

    public void testPruneUnusedAggSimple() {
        var plan = plan("""
              from test
            | stats c = count(salary), max = max(salary), min = min(salary)
            | keep c
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        assertThat(agg.groupings(), hasSize(0));
        assertThat(agg.aggregates(), hasSize(1));
        var aggOne = as(agg.aggregates().get(0), Alias.class);
        assertThat(aggOne.name(), is("c"));
        var count = as(aggOne.child(), Count.class);
        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[],[COUNT(salary{f}#19) AS x]]
     *   \_EsRelation[test][_meta_field{f}#20, emp_no{f}#14, first_name{f}#15, ..]
     */
    public void testPruneUnusedAggMixedWithEval() {
        var plan = plan("""
              from test
            | stats c = count(salary), max = max(salary), min = min(salary)
            | eval x = c
            | keep x
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        assertThat(agg.groupings(), hasSize(0));
        var aggs = agg.aggregates();
        assertThat(aggs, hasSize(1));
        assertThat(Expressions.names(aggs), contains("x"));
        aggFieldName(agg.aggregates().get(0), Count.class, "salary");
        var source = as(agg.child(), EsRelation.class);
    }

    public void testPruneUnusedAggsChainedAgg() {
        var plan = plan("""
              from test
            | stats c = count(salary), max = max(salary), min = min(salary)
            | eval x = max + min + c
            | eval y = min
            | eval z = c
            | keep c
            """);

        var project = as(plan, Project.class);
        var limit = as(project.child(), Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        assertThat(agg.groupings(), hasSize(0));
        var aggs = agg.aggregates();
        assertThat(aggs, hasSize(1));
        assertThat(Expressions.names(aggs), contains("c"));
        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Project[[c{r}#342]]
     * \_Limit[500[INTEGER]]
     *   \_Filter[min{r}#348 > 10[INTEGER]]
     *     \_Aggregate[[],[COUNT(salary{f}#367) AS c, MIN(salary{f}#367) AS min]]
     *       \_EsRelation[test][_meta_field{f}#368, emp_no{f}#362, first_name{f}#36..]
     */
    public void testPruneMixedAggInsideUnusedEval() {
        var plan = plan("""
              from test
            | stats c = count(salary), max = max(salary), min = min(salary)
            | eval x = max + min + c
            | eval y = min
            | where y > 10
            | eval z = c
            | keep c
            """);

        var project = as(plan, Project.class);
        var limit = as(project.child(), Limit.class);
        var filter = as(limit.child(), Filter.class);
        var agg = as(filter.child(), Aggregate.class);
        assertThat(agg.groupings(), hasSize(0));
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("c", "min"));
        aggFieldName(aggs.get(0), Count.class, "salary");
        aggFieldName(aggs.get(1), Min.class, "salary");
        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Eval[[max{r}#6 + min{r}#9 + c{r}#3 AS x, min{r}#9 AS y, c{r}#3 AS z]]
     * \_Limit[500[INTEGER]]
     *   \_Aggregate[[],[COUNT(salary{f}#26) AS c, MAX(salary{f}#26) AS max, MIN(salary{f}#26) AS min]]
     *     \_EsRelation[test][_meta_field{f}#27, emp_no{f}#21, first_name{f}#22, ..]
     */
    public void testNoPruningWhenDealingJustWithEvals() {
        var plan = plan("""
              from test
            | stats c = count(salary), max = max(salary), min = min(salary)
            | eval x = max + min + c
            | eval y = min
            | eval z = c
            """);

        var eval = as(plan, Eval.class);
        var limit = as(eval.child(), Limit.class);
        var agg = as(limit.child(), Aggregate.class);
    }

    /**
     * Expects
     * Project[[y{r}#6 AS z]]
     * \_Eval[[emp_no{f}#11 + 1[INTEGER] AS y]]
     *   \_Limit[500[INTEGER]]
     *     \_EsRelation[test][_meta_field{f}#17, emp_no{f}#11, first_name{f}#12, ..]
     */
    public void testNoPruningWhenChainedEvals() {
        var plan = plan("""
              from test
            | eval x = emp_no, y = x + 1, z = y
            | keep z
            """);

        var project = as(plan, Project.class);
        assertThat(Expressions.names(project.projections()), contains("z"));
        var eval = as(project.child(), Eval.class);
        assertThat(Expressions.names(eval.fields()), contains("y"));
        var limit = as(eval.child(), Limit.class);
        var source = as(limit.child(), EsRelation.class);
    }

    /**
     * Expects
     * Project[[salary{f}#20 AS x, emp_no{f}#15 AS y]]
     * \_Limit[500[INTEGER]]
     *   \_EsRelation[test][_meta_field{f}#21, emp_no{f}#15, first_name{f}#16, ..]
     */
    public void testPruningDuplicateEvals() {
        var plan = plan("""
              from test
            | eval x = emp_no, x = salary
            | eval y = salary
            | eval y = emp_no
            | keep x, y
            """);

        var project = as(plan, Project.class);
        var projections = project.projections();
        assertThat(Expressions.names(projections), contains("x", "y"));
        var child = aliased(projections.get(0), FieldAttribute.class);
        assertThat(child.name(), is("salary"));
        child = aliased(projections.get(1), FieldAttribute.class);
        assertThat(child.name(), is("emp_no"));

        var limit = as(project.child(), Limit.class);
        var source = as(limit.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[],[COUNT(salary{f}#24) AS cx, COUNT(emp_no{f}#19) AS cy]]
     *   \_EsRelation[test][_meta_field{f}#25, emp_no{f}#19, first_name{f}#20, ..]
     */
    public void testPruneEvalAliasOnAggUngrouped() {
        var plan = plan("""
              from test
            | eval x = emp_no, x = salary
            | eval y = salary
            | eval y = emp_no
            | stats cx = count(x), cy = count(y)
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cx", "cy"));
        aggFieldName(aggs.get(0), Count.class, "salary");
        aggFieldName(aggs.get(1), Count.class, "emp_no");
        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[x{r}#6],[COUNT(emp_no{f}#17) AS cy, salary{f}#22 AS x]]
     *   \_EsRelation[test][_meta_field{f}#23, emp_no{f}#17, first_name{f}#18, ..]
     */
    public void testPruneEvalAliasOnAggGroupedByAlias() {
        var plan = plan("""
              from test
            | eval x = emp_no, x = salary
            | eval y = salary
            | eval y = emp_no
            | stats cy = count(y) by x
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "x"));
        aggFieldName(aggs.get(0), Count.class, "emp_no");
        var x = aliased(aggs.get(1), FieldAttribute.class);
        assertThat(x.name(), is("salary"));
        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[gender{f}#22],[COUNT(emp_no{f}#20) AS cy, MIN(salary{f}#25) AS cx, gender{f}#22]]
     *   \_EsRelation[test][_meta_field{f}#26, emp_no{f}#20, first_name{f}#21, ..]
     */
    public void testPruneEvalAliasOnAggGrouped() {
        var plan = plan("""
              from test
            | eval x = emp_no, x = salary
            | eval y = salary
            | eval y = emp_no
            | stats cy = count(y), cx = min(x) by gender
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "cx", "gender"));
        aggFieldName(aggs.get(0), Count.class, "emp_no");
        aggFieldName(aggs.get(1), Min.class, "salary");
        var by = as(aggs.get(2), FieldAttribute.class);
        assertThat(Expressions.name(by), is("gender"));
        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[gender{f}#21],[COUNT(emp_no{f}#19) AS cy, MIN(salary{f}#24) AS cx, gender{f}#21]]
     *   \_EsRelation[test][_meta_field{f}#25, emp_no{f}#19, first_name{f}#20, ..]
     */
    public void testPruneEvalAliasMixedWithRenameOnAggGrouped() {
        var plan = plan("""
              from test
            | eval x = emp_no, x = salary
            | rename salary as x
            | eval y = emp_no
            | stats cy = count(y), cx = min(x) by gender
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "cx", "gender"));
        aggFieldName(aggs.get(0), Count.class, "emp_no");
        aggFieldName(aggs.get(1), Min.class, "salary");
        var by = as(aggs.get(2), FieldAttribute.class);
        assertThat(Expressions.name(by), is("gender"));
        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[gender{f}#19],[COUNT(x{r}#3) AS cy, MIN(x{r}#3) AS cx, gender{f}#19]]
     *   \_Eval[[emp_no{f}#17 + 1[INTEGER] AS x]]
     *     \_EsRelation[test][_meta_field{f}#23, emp_no{f}#17, first_name{f}#18, ..]
     */
    public void testEvalAliasingAcrossCommands() {
        var plan = plan("""
              from test
            | eval x = emp_no + 1
            | eval y = x
            | eval z = y + 1
            | stats cy = count(y), cx = min(x) by gender
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "cx", "gender"));
        aggFieldName(aggs.get(0), Count.class, "x");
        aggFieldName(aggs.get(1), Min.class, "x");
        var by = as(aggs.get(2), FieldAttribute.class);
        assertThat(Expressions.name(by), is("gender"));
        var eval = as(agg.child(), Eval.class);
        assertThat(Expressions.names(eval.fields()), contains("x"));
        var source = as(eval.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[gender{f}#19],[COUNT(x{r}#3) AS cy, MIN(x{r}#3) AS cx, gender{f}#19]]
     *   \_Eval[[emp_no{f}#17 + 1[INTEGER] AS x]]
     *     \_EsRelation[test][_meta_field{f}#23, emp_no{f}#17, first_name{f}#18, ..]
     */
    public void testEvalAliasingInsideSameCommand() {
        var plan = plan("""
              from test
            | eval x = emp_no + 1, y = x, z = y + 1
            | stats cy = count(y), cx = min(x) by gender
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "cx", "gender"));
        aggFieldName(aggs.get(0), Count.class, "x");
        aggFieldName(aggs.get(1), Min.class, "x");
        var by = as(aggs.get(2), FieldAttribute.class);
        assertThat(Expressions.name(by), is("gender"));
        var eval = as(agg.child(), Eval.class);
        assertThat(Expressions.names(eval.fields()), contains("x"));
        var source = as(eval.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[gender{f}#22],[COUNT(z{r}#9) AS cy, MIN(x{r}#3) AS cx, gender{f}#22]]
     *   \_Eval[[emp_no{f}#20 + 1[INTEGER] AS x, x{r}#3 + 1[INTEGER] AS z]]
     *     \_EsRelation[test][_meta_field{f}#26, emp_no{f}#20, first_name{f}#21, ..]
     */
    public void testEvalAliasingInsideSameCommandWithShadowing() {
        var plan = plan("""
              from test
            | eval x = emp_no + 1, y = x, z = y + 1, y = z
            | stats cy = count(y), cx = min(x) by gender
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "cx", "gender"));
        aggFieldName(aggs.get(0), Count.class, "z");
        aggFieldName(aggs.get(1), Min.class, "x");
        var by = as(aggs.get(2), FieldAttribute.class);
        assertThat(Expressions.name(by), is("gender"));
        var eval = as(agg.child(), Eval.class);
        assertThat(Expressions.names(eval.fields()), contains("x", "z"));
        var source = as(eval.child(), EsRelation.class);
    }

    public void testPruneRenameOnAgg() {
        var plan = plan("""
              from test
            | rename emp_no as x
            | rename salary as y
            | stats cy = count(y), cx = min(x) by gender
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "cx", "gender"));
        aggFieldName(aggs.get(0), Count.class, "salary");
        aggFieldName(aggs.get(1), Min.class, "emp_no");

        var source = as(agg.child(), EsRelation.class);
    }

    /**
     * Expects
     * Limit[500[INTEGER]]
     * \_Aggregate[[gender{f}#14],[COUNT(salary{f}#17) AS cy, MIN(emp_no{f}#12) AS cx, gender{f}#14]]
     *   \_EsRelation[test][_meta_field{f}#18, emp_no{f}#12, first_name{f}#13, ..]
     */
    public void testPruneRenameOnAggBy() {
        var plan = plan("""
              from test
            | rename emp_no as x
            | rename salary as y, gender as g
            | stats cy = count(y), cx = min(x) by g
            """);

        var limit = as(plan, Limit.class);
        var agg = as(limit.child(), Aggregate.class);
        var aggs = agg.aggregates();
        assertThat(Expressions.names(aggs), contains("cy", "cx", "g"));
        aggFieldName(aggs.get(0), Count.class, "salary");
        aggFieldName(aggs.get(1), Min.class, "emp_no");
        var groupby = aliased(aggs.get(2), FieldAttribute.class);
        assertThat(Expressions.name(groupby), is("gender"));

        var source = as(agg.child(), EsRelation.class);
    }

    private <T> T aliased(Expression exp, Class<T> clazz) {
        var alias = as(exp, Alias.class);
        return as(alias.child(), clazz);
    }

    private <T extends AggregateFunction> void aggFieldName(Expression exp, Class<T> aggType, String fieldName) {
        var alias = as(exp, Alias.class);
        var af = as(alias.child(), aggType);
        var field = af.field();
        assertThat(Expressions.name(field), is(fieldName));
    }

    private LogicalPlan optimizedPlan(String query) {
        return plan(query);
    }

    private LogicalPlan plan(String query) {
        var analyzed = analyzer.analyze(parser.createStatement(query));
        // System.out.println(analyzed);
        var optimized = logicalOptimizer.optimize(analyzed);
        // System.out.println(optimized);
        return optimized;
    }

    private void assertNullLiteral(Expression expression) {
        assertEquals(Literal.class, expression.getClass());
        assertNull(expression.fold());
    }

    // TODO: move these from org.elasticsearch.xpack.ql.optimizer.OptimizerRulesTests to org.elasticsearch.xpack.ql.TestUtils
    private static FieldAttribute getFieldAttribute(String name) {
        return getFieldAttribute(name, INTEGER);
    }

    private static FieldAttribute getFieldAttribute(String name, DataType dataType) {
        return new FieldAttribute(EMPTY, name, new EsField(name + "f", dataType, emptyMap(), true));
    }

    public static WildcardLike wildcardLike(Expression left, String exp) {
        return new WildcardLike(EMPTY, left, new WildcardPattern(exp));
    }

    public static RLike rlike(Expression left, String exp) {
        return new RLike(EMPTY, left, new RLikePattern(exp));
    }

}
