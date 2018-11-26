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

package org.elasticsearch.client.documentation;

import org.apache.http.client.methods.HttpPost;
import org.apache.http.entity.ContentType;
import org.apache.http.nio.entity.NStringEntity;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.LatchedActionListener;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.client.security.AuthenticateResponse;
import org.elasticsearch.client.security.ChangePasswordRequest;
import org.elasticsearch.client.security.ClearRealmCacheRequest;
import org.elasticsearch.client.security.ClearRealmCacheResponse;
import org.elasticsearch.client.security.ClearRolesCacheRequest;
import org.elasticsearch.client.security.ClearRolesCacheResponse;
import org.elasticsearch.client.security.CreateTokenRequest;
import org.elasticsearch.client.security.CreateTokenResponse;
import org.elasticsearch.client.security.DeletePrivilegesRequest;
import org.elasticsearch.client.security.DeletePrivilegesResponse;
import org.elasticsearch.client.security.DeleteRoleMappingRequest;
import org.elasticsearch.client.security.DeleteRoleMappingResponse;
import org.elasticsearch.client.security.DeleteRoleRequest;
import org.elasticsearch.client.security.DeleteRoleResponse;
import org.elasticsearch.client.security.DisableUserRequest;
import org.elasticsearch.client.security.EmptyResponse;
import org.elasticsearch.client.security.EnableUserRequest;
import org.elasticsearch.client.security.ExpressionRoleMapping;
import org.elasticsearch.client.security.GetPrivilegesRequest;
import org.elasticsearch.client.security.GetPrivilegesResponse;
import org.elasticsearch.client.security.GetRoleMappingsRequest;
import org.elasticsearch.client.security.GetRoleMappingsResponse;
import org.elasticsearch.client.security.GetRolesRequest;
import org.elasticsearch.client.security.GetRolesResponse;
import org.elasticsearch.client.security.GetSslCertificatesResponse;
import org.elasticsearch.client.security.HasPrivilegesRequest;
import org.elasticsearch.client.security.HasPrivilegesResponse;
import org.elasticsearch.client.security.InvalidateTokenRequest;
import org.elasticsearch.client.security.InvalidateTokenResponse;
import org.elasticsearch.client.security.PutRoleMappingRequest;
import org.elasticsearch.client.security.PutRoleMappingResponse;
import org.elasticsearch.client.security.PutUserRequest;
import org.elasticsearch.client.security.PutUserResponse;
import org.elasticsearch.client.security.RefreshPolicy;
import org.elasticsearch.client.security.support.CertificateInfo;
import org.elasticsearch.client.security.support.expressiondsl.RoleMapperExpression;
import org.elasticsearch.client.security.support.expressiondsl.expressions.AnyRoleMapperExpression;
import org.elasticsearch.client.security.support.expressiondsl.fields.FieldRoleMapperExpression;
import org.elasticsearch.client.security.user.User;
import org.elasticsearch.client.security.user.privileges.Role;
import org.elasticsearch.client.security.user.privileges.ApplicationPrivilege;
import org.elasticsearch.client.security.user.privileges.IndicesPrivileges;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.rest.RestStatus;
import org.hamcrest.Matchers;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.emptyIterable;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.isIn;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.nullValue;

public class SecurityDocumentationIT extends ESRestHighLevelClientTestCase {

    public void testPutUser() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            //tag::put-user-execute
            char[] password = new char[]{'p', 'a', 's', 's', 'w', 'o', 'r', 'd'};
            User user = new User("example", Collections.singletonList("superuser"));
            PutUserRequest request = new PutUserRequest(user, password, true, RefreshPolicy.NONE);
            PutUserResponse response = client.security().putUser(request, RequestOptions.DEFAULT);
            //end::put-user-execute

            //tag::put-user-response
            boolean isCreated = response.isCreated(); // <1>
            //end::put-user-response

