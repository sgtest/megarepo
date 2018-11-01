/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.analysis.analyzer;

import org.elasticsearch.xpack.sql.capabilities.Unresolvable;
import org.elasticsearch.xpack.sql.expression.Alias;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.AttributeSet;
import org.elasticsearch.xpack.sql.expression.Exists;
import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.Expressions;
import org.elasticsearch.xpack.sql.expression.FieldAttribute;
import org.elasticsearch.xpack.sql.expression.UnresolvedAttribute;
import org.elasticsearch.xpack.sql.expression.function.Function;
import org.elasticsearch.xpack.sql.expression.function.FunctionAttribute;
import org.elasticsearch.xpack.sql.expression.function.Functions;
import org.elasticsearch.xpack.sql.expression.function.Score;
import org.elasticsearch.xpack.sql.expression.function.scalar.ScalarFunction;
import org.elasticsearch.xpack.sql.expression.predicate.operator.comparison.In;
import org.elasticsearch.xpack.sql.plan.logical.Aggregate;
import org.elasticsearch.xpack.sql.plan.logical.Distinct;
import org.elasticsearch.xpack.sql.plan.logical.Filter;
import org.elasticsearch.xpack.sql.plan.logical.LogicalPlan;
import org.elasticsearch.xpack.sql.plan.logical.OrderBy;
import org.elasticsearch.xpack.sql.plan.logical.Project;
import org.elasticsearch.xpack.sql.tree.Node;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.util.StringUtils;

import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.function.Consumer;

import static java.lang.String.format;

final class Verifier {

    private Verifier() {}

    static class Failure {
        private final Node<?> source;
        private final String message;

        Failure(Node<?> source, String message) {
            this.source = source;
            this.message = message;
        }

        Node<?> source() {
            return source;
        }

        String message() {
            return message;
        }

        @Override
        public int hashCode() {
            return source.hashCode();
        }

        @Override
        public boolean equals(Object obj) {
            if (this == obj) {
                return true;
            }

            if (obj == null || getClass() != obj.getClass()) {
                return false;
            }

            Verifier.Failure other = (Verifier.Failure) obj;
            return Objects.equals(source, other.source);
        }

        @Override
        public String toString() {
            return message;
        }
    }

    private static Failure fail(Node<?> source, String message, Object... args) {
        return new Failure(source, format(Locale.ROOT, message, args));
    }

    static Collection<Failure> verify(LogicalPlan plan) {
        Set<Failure> failures = new LinkedHashSet<>();

        // start bottom-up
        plan.forEachUp(p -> {

            if (p.analyzed()) {
                return;
            }

            // if the children are unresolved, this node will also so counting it will only add noise
            if (!p.childrenResolved()) {
                return;
            }

            Set<Failure> localFailures = new LinkedHashSet<>();

            //
            // First handle usual suspects
            //

            if (p instanceof Unresolvable) {
                localFailures.add(fail(p, ((Unresolvable) p).unresolvedMessage()));
            } else if (p instanceof Distinct) {
                localFailures.add(fail(p, "SELECT DISTINCT is not yet supported"));
            } else {
                // then take a look at the expressions
                p.forEachExpressions(e -> {
                    // everything is fine, skip expression
                    if (e.resolved()) {
                        return;
                    }

                    e.forEachUp(ae -> {
                        // we're only interested in the children
                        if (!ae.childrenResolved()) {
                            return;
                        }
                        // again the usual suspects
                        if (ae instanceof Unresolvable) {
                            // handle Attributes different to provide more context
                            if (ae instanceof UnresolvedAttribute) {
                                UnresolvedAttribute ua = (UnresolvedAttribute) ae;
                                // only work out the synonyms for raw unresolved attributes
                                if (!ua.customMessage()) {
                                    boolean useQualifier = ua.qualifier() != null;
                                    List<String> potentialMatches = new ArrayList<>();
                                    for (Attribute a : p.intputSet()) {
                                        String nameCandidate = useQualifier ? a.qualifiedName() : a.name();
                                        // add only primitives (object types would only result in another error)
                                        if (!(a.dataType() == DataType.UNSUPPORTED) && a.dataType().isPrimitive()) {
                                            potentialMatches.add(nameCandidate);
                                        }
                                    }

                                    List<String> matches = StringUtils.findSimilar(ua.qualifiedName(), potentialMatches);
                                    if (!matches.isEmpty()) {
                                        ae = ua.withUnresolvedMessage(UnresolvedAttribute.errorMessage(ua.qualifiedName(), matches));
                                    }
                                }
                            }

                            localFailures.add(fail(ae, ((Unresolvable) ae).unresolvedMessage()));
                            return;
                        }
                        // type resolution
                        if (ae.typeResolved().unresolved()) {
                            localFailures.add(fail(ae, ae.typeResolved().message()));
                        } else if (ae instanceof Exists) {
                            localFailures.add(fail(ae, "EXISTS is not yet supported"));
                        }
                    });
                });
            }
            failures.addAll(localFailures);
        });

        // Concrete verifications

        // if there are no (major) unresolved failures, do more in-depth analysis

        if (failures.isEmpty()) {
            // collect Function to better reason about encountered attributes
            Map<String, Function> resolvedFunctions = Functions.collectFunctions(plan);

            // for filtering out duplicated errors
            final Set<LogicalPlan> groupingFailures = new LinkedHashSet<>();

            plan.forEachDown(p -> {
                if (p.analyzed()) {
                    return;
                }

                // if the children are unresolved, so will this node; counting it will only add noise
                if (!p.childrenResolved()) {
                    return;
                }

                Set<Failure> localFailures = new LinkedHashSet<>();

                validateInExpression(p, localFailures);

                if (!groupingFailures.contains(p)) {
                    checkGroupBy(p, localFailures, resolvedFunctions, groupingFailures);
                }

                checkForScoreInsideFunctions(p, localFailures);

                checkNestedUsedInGroupByOrHaving(p, localFailures);

                // everything checks out
                // mark the plan as analyzed
                if (localFailures.isEmpty()) {
                    p.setAnalyzed();
                }

                failures.addAll(localFailures);
            });
        }

        return failures;
    }

