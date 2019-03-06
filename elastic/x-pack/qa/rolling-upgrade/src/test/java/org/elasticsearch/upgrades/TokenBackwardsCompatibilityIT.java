/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.upgrades;

import org.apache.http.HttpHeaders;
import org.apache.http.HttpHost;
import org.elasticsearch.Version;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.test.rest.yaml.ObjectPath;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

public class TokenBackwardsCompatibilityIT extends AbstractUpgradeTestCase {

    public void testGeneratingTokenInOldCluster() throws Exception {
        assumeTrue("this test should only run against the old cluster", CLUSTER_TYPE == ClusterType.OLD);
        // Create a couple of tokens and store them in the token_backwards_compatibility_it index to be used for tests in the mixed/upgraded
        // clusters
        Request createTokenRequest = new Request("POST", "/_security/oauth2/token");
        createTokenRequest.setJsonEntity(
                "{\n" +
                "    \"username\": \"test_user\",\n" +
                "    \"password\": \"x-pack-test-password\",\n" +
                "    \"grant_type\": \"password\"\n" +
                "}");
        Response response = client().performRequest(createTokenRequest);
        assertOK(response);
        Map<String, Object> responseMap = entityAsMap(response);
        String token = (String) responseMap.get("access_token");
        assertNotNull(token);
        assertTokenWorks(token);

        Request indexRequest1 = new Request("PUT", "token_backwards_compatibility_it/_doc/old_cluster_token1");
        indexRequest1.setJsonEntity(
                "{\n" +
                "    \"token\": \"" + token + "\"\n" +
                "}");
        Response indexResponse1 = client().performRequest(indexRequest1);
        assertOK(indexResponse1);
        Request createSecondTokenRequest = new Request("POST", "/_security/oauth2/token");
        createSecondTokenRequest.setEntity(createTokenRequest.getEntity());
        response = client().performRequest(createSecondTokenRequest);
        responseMap = entityAsMap(response);
        token = (String) responseMap.get("access_token");
        assertNotNull(token);
        assertTokenWorks(token);
        Request indexRequest2 = new Request("PUT", "token_backwards_compatibility_it/_doc/old_cluster_token2");
        indexRequest2.setJsonEntity(
                "{\n" +
                "    \"token\": \"" + token + "\"\n" +
                "}");
        Response indexResponse2 = client().performRequest(indexRequest2);
        assertOK(indexResponse2);
    }

    public void testTokenWorksInMixedCluster() throws Exception {
        // Verify that an old token continues to work during all stages of the rolling upgrade
        assumeTrue("this test should only run against the mixed cluster", CLUSTER_TYPE == ClusterType.MIXED);
        Request getRequest = new Request("GET", "token_backwards_compatibility_it/_doc/old_cluster_token1");
        Response getResponse = client().performRequest(getRequest);
        assertOK(getResponse);
        Map<String, Object> source = (Map<String, Object>) entityAsMap(getResponse).get("_source");
        assertTokenWorks((String) source.get("token"));
    }

    public void testInvalidatingTokenInMixedCluster() throws Exception {
        // Verify that we can invalidate a token in a mixed cluster
        assumeTrue("this test should only run against the mixed cluster", CLUSTER_TYPE == ClusterType.MIXED);
        Request getRequest = new Request("GET", "token_backwards_compatibility_it/_doc/old_cluster_token2");
        Response getResponse = client().performRequest(getRequest);
        assertOK(getResponse);
        Map<String, Object> source = (Map<String, Object>) entityAsMap(getResponse).get("_source");
        String token = (String) source.get("token");
        // The token might be already invalidated by running testInvalidatingTokenInMixedCluster in a previous stage
        // we don't try to assert it works before invalidating. This case is handled by testTokenWorksInMixedCluster
        Request invalidateRequest = new Request("DELETE", "/_security/oauth2/token");
        invalidateRequest.setJsonEntity("{\"token\": \"" + token + "\"}");
        invalidateRequest.addParameter("error_trace", "true");
        client().performRequest(invalidateRequest);
        assertTokenDoesNotWork(token);
    }

