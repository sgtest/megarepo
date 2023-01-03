/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.transform.integration;

import org.apache.http.HttpHost;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.client.RestClientBuilder;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.core.Strings;
import org.junit.Before;

import java.io.IOException;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;

import static org.hamcrest.Matchers.equalTo;

public class TransformUpdateIT extends TransformRestTestCase {

    private static final String TEST_USER_NAME = "transform_user";
    private static final String BASIC_AUTH_VALUE_TRANSFORM_USER = basicAuthHeaderValue(TEST_USER_NAME, TEST_PASSWORD_SECURE_STRING);
    private static final String TEST_ADMIN_USER_NAME_1 = "transform_admin_1";
    private static final String BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1 = basicAuthHeaderValue(
        TEST_ADMIN_USER_NAME_1,
        TEST_PASSWORD_SECURE_STRING
    );
    private static final String TEST_ADMIN_USER_NAME_2 = "transform_admin_2";
    private static final String BASIC_AUTH_VALUE_TRANSFORM_ADMIN_2 = basicAuthHeaderValue(
        TEST_ADMIN_USER_NAME_2,
        TEST_PASSWORD_SECURE_STRING
    );
    private static final String TEST_ADMIN_USER_NAME_NO_DATA = "transform_admin_no_data";
    private static final String BASIC_AUTH_VALUE_TRANSFORM_ADMIN_NO_DATA = basicAuthHeaderValue(
        TEST_ADMIN_USER_NAME_NO_DATA,
        TEST_PASSWORD_SECURE_STRING
    );
    private static final String DATA_ACCESS_ROLE = "test_data_access";
    private static final String DATA_ACCESS_ROLE_2 = "test_data_access_2";

    // preserve indices in order to reuse source indices in several test cases
    @Override
    protected boolean preserveIndicesUponCompletion() {
        return false;
    }

    @Override
    protected boolean enableWarningsCheck() {
        return false;
    }

    @Override
    protected RestClient buildClient(Settings settings, HttpHost[] hosts) throws IOException {
        RestClientBuilder builder = RestClient.builder(hosts);
        configureClient(builder, settings);
        builder.setStrictDeprecationMode(false);
        return builder.build();
    }

    @Before
    public void createIndexes() throws IOException {
        setupDataAccessRole(DATA_ACCESS_ROLE, REVIEWS_INDEX_NAME);
        setupDataAccessRole(DATA_ACCESS_ROLE_2, REVIEWS_INDEX_NAME);

        setupUser(TEST_USER_NAME, List.of("transform_user", DATA_ACCESS_ROLE));
        setupUser(TEST_ADMIN_USER_NAME_1, List.of("transform_admin", DATA_ACCESS_ROLE));
        setupUser(TEST_ADMIN_USER_NAME_2, List.of("transform_admin", DATA_ACCESS_ROLE_2));
        setupUser(TEST_ADMIN_USER_NAME_NO_DATA, List.of("transform_admin"));
        createReviewsIndex();
    }