    /**
     * Check validity of Aggregate/GroupBy.
     * This rule is needed for multiple reasons:
     * 1. a user might specify an invalid aggregate (SELECT foo GROUP BY bar)
     * 2. the ORDER BY/HAVING might contain a non-grouped attribute. This is typically
     * caught by the Analyzer however if wrapped in a function (ABS()) it gets resolved
     * (because the expression gets resolved little by little without being pushed down,
     * without the Analyzer modifying anything.
     * 2a. HAVING also requires an Aggregate function
     * 3. composite agg (used for GROUP BY) allows ordering only on the group keys
     */
    private static boolean checkGroupBy(LogicalPlan p, Set<Failure> localFailures,
            Map<String, Function> resolvedFunctions, Set<LogicalPlan> groupingFailures) {
        return checkGroupByAgg(p, localFailures, groupingFailures, resolvedFunctions)
                && checkGroupByOrder(p, localFailures, groupingFailures, resolvedFunctions)
                && checkGroupByHaving(p, localFailures, groupingFailures, resolvedFunctions);
    }

    // check whether an orderBy failed or if it occurs on a non-key
    private static boolean checkGroupByOrder(LogicalPlan p, Set<Failure> localFailures,
            Set<LogicalPlan> groupingFailures, Map<String, Function> functions) {
        if (p instanceof OrderBy) {
            OrderBy o = (OrderBy) p;
            LogicalPlan child = o.child();

            if (child instanceof Project) {
                child = ((Project) child).child();
            }
            if (child instanceof Filter) {
                child = ((Filter) child).child();
            }

            if (child instanceof Aggregate) {
                Aggregate a = (Aggregate) child;

                Map<Expression, Node<?>> missing = new LinkedHashMap<>();
                o.order().forEach(oe -> {
                    Expression e = oe.child();
                    // cannot order by aggregates (not supported by composite)
                    if (Functions.isAggregate(e)) {
                        missing.put(e, oe);
                        return;
                    }

                    // take aliases declared inside the aggregates which point to the grouping (but are not included in there)
                    // to correlate them to the order
                    List<Expression> groupingAndMatchingAggregatesAliases = new ArrayList<>(a.groupings());

                    a.aggregates().forEach(as -> {
                        if (as instanceof Alias) {
                            Alias al = (Alias) as;
                            if (Expressions.anyMatch(a.groupings(), g -> Expressions.equalsAsAttribute(al.child(), g))) {
                                groupingAndMatchingAggregatesAliases.add(al);
                            }
                        }
                    });

                    // Make sure you can apply functions on top of the grouped by expressions in the ORDER BY:
                    // e.g.: if "GROUP BY f2(f1(field))" you can "ORDER BY f4(f3(f2(f1(field))))"
                    //
                    // Also, make sure to compare attributes directly
                    if (e.anyMatch(expression -> Expressions.anyMatch(groupingAndMatchingAggregatesAliases,
                        g -> expression.semanticEquals(expression instanceof Attribute ? Expressions.attribute(g) : g)))) {
                        return;
                    }

                    // nothing matched, cannot group by it
                    missing.put(e, oe);
                });

                if (!missing.isEmpty()) {
                    String plural = missing.size() > 1 ? "s" : StringUtils.EMPTY;
                    // get the location of the first missing expression as the order by might be on a different line
                    localFailures.add(
                            fail(missing.values().iterator().next(), "Cannot order by non-grouped column" + plural + " %s, expected %s",
                                    Expressions.names(missing.keySet()),
                                    Expressions.names(a.groupings())));
                    groupingFailures.add(a);
                    return false;
                }
            }
        }
        return true;
    }

