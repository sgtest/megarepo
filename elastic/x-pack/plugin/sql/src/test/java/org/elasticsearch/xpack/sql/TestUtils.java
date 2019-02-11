/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.sql;

import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.xpack.sql.proto.Mode;
import org.elasticsearch.xpack.sql.proto.Protocol;
import org.elasticsearch.xpack.sql.session.Configuration;
import org.elasticsearch.xpack.sql.util.DateUtils;

import java.time.ZoneId;

import static org.elasticsearch.test.ESTestCase.randomAlphaOfLength;
import static org.elasticsearch.test.ESTestCase.randomFrom;
import static org.elasticsearch.test.ESTestCase.randomIntBetween;
import static org.elasticsearch.test.ESTestCase.randomNonNegativeLong;
import static org.elasticsearch.test.ESTestCase.randomZone;


public class TestUtils {

    private TestUtils() {}

    public static final Configuration TEST_CFG = new Configuration(DateUtils.UTC, Protocol.FETCH_SIZE,
            Protocol.REQUEST_TIMEOUT, Protocol.PAGE_TIMEOUT, null, Mode.PLAIN,
            null, null, null);

    public static Configuration randomConfiguration() {
        return new Configuration(randomZone(),
                randomIntBetween(0,  1000),
                new TimeValue(randomNonNegativeLong()),
                new TimeValue(randomNonNegativeLong()),
                null,
                randomFrom(Mode.values()),
                randomAlphaOfLength(10),
                randomAlphaOfLength(10),
                randomAlphaOfLength(10));
    }

    public static Configuration randomConfiguration(ZoneId providedZoneId) {
        return new Configuration(providedZoneId,
                randomIntBetween(0,  1000),
                new TimeValue(randomNonNegativeLong()),
                new TimeValue(randomNonNegativeLong()),
                null,
                randomFrom(Mode.values()),
                randomAlphaOfLength(10),
                randomAlphaOfLength(10),
                randomAlphaOfLength(10));
    }

}