            assertTrue(isCreated);
        }

        {
            char[] password = new char[]{'p', 'a', 's', 's', 'w', 'o', 'r', 'd'};
            User user2 = new User("example2", Collections.singletonList("superuser"));
            PutUserRequest request = new PutUserRequest(user2, password, true, RefreshPolicy.NONE);
            // tag::put-user-execute-listener
            ActionListener<PutUserResponse> listener = new ActionListener<PutUserResponse>() {
                @Override
                public void onResponse(PutUserResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::put-user-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::put-user-execute-async
            client.security().putUserAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::put-user-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testPutRoleMapping() throws Exception {
        final RestHighLevelClient client = highLevelClient();

        {
            // tag::put-role-mapping-execute
            final RoleMapperExpression rules = AnyRoleMapperExpression.builder()
                .addExpression(FieldRoleMapperExpression.ofUsername("*"))
                .addExpression(FieldRoleMapperExpression.ofGroups("cn=admins,dc=example,dc=com"))
                .build();
            final PutRoleMappingRequest request = new PutRoleMappingRequest("mapping-example", true, Collections.singletonList("superuser"),
                rules, null, RefreshPolicy.NONE);
            final PutRoleMappingResponse response = client.security().putRoleMapping(request, RequestOptions.DEFAULT);
            // end::put-role-mapping-execute
            // tag::put-role-mapping-response
            boolean isCreated = response.isCreated(); // <1>
            // end::put-role-mapping-response
            assertTrue(isCreated);
        }

        {
            final RoleMapperExpression rules = AnyRoleMapperExpression.builder()
                .addExpression(FieldRoleMapperExpression.ofUsername("*"))
                .addExpression(FieldRoleMapperExpression.ofGroups("cn=admins,dc=example,dc=com"))
                .build();
            final PutRoleMappingRequest request = new PutRoleMappingRequest("mapping-example", true, Collections.singletonList("superuser"),
                rules, null, RefreshPolicy.NONE);
            // tag::put-role-mapping-execute-listener
            ActionListener<PutRoleMappingResponse> listener = new ActionListener<PutRoleMappingResponse>() {
                @Override
                public void onResponse(PutRoleMappingResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::put-role-mapping-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::put-role-mapping-execute-async
            client.security().putRoleMappingAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::put-role-mapping-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetRoleMappings() throws Exception {
        final RestHighLevelClient client = highLevelClient();

        final RoleMapperExpression rules1 = AnyRoleMapperExpression.builder().addExpression(FieldRoleMapperExpression.ofUsername("*"))
            .addExpression(FieldRoleMapperExpression.ofGroups("cn=admins,dc=example,dc=com")).build();
        final PutRoleMappingRequest putRoleMappingRequest1 = new PutRoleMappingRequest("mapping-example-1", true, Collections.singletonList(
            "superuser"), rules1, null, RefreshPolicy.NONE);
        final PutRoleMappingResponse putRoleMappingResponse1 = client.security().putRoleMapping(putRoleMappingRequest1,
            RequestOptions.DEFAULT);
        boolean isCreated1 = putRoleMappingResponse1.isCreated();
        assertTrue(isCreated1);
        final RoleMapperExpression rules2 = AnyRoleMapperExpression.builder().addExpression(FieldRoleMapperExpression.ofGroups(
            "cn=admins,dc=example,dc=com")).build();
        final Map<String, Object> metadata2 = new HashMap<>();
        metadata2.put("k1", "v1");
        final PutRoleMappingRequest putRoleMappingRequest2 = new PutRoleMappingRequest("mapping-example-2", true, Collections.singletonList(
            "monitoring"), rules2, metadata2, RefreshPolicy.NONE);
        final PutRoleMappingResponse putRoleMappingResponse2 = client.security().putRoleMapping(putRoleMappingRequest2,
            RequestOptions.DEFAULT);
        boolean isCreated2 = putRoleMappingResponse2.isCreated();
        assertTrue(isCreated2);

        {
            // tag::get-role-mappings-execute
            final GetRoleMappingsRequest request = new GetRoleMappingsRequest("mapping-example-1");
            final GetRoleMappingsResponse response = client.security().getRoleMappings(request, RequestOptions.DEFAULT);
            // end::get-role-mappings-execute
            // tag::get-role-mappings-response
            List<ExpressionRoleMapping> mappings = response.getMappings();
            // end::get-role-mappings-response
            assertNotNull(mappings);
            assertThat(mappings.size(), is(1));
            assertThat(mappings.get(0).isEnabled(), is(true));
            assertThat(mappings.get(0).getName(), is("mapping-example-1"));
            assertThat(mappings.get(0).getExpression(), equalTo(rules1));
            assertThat(mappings.get(0).getMetadata(), equalTo(Collections.emptyMap()));
            assertThat(mappings.get(0).getRoles(), contains("superuser"));
        }

        {
            // tag::get-role-mappings-list-execute
            final GetRoleMappingsRequest request = new GetRoleMappingsRequest("mapping-example-1", "mapping-example-2");
            final GetRoleMappingsResponse response = client.security().getRoleMappings(request, RequestOptions.DEFAULT);
            // end::get-role-mappings-list-execute
            List<ExpressionRoleMapping> mappings = response.getMappings();
            assertNotNull(mappings);
            assertThat(mappings.size(), is(2));
            for (ExpressionRoleMapping roleMapping : mappings) {
                assertThat(roleMapping.isEnabled(), is(true));
                assertThat(roleMapping.getName(), isIn(new String[]{"mapping-example-1", "mapping-example-2"}));
                if (roleMapping.getName().equals("mapping-example-1")) {
                    assertThat(roleMapping.getMetadata(), equalTo(Collections.emptyMap()));
                    assertThat(roleMapping.getExpression(), equalTo(rules1));
                    assertThat(roleMapping.getRoles(), contains("superuser"));
                } else {
                    assertThat(roleMapping.getMetadata(), equalTo(metadata2));
                    assertThat(roleMapping.getExpression(), equalTo(rules2));
                    assertThat(roleMapping.getRoles(), contains("monitoring"));
                }
            }
        }

        {
            // tag::get-role-mappings-all-execute
            final GetRoleMappingsRequest request = new GetRoleMappingsRequest();
            final GetRoleMappingsResponse response = client.security().getRoleMappings(request, RequestOptions.DEFAULT);
            // end::get-role-mappings-all-execute
            List<ExpressionRoleMapping> mappings = response.getMappings();
            assertNotNull(mappings);
            assertThat(mappings.size(), is(2));
            for (ExpressionRoleMapping roleMapping : mappings) {
                assertThat(roleMapping.isEnabled(), is(true));
                assertThat(roleMapping.getName(), isIn(new String[]{"mapping-example-1", "mapping-example-2"}));
                if (roleMapping.getName().equals("mapping-example-1")) {
                    assertThat(roleMapping.getMetadata(), equalTo(Collections.emptyMap()));
                    assertThat(roleMapping.getExpression(), equalTo(rules1));
                    assertThat(roleMapping.getRoles(), contains("superuser"));
                } else {
                    assertThat(roleMapping.getMetadata(), equalTo(metadata2));
                    assertThat(roleMapping.getExpression(), equalTo(rules2));
                    assertThat(roleMapping.getRoles(), contains("monitoring"));
                }
            }
        }

        {
            final GetRoleMappingsRequest request = new GetRoleMappingsRequest();
            // tag::get-role-mappings-execute-listener
            ActionListener<GetRoleMappingsResponse> listener = new ActionListener<GetRoleMappingsResponse>() {
                @Override
                public void onResponse(GetRoleMappingsResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::get-role-mappings-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-role-mappings-execute-async
            client.security().getRoleMappingsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-role-mappings-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testEnableUser() throws Exception {
        RestHighLevelClient client = highLevelClient();
        char[] password = new char[]{'p', 'a', 's', 's', 'w', 'o', 'r', 'd'};
        User enable_user = new User("enable_user", Collections.singletonList("superuser"));
        PutUserRequest putUserRequest = new PutUserRequest(enable_user, password, true, RefreshPolicy.IMMEDIATE);
        PutUserResponse putUserResponse = client.security().putUser(putUserRequest, RequestOptions.DEFAULT);
        assertTrue(putUserResponse.isCreated());

        {
            //tag::enable-user-execute
            EnableUserRequest request = new EnableUserRequest("enable_user", RefreshPolicy.NONE);
            EmptyResponse response = client.security().enableUser(request, RequestOptions.DEFAULT);
            //end::enable-user-execute

            assertNotNull(response);
        }

        {
            //tag::enable-user-execute-listener
            EnableUserRequest request = new EnableUserRequest("enable_user", RefreshPolicy.NONE);
            ActionListener<EmptyResponse> listener = new ActionListener<EmptyResponse>() {
                @Override
                public void onResponse(EmptyResponse setUserEnabledResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::enable-user-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::enable-user-execute-async
            client.security().enableUserAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::enable-user-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testDisableUser() throws Exception {
        RestHighLevelClient client = highLevelClient();
        char[] password = new char[]{'p', 'a', 's', 's', 'w', 'o', 'r', 'd'};
        User disable_user = new User("disable_user", Collections.singletonList("superuser"));
        PutUserRequest putUserRequest = new PutUserRequest(disable_user, password, true, RefreshPolicy.IMMEDIATE);
        PutUserResponse putUserResponse = client.security().putUser(putUserRequest, RequestOptions.DEFAULT);
        assertTrue(putUserResponse.isCreated());
        {
            //tag::disable-user-execute
            DisableUserRequest request = new DisableUserRequest("disable_user", RefreshPolicy.NONE);
            EmptyResponse response = client.security().disableUser(request, RequestOptions.DEFAULT);
            //end::disable-user-execute

            assertNotNull(response);
        }

        {
            //tag::disable-user-execute-listener
            DisableUserRequest request = new DisableUserRequest("disable_user", RefreshPolicy.NONE);
            ActionListener<EmptyResponse> listener = new ActionListener<EmptyResponse>() {
                @Override
                public void onResponse(EmptyResponse setUserEnabledResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::disable-user-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::disable-user-execute-async
            client.security().disableUserAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::disable-user-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetRoles() throws Exception {
        final RestHighLevelClient client = highLevelClient();
        addRole("my_role");
        addRole("my_role2");
        addRole("my_role3");
        {
            //tag::get-roles-request
            GetRolesRequest request = new GetRolesRequest("my_role");
            //end::get-roles-request
            //tag::get-roles-execute
            GetRolesResponse response = client.security().getRoles(request, RequestOptions.DEFAULT);
            //end::get-roles-execute
            //tag::get-roles-response
            List<Role> roles = response.getRoles();
            //end::get-roles-response

            assertNotNull(response);
            assertThat(roles.size(), equalTo(1));
            assertThat(roles.get(0).getName(), equalTo("my_role"));
            assertThat(roles.get(0).getClusterPrivileges().contains("all"), equalTo(true));
        }

        {
            //tag::get-roles-list-request
            GetRolesRequest request = new GetRolesRequest("my_role", "my_role2");
            GetRolesResponse response = client.security().getRoles(request, RequestOptions.DEFAULT);
            //end::get-roles-list-request

            List<Role> roles = response.getRoles();
            assertNotNull(response);
            assertThat(roles.size(), equalTo(2));
            assertThat(roles.get(0).getClusterPrivileges().contains("all"), equalTo(true));
            assertThat(roles.get(1).getClusterPrivileges().contains("all"), equalTo(true));
        }

        {
            //tag::get-roles-all-request
            GetRolesRequest request = new GetRolesRequest();
            GetRolesResponse response = client.security().getRoles(request, RequestOptions.DEFAULT);
            //end::get-roles-all-request

            List<Role> roles = response.getRoles();
            assertNotNull(response);
            // 21 system roles plus the three we created
            assertThat(roles.size(), equalTo(24));
        }

        {
            GetRolesRequest request = new GetRolesRequest("my_role");
            ActionListener<GetRolesResponse> listener;

            //tag::get-roles-execute-listener
            listener = new ActionListener<GetRolesResponse>() {
                @Override
                public void onResponse(GetRolesResponse getRolesResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::get-roles-execute-listener

            assertNotNull(listener);

            // Replace the empty listener by a blocking listener in test
            final PlainActionFuture<GetRolesResponse> future = new PlainActionFuture<>();
            listener = future;

            //tag::get-roles-execute-async
            client.security().getRolesAsync(request, RequestOptions.DEFAULT, listener); // <1>
            //end::get-roles-execute-async

            final GetRolesResponse response = future.get(30, TimeUnit.SECONDS);
            assertNotNull(response);
            assertThat(response.getRoles().size(), equalTo(1));
            assertThat(response.getRoles().get(0).getName(), equalTo("my_role"));
            assertThat(response.getRoles().get(0).getClusterPrivileges().contains("all"), equalTo(true));
        }
    }

    public void testAuthenticate() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            //tag::authenticate-execute
            AuthenticateResponse response = client.security().authenticate(RequestOptions.DEFAULT);
            //end::authenticate-execute

            //tag::authenticate-response
            User user = response.getUser(); // <1>
            boolean enabled = response.enabled(); // <2>
            //end::authenticate-response

            assertThat(user.getUsername(), is("test_user"));
            assertThat(user.getRoles(), contains(new String[]{"superuser"}));
            assertThat(user.getFullName(), nullValue());
            assertThat(user.getEmail(), nullValue());
            assertThat(user.getMetadata().isEmpty(), is(true));
            assertThat(enabled, is(true));
        }

        {
            // tag::authenticate-execute-listener
            ActionListener<AuthenticateResponse> listener = new ActionListener<AuthenticateResponse>() {
                @Override
                public void onResponse(AuthenticateResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::authenticate-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::authenticate-execute-async
            client.security().authenticateAsync(RequestOptions.DEFAULT, listener); // <1>
            // end::authenticate-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testHasPrivileges() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            //tag::has-privileges-request
            HasPrivilegesRequest request = new HasPrivilegesRequest(
                Sets.newHashSet("monitor", "manage"),
                Sets.newHashSet(
                    IndicesPrivileges.builder().indices("logstash-2018-10-05").privileges("read", "write").build(),
                    IndicesPrivileges.builder().indices("logstash-2018-*").privileges("read").build()
                ),
                null
            );
            //end::has-privileges-request

            //tag::has-privileges-execute
            HasPrivilegesResponse response = client.security().hasPrivileges(request, RequestOptions.DEFAULT);
            //end::has-privileges-execute

            //tag::has-privileges-response
            boolean hasMonitor = response.hasClusterPrivilege("monitor"); // <1>
            boolean hasWrite = response.hasIndexPrivilege("logstash-2018-10-05", "write"); // <2>
            boolean hasRead = response.hasIndexPrivilege("logstash-2018-*", "read"); // <3>
            //end::has-privileges-response

            assertThat(response.getUsername(), is("test_user"));
            assertThat(response.hasAllRequested(), is(true));
            assertThat(hasMonitor, is(true));
            assertThat(hasWrite, is(true));
            assertThat(hasRead, is(true));
            assertThat(response.getApplicationPrivileges().entrySet(), emptyIterable());
        }

        {
            HasPrivilegesRequest request = new HasPrivilegesRequest(Collections.singleton("monitor"), null, null);

            // tag::has-privileges-execute-listener
            ActionListener<HasPrivilegesResponse> listener = new ActionListener<HasPrivilegesResponse>() {
                @Override
                public void onResponse(HasPrivilegesResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::has-privileges-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::has-privileges-execute-async
            client.security().hasPrivilegesAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::has-privileges-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testClearRealmCache() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            //tag::clear-realm-cache-request
            ClearRealmCacheRequest request = new ClearRealmCacheRequest(Collections.emptyList(), Collections.emptyList());
            //end::clear-realm-cache-request
            //tag::clear-realm-cache-execute
            ClearRealmCacheResponse response = client.security().clearRealmCache(request, RequestOptions.DEFAULT);
            //end::clear-realm-cache-execute

            assertNotNull(response);
            assertThat(response.getNodes(), not(empty()));

            //tag::clear-realm-cache-response
            List<ClearRealmCacheResponse.Node> nodes = response.getNodes(); // <1>
            //end::clear-realm-cache-response
        }
        {
            //tag::clear-realm-cache-execute-listener
            ClearRealmCacheRequest request = new ClearRealmCacheRequest(Collections.emptyList(), Collections.emptyList());
            ActionListener<ClearRealmCacheResponse> listener = new ActionListener<ClearRealmCacheResponse>() {
                @Override
                public void onResponse(ClearRealmCacheResponse clearRealmCacheResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::clear-realm-cache-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::clear-realm-cache-execute-async
            client.security().clearRealmCacheAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::clear-realm-cache-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testClearRolesCache() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            //tag::clear-roles-cache-request
            ClearRolesCacheRequest request = new ClearRolesCacheRequest("my_role");
            //end::clear-roles-cache-request
            //tag::clear-roles-cache-execute
            ClearRolesCacheResponse response = client.security().clearRolesCache(request, RequestOptions.DEFAULT);
            //end::clear-roles-cache-execute

            assertNotNull(response);
            assertThat(response.getNodes(), not(empty()));

            //tag::clear-roles-cache-response
            List<ClearRolesCacheResponse.Node> nodes = response.getNodes(); // <1>
            //end::clear-roles-cache-response
        }

        {
            //tag::clear-roles-cache-execute-listener
            ClearRolesCacheRequest request = new ClearRolesCacheRequest("my_role");
            ActionListener<ClearRolesCacheResponse> listener = new ActionListener<ClearRolesCacheResponse>() {
                @Override
                public void onResponse(ClearRolesCacheResponse clearRolesCacheResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::clear-roles-cache-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::clear-roles-cache-execute-async
            client.security().clearRolesCacheAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::clear-roles-cache-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetSslCertificates() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            //tag::get-certificates-execute
            GetSslCertificatesResponse response = client.security().getSslCertificates(RequestOptions.DEFAULT);
            //end::get-certificates-execute

            assertNotNull(response);

            //tag::get-certificates-response
            List<CertificateInfo> certificates = response.getCertificates(); // <1>
            //end::get-certificates-response

            assertThat(certificates.size(), Matchers.equalTo(9));
            final Iterator<CertificateInfo> it = certificates.iterator();
            CertificateInfo c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=testnode-client-profile"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=Elasticsearch Test Node, OU=elasticsearch, O=org"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.crt"));
            assertThat(c.getFormat(), Matchers.equalTo("PEM"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=OpenLDAP, OU=Elasticsearch, O=Elastic, L=Mountain View, ST=CA, C=US"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=Elasticsearch Test Node, OU=elasticsearch, O=org"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=Elasticsearch Test Client, OU=elasticsearch, O=org"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=ad-ELASTICSEARCHAD-CA, DC=ad, DC=test, DC=elasticsearch, DC=com"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=Elasticsearch Test Node"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=samba4"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
            c = it.next();
            assertThat(c.getSubjectDn(), Matchers.equalTo("CN=Elasticsearch Test Node"));
            assertThat(c.getPath(), Matchers.equalTo("testnode.jks"));
            assertThat(c.getFormat(), Matchers.equalTo("jks"));
        }

        {
            // tag::get-certificates-execute-listener
            ActionListener<GetSslCertificatesResponse> listener = new ActionListener<GetSslCertificatesResponse>() {
                @Override
                public void onResponse(GetSslCertificatesResponse getSslCertificatesResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::get-certificates-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-certificates-execute-async
            client.security().getSslCertificatesAsync(RequestOptions.DEFAULT, listener); // <1>
            // end::get-certificates-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testChangePassword() throws Exception {
        RestHighLevelClient client = highLevelClient();
        char[] password = new char[]{'p', 'a', 's', 's', 'w', 'o', 'r', 'd'};
        char[] newPassword = new char[]{'n', 'e', 'w', 'p', 'a', 's', 's', 'w', 'o', 'r', 'd'};
        User user = new User("change_password_user", Collections.singletonList("superuser"), Collections.emptyMap(), null, null);
        PutUserRequest putUserRequest = new PutUserRequest(user, password, true, RefreshPolicy.NONE);
        PutUserResponse putUserResponse = client.security().putUser(putUserRequest, RequestOptions.DEFAULT);
        assertTrue(putUserResponse.isCreated());
        {
            //tag::change-password-execute
            ChangePasswordRequest request = new ChangePasswordRequest("change_password_user", newPassword, RefreshPolicy.NONE);
            EmptyResponse response = client.security().changePassword(request, RequestOptions.DEFAULT);
            //end::change-password-execute

            assertNotNull(response);
        }
        {
            //tag::change-password-execute-listener
            ChangePasswordRequest request = new ChangePasswordRequest("change_password_user", password, RefreshPolicy.NONE);
            ActionListener<EmptyResponse> listener = new ActionListener<EmptyResponse>() {
                @Override
                public void onResponse(EmptyResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::change-password-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            //tag::change-password-execute-async
            client.security().changePasswordAsync(request, RequestOptions.DEFAULT, listener); // <1>
            //end::change-password-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testDeleteRoleMapping() throws Exception {
        final RestHighLevelClient client = highLevelClient();

        {
            // Create role mappings
            final RoleMapperExpression rules = FieldRoleMapperExpression.ofUsername("*");
            final PutRoleMappingRequest request = new PutRoleMappingRequest("mapping-example", true, Collections.singletonList("superuser"),
                rules, null, RefreshPolicy.NONE);
            final PutRoleMappingResponse response = client.security().putRoleMapping(request, RequestOptions.DEFAULT);
            boolean isCreated = response.isCreated();
            assertTrue(isCreated);
        }

        {
            // tag::delete-role-mapping-execute
            final DeleteRoleMappingRequest request = new DeleteRoleMappingRequest("mapping-example", RefreshPolicy.NONE);
            final DeleteRoleMappingResponse response = client.security().deleteRoleMapping(request, RequestOptions.DEFAULT);
            // end::delete-role-mapping-execute
            // tag::delete-role-mapping-response
            boolean isFound = response.isFound(); // <1>
            // end::delete-role-mapping-response

            assertTrue(isFound);
        }

        {
            final DeleteRoleMappingRequest request = new DeleteRoleMappingRequest("mapping-example", RefreshPolicy.NONE);
            // tag::delete-role-mapping-execute-listener
            ActionListener<DeleteRoleMappingResponse> listener = new ActionListener<DeleteRoleMappingResponse>() {
                @Override
                public void onResponse(DeleteRoleMappingResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::delete-role-mapping-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::delete-role-mapping-execute-async
            client.security().deleteRoleMappingAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::delete-role-mapping-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testDeleteRole() throws Exception {
        RestHighLevelClient client = highLevelClient();
        addRole("testrole");

        {
            // tag::delete-role-request
            DeleteRoleRequest deleteRoleRequest = new DeleteRoleRequest(
                "testrole");    // <1>
            // end::delete-role-request

            // tag::delete-role-execute
            DeleteRoleResponse deleteRoleResponse = client.security().deleteRole(deleteRoleRequest, RequestOptions.DEFAULT);
            // end::delete-role-execute

            // tag::delete-role-response
            boolean found = deleteRoleResponse.isFound();    // <1>
            // end::delete-role-response
            assertTrue(found);

            // check if deleting the already deleted role again will give us a different response
            deleteRoleResponse = client.security().deleteRole(deleteRoleRequest, RequestOptions.DEFAULT);
            assertFalse(deleteRoleResponse.isFound());
        }

        {
            DeleteRoleRequest deleteRoleRequest = new DeleteRoleRequest("testrole");

            ActionListener<DeleteRoleResponse> listener;
            //tag::delete-role-execute-listener
            listener = new ActionListener<DeleteRoleResponse>() {
                @Override
                public void onResponse(DeleteRoleResponse deleteRoleResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::delete-role-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            //tag::delete-role-execute-async
            client.security().deleteRoleAsync(deleteRoleRequest, RequestOptions.DEFAULT, listener); // <1>
            //end::delete-role-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    // TODO: move all calls to high-level REST client once APIs for adding new role exist
    private void addRole(String roleName) throws IOException {
        Request addRoleRequest = new Request(HttpPost.METHOD_NAME, "/_xpack/security/role/" + roleName);
        try (XContentBuilder builder = jsonBuilder()) {
            builder.startObject();
            {
                builder.array("cluster", "all");
            }
            builder.endObject();
            addRoleRequest.setEntity(new NStringEntity(Strings.toString(builder), ContentType.APPLICATION_JSON));
        }
        client().performRequest(addRoleRequest);
    }

    public void testCreateToken() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            // Setup user
            User token_user = new User("token_user", Collections.singletonList("kibana_user"));
            PutUserRequest putUserRequest = new PutUserRequest(token_user, "password".toCharArray(), true, RefreshPolicy.IMMEDIATE);
            PutUserResponse putUserResponse = client.security().putUser(putUserRequest, RequestOptions.DEFAULT);
            assertTrue(putUserResponse.isCreated());
        }
        {
            // tag::create-token-password-request
            final char[] password = new char[]{'p', 'a', 's', 's', 'w', 'o', 'r', 'd'};
            CreateTokenRequest createTokenRequest = CreateTokenRequest.passwordGrant("token_user", password);
            // end::create-token-password-request

            // tag::create-token-execute
            CreateTokenResponse createTokenResponse = client.security().createToken(createTokenRequest, RequestOptions.DEFAULT);
            // end::create-token-execute

            // tag::create-token-response
            String accessToken = createTokenResponse.getAccessToken();    // <1>
            String refreshToken = createTokenResponse.getRefreshToken();    // <2>
            // end::create-token-response
            assertNotNull(accessToken);
            assertNotNull(refreshToken);
            assertNotNull(createTokenResponse.getExpiresIn());

            // tag::create-token-refresh-request
            createTokenRequest = CreateTokenRequest.refreshTokenGrant(refreshToken);
            // end::create-token-refresh-request

            CreateTokenResponse refreshResponse = client.security().createToken(createTokenRequest, RequestOptions.DEFAULT);
            assertNotNull(refreshResponse.getAccessToken());
            assertNotNull(refreshResponse.getRefreshToken());
        }

        {
            // tag::create-token-client-credentials-request
            CreateTokenRequest createTokenRequest = CreateTokenRequest.clientCredentialsGrant();
            // end::create-token-client-credentials-request

            ActionListener<CreateTokenResponse> listener;
            //tag::create-token-execute-listener
            listener = new ActionListener<CreateTokenResponse>() {
                @Override
                public void onResponse(CreateTokenResponse createTokenResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::create-token-execute-listener

            // Avoid unused variable warning
            assertNotNull(listener);

            // Replace the empty listener by a blocking listener in test
            final PlainActionFuture<CreateTokenResponse> future = new PlainActionFuture<>();
            listener = future;

            //tag::create-token-execute-async
            client.security().createTokenAsync(createTokenRequest, RequestOptions.DEFAULT, listener); // <1>
            //end::create-token-execute-async

            assertNotNull(future.get(30, TimeUnit.SECONDS));
            assertNotNull(future.get().getAccessToken());
            // "client-credentials" grants aren't refreshable
            assertNull(future.get().getRefreshToken());
        }
    }

    public void testInvalidateToken() throws Exception {
        RestHighLevelClient client = highLevelClient();

        String accessToken;
        String refreshToken;
        {
            // Setup user
            final char[] password = "password".toCharArray();
            User invalidate_token_user = new User("invalidate_token", Collections.singletonList("kibana_user"));
            PutUserRequest putUserRequest = new PutUserRequest(invalidate_token_user, password, true, RefreshPolicy.IMMEDIATE);
            PutUserResponse putUserResponse = client.security().putUser(putUserRequest, RequestOptions.DEFAULT);
            assertTrue(putUserResponse.isCreated());

            // Create tokens
            final CreateTokenRequest createTokenRequest = CreateTokenRequest.passwordGrant("invalidate_token", password);
            final CreateTokenResponse tokenResponse = client.security().createToken(createTokenRequest, RequestOptions.DEFAULT);
            accessToken = tokenResponse.getAccessToken();
            refreshToken = tokenResponse.getRefreshToken();
        }
        {
            // tag::invalidate-access-token-request
            InvalidateTokenRequest invalidateTokenRequest = InvalidateTokenRequest.accessToken(accessToken);
            // end::invalidate-access-token-request

            // tag::invalidate-token-execute
            InvalidateTokenResponse invalidateTokenResponse =
                client.security().invalidateToken(invalidateTokenRequest, RequestOptions.DEFAULT);
            // end::invalidate-token-execute

            // tag::invalidate-token-response
            boolean isCreated = invalidateTokenResponse.isCreated();
            // end::invalidate-token-response
            assertTrue(isCreated);
        }

        {
            // tag::invalidate-refresh-token-request
            InvalidateTokenRequest invalidateTokenRequest = InvalidateTokenRequest.refreshToken(refreshToken);
            // end::invalidate-refresh-token-request

            ActionListener<InvalidateTokenResponse> listener;
            //tag::invalidate-token-execute-listener
            listener = new ActionListener<InvalidateTokenResponse>() {
                @Override
                public void onResponse(InvalidateTokenResponse invalidateTokenResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::invalidate-token-execute-listener

            // Avoid unused variable warning
            assertNotNull(listener);

            // Replace the empty listener by a blocking listener in test
            final PlainActionFuture<InvalidateTokenResponse> future = new PlainActionFuture<>();
            listener = future;

            //tag::invalidate-token-execute-async
            client.security().invalidateTokenAsync(invalidateTokenRequest, RequestOptions.DEFAULT, listener); // <1>
            //end::invalidate-token-execute-async

            final InvalidateTokenResponse response = future.get(30, TimeUnit.SECONDS);
            assertNotNull(response);
            assertTrue(response.isCreated());// technically, this should be false, but the API is broken
            // See https://github.com/elastic/elasticsearch/issues/35115
        }
    }

    public void testGetPrivileges() throws Exception {
        final RestHighLevelClient client = highLevelClient();
        final ApplicationPrivilege readTestappPrivilege =
            new ApplicationPrivilege("testapp", "read", Arrays.asList("action:login", "data:read/*"), null);
        final Map<String, Object> metadata = new HashMap<>();
        metadata.put("key1", "value1");
        final ApplicationPrivilege writeTestappPrivilege =
            new ApplicationPrivilege("testapp", "write", Arrays.asList("action:login", "data:write/*"), metadata);
        final ApplicationPrivilege allTestappPrivilege =
            new ApplicationPrivilege("testapp", "all", Arrays.asList("action:login", "data:write/*", "manage:*"), null);
        final Map<String, Object> metadata2 = new HashMap<>();
        metadata2.put("key2", "value2");
        final ApplicationPrivilege readTestapp2Privilege =
            new ApplicationPrivilege("testapp2", "read", Arrays.asList("action:login", "data:read/*"), metadata2);
        final ApplicationPrivilege writeTestapp2Privilege =
            new ApplicationPrivilege("testapp2", "write", Arrays.asList("action:login", "data:write/*"), null);
        final ApplicationPrivilege allTestapp2Privilege =
            new ApplicationPrivilege("testapp2", "all", Arrays.asList("action:login", "data:write/*", "manage:*"), null);

        {
            //TODO Replace this with a call to PutPrivileges once it is implemented
            final Request createPrivilegeRequest = new Request("POST", "/_xpack/security/privilege");
            createPrivilegeRequest.setJsonEntity("{" +
                "  \"testapp\": {" +
                "    \"read\": {" +
                "      \"actions\": [ \"action:login\", \"data:read/*\" ]" +
                "    }," +
                "    \"write\": {" +
                "      \"actions\": [ \"action:login\", \"data:write/*\" ]," +
                "      \"metadata\": { \"key1\": \"value1\" }" +
                "    }," +
                "    \"all\": {" +
                "      \"actions\": [ \"action:login\", \"data:write/*\" , \"manage:*\"]" +
                "    }" +
                "  }," +
                "  \"testapp2\": {" +
                "    \"read\": {" +
                "      \"actions\": [ \"action:login\", \"data:read/*\" ]," +
                "      \"metadata\": { \"key2\": \"value2\" }" +
                "    }," +
                "    \"write\": {" +
                "      \"actions\": [ \"action:login\", \"data:write/*\" ]" +
                "    }," +
                "    \"all\": {" +
                "      \"actions\": [ \"action:login\", \"data:write/*\" , \"manage:*\"]" +
                "    }" +
                "  }" +
                "}");
            final Response createPrivilegeResponse = client.getLowLevelClient().performRequest(createPrivilegeRequest);
            assertEquals(RestStatus.OK.getStatus(), createPrivilegeResponse.getStatusLine().getStatusCode());
        }

        {
            //tag::get-privileges-request
            GetPrivilegesRequest request = new GetPrivilegesRequest("testapp", "write");
            //end::get-privileges-request
            //tag::get-privileges-execute
            GetPrivilegesResponse response = client.security().getPrivileges(request, RequestOptions.DEFAULT);
            //end::get-privileges-execute
            assertNotNull(response);
            assertThat(response.getPrivileges().size(), equalTo(1));
            assertThat(response.getPrivileges().contains(writeTestappPrivilege), equalTo(true));
        }

        {
            //tag::get-all-application-privileges-request
            GetPrivilegesRequest request = GetPrivilegesRequest.getApplicationPrivileges("testapp");
            //end::get-all-application-privileges-request
            GetPrivilegesResponse response = client.security().getPrivileges(request, RequestOptions.DEFAULT);

            assertNotNull(response);
            assertThat(response.getPrivileges().size(), equalTo(3));
            final GetPrivilegesResponse exptectedResponse =
                new GetPrivilegesResponse(Arrays.asList(readTestappPrivilege, writeTestappPrivilege, allTestappPrivilege));
            assertThat(response, equalTo(exptectedResponse));
            //tag::get-privileges-response
            Set<ApplicationPrivilege> privileges = response.getPrivileges();
            //end::get-privileges-response
            for (ApplicationPrivilege privilege : privileges) {
                assertThat(privilege.getApplication(), equalTo("testapp"));
                if (privilege.getName().equals("read")) {
                    assertThat(privilege.getActions(), containsInAnyOrder("action:login", "data:read/*"));
                    assertThat(privilege.getMetadata().isEmpty(), equalTo(true));
                } else if (privilege.getName().equals("write")) {
                    assertThat(privilege.getActions(), containsInAnyOrder("action:login", "data:write/*"));
                    assertThat(privilege.getMetadata().isEmpty(), equalTo(false));
                    assertThat(privilege.getMetadata().get("key1"), equalTo("value1"));
                } else if (privilege.getName().equals("all")) {
                    assertThat(privilege.getActions(), containsInAnyOrder("action:login", "data:write/*", "manage:*"));
                    assertThat(privilege.getMetadata().isEmpty(), equalTo(true));
                }
            }
        }

        {
            //tag::get-all-privileges-request
            GetPrivilegesRequest request = GetPrivilegesRequest.getAllPrivileges();
            //end::get-all-privileges-request
            GetPrivilegesResponse response = client.security().getPrivileges(request, RequestOptions.DEFAULT);

            assertNotNull(response);
            assertThat(response.getPrivileges().size(), equalTo(6));
            final GetPrivilegesResponse exptectedResponse =
                new GetPrivilegesResponse(Arrays.asList(readTestappPrivilege, writeTestappPrivilege, allTestappPrivilege,
                    readTestapp2Privilege, writeTestapp2Privilege, allTestapp2Privilege));
            assertThat(response, equalTo(exptectedResponse));
        }

        {
            GetPrivilegesRequest request = new GetPrivilegesRequest("testapp", "read");
            //tag::get-privileges-execute-listener
            ActionListener<GetPrivilegesResponse> listener = new ActionListener<GetPrivilegesResponse>() {
                @Override
                public void onResponse(GetPrivilegesResponse getPrivilegesResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::get-privileges-execute-listener

            // Avoid unused variable warning
            assertNotNull(listener);

            // Replace the empty listener by a blocking listener in test
            final PlainActionFuture<GetPrivilegesResponse> future = new PlainActionFuture<>();
            listener = future;

            //tag::get-privileges-execute-async
            client.security().getPrivilegesAsync(request, RequestOptions.DEFAULT, listener); // <1>
            //end::get-privileges-execute-async

            final GetPrivilegesResponse response = future.get(30, TimeUnit.SECONDS);
            assertNotNull(response);
            assertThat(response.getPrivileges().size(), equalTo(1));
            assertThat(response.getPrivileges().contains(readTestappPrivilege), equalTo(true));
        }
    }

    public void testDeletePrivilege() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            final Request createPrivilegeRequest = new Request("POST", "/_xpack/security/privilege");
            createPrivilegeRequest.setJsonEntity("{" +
                "  \"testapp\": {" +
                "    \"read\": {" +
                "      \"actions\": [ \"action:login\", \"data:read/*\" ]" +
                "    }," +
                "    \"write\": {" +
                "      \"actions\": [ \"action:login\", \"data:write/*\" ]" +
                "    }," +
                "    \"all\": {" +
                "      \"actions\": [ \"action:login\", \"data:write/*\" ]" +
                "    }" +
                "  }" +
                "}");

            final Response createPrivilegeResponse = client.getLowLevelClient().performRequest(createPrivilegeRequest);
            assertEquals(RestStatus.OK.getStatus(), createPrivilegeResponse.getStatusLine().getStatusCode());
        }
        {
            // tag::delete-privileges-request
            DeletePrivilegesRequest request = new DeletePrivilegesRequest(
                "testapp",          // <1>
                "read", "write"); // <2>
            // end::delete-privileges-request

            // tag::delete-privileges-execute
            DeletePrivilegesResponse response = client.security().deletePrivileges(request, RequestOptions.DEFAULT);
            // end::delete-privileges-execute

            // tag::delete-privileges-response
            String application = response.getApplication();        // <1>
            boolean found = response.isFound("read");              // <2>
            // end::delete-privileges-response
            assertThat(application, equalTo("testapp"));
            assertTrue(response.isFound("write"));
            assertTrue(found);

            // check if deleting the already deleted privileges again will give us a different response
            response = client.security().deletePrivileges(request, RequestOptions.DEFAULT);
            assertFalse(response.isFound("write"));
        }
        {
            DeletePrivilegesRequest deletePrivilegesRequest = new DeletePrivilegesRequest("testapp", "all");

            ActionListener<DeletePrivilegesResponse> listener;
            //tag::delete-privileges-execute-listener
            listener = new ActionListener<DeletePrivilegesResponse>() {
                @Override
                public void onResponse(DeletePrivilegesResponse deletePrivilegesResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::delete-privileges-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            //tag::delete-privileges-execute-async
            client.security().deletePrivilegesAsync(deletePrivilegesRequest, RequestOptions.DEFAULT, listener); // <1>
            //end::delete-privileges-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
}

