/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security;

import org.apache.http.HttpHeaders;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.test.XContentTestUtils;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.oneOf;

public class QueryApiKeyIT extends SecurityInBasicRestTestCase {

    private static final String API_KEY_ADMIN_AUTH_HEADER = "Basic YXBpX2tleV9hZG1pbjpzZWN1cml0eS10ZXN0LXBhc3N3b3Jk";
    private static final String API_KEY_USER_AUTH_HEADER = "Basic YXBpX2tleV91c2VyOnNlY3VyaXR5LXRlc3QtcGFzc3dvcmQ=";
    private static final String TEST_USER_AUTH_HEADER = "Basic c2VjdXJpdHlfdGVzdF91c2VyOnNlY3VyaXR5LXRlc3QtcGFzc3dvcmQ=";

    public void testQuery() throws IOException {
        createApiKeys();
        createUser("someone");

        // Admin with manage_api_key can search for all keys
        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            "{ \"query\": { \"wildcard\": {\"name\": \"*alert*\"} } }",
            apiKeys -> {
                assertThat(apiKeys.size(), equalTo(2));
                assertThat(apiKeys.get(0).get("name"), oneOf("my-org/alert-key-1", "my-alert-key-2"));
                assertThat(apiKeys.get(1).get("name"), oneOf("my-org/alert-key-1", "my-alert-key-2"));
            });

