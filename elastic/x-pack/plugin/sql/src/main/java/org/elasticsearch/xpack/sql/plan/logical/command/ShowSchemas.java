/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plan.logical.command;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.FieldAttribute;
import org.elasticsearch.xpack.sql.session.Rows;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.session.SqlSession;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;
import org.elasticsearch.xpack.sql.type.KeywordEsField;

import java.util.List;

import static java.util.Collections.singletonList;

public class ShowSchemas extends Command {

    public ShowSchemas(Location location) {
        super(location);
    }

    @Override
    protected NodeInfo<ShowSchemas> info() {
        return NodeInfo.create(this);
    }

    @Override
    public List<Attribute> output() {
        return singletonList(new FieldAttribute(location(), "schema", new KeywordEsField("schema")));
    }

    @Override
    public void execute(SqlSession session, ActionListener<SchemaRowSet> listener) {
        listener.onResponse(Rows.empty(output()));
    }

    @Override
    public int hashCode() {
        return getClass().hashCode();
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        return true;
    }
}
