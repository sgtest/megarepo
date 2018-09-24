/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.jdbc.jdbc;

import org.elasticsearch.test.ESTestCase;

import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.sql.SQLException;
import java.sql.SQLFeatureNotSupportedException;
import java.sql.SQLType;
import java.sql.Struct;
import java.sql.Time;
import java.sql.Timestamp;
import java.sql.Types;
import java.time.Clock;
import java.time.LocalDateTime;
import java.time.ZoneOffset;
import java.time.ZonedDateTime;
import java.util.Calendar;
import java.util.Date;
import java.util.Locale;
import java.util.Map;

import static java.lang.String.format;
import static java.sql.JDBCType.BIGINT;
import static java.sql.JDBCType.BOOLEAN;
import static java.sql.JDBCType.DOUBLE;
import static java.sql.JDBCType.FLOAT;
import static java.sql.JDBCType.INTEGER;
import static java.sql.JDBCType.REAL;
import static java.sql.JDBCType.SMALLINT;
import static java.sql.JDBCType.TIMESTAMP;
import static java.sql.JDBCType.TINYINT;
import static java.sql.JDBCType.VARBINARY;
import static java.sql.JDBCType.VARCHAR;

public class JdbcPreparedStatementTests extends ESTestCase {

    public void testSettingBooleanValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        jps.setBoolean(1, true);
        assertEquals(true, value(jps));
        assertEquals(BOOLEAN, jdbcType(jps));

        jps.setObject(1, false);
        assertEquals(false, value(jps));
        assertEquals(BOOLEAN, jdbcType(jps));

        jps.setObject(1, true, Types.BOOLEAN);
        assertEquals(true, value(jps));
        assertEquals(BOOLEAN, jdbcType(jps));
        assertTrue(value(jps) instanceof Boolean);

        jps.setObject(1, true, Types.INTEGER);
        assertEquals(1, value(jps));
        assertEquals(INTEGER, jdbcType(jps));

