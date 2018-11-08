/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.client;

import org.apache.http.client.methods.HttpGet;
import org.apache.http.client.methods.HttpDelete;
import org.apache.http.client.methods.HttpPost;
import org.apache.http.client.methods.HttpPut;
import org.elasticsearch.client.security.CreateTokenRequest;
import org.elasticsearch.client.security.DeleteRoleMappingRequest;
import org.elasticsearch.client.security.DeleteRoleRequest;
import org.elasticsearch.client.security.DisableUserRequest;
import org.elasticsearch.client.security.EnableUserRequest;
import org.elasticsearch.client.security.GetRoleMappingsRequest;
import org.elasticsearch.client.security.ChangePasswordRequest;
import org.elasticsearch.client.security.PutRoleMappingRequest;
import org.elasticsearch.client.security.PutUserRequest;
import org.elasticsearch.client.security.RefreshPolicy;
import org.elasticsearch.client.security.support.expressiondsl.RoleMapperExpression;
import org.elasticsearch.client.security.support.expressiondsl.expressions.AnyRoleMapperExpression;
import org.elasticsearch.client.security.support.expressiondsl.fields.FieldRoleMapperExpression;
import org.elasticsearch.client.security.user.User;
import org.elasticsearch.common.Strings;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.client.RequestConvertersTests.assertToXContentBody;

public class SecurityRequestConvertersTests extends ESTestCase {

