/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.qa.sql.jdbc;

import org.apache.logging.log4j.Logger;
import org.relique.jdbc.csv.CsvResultSet;

import java.sql.JDBCType;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.Types;
import java.util.ArrayList;
import java.util.Calendar;
import java.util.List;
import java.util.Locale;
import java.util.TimeZone;

import static java.lang.String.format;
import static java.sql.Types.BIGINT;
import static java.sql.Types.DOUBLE;
import static java.sql.Types.FLOAT;
import static java.sql.Types.INTEGER;
import static java.sql.Types.REAL;
import static java.sql.Types.SMALLINT;
import static java.sql.Types.TINYINT;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

/**
 * Utility class for doing JUnit-style asserts over JDBC.
 */
public class JdbcAssert {
    private static final Calendar UTC_CALENDAR = Calendar.getInstance(TimeZone.getTimeZone("UTC"), Locale.ROOT);

    public static void assertResultSets(ResultSet expected, ResultSet actual) throws SQLException {
        assertResultSets(expected, actual, null);
    }

    public static void assertResultSets(ResultSet expected, ResultSet actual, Logger logger) throws SQLException {
        assertResultSets(expected, actual, logger, false);
    }

    /**
     * Assert the given result sets, potentially in a lenient way.
     * When lenient is specified, the type comparison of a column is widden to reach a common, compatible ground.
     * This means promoting integer types to long and floating types to double and comparing their values.
     * For example in a non-lenient, strict case a comparison between an int and a tinyint would fail, with lenient it will succeed as
     * long as the actual value is the same.
     */
    public static void assertResultSets(ResultSet expected, ResultSet actual, Logger logger, boolean lenient) throws SQLException {
        try (ResultSet ex = expected; ResultSet ac = actual) {
            assertResultSetMetadata(ex, ac, logger, lenient);
            assertResultSetData(ex, ac, logger, lenient);
        }
    }

    public static void assertResultSetMetadata(ResultSet expected, ResultSet actual, Logger logger) throws SQLException {
        assertResultSetMetadata(expected, actual, logger, false);
    }

    // metadata doesn't consume a ResultSet thus it shouldn't close it
    public static void assertResultSetMetadata(ResultSet expected, ResultSet actual, Logger logger, boolean lenient) throws SQLException {
        ResultSetMetaData expectedMeta = expected.getMetaData();
        ResultSetMetaData actualMeta = actual.getMetaData();

        if (logger != null) {
            JdbcTestUtils.logResultSetMetadata(actual, logger);
        }

        if (expectedMeta.getColumnCount() != actualMeta.getColumnCount()) {
            List<String> expectedCols = new ArrayList<>();
            for (int i = 1; i <= expectedMeta.getColumnCount(); i++) {
                expectedCols.add(expectedMeta.getColumnName(i));

            }

            List<String> actualCols = new ArrayList<>();
            for (int i = 1; i <= actualMeta.getColumnCount(); i++) {
                actualCols.add(actualMeta.getColumnName(i));
            }

            assertEquals(format(Locale.ROOT, "Different number of columns returned (expected %d but was %d);",
                    expectedMeta.getColumnCount(), actualMeta.getColumnCount()),
                    expectedCols.toString(), actualCols.toString());
        }

        for (int column = 1; column <= expectedMeta.getColumnCount(); column++) {
            String expectedName = expectedMeta.getColumnName(column);
            String actualName = actualMeta.getColumnName(column);

            if (!expectedName.equals(actualName)) {
                // to help debugging, indicate the previous column (which also happened to match and thus was correct)
                String expectedSet = expectedName;
                String actualSet = actualName;
                if (column > 1) {
                    expectedSet = expectedMeta.getColumnName(column - 1) + "," + expectedName;
                    actualSet = actualMeta.getColumnName(column - 1) + "," + actualName;
                }

                assertEquals("Different column name [" + column + "]", expectedSet, actualSet);
            }

            // use the type not the name (timestamp with timezone returns spaces for example)
            int expectedType = typeOf(expectedMeta.getColumnType(column), lenient);
            int actualType = typeOf(actualMeta.getColumnType(column), lenient);

            // since H2 cannot use a fixed timezone, the data is stored in UTC (and thus with timezone)
            if (expectedType == Types.TIMESTAMP_WITH_TIMEZONE) {
                expectedType = Types.TIMESTAMP;
            }
            // since csv doesn't support real, we use float instead.....
            if (expectedType == Types.FLOAT && expected instanceof CsvResultSet) {
                expectedType = Types.REAL;
            }
            // when lenient is used, an int is equivalent to a short, etc...
            assertEquals("Different column type for column [" + expectedName + "] (" + JDBCType.valueOf(expectedType) + " != "
                    + JDBCType.valueOf(actualType) + ")", expectedType, actualType);
        }
    }

