/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plan.logical.command;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.FieldAttribute;
import org.elasticsearch.xpack.sql.expression.function.FunctionDefinition;
import org.elasticsearch.xpack.sql.expression.function.FunctionRegistry;
import org.elasticsearch.xpack.sql.expression.regex.LikePattern;
import org.elasticsearch.xpack.sql.session.Rows;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.session.SqlSession;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;
import org.elasticsearch.xpack.sql.type.KeywordEsField;

import java.util.Collection;
import java.util.List;
import java.util.Objects;

import static java.util.Arrays.asList;
import static java.util.stream.Collectors.toList;

public class ShowFunctions extends Command {

    private final LikePattern pattern;

    public ShowFunctions(Location location, LikePattern pattern) {
        super(location);
        this.pattern = pattern;
    }

    @Override
    protected NodeInfo<ShowFunctions> info() {
        return NodeInfo.create(this, ShowFunctions::new, pattern);
    }

    public LikePattern pattern() {
        return pattern;
    }

    @Override
    public List<Attribute> output() {
        return asList(new FieldAttribute(location(), "name", new KeywordEsField("name")),
                new FieldAttribute(location(), "type", new KeywordEsField("type")));
    }

    @Override
    public void execute(SqlSession session, ActionListener<SchemaRowSet> listener) {
        FunctionRegistry registry = session.functionRegistry();
        Collection<FunctionDefinition> functions = registry.listFunctions(pattern != null ? pattern.asJavaRegex() : null);

        listener.onResponse(Rows.of(output(), functions.stream()
                .map(f -> asList(f.name(), f.type().name()))
                .collect(toList())));
    }

    @Override
    public int hashCode() {
        return Objects.hash(pattern);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        ShowFunctions other = (ShowFunctions) obj;
        return Objects.equals(pattern, other.pattern);
    }
}
