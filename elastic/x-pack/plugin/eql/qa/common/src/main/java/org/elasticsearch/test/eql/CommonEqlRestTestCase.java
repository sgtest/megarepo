/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.test.eql;

import org.apache.http.util.EntityUtils;
import org.elasticsearch.Build;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.junit.After;
import org.junit.Before;
import org.junit.BeforeClass;

import java.util.ArrayList;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.is;

public abstract class CommonEqlRestTestCase extends ESRestTestCase {

    static class SearchTestConfiguration {
        final String input;
        final int expectedStatus;
        final String expectedMessage;

        SearchTestConfiguration(String input, int status, String msg) {
            this.input = input;
            this.expectedStatus = status;
            this.expectedMessage = msg;
        }
    }

    public static final String defaultValidationIndexName = "eql_search_validation_test";
    private static final String validQuery = "process where user = 'SYSTEM'";

    public static final ArrayList<SearchTestConfiguration> searchValidationTests;
    static {
        searchValidationTests = new ArrayList<>();
        searchValidationTests.add(new SearchTestConfiguration(null, 400, "request body or source parameter is required"));
        searchValidationTests.add(new SearchTestConfiguration("{}", 400, "query is null or empty"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"\"}", 400, "query is null or empty"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"timestamp_field\": \"\"}",
            400, "timestamp field is null or empty"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"event_category_field\": \"\"}",
            400, "event category field is null or empty"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"implicit_join_key_field\": \"\"}",
            400, "implicit join key field is null or empty"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"size\": 0}",
            400, "size must be greater than 0"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"size\": -1}",
            400, "size must be greater than 0"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"search_after\": null}",
            400, "search_after doesn't support values of type: VALUE_NULL"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"search_after\": []}",
            400, "must contains at least one value"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"filter\": null}",
            400, "filter doesn't support values of type: VALUE_NULL"));
        searchValidationTests.add(new SearchTestConfiguration("{\"query\": \"" + validQuery + "\", \"filter\": {}}",
            400, "query malformed, empty clause found"));
    }

    @BeforeClass
    public static void checkForSnapshot() {
        assumeTrue("Only works on snapshot builds for now", Build.CURRENT.isSnapshot());
    }

    @Before
    public void setup() throws Exception {
        createIndex(defaultValidationIndexName, Settings.EMPTY);
    }

    @After
    public void cleanup() throws Exception {
        deleteIndex(defaultValidationIndexName);
    }

    public void testSearchValidationFailures() throws Exception {
        final String contentType = "application/json";
        for (SearchTestConfiguration config : searchValidationTests) {
            final String endpoint = "/" + defaultValidationIndexName + "/_eql/search";
            Request request = new Request("GET", endpoint);
            request.setJsonEntity(config.input);

            Response response = null;
            if (config.expectedStatus == 400) {
                ResponseException e = expectThrows(ResponseException.class, () -> client().performRequest(request));
                response = e.getResponse();
            } else {
                response = client().performRequest(request);
            }

            assertThat(response.getHeader("Content-Type"), containsString(contentType));
            assertThat(EntityUtils.toString(response.getEntity()), containsString(config.expectedMessage));
            assertThat(response.getStatusLine().getStatusCode(), is(config.expectedStatus));
        }
    }
}