    public void testPutUser() throws IOException {
        final String username = randomAlphaOfLengthBetween(4, 12);
        final char[] password = randomBoolean() ? randomAlphaOfLengthBetween(8, 12).toCharArray() : null;
        final List<String> roles = Arrays.asList(generateRandomStringArray(randomIntBetween(2, 8), randomIntBetween(8, 16), false, true));
        final String email = randomBoolean() ? null : randomAlphaOfLengthBetween(12, 24);
        final String fullName = randomBoolean() ? null : randomAlphaOfLengthBetween(7, 14);
        final boolean enabled = randomBoolean();
        final Map<String, Object> metadata = new HashMap<>();
        if (randomBoolean()) {
            for (int i = 0; i < randomIntBetween(0, 10); i++) {
                metadata.put(String.valueOf(i), randomAlphaOfLengthBetween(1, 12));
            }
        }
        final User user = new User(username, roles, metadata, fullName, email);

        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams = getExpectedParamsFromRefreshPolicy(refreshPolicy);

        PutUserRequest putUserRequest = new PutUserRequest(user, password, enabled, refreshPolicy);
        Request request = SecurityRequestConverters.putUser(putUserRequest);
        assertEquals(HttpPut.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/user/" + putUserRequest.getUser().getUsername(), request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertToXContentBody(putUserRequest, request.getEntity());
    }

    public void testPutRoleMapping() throws IOException {
        final String username = randomAlphaOfLengthBetween(4, 7);
        final String rolename = randomAlphaOfLengthBetween(4, 7);
        final String roleMappingName = randomAlphaOfLengthBetween(4, 7);
        final String groupname = "cn="+randomAlphaOfLengthBetween(4, 7)+",dc=example,dc=com";
        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams;
        if (refreshPolicy != RefreshPolicy.NONE) {
            expectedParams = Collections.singletonMap("refresh", refreshPolicy.getValue());
        } else {
            expectedParams = Collections.emptyMap();
        }

        final RoleMapperExpression rules = AnyRoleMapperExpression.builder()
                .addExpression(FieldRoleMapperExpression.ofUsername(username))
                .addExpression(FieldRoleMapperExpression.ofGroups(groupname))
                .build();
        final PutRoleMappingRequest putRoleMappingRequest = new PutRoleMappingRequest(roleMappingName, true, Collections.singletonList(
                rolename), rules, null, refreshPolicy);

        final Request request = SecurityRequestConverters.putRoleMapping(putRoleMappingRequest);

        assertEquals(HttpPut.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/role_mapping/" + roleMappingName, request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertToXContentBody(putRoleMappingRequest, request.getEntity());
    }

    public void testGetRoleMappings() throws IOException {
        int noOfRoleMappingNames = randomIntBetween(0, 2);
        final String[] roleMappingNames =
                randomArray(noOfRoleMappingNames, noOfRoleMappingNames, String[]::new, () -> randomAlphaOfLength(5));
        final GetRoleMappingsRequest getRoleMappingsRequest = new GetRoleMappingsRequest(roleMappingNames);

        final Request request = SecurityRequestConverters.getRoleMappings(getRoleMappingsRequest);

        assertEquals(HttpGet.METHOD_NAME, request.getMethod());
        if (noOfRoleMappingNames == 0) {
            assertEquals("/_xpack/security/role_mapping", request.getEndpoint());
        } else {
            assertEquals("/_xpack/security/role_mapping/" +
                    Strings.collectionToCommaDelimitedString(getRoleMappingsRequest.getRoleMappingNames()), request.getEndpoint());
        }
        assertEquals(Collections.emptyMap(), request.getParameters());
        assertNull(request.getEntity());
    }

    public void testEnableUser() {
        final String username = randomAlphaOfLengthBetween(1, 12);
        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams = getExpectedParamsFromRefreshPolicy(refreshPolicy);
        EnableUserRequest enableUserRequest = new EnableUserRequest(username, refreshPolicy);
        Request request = SecurityRequestConverters.enableUser(enableUserRequest);
        assertEquals(HttpPut.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/user/" + username + "/_enable", request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertNull(request.getEntity());
    }

    public void testDisableUser() {
        final String username = randomAlphaOfLengthBetween(1, 12);
        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams = getExpectedParamsFromRefreshPolicy(refreshPolicy);
        DisableUserRequest disableUserRequest = new DisableUserRequest(username, refreshPolicy);
        Request request = SecurityRequestConverters.disableUser(disableUserRequest);
        assertEquals(HttpPut.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/user/" + username + "/_disable", request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertNull(request.getEntity());
    }

    private static Map<String, String> getExpectedParamsFromRefreshPolicy(RefreshPolicy refreshPolicy) {
        if (refreshPolicy != RefreshPolicy.NONE) {
            return Collections.singletonMap("refresh", refreshPolicy.getValue());
        } else {
            return Collections.emptyMap();
        }
    }

    public void testChangePassword() throws IOException {
        final String username = randomAlphaOfLengthBetween(4, 12);
        final char[] password = randomAlphaOfLengthBetween(8, 12).toCharArray();
        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams = getExpectedParamsFromRefreshPolicy(refreshPolicy);
        ChangePasswordRequest changePasswordRequest = new ChangePasswordRequest(username, password, refreshPolicy);
        Request request = SecurityRequestConverters.changePassword(changePasswordRequest);
        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/user/" + changePasswordRequest.getUsername() + "/_password", request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertToXContentBody(changePasswordRequest, request.getEntity());
    }

    public void testSelfChangePassword() throws IOException {
        final char[] password = randomAlphaOfLengthBetween(8, 12).toCharArray();
        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams = getExpectedParamsFromRefreshPolicy(refreshPolicy);
        ChangePasswordRequest changePasswordRequest = new ChangePasswordRequest(null, password, refreshPolicy);
        Request request = SecurityRequestConverters.changePassword(changePasswordRequest);
        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/user/_password", request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertToXContentBody(changePasswordRequest, request.getEntity());
    }

    public void testDeleteRoleMapping() throws IOException {
        final String roleMappingName = randomAlphaOfLengthBetween(4, 7);
        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams;
        if (refreshPolicy != RefreshPolicy.NONE) {
            expectedParams = Collections.singletonMap("refresh", refreshPolicy.getValue());
        } else {
            expectedParams = Collections.emptyMap();
        }
        final DeleteRoleMappingRequest deleteRoleMappingRequest = new DeleteRoleMappingRequest(roleMappingName, refreshPolicy);

        final Request request = SecurityRequestConverters.deleteRoleMapping(deleteRoleMappingRequest);

        assertEquals(HttpDelete.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/role_mapping/" + roleMappingName, request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertNull(request.getEntity());
    }

    public void testDeleteRole() {
        final String name = randomAlphaOfLengthBetween(1, 12);
        final RefreshPolicy refreshPolicy = randomFrom(RefreshPolicy.values());
        final Map<String, String> expectedParams = getExpectedParamsFromRefreshPolicy(refreshPolicy);
        DeleteRoleRequest deleteRoleRequest = new DeleteRoleRequest(name, refreshPolicy);
        Request request = SecurityRequestConverters.deleteRole(deleteRoleRequest);
        assertEquals(HttpDelete.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/role/" + name, request.getEndpoint());
        assertEquals(expectedParams, request.getParameters());
        assertNull(request.getEntity());
    }

    public void testCreateTokenWithPasswordGrant() throws Exception {
        final String username = randomAlphaOfLengthBetween(1, 12);
        final String password = randomAlphaOfLengthBetween(8, 12);
        CreateTokenRequest createTokenRequest = CreateTokenRequest.passwordGrant(username, password.toCharArray());
        Request request = SecurityRequestConverters.createToken(createTokenRequest);
        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/oauth2/token", request.getEndpoint());
        assertEquals(0, request.getParameters().size());
        assertToXContentBody(createTokenRequest, request.getEntity());
    }

    public void testCreateTokenWithRefreshTokenGrant() throws Exception {
        final String refreshToken = randomAlphaOfLengthBetween(8, 24);
        CreateTokenRequest createTokenRequest = CreateTokenRequest.refreshTokenGrant(refreshToken);
        Request request = SecurityRequestConverters.createToken(createTokenRequest);
        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/oauth2/token", request.getEndpoint());
        assertEquals(0, request.getParameters().size());
        assertToXContentBody(createTokenRequest, request.getEntity());
    }

    public void testCreateTokenWithClientCredentialsGrant() throws Exception {
        CreateTokenRequest createTokenRequest = CreateTokenRequest.clientCredentialsGrant();
        Request request = SecurityRequestConverters.createToken(createTokenRequest);
        assertEquals(HttpPost.METHOD_NAME, request.getMethod());
        assertEquals("/_xpack/security/oauth2/token", request.getEndpoint());
        assertEquals(0, request.getParameters().size());
        assertToXContentBody(createTokenRequest, request.getEntity());
    }
}
