/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plan.logical.command;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.regex.LikePattern;
import org.elasticsearch.xpack.sql.session.Rows;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.session.SqlSession;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;

import java.util.List;
import java.util.Objects;

import static java.util.Arrays.asList;
import static java.util.stream.Collectors.toList;

public class ShowTables extends Command {

    private final LikePattern pattern;

    public ShowTables(Location location, LikePattern pattern) {
        super(location);
        this.pattern = pattern;
    }

    @Override
    protected NodeInfo<ShowTables> info() {
        return NodeInfo.create(this, ShowTables::new, pattern);
    }

    public LikePattern pattern() {
        return pattern;
    }

    @Override
    public List<Attribute> output() {
        return asList(keyword("name"), keyword("type"));
    }

    @Override
    public final void execute(SqlSession session, ActionListener<SchemaRowSet> listener) {
        String index = pattern != null ? pattern.asIndexNameWildcard() : "*";
        String regex = pattern != null ? pattern.asJavaRegex() : null;
        session.indexResolver().resolveNames(index, regex, null, ActionListener.wrap(result -> {
            listener.onResponse(Rows.of(output(), result.stream()
                 .map(t -> asList(t.name(), t.type().toSql()))
                .collect(toList())));
        }, listener::onFailure));
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

        ShowTables other = (ShowTables) obj;
        return Objects.equals(pattern, other.pattern);
    }
}