    private static boolean checkGroupByHaving(LogicalPlan p, Set<Failure> localFailures,
            Set<LogicalPlan> groupingFailures, Map<String, Function> functions) {
        if (p instanceof Filter) {
            Filter f = (Filter) p;
            if (f.child() instanceof Aggregate) {
                Aggregate a = (Aggregate) f.child();

                Map<Expression, Node<?>> missing = new LinkedHashMap<>();
                Expression condition = f.condition();
                // variation of checkGroupMatch customized for HAVING, which requires just aggregations
                condition.collectFirstChildren(c -> checkGroupByHavingHasOnlyAggs(c, condition, missing, functions));

                if (!missing.isEmpty()) {
                    String plural = missing.size() > 1 ? "s" : StringUtils.EMPTY;
                    localFailures.add(
                            fail(condition, "Cannot filter HAVING on non-aggregate" + plural + " %s; consider using WHERE instead",
                            Expressions.names(missing.keySet())));
                    groupingFailures.add(a);
                    return false;
                }
            }
        }
        return true;
    }


    private static boolean checkGroupByHavingHasOnlyAggs(Expression e, Node<?> source,
            Map<Expression, Node<?>> missing, Map<String, Function> functions) {

        // resolve FunctionAttribute to backing functions
        if (e instanceof FunctionAttribute) {
            FunctionAttribute fa = (FunctionAttribute) e;
            Function function = functions.get(fa.functionId());
            // TODO: this should be handled by a different rule
            if (function == null) {
                return false;
            }
            e = function;
        }

        // scalar functions can be a binary tree
        // first test the function against the grouping
        // and if that fails, start unpacking hoping to find matches
        if (e instanceof ScalarFunction) {
            ScalarFunction sf = (ScalarFunction) e;

            // unwrap function to find the base
            for (Expression arg : sf.arguments()) {
                arg.collectFirstChildren(c -> checkGroupByHavingHasOnlyAggs(c, source, missing, functions));
            }
            return true;

        } else if (e instanceof Score) {
            // Score can't be used for having
            missing.put(e, source);
            return true;
        }

        // skip literals / foldable
        if (e.foldable()) {
            return true;
        }
        // skip aggs (allowed to refer to non-group columns)
        if (Functions.isAggregate(e)) {
            return true;
        }

        // left without leaves which have to match; that's a failure since everything should be based on an agg
        if (e instanceof Attribute) {
            missing.put(e, source);
            return true;
        }

        return false;
    }


    // check whether plain columns specified in an agg are mentioned in the group-by
    private static boolean checkGroupByAgg(LogicalPlan p, Set<Failure> localFailures,
            Set<LogicalPlan> groupingFailures, Map<String, Function> functions) {
        if (p instanceof Aggregate) {
            Aggregate a = (Aggregate) p;

            // The grouping can not be an aggregate function
            a.groupings().forEach(e -> e.forEachUp(c -> {
                if (Functions.isAggregate(c)) {
                    localFailures.add(fail(c, "Cannot use an aggregate [" + c.nodeName().toUpperCase(Locale.ROOT) + "] for grouping"));
                }
                if (c instanceof Score) {
                    localFailures.add(fail(c, "Cannot use [SCORE()] for grouping"));
                }
            }));

            if (!localFailures.isEmpty()) {
                return false;
            }

            // The agg can be:
            // 1. plain column - in which case, there should be an equivalent in groupings
            // 2. aggregate over non-grouped column
            // 3. scalar function on top of 1 and/or 2. the function needs unfolding to make sure
            //    the 'source' is valid.

            // Note that grouping can be done by a function (GROUP BY YEAR(date)) which means date
            // cannot be used as a plain column, only YEAR(date) or aggs(?) on top of it

            Map<Expression, Node<?>> missing = new LinkedHashMap<>();
            a.aggregates().forEach(ne ->
            ne.collectFirstChildren(c -> checkGroupMatch(c, ne, a.groupings(), missing, functions)));

            if (!missing.isEmpty()) {
                String plural = missing.size() > 1 ? "s" : StringUtils.EMPTY;
                localFailures.add(fail(missing.values().iterator().next(), "Cannot use non-grouped column" + plural + " %s, expected %s",
                        Expressions.names(missing.keySet()),
                        Expressions.names(a.groupings())));
                return false;
            }
        }

        return true;
    }

