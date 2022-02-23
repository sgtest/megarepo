/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.profile;

import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.test.rest.ESRestTestCase;

import java.io.IOException;
import java.time.Instant;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.anEmptyMap;
import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.hasSize;

public class ProfileIT extends ESRestTestCase {

    public static final String SAMPLE_PROFILE_DOCUMENT_TEMPLATE = """
        {
          "user_profile": {
            "uid": "%s",
            "enabled": true,
            "user": {
              "username": "foo",
              "roles": [
                "role1",
                "role2"
              ],
              "realm": {
                "name": "realm_name_1",
                "type": "realm_type_1",
                "domain": {
                  "name": "domainA",
                  "realms": [
                    { "name": "realm_name_1", "type": "realm_type_1" },
                    { "name": "realm_name_2", "type": "realm_type_2" }
                  ]
                },
                "node_name": "node1"
              },
              "email": "foo@example.com",
              "full_name": "User Foo",
              "active": true
            },
            "last_synchronized": %s,
            "access": {
            },
            "application_data": {
              "app1": { "name": "app1" },
              "app2": { "name": "app2" }
            }
          }
        }
        """;

    @Override
    protected Settings restAdminSettings() {
        return Settings.builder()
            .put(
                ThreadContext.PREFIX + ".Authorization",
                basicAuthHeaderValue("test_admin", new SecureString("x-pack-test-password".toCharArray()))
            )
            .build();
    }

    public void testActivateProfile() throws IOException {
        final Map<String, Object> activateProfileMap = doActivateProfile();

        final String profileUid = (String) activateProfileMap.get("uid");
        final Map<String, Object> profile1 = doGetProfile(profileUid);
        assertThat(profile1, equalTo(activateProfileMap));
    }

    public void testGetProfile() throws IOException {
        final String uid = randomAlphaOfLength(20);
        final String source = SAMPLE_PROFILE_DOCUMENT_TEMPLATE.formatted(uid, Instant.now().toEpochMilli());
        final Request indexRequest = new Request("PUT", ".security-profile/_doc/profile_" + uid);
        indexRequest.setJsonEntity(source);
        indexRequest.addParameter("refresh", "wait_for");
        indexRequest.setOptions(
            expectWarnings(
                "this request accesses system indices: [.security-profile-8], but in a future major version, "
                    + "direct access to system indices will be prevented by default"
            )
        );
        assertOK(adminClient().performRequest(indexRequest));

        final Map<String, Object> profileMap1 = doGetProfile(uid);
        assertThat(castToMap(profileMap1.get("user")).get("realm_name"), equalTo("realm_name_1"));
        assertThat(castToMap(profileMap1.get("user")).get("realm_domain"), equalTo("domainA"));
        assertThat(castToMap(profileMap1.get("data")), anEmptyMap());

        // Retrieve application data along the profile
        final Map<String, Object> profileMap2 = doGetProfile(uid, "app1");
        assertThat(castToMap(profileMap2.get("data")), equalTo(Map.of("app1", Map.of("name", "app1"))));

        // Retrieve multiple application data
        final Map<String, Object> profileMap3 = doGetProfile(uid, randomFrom("app1,app2", "*", "app*"));
        assertThat(castToMap(profileMap3.get("data")), equalTo(Map.of("app1", Map.of("name", "app1"), "app2", Map.of("name", "app2"))));

        // Non-existing profile
        final Request getProfileRequest4 = new Request("GET", "_security/profile/not_" + uid);
        final ResponseException e4 = expectThrows(ResponseException.class, () -> adminClient().performRequest(getProfileRequest4));
        assertThat(e4.getResponse().getStatusLine().getStatusCode(), equalTo(404));
    }

    public void testUpdateProfileData() throws IOException {
        final Map<String, Object> activateProfileMap = doActivateProfile();
        final String uid = (String) activateProfileMap.get("uid");
        final Request updateProfileRequest1 = new Request("POST", "_security/profile/_data/" + uid);
        updateProfileRequest1.setJsonEntity("""
            {
              "access": {
                "app1": { "tags": [ "prod", "east" ] }
              },
              "data": {
                "app1": { "theme": "default" }
              }
            }""");
        assertOK(adminClient().performRequest(updateProfileRequest1));

        final Map<String, Object> profileMap1 = doGetProfile(uid, "app1");
        assertThat(castToMap(profileMap1.get("access")), equalTo(Map.of("app1", Map.of("tags", List.of("prod", "east")))));
        assertThat(castToMap(profileMap1.get("data")), equalTo(Map.of("app1", Map.of("theme", "default"))));
    }

    public void testSearchProfile() throws IOException {
        final Map<String, Object> activateProfileMap = doActivateProfile();
        final String uid = (String) activateProfileMap.get("uid");
        final Request searchProfilesRequest1 = new Request(randomFrom("GET", "POST"), "_security/profile/_search");
        searchProfilesRequest1.setJsonEntity("""
            {
              "name": "rac",
              "size": 10
            }""");
        final Response searchProfilesResponse1 = adminClient().performRequest(searchProfilesRequest1);
        assertOK(searchProfilesResponse1);
        final Map<String, Object> searchProfileResponseMap1 = responseAsMap(searchProfilesResponse1);
        assertThat(searchProfileResponseMap1, hasKey("took"));
        assertThat(searchProfileResponseMap1.get("total"), equalTo(Map.of("value", 1, "relation", "eq")));
        @SuppressWarnings("unchecked")
        final List<Map<String, Object>> users = (List<Map<String, Object>>) searchProfileResponseMap1.get("profiles");
        assertThat(users, hasSize(1));
        assertThat(users.get(0).get("uid"), equalTo(uid));
    }

    private Map<String, Object> doActivateProfile() throws IOException {
        final Request activateProfileRequest = new Request("POST", "_security/profile/_activate");
        activateProfileRequest.setJsonEntity("""
            {
              "grant_type": "password",
              "username": "rac_user",
              "password": "x-pack-test-password"
            }""");

        final Response activateProfileResponse = adminClient().performRequest(activateProfileRequest);
        assertOK(activateProfileResponse);
        return responseAsMap(activateProfileResponse);
    }

    private Map<String, Object> doGetProfile(String uid) throws IOException {
        return doGetProfile(uid, null);
    }

    private Map<String, Object> doGetProfile(String uid, @Nullable String dataKey) throws IOException {
        final Request getProfileRequest1 = new Request("GET", "_security/profile/" + uid);
        if (dataKey != null) {
            getProfileRequest1.addParameter("data", dataKey);
        }
        final Response getProfileResponse1 = adminClient().performRequest(getProfileRequest1);
        assertOK(getProfileResponse1);
        final Map<String, Object> getProfileMap1 = responseAsMap(getProfileResponse1);
        assertThat(getProfileMap1.keySet(), contains(uid));
        return castToMap(getProfileMap1.get(uid));
    }

    @SuppressWarnings("unchecked")
    private Map<String, Object> castToMap(Object o) {
        return (Map<String, Object>) o;
    }
}