        // An empty request body means search for all keys
        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            randomBoolean() ? "" : "{\"query\":{\"match_all\":{}}}",
            apiKeys -> assertThat(apiKeys.size(), equalTo(6)));

        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            "{\"query\":{\"bool\":{\"must\":[" +
                "{\"prefix\":{\"metadata.application\":\"fleet\"}},{\"term\":{\"metadata.environment.os\":\"Cat\"}}]}}}",
            apiKeys -> {
                assertThat(apiKeys, hasSize(2));
                assertThat(
                    apiKeys.stream().map(k -> k.get("name")).collect(Collectors.toList()),
                    containsInAnyOrder("my-org/ingest-key-1", "my-org/management-key-1"));
            }
        );

        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            "{\"query\":{\"terms\":{\"metadata.tags\":[\"prod\",\"east\"]}}}",
            apiKeys -> {
                assertThat(apiKeys.size(), equalTo(5));
            });

        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            "{\"query\":{\"range\":{\"creation_time\":{\"lt\":\"now\"}}}}",
            apiKeys -> {
                assertThat(apiKeys.size(), equalTo(6));
            });

        // Search for keys belong to an user
        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            "{ \"query\": { \"term\": {\"username\": \"api_key_user\"} } }",
            apiKeys -> {
                assertThat(apiKeys.size(), equalTo(2));
                assertThat(apiKeys.stream().map(m -> m.get("name")).collect(Collectors.toSet()),
                    equalTo(Set.of("my-ingest-key-1", "my-alert-key-2")));
            });

        // Search for keys belong to users from a realm
        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            "{ \"query\": { \"term\": {\"realm_name\": \"default_file\"} } }",
            apiKeys -> {
                assertThat(apiKeys.size(), equalTo(6));
                // search using explicit IDs
                try {

                    var subset = randomSubsetOf(randomIntBetween(1,5), apiKeys);
                    assertQuery(API_KEY_ADMIN_AUTH_HEADER,
                        "{ \"query\": { \"ids\": { \"values\": ["
                            + subset.stream().map(m -> "\"" + m.get("id") + "\"").collect(Collectors.joining(",")) + "] } } }",
                        keys -> {
                            assertThat(keys, hasSize(subset.size()));
                        });
                } catch (IOException e) {
                    throw new RuntimeException(e);
                }
            });

        // Search for fields outside of the allowlist fails
        assertQueryError(API_KEY_ADMIN_AUTH_HEADER, 400,
            "{ \"query\": { \"prefix\": {\"api_key_hash\": \"{PBKDF2}10000$\"} } }");

        // Search for fields that are not allowed in Query DSL but used internally by the service itself
        final String fieldName = randomFrom("doc_type", "api_key_invalidated");
        assertQueryError(API_KEY_ADMIN_AUTH_HEADER, 400,
            "{ \"query\": { \"term\": {\"" + fieldName + "\": \"" + randomAlphaOfLengthBetween(3, 8) + "\"} } }");

        // Search for api keys won't return other entities
        assertQuery(API_KEY_ADMIN_AUTH_HEADER,
            "{ \"query\": { \"term\": {\"name\": \"someone\"} } }",
            apiKeys -> {
                assertThat(apiKeys, empty());
            });

        // User with manage_own_api_key will only see its own keys
        assertQuery(API_KEY_USER_AUTH_HEADER,
            randomBoolean() ? "" : "{\"query\":{\"match_all\":{}}}",
            apiKeys -> {
            assertThat(apiKeys.size(), equalTo(2));
            assertThat(apiKeys.stream().map(m -> m.get("name")).collect(Collectors.toSet()),
                containsInAnyOrder("my-ingest-key-1", "my-alert-key-2"));
        });

        assertQuery(API_KEY_USER_AUTH_HEADER,
            "{ \"query\": { \"wildcard\": {\"name\": \"*alert*\"} } }",
            apiKeys -> {
                assertThat(apiKeys.size(), equalTo(1));
                assertThat(apiKeys.get(0).get("name"), equalTo("my-alert-key-2"));
            });

        // User without manage_api_key or manage_own_api_key gets 403 trying to search API keys
        assertQueryError(TEST_USER_AUTH_HEADER, 403,
            "{ \"query\": { \"wildcard\": {\"name\": \"*alert*\"} } }");
    }

    public void testQueryShouldRespectOwnerIdentityWithApiKeyAuth() throws IOException {
        final Tuple<String, String> powerKey = createApiKey("power-key-1", null, null, API_KEY_ADMIN_AUTH_HEADER);
        final String powerKeyAuthHeader = "ApiKey " + Base64.getEncoder()
            .encodeToString((powerKey.v1() + ":" + powerKey.v2()).getBytes(StandardCharsets.UTF_8));

        final Tuple<String, String> limitKey = createApiKey("limit-key-1",
            Map.of("a", Map.of("cluster", List.of("manage_own_api_key"))), null, API_KEY_ADMIN_AUTH_HEADER);
        final String limitKeyAuthHeader = "ApiKey " + Base64.getEncoder()
            .encodeToString((limitKey.v1() + ":" + limitKey.v2()).getBytes(StandardCharsets.UTF_8));

        createApiKey("power-key-1-derived-1", Map.of("a", Map.of()), null, powerKeyAuthHeader);
        createApiKey("limit-key-1-derived-1", Map.of("a", Map.of()), null, limitKeyAuthHeader);

        createApiKey("user-key-1", Map.of(), API_KEY_USER_AUTH_HEADER);
        createApiKey("user-key-2", Map.of(), API_KEY_USER_AUTH_HEADER);

        // powerKey gets back all keys since it has manage_api_key privilege
        assertQuery(powerKeyAuthHeader, "", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(6));
            assertThat(
                apiKeys.stream().map(m -> (String) m.get("name")).collect(Collectors.toUnmodifiableSet()),
                equalTo(Set.of("power-key-1", "limit-key-1", "power-key-1-derived-1", "limit-key-1-derived-1",
                    "user-key-1", "user-key-2")));
        });

        // limitKey gets only keys owned by the original user, not including the derived keys since they are not
        // owned by the user (realm_name is _es_api_key).
        assertQuery(limitKeyAuthHeader, "", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(2));
            assertThat(
                apiKeys.stream().map(m -> (String) m.get("name")).collect(Collectors.toUnmodifiableSet()),
                equalTo(Set.of("power-key-1", "limit-key-1")));
        });
    }

    private void assertQueryError(String authHeader, int statusCode, String body) throws IOException {
        final Request request = new Request("GET", "/_security/_query/api_key");
        request.setJsonEntity(body);
        request.setOptions(
            request.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        final ResponseException responseException = expectThrows(ResponseException.class, () -> client().performRequest(request));
        assertThat(responseException.getResponse().getStatusLine().getStatusCode(), equalTo(statusCode));
    }

    private void assertQuery(String authHeader, String body,
                             Consumer<List<Map<String, Object>>> apiKeysVerifier) throws IOException {
        final Request request = new Request("GET", "/_security/_query/api_key");
        request.setJsonEntity(body);
        request.setOptions(
            request.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        final Response response = client().performRequest(request);
        assertOK(response);
        final Map<String, Object> responseMap = responseAsMap(response);
        @SuppressWarnings("unchecked")
        final List<Map<String, Object>> api_keys = (List<Map<String, Object>>) responseMap.get("api_keys");
        apiKeysVerifier.accept(api_keys);
    }

    private void createApiKeys() throws IOException {
        createApiKey(
            "my-org/ingest-key-1",
            Map.of(
                "application", "fleet-agent",
                "tags", List.of("prod", "east"),
                "environment", Map.of(
                    "os", "Cat", "level", 42, "system", false, "hostname", "my-org-host-1")
            ),
            API_KEY_ADMIN_AUTH_HEADER);

        createApiKey(
            "my-org/ingest-key-2",
            Map.of(
                "application", "fleet-server",
                "tags", List.of("staging", "east"),
                "environment", Map.of(
                    "os", "Dog", "level", 11, "system", true, "hostname", "my-org-host-2")
            ),
            API_KEY_ADMIN_AUTH_HEADER);

        createApiKey(
            "my-org/management-key-1",
            Map.of(
                "application", "fleet-agent",
                "tags", List.of("prod", "west"),
                "environment", Map.of(
                    "os", "Cat", "level", 11, "system", false, "hostname", "my-org-host-3")
            ),
            API_KEY_ADMIN_AUTH_HEADER);

        createApiKey(
            "my-org/alert-key-1",
            Map.of(
                "application", "siem",
                "tags", List.of("prod", "north", "upper"),
                "environment", Map.of(
                    "os", "Dog", "level", 3, "system", true, "hostname", "my-org-host-4")
            ),
            API_KEY_ADMIN_AUTH_HEADER);

        createApiKey(
            "my-ingest-key-1",
            Map.of(
                "application", "cli",
                "tags", List.of("user", "test"),
                "notes", Map.of(
                    "sun", "hot", "earth", "blue")
            ),
            API_KEY_USER_AUTH_HEADER);

        createApiKey(
            "my-alert-key-2",
            Map.of(
                "application", "web",
                "tags", List.of("app", "prod"),
                "notes", Map.of(
                    "shared", false, "weather", "sunny")
            ),
            API_KEY_USER_AUTH_HEADER);
    }

    private Tuple<String, String> createApiKey(String name, Map<String, Object> metadata, String authHeader) throws IOException {
        return createApiKey(name, null, metadata, authHeader);
    }

    private Tuple<String, String> createApiKey(String name,
                                               Map<String, Object> roleDescriptors,
                                               Map<String, Object> metadata,
                                               String authHeader) throws IOException {
        final Request request = new Request("POST", "/_security/api_key");
        final String roleDescriptorsString =
            XContentTestUtils.convertToXContent(roleDescriptors == null ? Map.of() : roleDescriptors, XContentType.JSON).utf8ToString();
        final String metadataString =
            XContentTestUtils.convertToXContent(metadata == null ? Map.of() : metadata, XContentType.JSON).utf8ToString();
        request.setJsonEntity("{\"name\":\"" + name
            + "\", \"role_descriptors\":" + roleDescriptorsString
            + ", \"metadata\":" + metadataString + "}");
        request.setOptions(request.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        final Response response = client().performRequest(request);
        assertOK(response);
        final Map<String, Object> m = responseAsMap(response);
        return new Tuple<>((String) m.get("id"), (String) m.get("api_key"));
    }

    private void createUser(String name) throws IOException {
        final Request request = new Request("POST", "/_security/user/" + name);
        request.setJsonEntity("{\"password\":\"super-strong-password\",\"roles\":[]}");
        assertOK(adminClient().performRequest(request));
    }
}
