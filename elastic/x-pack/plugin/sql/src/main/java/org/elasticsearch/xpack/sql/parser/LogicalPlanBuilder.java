/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.parser;

import org.antlr.v4.runtime.Token;
import org.antlr.v4.runtime.tree.TerminalNode;
import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.Literal;
import org.elasticsearch.xpack.sql.expression.NamedExpression;
import org.elasticsearch.xpack.sql.expression.Order;
import org.elasticsearch.xpack.sql.expression.UnresolvedAlias;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.AliasedQueryContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.AliasedRelationContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.FromClauseContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.GroupByContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.JoinCriteriaContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.JoinRelationContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.JoinTypeContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.NamedQueryContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.QueryContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.QueryNoWithContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.QuerySpecificationContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.RelationContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.SetQuantifierContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.SubqueryContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.TableNameContext;
import org.elasticsearch.xpack.sql.plan.TableIdentifier;
import org.elasticsearch.xpack.sql.plan.logical.Aggregate;
import org.elasticsearch.xpack.sql.plan.logical.Distinct;
import org.elasticsearch.xpack.sql.plan.logical.Filter;
import org.elasticsearch.xpack.sql.plan.logical.Join;
import org.elasticsearch.xpack.sql.plan.logical.Join.JoinType;
import org.elasticsearch.xpack.sql.plan.logical.Limit;
import org.elasticsearch.xpack.sql.plan.logical.LocalRelation;
import org.elasticsearch.xpack.sql.plan.logical.LogicalPlan;
import org.elasticsearch.xpack.sql.plan.logical.OrderBy;
import org.elasticsearch.xpack.sql.plan.logical.Project;
import org.elasticsearch.xpack.sql.plan.logical.SubQueryAlias;
import org.elasticsearch.xpack.sql.plan.logical.UnresolvedRelation;
import org.elasticsearch.xpack.sql.plan.logical.With;
import org.elasticsearch.xpack.sql.plugin.SqlTypedParamValue;
import org.elasticsearch.xpack.sql.session.EmptyExecutable;
import org.elasticsearch.xpack.sql.type.DataType;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static java.util.Collections.emptyList;
import static java.util.stream.Collectors.toList;

abstract class LogicalPlanBuilder extends ExpressionBuilder {

    protected LogicalPlanBuilder(Map<Token, SqlTypedParamValue> params) {
        super(params);
    }

    @Override
    public LogicalPlan visitQuery(QueryContext ctx) {
        LogicalPlan body = plan(ctx.queryNoWith());

        List<SubQueryAlias> namedQueries = visitList(ctx.namedQuery(), SubQueryAlias.class);

        // unwrap query (and validate while at it)
        Map<String, SubQueryAlias> cteRelations = new LinkedHashMap<>(namedQueries.size());
        for (SubQueryAlias namedQuery : namedQueries) {
            if (cteRelations.put(namedQuery.alias(), namedQuery) != null) {
                throw new ParsingException(namedQuery.location(), "Duplicate alias {}", namedQuery.alias());
            }
        }

        // return WITH
        return new With(source(ctx), body, cteRelations);
    }

    @Override
    public LogicalPlan visitNamedQuery(NamedQueryContext ctx) {
        return new SubQueryAlias(source(ctx), plan(ctx.queryNoWith()), ctx.name.getText());
    }

    @Override
    public LogicalPlan visitQueryNoWith(QueryNoWithContext ctx) {
        LogicalPlan plan = plan(ctx.queryTerm());

        if (!ctx.orderBy().isEmpty()) {
            plan = new OrderBy(source(ctx.ORDER()), plan, visitList(ctx.orderBy(), Order.class));
        }

        if (ctx.limit != null && ctx.INTEGER_VALUE() != null) {
            plan = new Limit(source(ctx.limit), new Literal(source(ctx),
                    Integer.parseInt(ctx.limit.getText()), DataType.INTEGER), plan);
        }

        return plan;
    }

