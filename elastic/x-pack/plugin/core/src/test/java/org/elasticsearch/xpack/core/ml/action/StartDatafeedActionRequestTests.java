/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractStreamableXContentTestCase;
import org.elasticsearch.xpack.core.ml.action.StartDatafeedAction.DatafeedParams;
import org.elasticsearch.xpack.core.ml.action.StartDatafeedAction.Request;

import static org.hamcrest.Matchers.equalTo;

public class StartDatafeedActionRequestTests extends AbstractStreamableXContentTestCase<Request> {

    @Override
    protected Request createTestInstance() {
        DatafeedParams params = new DatafeedParams(randomAlphaOfLength(10), randomNonNegativeLong());
        if (randomBoolean()) {
            params.setEndTime(randomNonNegativeLong());
        }
        if (randomBoolean()) {
            params.setTimeout(TimeValue.timeValueMillis(randomNonNegativeLong()));
        }
        return new Request(params);
    }

    @Override
    protected boolean supportsUnknownFields() {
        return false;
    }

    @Override
    protected Request createBlankInstance() {
        return new Request();
    }

    @Override
    protected Request doParseInstance(XContentParser parser) {
        return Request.parseRequest(null, parser);
    }

    public void testParseDateOrThrow() {
        assertEquals(0L, StartDatafeedAction.DatafeedParams.parseDateOrThrow("0",
                StartDatafeedAction.START_TIME, () -> System.currentTimeMillis()));
        assertEquals(0L, StartDatafeedAction.DatafeedParams.parseDateOrThrow("1970-01-01T00:00:00Z",
                StartDatafeedAction.START_TIME, () -> System.currentTimeMillis()));
        assertThat(StartDatafeedAction.DatafeedParams.parseDateOrThrow("now",
                StartDatafeedAction.START_TIME, () -> 123456789L), equalTo(123456789L));

        Exception e = expectThrows(ElasticsearchParseException.class,
                () -> StartDatafeedAction.DatafeedParams.parseDateOrThrow("not-a-date",
                        StartDatafeedAction.START_TIME, () -> System.currentTimeMillis()));
        assertEquals("Query param [start] with value [not-a-date] cannot be parsed as a date or converted to a number (epoch).",
                e.getMessage());
    }
}
