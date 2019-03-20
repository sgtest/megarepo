/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.client;

import org.apache.http.client.methods.HttpDelete;
import org.apache.http.client.methods.HttpPost;
import org.apache.http.client.methods.HttpPut;
import org.elasticsearch.client.dataframe.DeleteDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.PreviewDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.PutDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.StartDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.StopDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformConfig;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformConfigTests;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;
import java.util.Collections;

import static org.hamcrest.Matchers.equalTo;

public class DataFrameRequestConvertersTests extends ESTestCase {

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        SearchModule searchModule = new SearchModule(Settings.EMPTY, false, Collections.emptyList());
        return new NamedXContentRegistry(searchModule.getNamedXContents());
    }

    public void testPutDataFrameTransform() throws IOException {
        PutDataFrameTransformRequest putRequest = new PutDataFrameTransformRequest(
                DataFrameTransformConfigTests.randomDataFrameTransformConfig());
        Request request = DataFrameRequestConverters.putDataFrameTransform(putRequest);

        assertEquals(HttpPut.METHOD_NAME, request.getMethod());
        assertThat(request.getEndpoint(), equalTo("/_data_frame/transforms/" + putRequest.getConfig().getId()));

        try (XContentParser parser = createParser(JsonXContent.jsonXContent, request.getEntity().getContent())) {
            DataFrameTransformConfig parsedConfig = DataFrameTransformConfig.PARSER.apply(parser, null);
            assertThat(parsedConfig, equalTo(putRequest.getConfig()));
        }
    }

    public void testDeleteDataFrameTransform() {
        DeleteDataFrameTransformRequest deleteRequest = new DeleteDataFrameTransformRequest("foo");
        Request request = DataFrameRequestConverters.deleteDataFrameTransform(deleteRequest);

        assertEquals(HttpDelete.METHOD_NAME, request.getMethod());
        assertThat(request.getEndpoint(), equalTo("/_data_frame/transforms/foo"));
    }

    public void testStartDataFrameTransform() {
        String id = randomAlphaOfLength(10);
        TimeValue timeValue = null;
        if (randomBoolean()) {
            timeValue = TimeValue.parseTimeValue(randomTimeValue(), "timeout");
        }
        StartDataFrameTransformRequest startRequest = new StartDataFrameTransformRequest(id, timeValue);

        Request request = DataFrameRequestConverters.startDataFrameTransform(startRequest);
        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertThat(request.getEndpoint(), equalTo("/_data_frame/transforms/" + startRequest.getId() + "/_start"));

        if (timeValue != null) {
            assertTrue(request.getParameters().containsKey("timeout"));
            assertEquals(startRequest.getTimeout(), TimeValue.parseTimeValue(request.getParameters().get("timeout"), "timeout"));
        } else {
            assertFalse(request.getParameters().containsKey("timeout"));
        }
    }

    public void testStopDataFrameTransform() {
        String id = randomAlphaOfLength(10);
        Boolean waitForCompletion = null;
        if (randomBoolean()) {
            waitForCompletion = randomBoolean();
        }
        TimeValue timeValue = null;
        if (randomBoolean()) {
            timeValue = TimeValue.parseTimeValue(randomTimeValue(), "timeout");
        }
        StopDataFrameTransformRequest stopRequest = new StopDataFrameTransformRequest(id, waitForCompletion, timeValue);


        Request request = DataFrameRequestConverters.stopDataFrameTransform(stopRequest);
        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertThat(request.getEndpoint(), equalTo("/_data_frame/transforms/" + stopRequest.getId() + "/_stop"));

        if (waitForCompletion != null) {
            assertTrue(request.getParameters().containsKey("wait_for_completion"));
            assertEquals(stopRequest.getWaitForCompletion(), Boolean.parseBoolean(request.getParameters().get("wait_for_completion")));
        } else {
            assertFalse(request.getParameters().containsKey("wait_for_completion"));
        }

        if (timeValue != null) {
            assertTrue(request.getParameters().containsKey("timeout"));
            assertEquals(stopRequest.getTimeout(), TimeValue.parseTimeValue(request.getParameters().get("timeout"), "timeout"));
        } else {
            assertFalse(request.getParameters().containsKey("timeout"));
        }
    }

    public void testPreviewDataFrameTransform() throws IOException {
        PreviewDataFrameTransformRequest previewRequest = new PreviewDataFrameTransformRequest(
                DataFrameTransformConfigTests.randomDataFrameTransformConfig());
        Request request = DataFrameRequestConverters.previewDataFrameTransform(previewRequest);

        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertThat(request.getEndpoint(), equalTo("/_data_frame/transforms/_preview"));

        try (XContentParser parser = createParser(JsonXContent.jsonXContent, request.getEntity().getContent())) {
            DataFrameTransformConfig parsedConfig = DataFrameTransformConfig.PARSER.apply(parser, null);
            assertThat(parsedConfig, equalTo(previewRequest.getConfig()));
        }
    }
}