        jps.setObject(1, true, Types.VARCHAR);
        assertEquals("true", value(jps));
        assertEquals(VARCHAR, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingBooleanValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, true, Types.TIMESTAMP));
        assertEquals("Unable to convert value [true] of type [BOOLEAN] to a Timestamp", sqle.getMessage());
    }

    public void testSettingStringValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        jps.setString(1, "foo bar");
        assertEquals("foo bar", value(jps));
        assertEquals(VARCHAR, jdbcType(jps));

        jps.setObject(1, "foo bar");
        assertEquals("foo bar", value(jps));
        assertEquals(VARCHAR, jdbcType(jps));

        jps.setObject(1, "foo bar", Types.VARCHAR);
        assertEquals("foo bar", value(jps));
        assertEquals(VARCHAR, jdbcType(jps));
        assertTrue(value(jps) instanceof String);
    }

    public void testThrownExceptionsWhenSettingStringValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, "foo bar", Types.INTEGER));
        assertEquals("Unable to convert value [foo bar] of type [VARCHAR] to an Integer", sqle.getMessage());
    }

    public void testSettingByteTypeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        jps.setByte(1, (byte) 6);
        assertEquals((byte) 6, value(jps));
        assertEquals(TINYINT, jdbcType(jps));

        jps.setObject(1, (byte) 6);
        assertEquals((byte) 6, value(jps));
        assertEquals(TINYINT, jdbcType(jps));
        assertTrue(value(jps) instanceof Byte);

        jps.setObject(1, (byte) 0, Types.BOOLEAN);
        assertEquals(false, value(jps));
        assertEquals(BOOLEAN, jdbcType(jps));

        jps.setObject(1, (byte) 123, Types.BOOLEAN);
        assertEquals(true, value(jps));
        assertEquals(BOOLEAN, jdbcType(jps));

        jps.setObject(1, (byte) 123, Types.INTEGER);
        assertEquals(123, value(jps));
        assertEquals(INTEGER, jdbcType(jps));

        jps.setObject(1, (byte) -128, Types.DOUBLE);
        assertEquals(-128.0, value(jps));
        assertEquals(DOUBLE, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingByteTypeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, (byte) 6, Types.TIMESTAMP));
        assertEquals("Unable to convert value [6] of type [TINYINT] to a Timestamp", sqle.getMessage());
    }

    public void testSettingShortTypeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        short someShort = randomShort();
        jps.setShort(1, someShort);
        assertEquals(someShort, value(jps));
        assertEquals(SMALLINT, jdbcType(jps));

        jps.setObject(1, someShort);
        assertEquals(someShort, value(jps));
        assertEquals(SMALLINT, jdbcType(jps));
        assertTrue(value(jps) instanceof Short);

        jps.setObject(1, (short) 1, Types.BOOLEAN);
        assertEquals(true, value(jps));
        assertEquals(BOOLEAN, jdbcType(jps));

        jps.setObject(1, (short) -32700, Types.DOUBLE);
        assertEquals(-32700.0, value(jps));
        assertEquals(DOUBLE, jdbcType(jps));

        jps.setObject(1, someShort, Types.INTEGER);
        assertEquals((int) someShort, value(jps));
        assertEquals(INTEGER, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingShortTypeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, (short) 6, Types.TIMESTAMP));
        assertEquals("Unable to convert value [6] of type [SMALLINT] to a Timestamp", sqle.getMessage());

        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, 256, Types.TINYINT));
        assertEquals("Numeric " + 256 + " out of range", sqle.getMessage());
    }

    public void testSettingIntegerValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        int someInt = randomInt();
        jps.setInt(1, someInt);
        assertEquals(someInt, value(jps));
        assertEquals(INTEGER, jdbcType(jps));

        jps.setObject(1, someInt);
        assertEquals(someInt, value(jps));
        assertEquals(INTEGER, jdbcType(jps));
        assertTrue(value(jps) instanceof Integer);

        jps.setObject(1, someInt, Types.VARCHAR);
        assertEquals(String.valueOf(someInt), value(jps));
        assertEquals(VARCHAR, jdbcType(jps));

        jps.setObject(1, someInt, Types.FLOAT);
        assertEquals(Double.valueOf(someInt), value(jps));
        assertTrue(value(jps) instanceof Double);
        assertEquals(FLOAT, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingIntegerValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        int someInt = randomInt();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, someInt, Types.TIMESTAMP));
        assertEquals(format(Locale.ROOT, "Unable to convert value [%.128s] of type [INTEGER] to a Timestamp", someInt), sqle.getMessage());

        Integer randomIntNotShort = randomIntBetween(32768, Integer.MAX_VALUE);
        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, randomIntNotShort, Types.SMALLINT));
        assertEquals("Numeric " + randomIntNotShort + " out of range", sqle.getMessage());

        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, randomIntNotShort, Types.TINYINT));
        assertEquals("Numeric " + randomIntNotShort + " out of range", sqle.getMessage());
    }

    public void testSettingLongValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        long someLong = randomLong();
        jps.setLong(1, someLong);
        assertEquals(someLong, value(jps));
        assertEquals(BIGINT, jdbcType(jps));

        jps.setObject(1, someLong);
        assertEquals(someLong, value(jps));
        assertEquals(BIGINT, jdbcType(jps));
        assertTrue(value(jps) instanceof Long);

        jps.setObject(1, someLong, Types.VARCHAR);
        assertEquals(String.valueOf(someLong), value(jps));
        assertEquals(VARCHAR, jdbcType(jps));

        jps.setObject(1, someLong, Types.DOUBLE);
        assertEquals((double) someLong, value(jps));
        assertEquals(DOUBLE, jdbcType(jps));

        jps.setObject(1, someLong, Types.FLOAT);
        assertEquals((double) someLong, value(jps));
        assertEquals(FLOAT, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingLongValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        long someLong = randomLong();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, someLong, Types.TIMESTAMP));
        assertEquals(format(Locale.ROOT, "Unable to convert value [%.128s] of type [BIGINT] to a Timestamp", someLong), sqle.getMessage());

        Long randomLongNotShort = randomLongBetween(Integer.MAX_VALUE + 1, Long.MAX_VALUE);
        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, randomLongNotShort, Types.INTEGER));
        assertEquals("Numeric " + randomLongNotShort + " out of range", sqle.getMessage());

        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, randomLongNotShort, Types.SMALLINT));
        assertEquals("Numeric " + randomLongNotShort + " out of range", sqle.getMessage());
    }

    public void testSettingFloatValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        float someFloat = randomFloat();
        jps.setFloat(1, someFloat);
        assertEquals(someFloat, value(jps));
        assertEquals(REAL, jdbcType(jps));

        jps.setObject(1, someFloat);
        assertEquals(someFloat, value(jps));
        assertEquals(REAL, jdbcType(jps));
        assertTrue(value(jps) instanceof Float);

        jps.setObject(1, someFloat, Types.VARCHAR);
        assertEquals(String.valueOf(someFloat), value(jps));
        assertEquals(VARCHAR, jdbcType(jps));

        jps.setObject(1, someFloat, Types.DOUBLE);
        assertEquals((double) someFloat, value(jps));
        assertEquals(DOUBLE, jdbcType(jps));

        jps.setObject(1, someFloat, Types.FLOAT);
        assertEquals((double) someFloat, value(jps));
        assertEquals(FLOAT, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingFloatValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        float someFloat = randomFloat();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, someFloat, Types.TIMESTAMP));
        assertEquals(format(Locale.ROOT, "Unable to convert value [%.128s] of type [REAL] to a Timestamp", someFloat), sqle.getMessage());

        Float floatNotInt =  5_155_000_000f;
        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, floatNotInt, Types.INTEGER));
        assertEquals(String.format(Locale.ROOT, "Numeric %s out of range",
                Long.toString(Math.round(floatNotInt.doubleValue()))), sqle.getMessage());

        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, floatNotInt, Types.SMALLINT));
        assertEquals(String.format(Locale.ROOT, "Numeric %s out of range",
                Long.toString(Math.round(floatNotInt.doubleValue()))), sqle.getMessage());
    }

    public void testSettingDoubleValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        double someDouble = randomDouble();
        jps.setDouble(1, someDouble);
        assertEquals(someDouble, value(jps));
        assertEquals(DOUBLE, jdbcType(jps));

        jps.setObject(1, someDouble);
        assertEquals(someDouble, value(jps));
        assertEquals(DOUBLE, jdbcType(jps));
        assertTrue(value(jps) instanceof Double);

        jps.setObject(1, someDouble, Types.VARCHAR);
        assertEquals(String.valueOf(someDouble), value(jps));
        assertEquals(VARCHAR, jdbcType(jps));

        jps.setObject(1, someDouble, Types.REAL);
        assertEquals((float) someDouble, value(jps));
        assertEquals(REAL, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingDoubleValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        double someDouble = randomDouble();

        SQLException sqle = expectThrows(SQLException.class, () -> jps.setObject(1, someDouble, Types.TIMESTAMP));
        assertEquals(
                format(Locale.ROOT, "Unable to convert value [%.128s] of type [DOUBLE] to a Timestamp", someDouble), sqle.getMessage());

        Double doubleNotInt = 5_155_000_000d;
        sqle = expectThrows(SQLException.class, () -> jps.setObject(1, doubleNotInt, Types.INTEGER));
        assertEquals(String.format(Locale.ROOT, "Numeric %s out of range",
                Long.toString(((Number) doubleNotInt).longValue())), sqle.getMessage());
    }

    public void testUnsupportedClasses() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        SQLFeatureNotSupportedException sfnse = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, new Struct() {
            @Override
            public String getSQLTypeName() throws SQLException {
                return null;
            }
            @Override
            public Object[] getAttributes(Map<String, Class<?>> map) throws SQLException {
                return null;
            }
            @Override
            public Object[] getAttributes() throws SQLException {
                return null;
            }
        }));
        assertEquals("Objects of type java.sql.Struct are not supported", sfnse.getMessage());

        sfnse = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, new URL("http://test")));
        assertEquals("Objects of type java.net.URL are not supported", sfnse.getMessage());

        sfnse = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setURL(1, new URL("http://test")));
        assertEquals("Objects of type java.net.URL are not supported", sfnse.getMessage());

        sfnse = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, this, Types.TIMESTAMP));
        assertEquals("Conversion from type " + this.getClass().getName() + " to TIMESTAMP not supported", sfnse.getMessage());

        SQLException se = expectThrows(SQLException.class, () -> jps.setObject(1, this, 1_000_000));
        assertEquals("Type:1000000 is not a valid Types.java value.", se.getMessage());

        IllegalArgumentException iae = expectThrows(IllegalArgumentException.class, () -> jps.setObject(1, randomShort(), Types.CHAR));
        assertEquals("Unsupported JDBC type [CHAR]", iae.getMessage());
    }

    public void testSettingTimestampValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        Timestamp someTimestamp = new Timestamp(randomLong());
        jps.setTimestamp(1, someTimestamp);
        assertEquals(someTimestamp.getTime(), ((Date)value(jps)).getTime());
        assertEquals(TIMESTAMP, jdbcType(jps));

        Calendar nonDefaultCal = randomCalendar();
        // February 29th, 2016. 01:17:55 GMT = 1456708675000 millis since epoch
        jps.setTimestamp(1, new Timestamp(1456708675000L), nonDefaultCal);
        assertEquals(1456708675000L, convertFromUTCtoCalendar(((Date)value(jps)), nonDefaultCal));
        assertEquals(TIMESTAMP, jdbcType(jps));

        long beforeEpochTime = randomLongBetween(Long.MIN_VALUE, 0);
        jps.setTimestamp(1, new Timestamp(beforeEpochTime), nonDefaultCal);
        assertEquals(beforeEpochTime, convertFromUTCtoCalendar(((Date)value(jps)), nonDefaultCal));
        assertTrue(value(jps) instanceof java.util.Date);

        jps.setObject(1, someTimestamp, Types.VARCHAR);
        assertEquals(someTimestamp.toString(), value(jps).toString());
        assertEquals(VARCHAR, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingTimestampValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        Timestamp someTimestamp = new Timestamp(randomLong());

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, someTimestamp, Types.INTEGER));
        assertEquals("Conversion from type java.sql.Timestamp to INTEGER not supported", sqle.getMessage());
    }

    public void testSettingTimeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        Time time = new Time(4675000);
        Calendar nonDefaultCal = randomCalendar();
        jps.setTime(1, time, nonDefaultCal);
        assertEquals(4675000, convertFromUTCtoCalendar(((Date)value(jps)), nonDefaultCal));
        assertEquals(TIMESTAMP, jdbcType(jps));
        assertTrue(value(jps) instanceof java.util.Date);

        jps.setObject(1, time, Types.VARCHAR);
        assertEquals(time.toString(), value(jps).toString());
        assertEquals(VARCHAR, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingTimeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        Time time = new Time(4675000);

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, time, Types.INTEGER));
        assertEquals("Conversion from type java.sql.Time to INTEGER not supported", sqle.getMessage());
    }

    public void testSettingSqlDateValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        java.sql.Date someSqlDate = new java.sql.Date(randomLong());
        jps.setDate(1, someSqlDate);
        assertEquals(someSqlDate.getTime(), ((Date)value(jps)).getTime());
        assertEquals(TIMESTAMP, jdbcType(jps));

        someSqlDate = new java.sql.Date(randomLong());
        Calendar nonDefaultCal = randomCalendar();
        jps.setDate(1, someSqlDate, nonDefaultCal);
        assertEquals(someSqlDate.getTime(), convertFromUTCtoCalendar(((Date)value(jps)), nonDefaultCal));
        assertEquals(TIMESTAMP, jdbcType(jps));
        assertTrue(value(jps) instanceof java.util.Date);

        jps.setObject(1, someSqlDate, Types.VARCHAR);
        assertEquals(someSqlDate.toString(), value(jps).toString());
        assertEquals(VARCHAR, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingSqlDateValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        java.sql.Date someSqlDate = new java.sql.Date(randomLong());

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class,
                () -> jps.setObject(1, new java.sql.Date(randomLong()), Types.DOUBLE));
        assertEquals("Conversion from type " + someSqlDate.getClass().getName() + " to DOUBLE not supported", sqle.getMessage());
    }

    public void testSettingCalendarValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        Calendar someCalendar = randomCalendar();
        someCalendar.setTimeInMillis(randomLong());

        jps.setObject(1, someCalendar);
        assertEquals(someCalendar.getTime(), value(jps));
        assertEquals(TIMESTAMP, jdbcType(jps));
        assertTrue(value(jps) instanceof java.util.Date);

        jps.setObject(1, someCalendar, Types.VARCHAR);
        assertEquals(someCalendar.toString(), value(jps).toString());
        assertEquals(VARCHAR, jdbcType(jps));

        Calendar nonDefaultCal = randomCalendar();
        jps.setObject(1, nonDefaultCal);
        assertEquals(nonDefaultCal.getTime(), value(jps));
        assertEquals(TIMESTAMP, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingCalendarValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        Calendar someCalendar = randomCalendar();

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, someCalendar, Types.DOUBLE));
        assertEquals("Conversion from type " + someCalendar.getClass().getName() + " to DOUBLE not supported", sqle.getMessage());
    }

    public void testSettingDateValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        Date someDate = new Date(randomLong());

        jps.setObject(1, someDate);
        assertEquals(someDate, value(jps));
        assertEquals(TIMESTAMP, jdbcType(jps));
        assertTrue(value(jps) instanceof java.util.Date);

        jps.setObject(1, someDate, Types.VARCHAR);
        assertEquals(someDate.toString(), value(jps).toString());
        assertEquals(VARCHAR, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingDateValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        Date someDate = new Date(randomLong());

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, someDate, Types.BIGINT));
        assertEquals("Conversion from type " + someDate.getClass().getName() + " to BIGINT not supported", sqle.getMessage());
    }

    public void testSettingLocalDateTimeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        LocalDateTime ldt = LocalDateTime.now(Clock.systemDefaultZone());

        jps.setObject(1, ldt);
        assertEquals(Date.class, value(jps).getClass());
        assertEquals(TIMESTAMP, jdbcType(jps));
        assertTrue(value(jps) instanceof java.util.Date);

        jps.setObject(1, ldt, Types.VARCHAR);
        assertEquals(ldt.toString(), value(jps).toString());
        assertEquals(VARCHAR, jdbcType(jps));
    }

    public void testThrownExceptionsWhenSettingLocalDateTimeValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        LocalDateTime ldt = LocalDateTime.now(Clock.systemDefaultZone());

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, ldt, Types.BIGINT));
        assertEquals("Conversion from type " + ldt.getClass().getName() + " to BIGINT not supported", sqle.getMessage());
    }

    public void testSettingByteArrayValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();

        byte[] buffer = "some data".getBytes(StandardCharsets.UTF_8);
        jps.setBytes(1, buffer);
        assertEquals(byte[].class, value(jps).getClass());
        assertEquals(VARBINARY, jdbcType(jps));

        jps.setObject(1, buffer);
        assertEquals(byte[].class, value(jps).getClass());
        assertEquals(VARBINARY, jdbcType(jps));
        assertTrue(value(jps) instanceof byte[]);

        jps.setObject(1, buffer, Types.VARBINARY);
        assertEquals(value(jps), buffer);
        assertEquals(VARBINARY, jdbcType(jps));

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, buffer, Types.VARCHAR));
        assertEquals("Conversion from type byte[] to VARCHAR not supported", sqle.getMessage());

        sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, buffer, Types.DOUBLE));
        assertEquals("Conversion from type byte[] to DOUBLE not supported", sqle.getMessage());
    }

    public void testThrownExceptionsWhenSettingByteArrayValues() throws SQLException {
        JdbcPreparedStatement jps = createJdbcPreparedStatement();
        byte[] buffer = "foo".getBytes(StandardCharsets.UTF_8);

        SQLException sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, buffer, Types.VARCHAR));
        assertEquals("Conversion from type byte[] to VARCHAR not supported", sqle.getMessage());

        sqle = expectThrows(SQLFeatureNotSupportedException.class, () -> jps.setObject(1, buffer, Types.DOUBLE));
        assertEquals("Conversion from type byte[] to DOUBLE not supported", sqle.getMessage());
    }

    private JdbcPreparedStatement createJdbcPreparedStatement() throws SQLException {
        return new JdbcPreparedStatement(null, JdbcConfiguration.create("jdbc:es://l:1", null, 0), "?");
    }

    private SQLType jdbcType(JdbcPreparedStatement jps) throws SQLException {
        return jps.query.getParam(1).type;
    }

    private Object value(JdbcPreparedStatement jps) throws SQLException {
        return jps.query.getParam(1).value;
    }

    private Calendar randomCalendar() {
        return Calendar.getInstance(randomTimeZone(), Locale.ROOT);
    }

    /*
     * Converts from UTC to the provided Calendar.
     * Helps checking if the converted date/time values using Calendars in set*(...,Calendar) methods did convert
     * the values correctly to UTC.
     */
    private long convertFromUTCtoCalendar(Date date, Calendar nonDefaultCal) throws SQLException {
        return ZonedDateTime.ofInstant(date.toInstant(), ZoneOffset.UTC)
                .withZoneSameLocal(nonDefaultCal.getTimeZone().toZoneId())
                .toInstant().toEpochMilli();
    }
}
