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
import org.elasticsearch.core.Strings;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.test.XContentTestUtils;
import org.elasticsearch.xcontent.XContentType;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;
import java.util.stream.Collectors;
import java.util.stream.IntStream;

import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.oneOf;

public class QueryApiKeyIT extends SecurityInBasicRestTestCase {

    private static final String API_KEY_ADMIN_AUTH_HEADER = "Basic YXBpX2tleV9hZG1pbjpzZWN1cml0eS10ZXN0LXBhc3N3b3Jk";
    private static final String API_KEY_USER_AUTH_HEADER = "Basic YXBpX2tleV91c2VyOnNlY3VyaXR5LXRlc3QtcGFzc3dvcmQ=";
    private static final String TEST_USER_AUTH_HEADER = "Basic c2VjdXJpdHlfdGVzdF91c2VyOnNlY3VyaXR5LXRlc3QtcGFzc3dvcmQ=";

    public void testQuery() throws IOException {
        createApiKeys();
        createUser("someone");

        // Admin with manage_api_key can search for all keys
        assertQuery(API_KEY_ADMIN_AUTH_HEADER, """
            { "query": { "wildcard": {"name": "*alert*"} } }""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(2));
            assertThat(apiKeys.get(0).get("name"), oneOf("my-org/alert-key-1", "my-alert-key-2"));
            assertThat(apiKeys.get(1).get("name"), oneOf("my-org/alert-key-1", "my-alert-key-2"));
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

        // An empty request body means search for all keys
        assertQuery(
            API_KEY_ADMIN_AUTH_HEADER,
            randomBoolean() ? "" : """
                {"query":{"match_all":{}}}""",
            apiKeys -> assertThat(apiKeys.size(), equalTo(6))
        );

        assertQuery(
            API_KEY_ADMIN_AUTH_HEADER,
            """
                {"query":{"bool":{"must":[{"prefix":{"metadata.application":"fleet"}},{"term":{"metadata.environment.os":"Cat"}}]}}}""",
            apiKeys -> {
                assertThat(apiKeys, hasSize(2));
                assertThat(
                    apiKeys.stream().map(k -> k.get("name")).collect(Collectors.toList()),
                    containsInAnyOrder("my-org/ingest-key-1", "my-org/management-key-1")
                );
                apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
            }
        );

        assertQuery(API_KEY_ADMIN_AUTH_HEADER, """
            {"query":{"terms":{"metadata.tags":["prod","east"]}}}""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(5));
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

        assertQuery(API_KEY_ADMIN_AUTH_HEADER, """
            {"query":{"range":{"creation":{"lt":"now"}}}}""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(6));
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

        // Search for keys belong to an user
        assertQuery(API_KEY_ADMIN_AUTH_HEADER, """
            { "query": { "term": {"username": "api_key_user"} } }""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(2));
            assertThat(
                apiKeys.stream().map(m -> m.get("name")).collect(Collectors.toSet()),
                equalTo(Set.of("my-ingest-key-1", "my-alert-key-2"))
            );
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

        // Search for keys belong to users from a realm
        assertQuery(API_KEY_ADMIN_AUTH_HEADER, """
            { "query": { "term": {"realm_name": "default_file"} } }""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(6));
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
            // search using explicit IDs
            try {

                var subset = randomSubsetOf(randomIntBetween(1, 5), apiKeys);
                assertQuery(
                    API_KEY_ADMIN_AUTH_HEADER,
                        Strings.format("""
                                { "query": { "ids": { "values": [%s] } } }""",
                            subset.stream().map(m -> "\"" + m.get("id") + "\"").collect(Collectors.joining(","))),
                    keys -> {
                        assertThat(keys, hasSize(subset.size()));
                        keys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
                    }
                );
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
        });

        // Search for fields outside of the allowlist fails
        assertQueryError(API_KEY_ADMIN_AUTH_HEADER, 400, """
            { "query": { "prefix": {"api_key_hash": "{PBKDF2}10000$"} } }""");

        // Search for fields that are not allowed in Query DSL but used internally by the service itself
        final String fieldName = randomFrom("doc_type", "api_key_invalidated");
        assertQueryError(API_KEY_ADMIN_AUTH_HEADER, 400, Strings.format("""
                { "query": { "term": {"%s": "%s"} } }""", fieldName, randomAlphaOfLengthBetween(3, 8)));

        // Search for api keys won't return other entities
        assertQuery(API_KEY_ADMIN_AUTH_HEADER, """
            { "query": { "term": {"name": "someone"} } }""", apiKeys -> { assertThat(apiKeys, empty()); });

        // User with manage_own_api_key will only see its own keys
        assertQuery(API_KEY_USER_AUTH_HEADER, randomBoolean() ? "" : "{\"query\":{\"match_all\":{}}}", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(2));
            assertThat(
                apiKeys.stream().map(m -> m.get("name")).collect(Collectors.toSet()),
                containsInAnyOrder("my-ingest-key-1", "my-alert-key-2")
            );
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

        assertQuery(API_KEY_USER_AUTH_HEADER, """
            { "query": { "wildcard": {"name": "*alert*"} } }""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(1));
            assertThat(apiKeys.get(0).get("name"), equalTo("my-alert-key-2"));
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

        // User without manage_api_key or manage_own_api_key gets 403 trying to search API keys
        assertQueryError(TEST_USER_AUTH_HEADER, 403, """
            { "query": { "wildcard": {"name": "*alert*"} } }""");

        // Invalidated API keys are returned by default, but can be filtered out
        final String authHeader = randomFrom(API_KEY_ADMIN_AUTH_HEADER, API_KEY_USER_AUTH_HEADER);
        final String invalidatedApiKeyId1 = createAndInvalidateApiKey("temporary-key-1", authHeader);
        final String queryString = randomFrom("""
            {"query": { "term": {"name": "temporary-key-1"} } }""", Strings.format("""
                {"query":{"bool":{"must":[{"term":{"name":{"value":"temporary-key-1"}}},\
                {"term":{"invalidated":{"value":"%s"}}}]}}}
                """, randomBoolean()));

        assertQuery(authHeader, queryString, apiKeys -> {
            if (queryString.contains("""
                "invalidated":{"value":"false\"""")) {
                assertThat(apiKeys, empty());
            } else {
                assertThat(apiKeys.size(), equalTo(1));
                assertThat(apiKeys.get(0).get("name"), equalTo("temporary-key-1"));
                assertThat(apiKeys.get(0).get("id"), equalTo(invalidatedApiKeyId1));
                assertThat(apiKeys.get(0).get("invalidated"), is(true));
            }
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });
    }

