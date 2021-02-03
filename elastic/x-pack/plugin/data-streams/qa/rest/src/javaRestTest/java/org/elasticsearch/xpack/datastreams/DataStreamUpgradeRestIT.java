/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.datastreams;

import org.apache.http.util.EntityUtils;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.test.rest.ESRestTestCase;

import java.io.IOException;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.rest.action.search.RestSearchAction.TOTAL_HITS_AS_INT_PARAM;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.notNullValue;

/**
 * Contains integration tests that simulate the new indexing strategy upgrade scenarios.
 */
public class DataStreamUpgradeRestIT extends ESRestTestCase {

    public void testCompatibleMappingUpgrade() throws Exception {
        // Create pipeline
        Request putPipelineRequest = new Request("PUT", "/_ingest/pipeline/mysql-error1");
        putPipelineRequest.setJsonEntity("{\"processors\":[]}");
        assertOK(client().performRequest(putPipelineRequest));

        // Create a template
        Request putComposableIndexTemplateRequest = new Request("POST", "/_index_template/mysql-error");
        putComposableIndexTemplateRequest.setJsonEntity(
            "{"
                + "\"index_patterns\":[\"logs-mysql-*\"],"
                + "\"priority\":200,"
                + "\"composed_of\":[\"logs-mappings\",\"logs-settings\"],"
                + "\"data_stream\":{},"
                + "\"template\":{"
                + "\"mappings\":{"
                + "\"properties\":{"
                + "\"thread_id\":{\"type\":\"long\"}"
                + "}"
                + "},"
                + "\"settings\":{"
                + "\"index.default_pipeline\":\"mysql-error1\""
                + "}"
                + "}"
                + "}"
        );
        assertOK(client().performRequest(putComposableIndexTemplateRequest));

        // Create a data stream and index first doc
        Request indexRequest = new Request("POST", "/logs-mysql-error/_doc");
        indexRequest.setJsonEntity("{\"@timestamp\": \"2020-12-12\",\"message\":\"abc\",\"thread_id\":23}");
        assertOK(client().performRequest(indexRequest));

        // Create new pipeline and update default pipeline:
        putPipelineRequest = new Request("PUT", "/_ingest/pipeline/mysql-error2");
        putPipelineRequest.setJsonEntity(
            "{\"processors\":[{\"rename\":{\"field\":\"thread_id\",\"target_field\":\"thread.id\"," + "\"ignore_failure\":true}}]}"
        );
        assertOK(client().performRequest(putPipelineRequest));
        Request updateSettingsRequest = new Request("PUT", "/logs-mysql-error/_settings");
        updateSettingsRequest.setJsonEntity("{ \"index\": { \"default_pipeline\" : \"mysql-error2\" }}");
        assertOK(client().performRequest(updateSettingsRequest));

        // Update template
        putComposableIndexTemplateRequest = new Request("POST", "/_index_template/mysql-error");
        putComposableIndexTemplateRequest.setJsonEntity(
            "{"
                + "\"index_patterns\":[\"logs-mysql-*\"],"
                + "\"priority\":200,"
                + "\"composed_of\":[\"logs-mappings\",\"logs-settings\"],"
                + "\"data_stream\":{},"
                + "\"template\":{"
                + "\"mappings\":{"
                + "\"properties\":{"
                + "\"thread\":{"
                + "\"properties\":{"
                + "\"id\":{\"type\":\"long\"}"
                + "}"
                + "}"
                + "}"
                + "},"
                + "\"settings\":{"
                + "\"index.default_pipeline\":\"mysql-error2\""
                + "}"
                + "}"
                + "}"
        );
        assertOK(client().performRequest(putComposableIndexTemplateRequest));

        // Update mapping
        Request putMappingRequest = new Request("PUT", "/logs-mysql-error/_mappings");
        putMappingRequest.addParameters(Map.of("write_index_only", "true"));
        putMappingRequest.setJsonEntity("{\"properties\":{\"thread\":{\"properties\":{\"id\":{\"type\":\"long\"}}}}}");
        assertOK(client().performRequest(putMappingRequest));

        // Delete old pipeline
        Request deletePipeline = new Request("DELETE", "/_ingest/pipeline/mysql-error1");
        assertOK(client().performRequest(deletePipeline));

        // Index more docs
        indexRequest = new Request("POST", "/logs-mysql-error/_doc");
        indexRequest.setJsonEntity("{\"@timestamp\": \"2020-12-12\",\"message\":\"abc\",\"thread_id\":24}");
        assertOK(client().performRequest(indexRequest));
        indexRequest = new Request("POST", "/logs-mysql-error/_doc");
        indexRequest.setJsonEntity("{\"@timestamp\": \"2020-12-12\",\"message\":\"abc\",\"thread\":{\"id\":24}}");
        assertOK(client().performRequest(indexRequest));

        Request refreshRequest = new Request("POST", "/logs-mysql-error/_refresh");
        assertOK(client().performRequest(refreshRequest));

        verifyTotalHitCount("logs-mysql-error", "{\"query\":{\"match\":{\"thread.id\": 24}}}", 2, "thread.id");

        Request deleteDateStreamRequest = new Request("DELETE", "/_data_stream/logs-mysql-error");
        assertOK(client().performRequest(deleteDateStreamRequest));
    }

