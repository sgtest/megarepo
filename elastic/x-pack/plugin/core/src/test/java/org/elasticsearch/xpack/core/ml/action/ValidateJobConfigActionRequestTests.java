/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.test.AbstractStreamableTestCase;
import org.elasticsearch.xpack.core.ml.action.ValidateJobConfigAction.Request;
import org.elasticsearch.xpack.core.ml.job.config.Job;

import java.io.IOException;
import java.util.Date;

import static org.elasticsearch.xpack.core.ml.job.config.JobTests.buildJobBuilder;
import static org.elasticsearch.xpack.core.ml.job.config.JobTests.randomValidJobId;

public class ValidateJobConfigActionRequestTests extends AbstractStreamableTestCase<Request> {

    @Override
    protected Request createTestInstance() {
        return new Request(buildJobBuilder(randomValidJobId(), new Date()).build());
    }

    @Override
    protected Request createBlankInstance() {
        return new Request();
    }

    public void testParseRequest_InvalidCreateSetting() throws IOException {
        String jobId = randomValidJobId();
        Job.Builder jobConfiguration = buildJobBuilder(jobId, null);
        jobConfiguration.setLastDataTime(new Date());

        XContentBuilder builder = XContentFactory.contentBuilder(XContentType.JSON);
        XContentBuilder xContentBuilder = jobConfiguration.toXContent(builder, ToXContent.EMPTY_PARAMS);
        XContentParser parser = XContentFactory.xContent(XContentType.JSON)
                .createParser(NamedXContentRegistry.EMPTY, DeprecationHandler.THROW_UNSUPPORTED_OPERATION,
                        BytesReference.bytes(xContentBuilder).streamInput());

        expectThrows(IllegalArgumentException.class, () -> Request.parseRequest(parser));
    }
}