    public void testQueryShouldRespectOwnerIdentityWithApiKeyAuth() throws IOException {
        final Tuple<String, String> powerKey = createApiKey("power-key-1", null, null, API_KEY_ADMIN_AUTH_HEADER);
        final String powerKeyAuthHeader = "ApiKey "
            + Base64.getEncoder().encodeToString((powerKey.v1() + ":" + powerKey.v2()).getBytes(StandardCharsets.UTF_8));

        final Tuple<String, String> limitKey = createApiKey(
            "limit-key-1",
            Map.of("a", Map.of("cluster", List.of("manage_own_api_key"))),
            null,
            API_KEY_ADMIN_AUTH_HEADER
        );
        final String limitKeyAuthHeader = "ApiKey "
            + Base64.getEncoder().encodeToString((limitKey.v1() + ":" + limitKey.v2()).getBytes(StandardCharsets.UTF_8));

        createApiKey("power-key-1-derived-1", Map.of("a", Map.of()), null, powerKeyAuthHeader);
        createApiKey("limit-key-1-derived-1", Map.of("a", Map.of()), null, limitKeyAuthHeader);

        createApiKey("user-key-1", Map.of(), API_KEY_USER_AUTH_HEADER);
        createApiKey("user-key-2", Map.of(), API_KEY_USER_AUTH_HEADER);

        // powerKey gets back all keys since it has manage_api_key privilege
        assertQuery(powerKeyAuthHeader, "", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(6));
            assertThat(
                apiKeys.stream().map(m -> (String) m.get("name")).collect(Collectors.toUnmodifiableSet()),
                equalTo(Set.of("power-key-1", "limit-key-1", "power-key-1-derived-1", "limit-key-1-derived-1", "user-key-1", "user-key-2"))
            );
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

        // limitKey gets only itself. It cannot view other keys owned by the owner user. This is consistent with how
        // get api key works
        assertQuery(limitKeyAuthHeader, "", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(1));
            assertThat(
                apiKeys.stream().map(m -> (String) m.get("name")).collect(Collectors.toUnmodifiableSet()),
                equalTo(Set.of("limit-key-1"))
            );
            apiKeys.forEach(k -> assertThat(k, not(hasKey("_sort"))));
        });

    }

    public void testPagination() throws IOException, InterruptedException {
        final String authHeader = randomFrom(API_KEY_ADMIN_AUTH_HEADER, API_KEY_USER_AUTH_HEADER);
        final int total = randomIntBetween(8, 12);
        final List<String> apiKeyNames = IntStream.range(0, total)
            .mapToObj(i -> Strings.format("k-%02d", i))
            .toList();
        final List<String> apiKeyIds = new ArrayList<>(total);
        for (int i = 0; i < total; i++) {
            apiKeyIds.add(createApiKey(apiKeyNames.get(i), null, authHeader).v1());
            Thread.sleep(10); // make sure keys are created with sufficient time interval to guarantee sorting order
        }

        final int from = randomIntBetween(0, 3);
        final int size = randomIntBetween(2, 5);
        final int remaining = total - from;
        final String sortField = randomFrom("name", "creation");

        final List<Map<String, Object>> apiKeyInfos = new ArrayList<>(remaining);
        final Request request1 = new Request("GET", "/_security/_query/api_key");
        request1.setOptions(request1.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        request1.setJsonEntity("{\"from\":" + from + ",\"size\":" + size + ",\"sort\":[\"" + sortField + "\"]}");
        int actualSize = collectApiKeys(apiKeyInfos, request1, total, size);
        assertThat(actualSize, equalTo(size));  // first batch should be a full page

        while (apiKeyInfos.size() < remaining) {
            final Request request2 = new Request("GET", "/_security/_query/api_key");
            request2.setOptions(request2.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
            final StringBuilder searchAfter = new StringBuilder();
            final List<Object> sortValues = extractSortValues(apiKeyInfos.get(apiKeyInfos.size() - 1));
            if ("name".equals(sortField)) {
                searchAfter.append("\"").append(sortValues.get(0)).append("\"");
            } else {
                searchAfter.append(sortValues.get(0));
            }
            request2.setJsonEntity(Strings.format("""
                    {"size":%s,"sort":["%s"],"search_after":[%s]}
                    """, size, sortField, searchAfter));
            actualSize = collectApiKeys(apiKeyInfos, request2, total, size);
            if (actualSize == 0 && apiKeyInfos.size() < remaining) {
                fail("fail to retrieve all API keys, expect [" + remaining + "] keys, got [" + apiKeyInfos + "]");
            }
            // Before all keys are retrieved, each page should be a full page
            if (apiKeyInfos.size() < remaining) {
                assertThat(actualSize, equalTo(size));
            }
        }

        // assert sort values match the field of API key information
        if ("name".equals(sortField)) {
            assertThat(
                apiKeyInfos.stream().map(m -> (String) m.get("name")).toList(),
                equalTo(apiKeyInfos.stream().map(m -> (String) extractSortValues(m).get(0)).toList())
            );
        } else {
            assertThat(
                apiKeyInfos.stream().map(m -> (long) m.get("creation")).toList(),
                equalTo(apiKeyInfos.stream().map(m -> (long) extractSortValues(m).get(0)).toList())
            );
        }
        assertThat(
            apiKeyInfos.stream().map(m -> (String) m.get("id")).toList(),
            equalTo(apiKeyIds.subList(from, total))
        );
        assertThat(
            apiKeyInfos.stream().map(m -> (String) m.get("name")).toList(),
            equalTo(apiKeyNames.subList(from, total))
        );

        // size can be zero, but total should still reflect the number of keys matched
        final Request request2 = new Request("GET", "/_security/_query/api_key");
        request2.setOptions(request2.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        request2.setJsonEntity("{\"size\":0}");
        final Response response2 = client().performRequest(request2);
        assertOK(response2);
        final Map<String, Object> responseMap2 = responseAsMap(response2);
        assertThat(responseMap2.get("total"), equalTo(total));
        assertThat(responseMap2.get("count"), equalTo(0));
    }

    @SuppressWarnings("unchecked")
    public void testSort() throws IOException {
        final String authHeader = randomFrom(API_KEY_ADMIN_AUTH_HEADER, API_KEY_USER_AUTH_HEADER);
        final List<String> apiKeyIds = new ArrayList<>(3);
        apiKeyIds.add(createApiKey("k2", Map.of("letter", "a", "symbol", "2"), authHeader).v1());
        apiKeyIds.add(createApiKey("k1", Map.of("letter", "b", "symbol", "2"), authHeader).v1());
        apiKeyIds.add(createApiKey("k0", Map.of("letter", "c", "symbol", "1"), authHeader).v1());

        assertQuery(authHeader, """
            {"sort":[{"creation":{"order":"desc"}}]}""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(3));
            for (int i = 2, j = 0; i >= 0; i--, j++) {
                assertThat(apiKeys.get(i).get("id"), equalTo(apiKeyIds.get(j)));
                assertThat(apiKeys.get(i).get("creation"), equalTo(((List<Integer>) apiKeys.get(i).get("_sort")).get(0)));
            }
        });

        assertQuery(authHeader, """
            {"sort":[{"name":{"order":"asc"}}]}""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(3));
            for (int i = 2, j = 0; i >= 0; i--, j++) {
                assertThat(apiKeys.get(i).get("id"), equalTo(apiKeyIds.get(j)));
                assertThat(apiKeys.get(i).get("name"), equalTo(((List<String>) apiKeys.get(i).get("_sort")).get(0)));
            }
        });

        assertQuery(authHeader, """
            {"sort":["metadata.letter"]}""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(3));
            for (int i = 0; i < 3; i++) {
                assertThat(apiKeys.get(i).get("id"), equalTo(apiKeyIds.get(i)));
            }
        });

        assertQuery(authHeader, """
            {"sort":["metadata.symbol","metadata.letter"]}""", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(3));
            assertThat(apiKeys.get(0).get("id"), equalTo(apiKeyIds.get(2)));
            assertThat(apiKeys.get(1).get("id"), equalTo(apiKeyIds.get(0)));
            assertThat(apiKeys.get(2).get("id"), equalTo(apiKeyIds.get(1)));
            apiKeys.forEach(k -> {
                final Map<String, Object> metadata = (Map<String, Object>) k.get("metadata");
                assertThat(metadata.get("symbol"), equalTo(((List<Integer>) k.get("_sort")).get(0)));
                assertThat(metadata.get("letter"), equalTo(((List<Integer>) k.get("_sort")).get(1)));
            });
        });

        assertQuery(authHeader, "{\"sort\":[\"_doc\"]}", apiKeys -> {
            assertThat(apiKeys.size(), equalTo(3));
            final List<String> ids = new ArrayList<>(3);
            for (int i = 0; i < 3; i++) {
                ids.add((String) apiKeys.get(i).get("id"));
                assertThat(apiKeys.get(i).get("_sort"), notNullValue());
            }
            // There is no guarantee that _doc order is the same as creation order
            assertThat(ids, containsInAnyOrder(apiKeyIds.toArray()));
        });

        final String invalidFieldName = randomFrom("doc_type", "api_key_invalidated", "metadata_flattened.letter");
        assertQueryError(authHeader, 400, "{\"sort\":[\"" + invalidFieldName + "\"]}");
    }

    public void testExistsQuery() throws IOException, InterruptedException {
        final String authHeader = randomFrom(API_KEY_ADMIN_AUTH_HEADER, API_KEY_USER_AUTH_HEADER);

        // No expiration
        createApiKey("test-exists-1", null, null, Map.of("value", 42), authHeader);
        // A short-lived key
        createApiKey("test-exists-2", "1ms", null, Map.of("label", "prod"), authHeader);
        createApiKey("test-exists-3", "1d", null, Map.of("value", 42, "label", "prod"), authHeader);

        final long startTime = Instant.now().toEpochMilli();

        assertQuery(
            authHeader,
            """
                {"query": {"exists": {"field": "expiration" }}}""",
            apiKeys -> {
                assertThat(
                    apiKeys.stream().map(k -> (String) k.get("name")).toList(),
                    containsInAnyOrder("test-exists-2", "test-exists-3")
                );
            }
        );

        assertQuery(
            authHeader,
            """
                {"query": {"exists": {"field": "metadata.value" }}}""",
            apiKeys -> {
                assertThat(
                    apiKeys.stream().map(k -> (String) k.get("name")).toList(),
                    containsInAnyOrder("test-exists-1", "test-exists-3")
                );
            }
        );

        assertQuery(
            authHeader,
            """
                {"query": {"exists": {"field": "metadata.label" }}}""",
            apiKeys -> {
                assertThat(
                    apiKeys.stream().map(k -> (String) k.get("name")).toList(),
                    containsInAnyOrder("test-exists-2", "test-exists-3")
                );
            }
        );

        // Create an invalidated API key
        createAndInvalidateApiKey("test-exists-4", authHeader);

        // Ensure the short-lived key is expired
        final long elapsed = Instant.now().toEpochMilli() - startTime;
        if (elapsed < 10) {
            Thread.sleep(10 - elapsed);
        }

        // Find valid API keys (not invalidated nor expired)
        assertQuery(
            authHeader,
            """
                {
                  "query": {
                    "bool": {
                      "must": {
                        "term": {
                          "invalidated": false
                        }
                      },
                      "should": [
                        {
                          "range": {
                            "expiration": {
                              "gte": "now"
                            }
                          }
                        },
                        {
                          "bool": {
                            "must_not": {
                              "exists": {
                                "field": "expiration"
                              }
                            }
                          }
                        }
                      ],
                      "minimum_should_match": 1
                    }
                  }
                }""",
            apiKeys -> {
                assertThat(
                    apiKeys.stream().map(k -> (String) k.get("name")).toList(),
                    containsInAnyOrder("test-exists-1", "test-exists-3")
                );
            }
        );
    }

    @SuppressWarnings("unchecked")
    private List<Object> extractSortValues(Map<String, Object> apiKeyInfo) {
        return (List<Object>) apiKeyInfo.get("_sort");
    }

    private int collectApiKeys(List<Map<String, Object>> apiKeyInfos, Request request, int total, int size) throws IOException {
        final Response response = client().performRequest(request);
        assertOK(response);
        final Map<String, Object> responseMap = responseAsMap(response);
        final int before = apiKeyInfos.size();
        @SuppressWarnings("unchecked")
        final List<Map<String, Object>> apiKeysMap = (List<Map<String, Object>>) responseMap.get("api_keys");
        apiKeyInfos.addAll(apiKeysMap);
        assertThat(responseMap.get("total"), equalTo(total));
        final int actualSize = apiKeyInfos.size() - before;
        assertThat(responseMap.get("count"), equalTo(actualSize));
        return actualSize;
    }

    private void assertQueryError(String authHeader, int statusCode, String body) throws IOException {
        final Request request = new Request("GET", "/_security/_query/api_key");
        request.setJsonEntity(body);
        request.setOptions(request.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        final ResponseException responseException = expectThrows(ResponseException.class, () -> client().performRequest(request));
        assertThat(responseException.getResponse().getStatusLine().getStatusCode(), equalTo(statusCode));
    }

    private void assertQuery(String authHeader, String body, Consumer<List<Map<String, Object>>> apiKeysVerifier) throws IOException {
        final Request request = new Request("GET", "/_security/_query/api_key");
        request.setJsonEntity(body);
        request.setOptions(request.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        final Response response = client().performRequest(request);
        assertOK(response);
        final Map<String, Object> responseMap = responseAsMap(response);
        @SuppressWarnings("unchecked")
        final List<Map<String, Object>> apiKeys = (List<Map<String, Object>>) responseMap.get("api_keys");
        apiKeysVerifier.accept(apiKeys);
    }

    private void createApiKeys() throws IOException {
        createApiKey(
            "my-org/ingest-key-1",
            Map.of(
                "application",
                "fleet-agent",
                "tags",
                List.of("prod", "east"),
                "environment",
                Map.of("os", "Cat", "level", 42, "system", false, "hostname", "my-org-host-1")
            ),
            API_KEY_ADMIN_AUTH_HEADER
        );

        createApiKey(
            "my-org/ingest-key-2",
            Map.of(
                "application",
                "fleet-server",
                "tags",
                List.of("staging", "east"),
                "environment",
                Map.of("os", "Dog", "level", 11, "system", true, "hostname", "my-org-host-2")
            ),
            API_KEY_ADMIN_AUTH_HEADER
        );

        createApiKey(
            "my-org/management-key-1",
            Map.of(
                "application",
                "fleet-agent",
                "tags",
                List.of("prod", "west"),
                "environment",
                Map.of("os", "Cat", "level", 11, "system", false, "hostname", "my-org-host-3")
            ),
            API_KEY_ADMIN_AUTH_HEADER
        );

        createApiKey(
            "my-org/alert-key-1",
            Map.of(
                "application",
                "siem",
                "tags",
                List.of("prod", "north", "upper"),
                "environment",
                Map.of("os", "Dog", "level", 3, "system", true, "hostname", "my-org-host-4")
            ),
            API_KEY_ADMIN_AUTH_HEADER
        );

        createApiKey(
            "my-ingest-key-1",
            Map.of("application", "cli", "tags", List.of("user", "test"), "notes", Map.of("sun", "hot", "earth", "blue")),
            API_KEY_USER_AUTH_HEADER
        );

        createApiKey(
            "my-alert-key-2",
            Map.of("application", "web", "tags", List.of("app", "prod"), "notes", Map.of("shared", false, "weather", "sunny")),
            API_KEY_USER_AUTH_HEADER
        );
    }

    private Tuple<String, String> createApiKey(String name, Map<String, Object> metadata, String authHeader) throws IOException {
        return createApiKey(name, null, metadata, authHeader);
    }

    private Tuple<String, String> createApiKey(
        String name,
        Map<String, Object> roleDescriptors,
        Map<String, Object> metadata,
        String authHeader
    ) throws IOException {
        return createApiKey(name, randomFrom("10d", null), roleDescriptors, metadata, authHeader);
    }

    private Tuple<String, String> createApiKey(
        String name,
        String expiration,
        Map<String, Object> roleDescriptors,
        Map<String, Object> metadata,
        String authHeader
    ) throws IOException {
        final Request request = new Request("POST", "/_security/api_key");
        final String roleDescriptorsString = XContentTestUtils.convertToXContent(
            roleDescriptors == null ? Map.of() : roleDescriptors,
            XContentType.JSON
        ).utf8ToString();
        final String metadataString = XContentTestUtils.convertToXContent(metadata == null ? Map.of() : metadata, XContentType.JSON)
            .utf8ToString();
        if (expiration == null) {
            request.setJsonEntity(Strings.format("""
                {"name":"%s", "role_descriptors":%s, "metadata":%s}""", name, roleDescriptorsString, metadataString));
        } else {
            request.setJsonEntity(Strings.format("""
                {"name":"%s", "expiration": "%s", "role_descriptors":%s,\
                "metadata":%s}""", name, expiration, roleDescriptorsString, metadataString));
        }
        request.setOptions(request.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        final Response response = client().performRequest(request);
        assertOK(response);
        final Map<String, Object> m = responseAsMap(response);
        return new Tuple<>((String) m.get("id"), (String) m.get("api_key"));
    }

    private String createAndInvalidateApiKey(String name, String authHeader) throws IOException {
        final Tuple<String, String> tuple = createApiKey(name, null, authHeader);
        final Request request = new Request("DELETE", "/_security/api_key");
        request.setOptions(request.getOptions().toBuilder().addHeader(HttpHeaders.AUTHORIZATION, authHeader));
        request.setJsonEntity(Strings.format("""
            {"ids": ["%s"],"owner":true}""", tuple.v1()));
        assertOK(client().performRequest(request));
        return tuple.v1();
    }

    private void createUser(String name) throws IOException {
        final Request request = new Request("POST", "/_security/user/" + name);
        request.setJsonEntity("""
            {"password":"super-strong-password","roles":[]}""");
        assertOK(adminClient().performRequest(request));
    }
}