    // The ResultSet is consumed and thus it should be closed
    public static void assertResultSetData(ResultSet expected, ResultSet actual, Logger logger) throws SQLException {
        assertResultSetData(expected, actual, logger, false);
    }

    public static void assertResultSetData(ResultSet expected, ResultSet actual, Logger logger, boolean lenient) throws SQLException {
        try (ResultSet ex = expected; ResultSet ac = actual) {
            doAssertResultSetData(ex, ac, logger, lenient);
        }
    }
    
    private static void doAssertResultSetData(ResultSet expected, ResultSet actual, Logger logger, boolean lenient) throws SQLException {
        ResultSetMetaData metaData = expected.getMetaData();
        int columns = metaData.getColumnCount();

        long count = 0;
        try {
            for (count = 0; expected.next(); count++) {
                assertTrue("Expected more data but no more entries found after [" + count + "]", actual.next());

                if (logger != null) {
                    logger.info(JdbcTestUtils.resultSetCurrentData(actual));
                }

                for (int column = 1; column <= columns; column++) {
                    int type = metaData.getColumnType(column);
                    Class<?> expectedColumnClass = null;
                    try {
                        String columnClassName = metaData.getColumnClassName(column);

                        // fix for CSV which returns the shortName not fully-qualified name
                        if (!columnClassName.contains(".")) {
                            switch (columnClassName) {
                                case "Timestamp":
                                    columnClassName = "java.sql.Timestamp";
                                    break;
                                case "Int":
                                    columnClassName = "java.lang.Integer";
                                    break;
                                default:
                                    columnClassName = "java.lang." + columnClassName;
                                    break;
                            }
                        }

                        expectedColumnClass = Class.forName(columnClassName);
                    } catch (ClassNotFoundException cnfe) {
                        throw new SQLException(cnfe);
                    }
                    
                    Object expectedObject = expected.getObject(column);
                    Object actualObject = lenient ? actual.getObject(column, expectedColumnClass) : actual.getObject(column);

                    String msg = format(Locale.ROOT, "Different result for column [%s], entry [%d]",
                        metaData.getColumnName(column), count + 1);

                    // handle nulls first
                    if (expectedObject == null || actualObject == null) {
                        assertEquals(msg, expectedObject, actualObject);
                    }
                    // then timestamp
                    else if (type == Types.TIMESTAMP || type == Types.TIMESTAMP_WITH_TIMEZONE) {
                        assertEquals(msg, expected.getTimestamp(column), actual.getTimestamp(column));
                    }
                    // and floats/doubles
                    else if (type == Types.DOUBLE) {
                        // the 1d/1f difference is used due to rounding/flooring
                        assertEquals(msg, (double) expectedObject, (double) actualObject, 1d);
                    } else if (type == Types.FLOAT) {
                        assertEquals(msg, (float) expectedObject, (float) actualObject, 1f);
                    }
                    // finally the actual comparison
                    else {
                        assertEquals(msg, expectedObject, actualObject);
                    }
                }
            }
        } catch (AssertionError ae) {
            if (logger != null && actual.next()) {
                logger.info("^^^ Assertion failure ^^^");
                logger.info(JdbcTestUtils.resultSetCurrentData(actual));
            }
            throw ae;
        }

        if (actual.next()) {
            fail("Elasticsearch [" + actual + "] still has data after [" + count + "] entries:\n"
                    + JdbcTestUtils.resultSetCurrentData(actual));
        }
    }

    /**
     * Returns the value of the given type either in a lenient fashion (widened) or strict.
     */
    private static int typeOf(int columnType, boolean lenient) {
        if (lenient) {
            // integer upcast to long
            if (columnType == TINYINT || columnType == SMALLINT || columnType == INTEGER || columnType == BIGINT) {
                return BIGINT;
            }
            if (columnType == FLOAT || columnType == REAL || columnType == DOUBLE) {
                return REAL;
            }
        }

        return columnType;
    }
}
