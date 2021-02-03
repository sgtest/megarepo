/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.ingest.common;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.transport.TransportService;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;

import static org.hamcrest.CoreMatchers.not;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.sameInstance;
import static org.hamcrest.core.IsNull.notNullValue;
import static org.hamcrest.core.IsNull.nullValue;
import static org.mockito.Mockito.mock;

public class GrokProcessorGetActionTests extends ESTestCase {
    private static final Map<String, String> TEST_PATTERNS = Map.of("PATTERN2", "foo2", "PATTERN1", "foo1");

    public void testRequest() throws Exception {
        GrokProcessorGetAction.Request request = new GrokProcessorGetAction.Request(false);
        BytesStreamOutput out = new BytesStreamOutput();
        request.writeTo(out);
        StreamInput streamInput = out.bytes().streamInput();
        GrokProcessorGetAction.Request otherRequest = new GrokProcessorGetAction.Request(streamInput);
        assertThat(otherRequest.validate(), nullValue());
    }

    public void testResponseSerialization() throws Exception {
        GrokProcessorGetAction.Response response = new GrokProcessorGetAction.Response(TEST_PATTERNS);
        BytesStreamOutput out = new BytesStreamOutput();
        response.writeTo(out);
        StreamInput streamInput = out.bytes().streamInput();
        GrokProcessorGetAction.Response otherResponse = new GrokProcessorGetAction.Response(streamInput);
        assertThat(response.getGrokPatterns(), equalTo(TEST_PATTERNS));
        assertThat(response.getGrokPatterns(), equalTo(otherResponse.getGrokPatterns()));
    }

    public void testResponseSorting() {
        List<String> sortedKeys = new ArrayList<>(TEST_PATTERNS.keySet());
        Collections.sort(sortedKeys);
        GrokProcessorGetAction.TransportAction transportAction =
            new GrokProcessorGetAction.TransportAction(mock(TransportService.class), mock(ActionFilters.class), TEST_PATTERNS);
        GrokProcessorGetAction.Response[] receivedResponse = new GrokProcessorGetAction.Response[1];
        transportAction.doExecute(null, new GrokProcessorGetAction.Request(true), new ActionListener<>() {
            @Override
            public void onResponse(GrokProcessorGetAction.Response response) {
                receivedResponse[0] = response;
            }

            @Override
            public void onFailure(Exception e) {
                fail();
            }
        });
        assertThat(receivedResponse[0], notNullValue());
        assertThat(receivedResponse[0].getGrokPatterns().keySet().toArray(), equalTo(sortedKeys.toArray()));

        GrokProcessorGetAction.Response firstResponse = receivedResponse[0];
        transportAction.doExecute(null, new GrokProcessorGetAction.Request(true), new ActionListener<>() {
            @Override
            public void onResponse(GrokProcessorGetAction.Response response) {
                receivedResponse[0] = response;
            }

            @Override
            public void onFailure(Exception e) {
                fail();
            }
        });
        assertThat(receivedResponse[0], notNullValue());
        assertThat(receivedResponse[0], not(sameInstance(firstResponse)));
        assertThat(receivedResponse[0].getGrokPatterns(), sameInstance(firstResponse.getGrokPatterns()));
    }

    @SuppressWarnings("unchecked")
    public void testResponseToXContent() throws Exception {
        GrokProcessorGetAction.Response response = new GrokProcessorGetAction.Response(TEST_PATTERNS);
        try (XContentBuilder builder = JsonXContent.contentBuilder()) {
            response.toXContent(builder, ToXContent.EMPTY_PARAMS);
            Map<String, Object> converted = XContentHelper.convertToMap(BytesReference.bytes(builder), false, builder.contentType()).v2();
            Map<String, String> patterns = (Map<String, String>) converted.get("patterns");
            assertThat(patterns.size(), equalTo(2));
            assertThat(patterns.get("PATTERN1"), equalTo("foo1"));
            assertThat(patterns.get("PATTERN2"), equalTo("foo2"));
        }
    }
}
