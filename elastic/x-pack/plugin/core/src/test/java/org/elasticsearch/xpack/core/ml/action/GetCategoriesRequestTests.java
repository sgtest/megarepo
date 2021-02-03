/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.action.util.PageParams;

public class GetCategoriesRequestTests extends AbstractSerializingTestCase<GetCategoriesAction.Request> {

    @Override
    protected GetCategoriesAction.Request createTestInstance() {
        String jobId = randomAlphaOfLength(10);
        GetCategoriesAction.Request request = new GetCategoriesAction.Request(jobId);
        if (randomBoolean()) {
            request.setCategoryId(randomNonNegativeLong());
        } else {
            int from = randomInt(10000);
            int size = randomInt(10000);
            request.setPageParams(new PageParams(from, size));
        }
        if (randomBoolean()) {
            request.setPartitionFieldValue(randomAlphaOfLength(10));
        }
        return request;
    }

    @Override
    protected Writeable.Reader<GetCategoriesAction.Request> instanceReader() {
        return GetCategoriesAction.Request::new;
    }

    @Override
    protected boolean supportsUnknownFields() {
        return false;
    }

    @Override
    protected GetCategoriesAction.Request doParseInstance(XContentParser parser) {
        return GetCategoriesAction.Request.parseRequest(null, parser);
    }
}
