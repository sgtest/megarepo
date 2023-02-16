/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch;

import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.TransportVersionUtils;

import java.lang.reflect.Modifier;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.lessThan;
import static org.hamcrest.Matchers.sameInstance;

public class TransportVersionTests extends ESTestCase {

    public void testVersionComparison() {
        TransportVersion V_7_2_0 = TransportVersion.V_7_2_0;
        TransportVersion V_8_0_0 = TransportVersion.V_8_0_0;
        assertThat(V_7_2_0.before(V_8_0_0), is(true));
        assertThat(V_7_2_0.before(V_7_2_0), is(false));
        assertThat(V_8_0_0.before(V_7_2_0), is(false));

        assertThat(V_7_2_0.onOrBefore(V_8_0_0), is(true));
        assertThat(V_7_2_0.onOrBefore(V_7_2_0), is(true));
        assertThat(V_8_0_0.onOrBefore(V_7_2_0), is(false));

        assertThat(V_7_2_0.after(V_8_0_0), is(false));
        assertThat(V_7_2_0.after(V_7_2_0), is(false));
        assertThat(V_8_0_0.after(V_7_2_0), is(true));

        assertThat(V_7_2_0.onOrAfter(V_8_0_0), is(false));
        assertThat(V_7_2_0.onOrAfter(V_7_2_0), is(true));
        assertThat(V_8_0_0.onOrAfter(V_7_2_0), is(true));

        assertThat(V_7_2_0, is(lessThan(V_8_0_0)));
        assertThat(V_7_2_0.compareTo(V_7_2_0), is(0));
        assertThat(V_8_0_0, is(greaterThan(V_7_2_0)));
    }

    private static String padNumber(String number) {
        return number.length() == 1 ? "0" + number : number;
    }

    public static class CorrectFakeVersion {
        public static final TransportVersion V1 = new TransportVersion(1, "1");
        public static final TransportVersion V2 = new TransportVersion(2, "2");
        public static final TransportVersion V3 = new TransportVersion(3, "3");
    }

    public static class DuplicatedIdFakeVersion {
        public static final TransportVersion V1 = new TransportVersion(1, "1");
        public static final TransportVersion V2 = new TransportVersion(2, "2");
        public static final TransportVersion V3 = new TransportVersion(2, "3");
    }

    public static class DuplicatedStringIdFakeVersion {
        public static final TransportVersion V1 = new TransportVersion(1, "1");
        public static final TransportVersion V2 = new TransportVersion(2, "2");
        public static final TransportVersion V3 = new TransportVersion(3, "2");
    }

    public void testStaticTransportVersionChecks() {
        assertThat(
            TransportVersion.getAllVersionIds(CorrectFakeVersion.class),
            equalTo(Map.of(1, CorrectFakeVersion.V1, 2, CorrectFakeVersion.V2, 3, CorrectFakeVersion.V3))
        );
        expectThrows(AssertionError.class, () -> TransportVersion.getAllVersionIds(DuplicatedIdFakeVersion.class));
        expectThrows(AssertionError.class, () -> TransportVersion.getAllVersionIds(DuplicatedStringIdFakeVersion.class));
    }

    public void testDefinedConstants() throws IllegalAccessException {
        Pattern historicalVersion = Pattern.compile("^V_(\\d{1,2})_(\\d{1,2})_(\\d{1,2})$");
        Pattern transportVersion = Pattern.compile("^V_(\\d{2,})_(\\d{3})_(\\d{3})$");
        Set<String> ignore = Set.of("ZERO", "CURRENT", "MINIMUM_COMPATIBLE");

        for (java.lang.reflect.Field field : TransportVersion.class.getFields()) {
            if (Modifier.isStatic(field.getModifiers())
                && field.getType() == TransportVersion.class
                && ignore.contains(field.getName()) == false) {
                Matcher historical = historicalVersion.matcher(field.getName());
                Matcher transport;
                if (historical.matches()) {
                    // old-style version constant
                    String idString = historical.group(1) + padNumber(historical.group(2)) + padNumber(historical.group(3)) + "99";
                    assertEquals(
                        "Field " + field.getName() + " does not have expected id " + idString,
                        idString,
                        field.get(null).toString()
                    );
                } else if ((transport = transportVersion.matcher(field.getName())).matches()) {
                    String idString = transport.group(1) + transport.group(2) + transport.group(3);
                    assertEquals(
                        "Field " + field.getName() + " does not have expected id " + idString,
                        idString,
                        field.get(null).toString()
                    );
                } else {
                    fail("Field " + field.getName() + " does not have expected format");
                }
            }
        }
    }

    public void testMin() {
        assertEquals(
            TransportVersionUtils.getPreviousVersion(),
            TransportVersion.min(TransportVersion.CURRENT, TransportVersionUtils.getPreviousVersion())
        );
        assertEquals(
            TransportVersion.fromId(1_01_01_99),
            TransportVersion.min(TransportVersion.fromId(1_01_01_99), TransportVersion.CURRENT)
        );
        TransportVersion version = TransportVersionUtils.randomVersion();
        TransportVersion version1 = TransportVersionUtils.randomVersion();
        if (version.id <= version1.id) {
            assertEquals(version, TransportVersion.min(version1, version));
        } else {
            assertEquals(version1, TransportVersion.min(version1, version));
        }
    }

    public void testMax() {
        assertEquals(TransportVersion.CURRENT, TransportVersion.max(TransportVersion.CURRENT, TransportVersionUtils.getPreviousVersion()));
        assertEquals(TransportVersion.CURRENT, TransportVersion.max(TransportVersion.fromId(1_01_01_99), TransportVersion.CURRENT));
        TransportVersion version = TransportVersionUtils.randomVersion();
        TransportVersion version1 = TransportVersionUtils.randomVersion();
        if (version.id >= version1.id) {
            assertEquals(version, TransportVersion.max(version1, version));
        } else {
            assertEquals(version1, TransportVersion.max(version1, version));
        }
    }

    public void testVersionConstantPresent() {
        // TODO those versions are not cached at the moment, perhaps we should add them to idToVersion set too?
        Set<TransportVersion> ignore = Set.of(TransportVersion.ZERO, TransportVersion.CURRENT, TransportVersion.MINIMUM_COMPATIBLE);
        assertThat(TransportVersion.CURRENT, sameInstance(TransportVersion.fromId(TransportVersion.CURRENT.id)));
        final int iters = scaledRandomIntBetween(20, 100);
        for (int i = 0; i < iters; i++) {
            TransportVersion version = TransportVersionUtils.randomVersion(ignore);

            assertThat(version, sameInstance(TransportVersion.fromId(version.id)));
        }
    }

    public void testCURRENTIsLatest() {
        final int iters = scaledRandomIntBetween(100, 1000);
        for (int i = 0; i < iters; i++) {
            TransportVersion version = TransportVersionUtils.randomVersion();
            if (version != TransportVersion.CURRENT) {
                assertThat(
                    "Version: " + version + " should be before: " + Version.CURRENT + " but wasn't",
                    version.before(TransportVersion.CURRENT),
                    is(true)
                );
            }
        }
    }

    public void testToString() {
        assertEquals("5000099", TransportVersion.fromId(5_00_00_99).toString());
        assertEquals("2030099", TransportVersion.fromId(2_03_00_99).toString());
        assertEquals("1000099", TransportVersion.fromId(1_00_00_99).toString());
        assertEquals("2000099", TransportVersion.fromId(2_00_00_99).toString());
        assertEquals("5000099", TransportVersion.fromId(5_00_00_99).toString());
    }
}