    public void testMixedClusterWithUpgradedMaster() throws Exception {
        assumeTrue("this test should only run against the mixed cluster", CLUSTER_TYPE == ClusterType.MIXED);
        assumeTrue("the master must be on the latest version before we can write", isMasterOnLatestVersion());

        // create token and refresh on version that supports it
        Request createTokenRequest = new Request("POST", "/_security/oauth2/token");
        createTokenRequest.setJsonEntity(
                "{\n" +
                "    \"username\": \"test_user\",\n" +
                "    \"password\": \"x-pack-test-password\",\n" +
                "    \"grant_type\": \"password\"\n" +
                "}");
        try (RestClient client = getRestClientForCurrentVersionNodesOnly()) {
            Response response = client.performRequest(createTokenRequest);
            Map<String, Object> responseMap = entityAsMap(response);
            String accessToken = (String) responseMap.get("access_token");
            String refreshToken = (String) responseMap.get("refresh_token");
            assertNotNull(accessToken);
            assertNotNull(refreshToken);
            assertTokenWorks(accessToken);

            Request tokenRefreshRequest = new Request("POST", "/_security/oauth2/token");
            tokenRefreshRequest.setJsonEntity(
                    "{\n" +
                    "    \"refresh_token\": \"" + refreshToken + "\",\n" +
                    "    \"grant_type\": \"refresh_token\"\n" +
                    "}");
            response = client.performRequest(tokenRefreshRequest);
            responseMap = entityAsMap(response);
            String updatedAccessToken = (String) responseMap.get("access_token");
            String updatedRefreshToken = (String) responseMap.get("refresh_token");
            assertNotNull(updatedAccessToken);
            assertNotNull(updatedRefreshToken);
            assertTokenWorks(updatedAccessToken);
            assertTokenWorks(accessToken);
            assertNotEquals(accessToken, updatedAccessToken);
            assertNotEquals(refreshToken, updatedRefreshToken);
            // Invalidate the new access token and ensure that it no longer works
            Request invalidateTokenRequest = new Request("DELETE", "/_security/oauth2/token");
            invalidateTokenRequest.setJsonEntity(
                "{\n" +
                    "    \"token\": \"" + updatedAccessToken + "\"\n" +
                    "}");
            Response invalidateTokenResponse = client.performRequest(invalidateTokenRequest);
            assertOK(invalidateTokenResponse);
            assertTokenDoesNotWork(updatedAccessToken);
        }
    }

    public void testUpgradedCluster() throws Exception {
        assumeTrue("this test should only run against the upgraded cluster", CLUSTER_TYPE == ClusterType.UPGRADED);

        // Use an old token to authenticate, then invalidate it and verify that it can no longer be used
        Request getRequest = new Request("GET", "token_backwards_compatibility_it/_doc/old_cluster_token1");
        Response getResponse = client().performRequest(getRequest);
        assertOK(getResponse);
        Map<String, Object> source = (Map<String, Object>) entityAsMap(getResponse).get("_source");
        final String token = (String) source.get("token");

        Request invalidateRequest = new Request("DELETE", "/_security/oauth2/token");
        invalidateRequest.setJsonEntity("{\"token\": \"" + token + "\"}");
        invalidateRequest.addParameter("error_trace", "true");
        Response invalidationResponse = client().performRequest(invalidateRequest);
        assertOK(invalidationResponse);
        assertTokenDoesNotWork(token);
    }

    private void assertTokenWorks(String token) throws IOException {
        Request request = new Request("GET", "/_security/_authenticate");
        RequestOptions.Builder options = request.getOptions().toBuilder();
        options.addHeader(HttpHeaders.AUTHORIZATION, "Bearer " + token);
        request.setOptions(options);
        Response authenticateResponse = client().performRequest(request);
        assertOK(authenticateResponse);
        assertEquals("test_user", entityAsMap(authenticateResponse).get("username"));
    }

    private void assertTokenDoesNotWork(String token) {
        Request request = new Request("GET", "/_security/_authenticate");
        RequestOptions.Builder options = request.getOptions().toBuilder();
        options.addHeader(HttpHeaders.AUTHORIZATION, "Bearer " + token);
        request.setOptions(options);
        ResponseException e = expectThrows(ResponseException.class, () -> client().performRequest(request));
        assertEquals(401, e.getResponse().getStatusLine().getStatusCode());
        Response response = e.getResponse();
        assertEquals("Bearer realm=\"security\", error=\"invalid_token\", error_description=\"The access token expired\"",
                response.getHeader("WWW-Authenticate"));
    }

    private boolean isMasterOnLatestVersion() throws Exception {
        Response response = client().performRequest(new Request("GET", "_cluster/state"));
        assertOK(response);
        final String masterNodeId = ObjectPath.createFromResponse(response).evaluate("master_node");
        response = client().performRequest(new Request("GET", "_nodes"));
        assertOK(response);
        ObjectPath objectPath = ObjectPath.createFromResponse(response);
        logger.info("Master node is on version: " + objectPath.evaluate("nodes." + masterNodeId + ".version"));
        return Version.CURRENT.equals(Version.fromString(objectPath.evaluate("nodes." + masterNodeId + ".version")));
    }

    private RestClient getRestClientForCurrentVersionNodesOnly() throws IOException {
        Response response = client().performRequest(new Request("GET", "_nodes"));
        assertOK(response);
        ObjectPath objectPath = ObjectPath.createFromResponse(response);
        Map<String, Object> nodesAsMap = objectPath.evaluate("nodes");
        List<HttpHost> hosts = new ArrayList<>();
        for (Map.Entry<String, Object> entry : nodesAsMap.entrySet()) {
            Map<String, Object> nodeDetails = (Map<String, Object>) entry.getValue();
            Version version = Version.fromString((String) nodeDetails.get("version"));
            if (Version.CURRENT.equals(version)) {
                Map<String, Object> httpInfo = (Map<String, Object>) nodeDetails.get("http");
                hosts.add(HttpHost.create((String) httpInfo.get("publish_address")));
            }
        }

        return buildClient(restClientSettings(), hosts.toArray(new HttpHost[0]));
    }
}
