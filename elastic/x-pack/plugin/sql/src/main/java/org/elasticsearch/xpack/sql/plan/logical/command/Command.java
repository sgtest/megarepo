/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plan.logical.command;

import org.elasticsearch.xpack.sql.expression.FieldAttribute;
import org.elasticsearch.xpack.sql.plan.logical.LogicalPlan;
import org.elasticsearch.xpack.sql.session.Executable;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.type.EsField;
import org.elasticsearch.xpack.sql.type.KeywordEsField;

import java.util.List;

import static java.util.Collections.emptyList;
import static java.util.Collections.emptyMap;

public abstract class Command extends LogicalPlan implements Executable {

    protected Command(Location location) {
        super(location, emptyList());
    }

    @Override
    public final LogicalPlan replaceChildren(List<LogicalPlan> newChildren) {
        throw new UnsupportedOperationException("this type of node doesn't have any children to replace");
    }

    @Override
    public boolean expressionsResolved() {
        return true;
    }

    /**
     * Syntactic sugar for creating a schema keyword/string field.
     */
    protected final FieldAttribute keyword(String name) {
        return field(name, new KeywordEsField(name));
    }

    /**
     * Syntactic sugar for creating a schema field.
     */
    protected final FieldAttribute field(String name, DataType type) {
        return field(name, new EsField(name, type, emptyMap(), true));
    }

    private FieldAttribute field(String name, EsField field) {
        return new FieldAttribute(location(), name, field);
    }
}
