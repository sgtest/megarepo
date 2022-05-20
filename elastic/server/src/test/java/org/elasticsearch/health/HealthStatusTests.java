/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.health;

import org.elasticsearch.test.ESTestCase;

import java.util.ArrayList;
import java.util.stream.Stream;

import static org.elasticsearch.health.HealthStatus.GREEN;
import static org.elasticsearch.health.HealthStatus.RED;
import static org.elasticsearch.health.HealthStatus.UNKNOWN;
import static org.elasticsearch.health.HealthStatus.YELLOW;

public class HealthStatusTests extends ESTestCase {

    public void testAllGreenStatuses() {
        assertEquals(GREEN, HealthStatus.merge(randomStatusesContaining(GREEN)));
    }

    public void testUnknownStatus() {
        assertEquals(UNKNOWN, HealthStatus.merge(randomStatusesContaining(GREEN, UNKNOWN)));
    }

    public void testYellowStatus() {
        assertEquals(YELLOW, HealthStatus.merge(randomStatusesContaining(GREEN, UNKNOWN, YELLOW)));
    }

    public void testRedStatus() {
        assertEquals(RED, HealthStatus.merge(randomStatusesContaining(GREEN, UNKNOWN, YELLOW, RED)));
    }

    public void testEmpty() {
        expectThrows(IllegalArgumentException.class, () -> HealthStatus.merge(Stream.empty()));
    }

    public void testStatusIndicatesHealthProblem() {
        assertFalse(GREEN.indicatesHealthProblem());
        assertFalse(UNKNOWN.indicatesHealthProblem());
        assertTrue(YELLOW.indicatesHealthProblem());
        assertTrue(RED.indicatesHealthProblem());
    }

    private static Stream<HealthStatus> randomStatusesContaining(HealthStatus... statuses) {
        var result = new ArrayList<HealthStatus>();
        for (HealthStatus status : statuses) {
            result.addAll(randomList(1, 10, () -> status));
        }
        return result.stream();
    }
}
