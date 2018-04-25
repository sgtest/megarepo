/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression;

import java.util.Collections;
import java.util.List;
import java.util.Objects;

import org.elasticsearch.xpack.sql.plan.logical.LogicalPlan;
import org.elasticsearch.xpack.sql.tree.Location;

public abstract class SubQueryExpression extends Expression {

    private final LogicalPlan query;
    private final ExpressionId id;

    public SubQueryExpression(Location location, LogicalPlan query) {
        this(location, query, null);
    }

    public SubQueryExpression(Location location, LogicalPlan query, ExpressionId id) {
        super(location, Collections.emptyList());
        this.query = query;
        this.id = id == null ? new ExpressionId() : id;
    }

    @Override
    public final Expression replaceChildren(List<Expression> newChildren) {
        throw new UnsupportedOperationException("this type of node doesn't have any children to replace");
    }

    public LogicalPlan query() {
        return query;
    }

    public ExpressionId id() {
        return id;
    }

    @Override
    public boolean resolved() {
        return false;
    }

    public SubQueryExpression withQuery(LogicalPlan newQuery) {
        return (Objects.equals(query, newQuery) ? this : clone(newQuery));
    }

    protected abstract SubQueryExpression clone(LogicalPlan newQuery);

    @Override
    public int hashCode() {
        return Objects.hash(query());
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        SubQueryExpression other = (SubQueryExpression) obj;
        return Objects.equals(query(), other.query());
    }
}
