/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.action;

import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractStreamableXContentTestCase;
import org.elasticsearch.xpack.dataframe.action.PreviewDataFrameTransformAction.Response;

import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class PreviewDataFrameTransformsActionResponseTests extends AbstractStreamableXContentTestCase<Response> {


    @Override
    protected Response doParseInstance(XContentParser parser) throws IOException {
        return Response.fromXContent(parser);
    }

    @Override
    protected Response createBlankInstance() {
        return new Response();
    }

    @Override
    protected Response createTestInstance() {
        int size = randomIntBetween(0, 10);
        List<Map<String, Object>> data = new ArrayList<>(size);
        for (int i = 0; i < size; i++) {
            Map<String, Object> datum = new HashMap<>();
            Map<String, Object> entry = new HashMap<>();
            entry.put("value1", randomIntBetween(1, 100));
            datum.put(randomAlphaOfLength(10), entry);
            data.add(datum);
        }
        return new Response(data);
    }

    @Override
    protected boolean supportsUnknownFields() {
        return false;
    }
}