    @SuppressWarnings("unchecked")
    public void testUpdateDeprecatedSettings() throws Exception {
        String transformId = "old_transform";
        String transformDest = transformId + "_idx";
        setupDataAccessRole(DATA_ACCESS_ROLE, REVIEWS_INDEX_NAME, transformDest);

        final Request createTransformRequest = createRequestWithAuth(
            "PUT",
            getTransformEndpoint() + transformId,
            BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1
        );
        String config = Strings.format("""
            {
              "dest": {
                "index": "%s"
              },
              "source": {
                "index": "%s"
              },
              "pivot": {
                "group_by": {
                  "reviewer": {
                    "terms": {
                      "field": "user_id"
                    }
                  }
                },
                "aggregations": {
                  "avg_rating": {
                    "avg": {
                      "field": "stars"
                    }
                  }
                },
                "max_page_search_size": 555
              }
            }""", transformDest, REVIEWS_INDEX_NAME);

        createTransformRequest.setJsonEntity(config);
        Map<String, Object> createTransformResponse = entityAsMap(client().performRequest(createTransformRequest));

        assertThat(createTransformResponse.get("acknowledged"), equalTo(Boolean.TRUE));
        Request getRequest = createRequestWithAuth("GET", getTransformEndpoint() + transformId, BASIC_AUTH_VALUE_TRANSFORM_USER);
        Map<String, Object> transforms = entityAsMap(client().performRequest(getRequest));
        assertEquals(1, XContentMapValues.extractValue("count", transforms));
        Map<String, Object> transform = ((List<Map<String, Object>>) XContentMapValues.extractValue("transforms", transforms)).get(0);
        assertThat(XContentMapValues.extractValue("pivot.max_page_search_size", transform), equalTo(555));

        final Request updateRequest = createRequestWithAuth(
            "POST",
            getTransformEndpoint() + transformId + "/_update",
            BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1
        );
        updateRequest.setJsonEntity("{}");

        Map<String, Object> updateResponse = entityAsMap(client().performRequest(updateRequest));

        assertNull(XContentMapValues.extractValue("pivot.max_page_search_size", updateResponse));
        assertThat(XContentMapValues.extractValue("settings.max_page_search_size", updateResponse), equalTo(555));

        getRequest = createRequestWithAuth("GET", getTransformEndpoint() + transformId, BASIC_AUTH_VALUE_TRANSFORM_USER);
        transforms = entityAsMap(client().performRequest(getRequest));
        assertEquals(1, XContentMapValues.extractValue("count", transforms));
        transform = ((List<Map<String, Object>>) XContentMapValues.extractValue("transforms", transforms)).get(0);

        assertNull(XContentMapValues.extractValue("pivot.max_page_search_size", transform));
        assertThat(XContentMapValues.extractValue("settings.max_page_search_size", transform), equalTo(555));
    }

    public void testUpdateTransferRights() throws Exception {
        updateTransferRightsTester(false);
    }

    public void testUpdateTransferRightsSecondaryAuthHeaders() throws Exception {
        updateTransferRightsTester(true);
    }

