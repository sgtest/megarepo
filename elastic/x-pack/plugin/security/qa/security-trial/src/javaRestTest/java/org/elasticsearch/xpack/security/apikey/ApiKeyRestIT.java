/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.apikey;

import org.elasticsearch.client.Request;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.client.security.support.ApiKey;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.test.XContentTestUtils;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;
import org.elasticsearch.xpack.security.SecurityOnTrialLicenseRestTestCase;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.lessThanOrEqualTo;
import static org.hamcrest.Matchers.notNullValue;

/**
 * Integration Rest Tests relating to API Keys.
 * Tested against a trial license
 */
public class ApiKeyRestIT extends SecurityOnTrialLicenseRestTestCase {

    private static final String SYSTEM_USER = "system_user";
    private static final SecureString SYSTEM_USER_PASSWORD = new SecureString("system-user-password".toCharArray());
    private static final String END_USER = "end_user";
    private static final SecureString END_USER_PASSWORD = new SecureString("end-user-password".toCharArray());

    @Before
    public void createUsers() throws IOException {
        createUser(SYSTEM_USER, SYSTEM_USER_PASSWORD, List.of("system_role"));
        createRole("system_role", Set.of("grant_api_key"));
        createUser(END_USER, END_USER_PASSWORD, List.of("user_role"));
        createRole("user_role", Set.of("monitor"));
    }

    @After
    public void cleanUp() throws IOException {
        deleteUser("system_user");
        deleteUser("end_user");
        deleteRole("system_role");
        deleteRole("user_role");
        invalidateApiKeysForUser(END_USER);
    }

    public void testGrantApiKeyForOtherUserWithPassword() throws IOException {
        Request request = new Request("POST", "_security/api_key/grant");
        request.setOptions(RequestOptions.DEFAULT.toBuilder().addHeader("Authorization",
            UsernamePasswordToken.basicAuthHeaderValue(SYSTEM_USER, SYSTEM_USER_PASSWORD)));
        final Map<String, Object> requestBody = Map.ofEntries(
            Map.entry("grant_type", "password"),
            Map.entry("username", END_USER),
            Map.entry("password", END_USER_PASSWORD.toString()),
            Map.entry("api_key", Map.of("name", "test_api_key_password"))
        );
        request.setJsonEntity(XContentTestUtils.convertToXContent(requestBody, XContentType.JSON).utf8ToString());

        final Response response = client().performRequest(request);
        final Map<String, Object> responseBody = entityAsMap(response);

        assertThat(responseBody.get("name"), equalTo("test_api_key_password"));
        assertThat(responseBody.get("id"), notNullValue());
        assertThat(responseBody.get("id"), instanceOf(String.class));

        ApiKey apiKey = getApiKey((String) responseBody.get("id"));
        assertThat(apiKey.getUsername(), equalTo(END_USER));
    }

    public void testGrantApiKeyForOtherUserWithAccessToken() throws IOException {
        final Tuple<String, String> token = super.createOAuthToken(END_USER, END_USER_PASSWORD);
        final String accessToken = token.v1();

        final Request request = new Request("POST", "_security/api_key/grant");
        request.setOptions(RequestOptions.DEFAULT.toBuilder().addHeader("Authorization",
            UsernamePasswordToken.basicAuthHeaderValue(SYSTEM_USER, SYSTEM_USER_PASSWORD)));
        final Map<String, Object> requestBody = Map.ofEntries(
            Map.entry("grant_type", "access_token"),
            Map.entry("access_token", accessToken),
            Map.entry("api_key", Map.of("name", "test_api_key_token", "expiration", "2h"))
        );
        request.setJsonEntity(XContentTestUtils.convertToXContent(requestBody, XContentType.JSON).utf8ToString());

        final Instant before = Instant.now();
        final Response response = client().performRequest(request);
        final Instant after = Instant.now();
        final Map<String, Object> responseBody = entityAsMap(response);

        assertThat(responseBody.get("name"), equalTo("test_api_key_token"));
        assertThat(responseBody.get("id"), notNullValue());
        assertThat(responseBody.get("id"), instanceOf(String.class));

        ApiKey apiKey = getApiKey((String) responseBody.get("id"));
        assertThat(apiKey.getUsername(), equalTo(END_USER));

        Instant minExpiry = before.plus(2, ChronoUnit.HOURS);
        Instant maxExpiry = after.plus(2, ChronoUnit.HOURS);
        assertThat(apiKey.getExpiration(), notNullValue());
        assertThat(apiKey.getExpiration(), greaterThanOrEqualTo(minExpiry));
        assertThat(apiKey.getExpiration(), lessThanOrEqualTo(maxExpiry));
    }

    public void testGrantApiKeyWithoutApiKeyNameWillFail() throws IOException {
        Request request = new Request("POST", "_security/api_key/grant");
        request.setOptions(RequestOptions.DEFAULT.toBuilder().addHeader("Authorization",
            UsernamePasswordToken.basicAuthHeaderValue(SYSTEM_USER, SYSTEM_USER_PASSWORD)));
        final Map<String, Object> requestBody = Map.ofEntries(
            Map.entry("grant_type", "password"),
            Map.entry("username", END_USER),
            Map.entry("password", END_USER_PASSWORD.toString())
        );
        request.setJsonEntity(XContentTestUtils.convertToXContent(requestBody, XContentType.JSON).utf8ToString());

        final ResponseException e =
            expectThrows(ResponseException.class, () -> client().performRequest(request));

        assertEquals(400, e.getResponse().getStatusLine().getStatusCode());
        assertThat(e.getMessage(), containsString("api key name is required"));
    }
}
