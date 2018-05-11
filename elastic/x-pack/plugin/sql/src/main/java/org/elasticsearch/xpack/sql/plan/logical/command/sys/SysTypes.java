/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plan.logical.command.sys;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.plan.logical.command.Command;
import org.elasticsearch.xpack.sql.session.Rows;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.session.SqlSession;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.type.DataTypes;

import java.sql.DatabaseMetaData;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.stream.Stream;

import static java.util.Arrays.asList;
import static java.util.stream.Collectors.toList;
import static org.elasticsearch.xpack.sql.type.DataType.BOOLEAN;
import static org.elasticsearch.xpack.sql.type.DataType.INTEGER;
import static org.elasticsearch.xpack.sql.type.DataType.SHORT;

public class SysTypes extends Command {

    public SysTypes(Location location) {
        super(location);
    }

    @Override
    protected NodeInfo<SysTypes> info() {
        return NodeInfo.create(this);
    }

    @Override
    public List<Attribute> output() {
        return asList(keyword("TYPE_NAME"),
                      field("DATA_TYPE", INTEGER),
                      field("PRECISION",INTEGER),
                      keyword("LITERAL_PREFIX"),
                      keyword("LITERAL_SUFFIX"),
                      keyword("CREATE_PARAMS"),
                      field("NULLABLE", SHORT),
                      field("CASE_SENSITIVE", BOOLEAN),
                      field("SEARCHABLE", SHORT),
                      field("UNSIGNED_ATTRIBUTE", BOOLEAN),
                      field("FIXED_PREC_SCALE", BOOLEAN),
                      field("AUTO_INCREMENT", BOOLEAN),
                      keyword("LOCAL_TYPE_NAME"),
                      field("MINIMUM_SCALE", SHORT),
                      field("MAXIMUM_SCALE", SHORT),
                      field("SQL_DATA_TYPE", INTEGER),
                      field("SQL_DATETIME_SUB", INTEGER),
                      field("NUM_PREC_RADIX", INTEGER),
                      // ODBC
                      field("INTERVAL_PRECISION", INTEGER)
                      );
    }

    @Override
    public final void execute(SqlSession session, ActionListener<SchemaRowSet> listener) {
        List<List<?>> rows = Stream.of(DataType.values())
                // sort by SQL int type (that's what the JDBC/ODBC specs want)
                .sorted(Comparator.comparing(t -> t.jdbcType.getVendorTypeNumber()))
                .map(t -> asList(t.esType.toUpperCase(Locale.ROOT),
                        t.jdbcType.getVendorTypeNumber(),
                        //https://docs.microsoft.com/en-us/sql/odbc/reference/appendixes/column-size?view=sql-server-2017
                        t.defaultPrecision,
                        "'",
                        "'",
                        null,
                        // don't be specific on nullable
                        DatabaseMetaData.typeNullableUnknown,
                        // all strings are case-sensitive
                        t.isString(),
                        // everything is searchable,
                        DatabaseMetaData.typeSearchable,
                        // only numerics are signed
                        !t.isSigned(),
                        //no fixed precision scale SQL_FALSE
                        Boolean.FALSE,
                        // not auto-incremented
                        Boolean.FALSE,
                        null,
                        DataTypes.metaSqlMinimumScale(t),
                        DataTypes.metaSqlMaximumScale(t),
                        // SQL_DATA_TYPE - ODBC wants this to be not null
                        DataTypes.metaSqlDataType(t),
                        DataTypes.metaSqlDateTimeSub(t),
                        // Radix
                        DataTypes.metaSqlRadix(t),
                        null
                        ))
                .collect(toList());
        
        listener.onResponse(Rows.of(output(), rows));
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