    public void testConflictingMappingUpgrade() throws Exception {
        // Create pipeline
        Request putPipelineRequest = new Request("PUT", "/_ingest/pipeline/mysql-error1");
        putPipelineRequest.setJsonEntity("{\"processors\":[]}");
        assertOK(client().performRequest(putPipelineRequest));

        // Create a template
        Request putComposableIndexTemplateRequest = new Request("POST", "/_index_template/mysql-error");
        putComposableIndexTemplateRequest.setJsonEntity(
            "{"
                + "\"index_patterns\":[\"logs-mysql-*\"],"
                + "\"priority\":200,"
                + "\"composed_of\":[\"logs-mappings\",\"logs-settings\"],"
                + "\"data_stream\":{},"
                + "\"template\":{"
                + "\"mappings\":{"
                + "\"properties\":{"
                + "\"thread\":{\"type\":\"long\"}"
                + "}"
                + "},"
                + "\"settings\":{"
                + "\"index.default_pipeline\":\"mysql-error1\""
                + "}"
                + "}"
                + "}"
        );
        assertOK(client().performRequest(putComposableIndexTemplateRequest));

        // Create a data stream and index first doc
        Request indexRequest = new Request("POST", "/logs-mysql-error/_doc");
        indexRequest.setJsonEntity("{\"@timestamp\": \"2020-12-12\",\"message\":\"abc\",\"thread\":23}");
        assertOK(client().performRequest(indexRequest));

        // Create new pipeline and update default pipeline:
        putPipelineRequest = new Request("PUT", "/_ingest/pipeline/mysql-error2");
        putPipelineRequest.setJsonEntity(
            "{\"processors\":[{\"rename\":{\"field\":\"thread\",\"target_field\":\"thread.id\"," + "\"ignore_failure\":true}}]}"
        );
        assertOK(client().performRequest(putPipelineRequest));
        Request updateSettingsRequest = new Request("PUT", "/logs-mysql-error/_settings");
        updateSettingsRequest.setJsonEntity("{ \"index\": { \"default_pipeline\" : \"mysql-error2\" }}");
        assertOK(client().performRequest(updateSettingsRequest));

        // Update template
        putComposableIndexTemplateRequest = new Request("POST", "/_index_template/mysql-error");
        putComposableIndexTemplateRequest.setJsonEntity(
            "{"
                + "\"index_patterns\":[\"logs-mysql-*\"],"
                + "\"priority\":200,"
                + "\"composed_of\":[\"logs-mappings\",\"logs-settings\"],"
                + "\"data_stream\":{},"
                + "\"template\":{"
                + "\"mappings\":{"
                + "\"properties\":{"
                + "\"thread\":{"
                + "\"properties\":{"
                + "\"id\":{\"type\":\"long\"}"
                + "}"
                + "}"
                + "}"
                + "},"
                + "\"settings\":{"
                + "\"index.default_pipeline\":\"mysql-error2\""
                + "}"
                + "}"
                + "}"
        );
        assertOK(client().performRequest(putComposableIndexTemplateRequest));

        // Update mapping
        Request putMappingRequest = new Request("PUT", "/logs-mysql-error/_mappings");
        putMappingRequest.addParameters(Map.of("write_index_only", "true"));
        putMappingRequest.setJsonEntity("{\"properties\":{\"thread\":{\"properties\":{\"id\":{\"type\":\"long\"}}}}}");
        Exception e = expectThrows(ResponseException.class, () -> client().performRequest(putMappingRequest));
        assertThat(e.getMessage(), containsString("can't merge a non object mapping [thread] with an object mapping"));

        // Rollover
        Request rolloverRequest = new Request("POST", "/logs-mysql-error/_rollover");
        assertOK(client().performRequest(rolloverRequest));

        // Delete old pipeline
        Request deletePipeline = new Request("DELETE", "/_ingest/pipeline/mysql-error1");
        assertOK(client().performRequest(deletePipeline));

        // Index more docs
        indexRequest = new Request("POST", "/logs-mysql-error/_doc");
        indexRequest.setJsonEntity("{\"@timestamp\": \"2020-12-12\",\"message\":\"abc\",\"thread\":24}");
        assertOK(client().performRequest(indexRequest));
        indexRequest = new Request("POST", "/logs-mysql-error/_doc");
        indexRequest.setJsonEntity("{\"@timestamp\": \"2020-12-12\",\"message\":\"abc\",\"thread\":{\"id\":24}}");
        assertOK(client().performRequest(indexRequest));

        Request refreshRequest = new Request("POST", "/logs-mysql-error/_refresh");
        assertOK(client().performRequest(refreshRequest));

        verifyTotalHitCount("logs-mysql-error", "{\"query\":{\"match\":{\"thread.id\": 24}}}", 2, "thread.id");

        Request deleteDateStreamRequest = new Request("DELETE", "/_data_stream/logs-mysql-error");
        assertOK(client().performRequest(deleteDateStreamRequest));
    }

    static void verifyTotalHitCount(String index, String requestBody, int expectedTotalHits, String requiredField) throws IOException {
        Request request = new Request("GET", "/" + index + "/_search");
        request.addParameter(TOTAL_HITS_AS_INT_PARAM, "true");
        request.setJsonEntity(requestBody);
        Response response = client().performRequest(request);
        assertThat(response.getStatusLine().getStatusCode(), equalTo(200));

        Map<?, ?> responseBody = XContentHelper.convertToMap(JsonXContent.jsonXContent, EntityUtils.toString(response.getEntity()), false);
        int totalHits = (int) XContentMapValues.extractValue("hits.total", responseBody);
        assertThat(totalHits, equalTo(expectedTotalHits));

        List<?> hits = (List<?>) XContentMapValues.extractValue("hits.hits", responseBody);
        assertThat(hits.size(), equalTo(expectedTotalHits));
        for (Object element : hits) {
            Map<?, ?> hit = (Map<?, ?>) element;
            Object value = XContentMapValues.extractValue("_source." + requiredField, hit);
            assertThat(value, notNullValue());
        }
    }
}