    private void updateTransferRightsTester(boolean useSecondaryAuthHeaders) throws Exception {
        String transformId = "transform1";
        // Note: Due to a bug the transform does not fail to start after deleting the user and role, therefore invalidating
        // the credentials stored with the config. As a workaround we use a 2nd transform that uses the same config
        // once the bug is fixed, delete this 2nd transform
        String transformIdCloned = "transform2";
        String transformDest = transformId + "_idx";
        setupDataAccessRole(DATA_ACCESS_ROLE, REVIEWS_INDEX_NAME, transformDest);
        setupDataAccessRole(DATA_ACCESS_ROLE_2, REVIEWS_INDEX_NAME, transformDest);

        final Request createTransformRequest = useSecondaryAuthHeaders
            ? createRequestWithSecondaryAuth(
                "PUT",
                getTransformEndpoint() + transformId,
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_NO_DATA,
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_2
            )
            : createRequestWithAuth("PUT", getTransformEndpoint() + transformId, BASIC_AUTH_VALUE_TRANSFORM_ADMIN_2);

        final Request createTransformRequest_2 = useSecondaryAuthHeaders
            ? createRequestWithSecondaryAuth(
                "PUT",
                getTransformEndpoint() + transformIdCloned,
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_NO_DATA,
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_2
            )
            : createRequestWithAuth("PUT", getTransformEndpoint() + transformIdCloned, BASIC_AUTH_VALUE_TRANSFORM_ADMIN_2);

        String config = Strings.format("""
            {
              "dest": {
                "index": "%s"
              },
              "source": {
                "index": "%s"
              },
              "pivot": {
                "group_by": {
                  "reviewer": {
                    "terms": {
                      "field": "user_id"
                    }
                  }
                },
                "aggregations": {
                  "avg_rating": {
                    "avg": {
                      "field": "stars"
                    }
                  }
                }
              }
            }""", transformDest, REVIEWS_INDEX_NAME);

        createTransformRequest.setJsonEntity(config);
        Map<String, Object> createTransformResponse = entityAsMap(client().performRequest(createTransformRequest));

        assertThat(createTransformResponse.get("acknowledged"), equalTo(Boolean.TRUE));
        Request getRequest = createRequestWithAuth("GET", getTransformEndpoint() + transformId, BASIC_AUTH_VALUE_TRANSFORM_ADMIN_2);
        Map<String, Object> transforms = entityAsMap(client().performRequest(getRequest));
        assertEquals(1, XContentMapValues.extractValue("count", transforms));
        // Confirm the roles were recorded as expected in the stored headers
        @SuppressWarnings("unchecked")
        Map<String, Object> transformConfig = ((List<Map<String, Object>>) transforms.get("transforms")).get(0);
        assertThat(transformConfig.get("authorization"), equalTo(Map.of("roles", List.of("transform_admin", DATA_ACCESS_ROLE_2))));

        // create a 2nd, identical one
        createTransformRequest_2.setJsonEntity(config);
        createTransformResponse = entityAsMap(client().performRequest(createTransformRequest_2));
        assertThat(createTransformResponse.get("acknowledged"), equalTo(Boolean.TRUE));

        // delete the user _and_ the role to access the data
        deleteUser(TEST_ADMIN_USER_NAME_2);
        deleteDataAccessRole(DATA_ACCESS_ROLE_2);

        // getting the transform with the just deleted admin 2 user should fail
        try {
            client().performRequest(getRequest);
            fail("request should have failed");
        } catch (ResponseException e) {
            assertThat(e.getResponse().getStatusLine().getStatusCode(), equalTo(401));
        }

        // get the transform with admin 1
        getRequest = createRequestWithAuth("GET", getTransformEndpoint() + transformId, BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1);
        transforms = entityAsMap(client().performRequest(getRequest));
        assertEquals(1, XContentMapValues.extractValue("count", transforms));

        // start using admin 1, but as the header is still admin 2
        // This fails as the stored header is still admin 2
        try {
            if (useSecondaryAuthHeaders) {
                startAndWaitForTransform(
                    transformId,
                    transformDest,
                    BASIC_AUTH_VALUE_TRANSFORM_ADMIN_NO_DATA,
                    BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1,
                    new String[0]
                );
            } else {
                startAndWaitForTransform(transformId, transformDest, BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1);
            }
            fail("request should have failed");
        } catch (ResponseException e) {
            assertThat(e.getResponse().getStatusLine().getStatusCode(), equalTo(500));
        }
        assertBusy(() -> {
            Map<?, ?> transformStatsAsMap = getTransformStateAndStats(transformId);
            assertThat(XContentMapValues.extractValue("stats.documents_indexed", transformStatsAsMap), equalTo(0));
        }, 3, TimeUnit.SECONDS);

        // update the transform with an empty body, the credentials (headers) should change
        final Request updateRequest = useSecondaryAuthHeaders
            ? createRequestWithSecondaryAuth(
                "POST",
                getTransformEndpoint() + transformIdCloned + "/_update",
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_NO_DATA,
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1
            )
            : createRequestWithAuth("POST", getTransformEndpoint() + transformIdCloned + "/_update", BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1);
        updateRequest.setJsonEntity("{}");
        assertOK(client().performRequest(updateRequest));

        // get should still work
        getRequest = createRequestWithAuth("GET", getTransformEndpoint() + transformIdCloned, BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1);
        transforms = entityAsMap(client().performRequest(getRequest));
        assertEquals(1, XContentMapValues.extractValue("count", transforms));

        // start with updated configuration should succeed
        if (useSecondaryAuthHeaders) {
            startAndWaitForTransform(
                transformIdCloned,
                transformDest,
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_NO_DATA,
                BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1,
                new String[0]
            );
        } else {
            startAndWaitForTransform(transformIdCloned, transformDest, BASIC_AUTH_VALUE_TRANSFORM_ADMIN_1);
        }
        assertBusy(() -> {
            Map<?, ?> transformStatsAsMap = getTransformStateAndStats(transformIdCloned);
            assertThat(XContentMapValues.extractValue("stats.documents_indexed", transformStatsAsMap), equalTo(27));
        }, 15, TimeUnit.SECONDS);
    }

    private void deleteUser(String user) throws IOException {
        Request request = new Request("DELETE", "/_security/user/" + user);
        client().performRequest(request);
    }

    protected void deleteDataAccessRole(String role) throws IOException {
        Request request = new Request("DELETE", "/_security/role/" + role);
        client().performRequest(request);
    }
}
