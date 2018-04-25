/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression;

import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.capabilities.Resolvable;
import org.elasticsearch.xpack.sql.capabilities.Resolvables;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.Node;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.util.StringUtils;

import java.util.List;
import java.util.Locale;

import static java.lang.String.format;

/**
 * In a SQL statement, an Expression is whatever a user specifies inside an
 * action, so for instance:
 *
 * {@code SELECT a, b, MAX(c, d) FROM i}
 *
 * a, b, ABS(c), and i are all Expressions, with ABS(c) being a Function
 * (which is a type of expression) with a single child, c.
 */
public abstract class Expression extends Node<Expression> implements Resolvable {

    public static class TypeResolution {
        private final boolean failed;
        private final String message;

        public static final TypeResolution TYPE_RESOLVED = new TypeResolution(false, StringUtils.EMPTY);

        public TypeResolution(String message, Object... args) {
            this(true, format(Locale.ROOT, message, args));
        }

        private TypeResolution(boolean unresolved, String message) {
            this.failed = unresolved;
            this.message = message;
        }

        public boolean unresolved() {
            return failed;
        }

        public boolean resolved() {
            return !failed;
        }

        public String message() {
            return message;
        }
    }

    private TypeResolution lazyTypeResolution = null;
    private Boolean lazyChildrenResolved = null;
    private Expression lazyCanonical = null;

    public Expression(Location location, List<Expression> children) {
        super(location, children);
    }

    // whether the expression can be evaluated statically (folded) or not
    public boolean foldable() {
        return false;
    }

    public Object fold() {
        throw new SqlIllegalArgumentException("Should not fold expression");
    }

    public abstract boolean nullable();

    // the references/inputs/leaves of the expression tree
    public AttributeSet references() {
        return Expressions.references(children());
    }

    public boolean childrenResolved() {
        if (lazyChildrenResolved == null) {
            lazyChildrenResolved = Boolean.valueOf(Resolvables.resolved(children()));
        }
        return lazyChildrenResolved;
    }

    public final TypeResolution typeResolved() {
        if (lazyTypeResolution == null) {
            lazyTypeResolution = resolveType();
        }
        return lazyTypeResolution;
    }

    protected TypeResolution resolveType() {
        return TypeResolution.TYPE_RESOLVED;
    }

    public final Expression canonical() {
        if (lazyCanonical == null) {
            lazyCanonical = canonicalize();
        }
        return lazyCanonical;
    }

    protected Expression canonicalize() {
        return this;
    }

    public boolean semanticEquals(Expression other) {
        return canonical().equals(other.canonical());
    }

    public int semanticHash() {
        return canonical().hashCode();
    }

    @Override
    public boolean resolved() {
        return childrenResolved() && typeResolved().resolved();
    }

    public abstract DataType dataType();

    @Override
    public abstract int hashCode();

    @Override
    public String toString() {
        return nodeName() + "[" + propertiesToString(false) + "]";
    }
}
