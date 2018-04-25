/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.qa.sql.jdbc;

import org.apache.logging.log4j.Logger;

import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;

public abstract class JdbcTestUtils {

    public static final String SQL_TRACE = "org.elasticsearch.xpack.sql:TRACE";

    public static void logResultSetMetadata(ResultSet rs, Logger logger) throws SQLException {
        ResultSetMetaData metaData = rs.getMetaData();
        // header
        StringBuilder sb = new StringBuilder();
        StringBuilder column = new StringBuilder();

        int columns = metaData.getColumnCount();
        for (int i = 1; i <= columns; i++) {
            if (i > 1) {
                sb.append(" | ");
            }
            column.setLength(0);
            column.append(metaData.getColumnName(i));
            column.append("(");
            column.append(metaData.getColumnTypeName(i));
            column.append(")");

            sb.append(trimOrPad(column));
        }

        int l = sb.length();
        logger.info(sb.toString());
        sb.setLength(0);
        for (int i = 0; i < l; i++) {
            sb.append("-");
        }

        logger.info(sb.toString());
    }

    private static final int MAX_WIDTH = 20;

    public static void logResultSetData(ResultSet rs, Logger log) throws SQLException {
        ResultSetMetaData metaData = rs.getMetaData();
        StringBuilder sb = new StringBuilder();
        StringBuilder column = new StringBuilder();

        int columns = metaData.getColumnCount();

        while (rs.next()) {
            sb.setLength(0);
            for (int i = 1; i <= columns; i++) {
                column.setLength(0);
                if (i > 1) {
                    sb.append(" | ");
                }
                sb.append(trimOrPad(column.append(rs.getString(i))));
            }
            log.info(sb);
        }
    }

    public static String resultSetCurrentData(ResultSet rs) throws SQLException {
        ResultSetMetaData metaData = rs.getMetaData();
        StringBuilder column = new StringBuilder();

        int columns = metaData.getColumnCount();

        StringBuilder sb = new StringBuilder();
        for (int i = 1; i <= columns; i++) {
            column.setLength(0);
            if (i > 1) {
                sb.append(" | ");
            }
            sb.append(trimOrPad(column.append(rs.getString(i))));
        }
        return sb.toString();
    }

    private static StringBuilder trimOrPad(StringBuilder buffer) {
        if (buffer.length() > MAX_WIDTH) {
            buffer.setLength(MAX_WIDTH - 1);
            buffer.append("~");
        }
        else {
            for (int i = buffer.length(); i < MAX_WIDTH; i++) {
                buffer.append(" ");
            }
        }
        return buffer;
    }
}