    @Override
    public LogicalPlan visitQuerySpecification(QuerySpecificationContext ctx) {
        LogicalPlan query;
        if (ctx.fromClause() == null) {
            query = new LocalRelation(source(ctx), new EmptyExecutable(emptyList()));
        } else {
            query = plan(ctx.fromClause());
        }

        // add WHERE
        if (ctx.where != null) {
            query = new Filter(source(ctx), query, expression(ctx.where));
        }

        List<NamedExpression> selectTarget = emptyList();

        // SELECT a, b, c ...
        if (!ctx.selectItem().isEmpty()) {
            selectTarget = expressions(ctx.selectItem()).stream()
                    .map(e -> (e instanceof NamedExpression) ? (NamedExpression) e : new UnresolvedAlias(e.location(), e))
                    .collect(toList());
        }

        // GROUP BY
        GroupByContext groupByCtx = ctx.groupBy();
        if (groupByCtx != null) {
            SetQuantifierContext setQualifierContext = groupByCtx.setQuantifier();
            TerminalNode groupByAll = setQualifierContext == null ? null : setQualifierContext.ALL();
            if (groupByAll != null) {
                throw new ParsingException(source(groupByAll), "GROUP BY ALL is not supported");
            }
            List<Expression> groupBy = expressions(groupByCtx.groupingElement());
            query = new Aggregate(source(groupByCtx), query, groupBy, selectTarget);
        }
        else if (!selectTarget.isEmpty()) {
            query = new Project(source(ctx.selectItem(0)), query, selectTarget);
        }

        // HAVING
        if (ctx.having != null) {
            query = new Filter(source(ctx.having), query, expression(ctx.having));
        }

        if (ctx.setQuantifier() != null && ctx.setQuantifier().DISTINCT() != null) {
            query = new Distinct(source(ctx.setQuantifier()), query);
        }
        return query;
    }

    @Override
    public LogicalPlan visitFromClause(FromClauseContext ctx) {
        // if there are multiple FROM clauses, convert each pair in a inner join
        List<LogicalPlan> plans = plans(ctx.relation());
        return plans.stream()
                .reduce((left, right) -> new Join(source(ctx), left, right, Join.JoinType.IMPLICIT, null))
                .get();
    }

    @Override
    public LogicalPlan visitRelation(RelationContext ctx) {
        // check if there are multiple join clauses. ANTLR produces a right nested tree with the left join clause
        // at the top. However the fields previously references might be used in the following clauses.
        // As such, swap/reverse the tree.

        LogicalPlan result = plan(ctx.relationPrimary());
        for (JoinRelationContext j : ctx.joinRelation()) {
            result = doJoin(result, j);
        }

        return result;
    }

    private Join doJoin(LogicalPlan left, JoinRelationContext ctx) {
        JoinTypeContext joinType = ctx.joinType();

        Join.JoinType type = JoinType.INNER;
        if (joinType != null) {
            if (joinType.FULL() != null) {
                type = JoinType.FULL;
            }
            if (joinType.LEFT() != null) {
                type = JoinType.LEFT;
            }
            if (joinType.RIGHT() != null) {
                type = JoinType.RIGHT;
            }
        }

        Expression condition = null;
        JoinCriteriaContext criteria = ctx.joinCriteria();
        if (criteria != null) {
            if (criteria.USING() != null) {
                throw new UnsupportedOperationException();
            }
            if (criteria.booleanExpression() != null) {
                condition = expression(criteria.booleanExpression());
            }
        }

        // We would return this if we actually supported JOINs, but we don't yet.
        // new Join(source(ctx), left, plan(ctx.right), type, condition);
        throw new ParsingException(source(ctx), "Queries with JOIN are not yet supported");
    }

    @Override
    public Object visitAliasedRelation(AliasedRelationContext ctx) {
        return new SubQueryAlias(source(ctx), plan(ctx.relation()), visitQualifiedName(ctx.qualifiedName()));
    }

    @Override
    public Object visitAliasedQuery(AliasedQueryContext ctx) {
        return new SubQueryAlias(source(ctx), plan(ctx.queryNoWith()), visitQualifiedName(ctx.qualifiedName()));
    }

    @Override
    public Object visitSubquery(SubqueryContext ctx) {
        return plan(ctx.queryNoWith());
    }

    @Override
    public LogicalPlan visitTableName(TableNameContext ctx) {
        String alias = visitQualifiedName(ctx.qualifiedName());
        TableIdentifier tableIdentifier = visitTableIdentifier(ctx.tableIdentifier());
        return new UnresolvedRelation(source(ctx), tableIdentifier, alias);
    }
}