    private static boolean checkGroupMatch(Expression e, Node<?> source, List<Expression> groupings,
            Map<Expression, Node<?>> missing, Map<String, Function> functions) {
        // resolve FunctionAttribute to backing functions
        if (e instanceof FunctionAttribute) {
            FunctionAttribute fa = (FunctionAttribute) e;
            Function function = functions.get(fa.functionId());
            // TODO: this should be handled by a different rule
            if (function == null) {
                return false;
            }
            e = function;
        }

        // scalar functions can be a binary tree
        // first test the function against the grouping
        // and if that fails, start unpacking hoping to find matches
        if (e instanceof ScalarFunction) {
            ScalarFunction sf = (ScalarFunction) e;

            // found group for the expression
            if (Expressions.anyMatch(groupings, e::semanticEquals)) {
                return true;
            }

            // unwrap function to find the base
            for (Expression arg : sf.arguments()) {
                arg.collectFirstChildren(c -> checkGroupMatch(c, source, groupings, missing, functions));
            }

            return true;
        } else if (e instanceof Score) {
            // Score can't be an aggregate function
            missing.put(e, source);
            return true;
        }

        // skip literals / foldable
        if (e.foldable()) {
            return true;
        }
        // skip aggs (allowed to refer to non-group columns)
        // TODO: need to check whether it's possible to agg on a field used inside a scalar for grouping
        if (Functions.isAggregate(e)) {
            return true;
        }
        // left without leaves which have to match; if not there's a failure

        final Expression exp = e;
        if (e.children().isEmpty()) {
            if (!Expressions.anyMatch(groupings, c -> exp.semanticEquals(exp instanceof Attribute ? Expressions.attribute(c) : c))) {
                missing.put(e, source);
            }
            return true;
        }
        return false;
    }

    private static void checkForScoreInsideFunctions(LogicalPlan p, Set<Failure> localFailures) {
        // Make sure that SCORE is only used in "top level" functions
        p.forEachExpressions(e ->
            e.forEachUp((Function f) ->
                f.arguments().stream()
                    .filter(exp -> exp.anyMatch(Score.class::isInstance))
                    .forEach(exp -> localFailures.add(fail(exp, "[SCORE()] cannot be an argument to a function"))),
                Function.class));
    }

    private static void checkNestedUsedInGroupByOrHaving(LogicalPlan p, Set<Failure> localFailures) {
        List<FieldAttribute> nested = new ArrayList<>();
        Consumer<FieldAttribute> match = fa -> {
            if (fa.isNested()) {
                nested.add(fa);
            }
        };

        // nested fields shouldn't be used in aggregates or having (yet)
        p.forEachDown(a -> a.groupings().forEach(agg -> agg.forEachUp(match, FieldAttribute.class)), Aggregate.class);

        if (!nested.isEmpty()) {
            localFailures.add(
                    fail(nested.get(0), "Grouping isn't (yet) compatible with nested fields " + new AttributeSet(nested).names()));
            nested.clear();
        }
        
        // check in having
        p.forEachDown(f -> {
            if (f.child() instanceof Aggregate) {
                f.condition().forEachUp(match, FieldAttribute.class);
            }
        }, Filter.class);
        
        if (!nested.isEmpty()) {
            localFailures.add(
                    fail(nested.get(0), "HAVING isn't (yet) compatible with nested fields " + new AttributeSet(nested).names()));
        }
    }

    private static void validateInExpression(LogicalPlan p, Set<Failure> localFailures) {
        p.forEachExpressions(e ->
            e.forEachUp((In in) -> {
                    DataType dt = in.value().dataType();
                    for (Expression value : in.list()) {
                        if (!in.value().dataType().isCompatibleWith(value.dataType())) {
                            localFailures.add(fail(value, "expected data type [%s], value provided is of type [%s]",
                                dt, value.dataType()));
                            return;
                        }
                    }
                },
                In.class));
    }
}
