/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plan.logical.command.sys;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.xpack.sql.analysis.index.IndexResolver.IndexType;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.regex.LikePattern;
import org.elasticsearch.xpack.sql.plan.logical.command.Command;
import org.elasticsearch.xpack.sql.session.Rows;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.session.SqlSession;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;
import org.elasticsearch.xpack.sql.util.CollectionUtils;

import java.util.ArrayList;
import java.util.EnumSet;
import java.util.List;
import java.util.Objects;
import java.util.regex.Pattern;

import static java.util.Arrays.asList;
import static java.util.stream.Collectors.toList;
import static org.elasticsearch.xpack.sql.util.StringUtils.EMPTY;
import static org.elasticsearch.xpack.sql.util.StringUtils.SQL_WILDCARD;

public class SysTables extends Command {

    private final LikePattern pattern;
    private final LikePattern clusterPattern;
    private final EnumSet<IndexType> types;

    public SysTables(Location location, LikePattern clusterPattern, LikePattern pattern, EnumSet<IndexType> types) {
        super(location);
        this.clusterPattern = clusterPattern;
        this.pattern = pattern;
        this.types = types;
    }

    @Override
    protected NodeInfo<SysTables> info() {
        return NodeInfo.create(this, SysTables::new, clusterPattern, pattern, types);
    }

    @Override
    public List<Attribute> output() {
        return asList(keyword("TABLE_CAT"),
                      keyword("TABLE_SCHEM"),
                      keyword("TABLE_NAME"),
                      keyword("TABLE_TYPE"),
                      keyword("REMARKS"),
                      keyword("TYPE_CAT"),
                      keyword("TYPE_SCHEM"),
                      keyword("TYPE_NAME"),
                      keyword("SELF_REFERENCING_COL_NAME"),
                      keyword("REF_GENERATION")
                      );
    }

    @Override
    public final void execute(SqlSession session, ActionListener<SchemaRowSet> listener) {
        String cluster = session.indexResolver().clusterName();

        // first check if where dealing with ODBC enumeration
        // namely one param specified with '%', everything else empty string
        // https://docs.microsoft.com/en-us/sql/odbc/reference/syntax/sqltables-function?view=ssdt-18vs2017#comments

        if (clusterPattern != null && clusterPattern.pattern().equals(SQL_WILDCARD)) {
            if (pattern != null && pattern.pattern().isEmpty() && CollectionUtils.isEmpty(types)) {
                Object[] enumeration = new Object[10];
                // send only the cluster, everything else null
                enumeration[0] = cluster;
                listener.onResponse(Rows.singleton(output(), enumeration));
                return;
            }
        }
        
        // if no types were specified (the parser takes care of the % case)
        if (CollectionUtils.isEmpty(types)) {
            if (clusterPattern != null && clusterPattern.pattern().isEmpty()) {
                List<List<?>> values = new ArrayList<>();
                // send only the types, everything else null
                for (IndexType type : IndexType.VALID) {
                    Object[] enumeration = new Object[10];
                    enumeration[3] = type.toSql();
                    values.add(asList(enumeration));
                }
                listener.onResponse(Rows.of(output(), values));
                return;
            }
        }

        
        String cRegex = clusterPattern != null ? clusterPattern.asJavaRegex() : null;

        // if the catalog doesn't match, don't return any results
        if (cRegex != null && !Pattern.matches(cRegex, cluster)) {
            listener.onResponse(Rows.empty(output()));
            return;
        }

        String index = pattern != null ? pattern.asIndexNameWildcard() : "*";
        String regex = pattern != null ? pattern.asJavaRegex() : null;

        session.indexResolver().resolveNames(index, regex, types, ActionListener.wrap(result -> listener.onResponse(
                Rows.of(output(), result.stream()
                 .map(t -> asList(cluster,
                         EMPTY,
                         t.name(),
                         t.type().toSql(),
                         EMPTY,
                         null,
                         null,
                         null,
                         null,
                         null))
                .collect(toList())))
        , listener::onFailure));
    }

    @Override
    public int hashCode() {
        return Objects.hash(clusterPattern, pattern, types);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        SysTables other = (SysTables) obj;
        return Objects.equals(clusterPattern, other.clusterPattern)
                && Objects.equals(pattern, other.pattern)
                && Objects.equals(types, other.types);
    }
}