/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.authc.apikey;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.admin.indices.create.CreateIndexAction;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.get.GetAction;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.main.MainAction;
import org.elasticsearch.action.main.MainRequest;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.test.SecuritySingleNodeTestCase;
import org.elasticsearch.test.XContentTestUtils;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.action.apikey.CreateApiKeyAction;
import org.elasticsearch.xpack.core.security.action.apikey.CreateApiKeyRequest;
import org.elasticsearch.xpack.core.security.action.apikey.CreateApiKeyResponse;
import org.elasticsearch.xpack.core.security.action.apikey.GetApiKeyAction;
import org.elasticsearch.xpack.core.security.action.apikey.GetApiKeyRequest;
import org.elasticsearch.xpack.core.security.action.apikey.GetApiKeyResponse;
import org.elasticsearch.xpack.core.security.action.apikey.GrantApiKeyAction;
import org.elasticsearch.xpack.core.security.action.apikey.GrantApiKeyRequest;
import org.elasticsearch.xpack.core.security.action.apikey.QueryApiKeyAction;
import org.elasticsearch.xpack.core.security.action.apikey.QueryApiKeyRequest;
import org.elasticsearch.xpack.core.security.action.apikey.QueryApiKeyResponse;
import org.elasticsearch.xpack.core.security.action.service.CreateServiceAccountTokenAction;
import org.elasticsearch.xpack.core.security.action.service.CreateServiceAccountTokenRequest;
import org.elasticsearch.xpack.core.security.action.service.CreateServiceAccountTokenResponse;
import org.elasticsearch.xpack.core.security.action.token.CreateTokenAction;
import org.elasticsearch.xpack.core.security.action.token.CreateTokenRequestBuilder;
import org.elasticsearch.xpack.core.security.action.token.CreateTokenResponse;
import org.elasticsearch.xpack.core.security.action.user.PutUserAction;
import org.elasticsearch.xpack.core.security.action.user.PutUserRequest;
import org.elasticsearch.xpack.core.security.authc.support.Hasher;
import org.elasticsearch.xpack.core.security.authz.RoleDescriptor;
import org.elasticsearch.xpack.security.authc.service.ServiceAccountService;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.Base64;
import java.util.Collections;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.xpack.security.support.SecuritySystemIndices.SECURITY_MAIN_ALIAS;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.emptyArray;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasKey;

public class ApiKeySingleNodeTests extends SecuritySingleNodeTestCase {

    @Override
    protected Settings nodeSettings() {
        Settings.Builder builder = Settings.builder().put(super.nodeSettings());
        builder.put(XPackSettings.API_KEY_SERVICE_ENABLED_SETTING.getKey(), true);
        builder.put(XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.getKey(), true);
        return builder.build();
    }

    public void testQueryWithExpiredKeys() throws InterruptedException {
        final String id1 = client().execute(
            CreateApiKeyAction.INSTANCE,
            new CreateApiKeyRequest("expired-shortly", null, TimeValue.timeValueMillis(1), null)
        ).actionGet().getId();
        final String id2 = client().execute(
            CreateApiKeyAction.INSTANCE,
            new CreateApiKeyRequest("long-lived", null, TimeValue.timeValueDays(1), null)
        ).actionGet().getId();
        Thread.sleep(10); // just to be 100% sure that the 1st key is expired when we search for it

        final QueryApiKeyRequest queryApiKeyRequest = new QueryApiKeyRequest(
            QueryBuilders.boolQuery()
                .filter(QueryBuilders.idsQuery().addIds(id1, id2))
                .filter(QueryBuilders.rangeQuery("expiration").from(Instant.now().toEpochMilli()))
        );
        final QueryApiKeyResponse queryApiKeyResponse = client().execute(QueryApiKeyAction.INSTANCE, queryApiKeyRequest).actionGet();
        assertThat(queryApiKeyResponse.getItems().length, equalTo(1));
        assertThat(queryApiKeyResponse.getItems()[0].getApiKey().getId(), equalTo(id2));
        assertThat(queryApiKeyResponse.getItems()[0].getApiKey().getName(), equalTo("long-lived"));
        assertThat(queryApiKeyResponse.getItems()[0].getSortValues(), emptyArray());
    }

    public void testCreatingApiKeyWithNoAccess() {
        final PutUserRequest putUserRequest = new PutUserRequest();
        final String username = randomAlphaOfLength(8);
        putUserRequest.username(username);
        final SecureString password = new SecureString("super-strong-password".toCharArray());
        putUserRequest.passwordHash(Hasher.PBKDF2.hash(password));
        putUserRequest.roles(Strings.EMPTY_ARRAY);
        client().execute(PutUserAction.INSTANCE, putUserRequest).actionGet();

        final GrantApiKeyRequest grantApiKeyRequest = new GrantApiKeyRequest();
        grantApiKeyRequest.getGrant().setType("password");
        grantApiKeyRequest.getGrant().setUsername(username);
        grantApiKeyRequest.getGrant().setPassword(password);
        grantApiKeyRequest.getApiKeyRequest().setName(randomAlphaOfLength(8));
        grantApiKeyRequest.getApiKeyRequest()
            .setRoleDescriptors(
                List.of(
                    new RoleDescriptor(
                        "x",
                        new String[] { "all" },
                        new RoleDescriptor.IndicesPrivileges[] {
                            RoleDescriptor.IndicesPrivileges.builder()
                                .indices("*")
                                .privileges("all")
                                .allowRestrictedIndices(true)
                                .build() },
                        null,
                        null,
                        null,
                        null,
                        null
                    )
                )
            );
        final CreateApiKeyResponse createApiKeyResponse = client().execute(GrantApiKeyAction.INSTANCE, grantApiKeyRequest).actionGet();

        final String base64ApiKeyKeyValue = Base64.getEncoder()
            .encodeToString(
                (createApiKeyResponse.getId() + ":" + createApiKeyResponse.getKey().toString()).getBytes(StandardCharsets.UTF_8)
            );

        // No cluster access
        final ElasticsearchSecurityException e1 = expectThrows(
            ElasticsearchSecurityException.class,
            () -> client().filterWithHeader(Map.of("Authorization", "ApiKey " + base64ApiKeyKeyValue))
                .execute(MainAction.INSTANCE, new MainRequest())
                .actionGet()
        );
        assertThat(e1.status().getStatus(), equalTo(403));
        assertThat(e1.getMessage(), containsString("is unauthorized for API key"));

        // No index access
        final ElasticsearchSecurityException e2 = expectThrows(
            ElasticsearchSecurityException.class,
            () -> client().filterWithHeader(Map.of("Authorization", "ApiKey " + base64ApiKeyKeyValue))
                .execute(
                    CreateIndexAction.INSTANCE,
                    new CreateIndexRequest(randomFrom(randomAlphaOfLengthBetween(3, 8), SECURITY_MAIN_ALIAS))
                )
                .actionGet()
        );
        assertThat(e2.status().getStatus(), equalTo(403));
        assertThat(e2.getMessage(), containsString("is unauthorized for API key"));
    }

    public void testServiceAccountApiKey() throws IOException {
        final CreateServiceAccountTokenRequest createServiceAccountTokenRequest = new CreateServiceAccountTokenRequest(
            "elastic",
            "fleet-server",
            randomAlphaOfLength(8)
        );
        final CreateServiceAccountTokenResponse createServiceAccountTokenResponse = client().execute(
            CreateServiceAccountTokenAction.INSTANCE,
            createServiceAccountTokenRequest
        ).actionGet();

        final CreateApiKeyResponse createApiKeyResponse = client().filterWithHeader(
            Map.of("Authorization", "Bearer " + createServiceAccountTokenResponse.getValue())
        ).execute(CreateApiKeyAction.INSTANCE, new CreateApiKeyRequest(randomAlphaOfLength(8), null, null)).actionGet();

        final Map<String, Object> apiKeyDocument = getApiKeyDocument(createApiKeyResponse.getId());

        @SuppressWarnings("unchecked")
        final Map<String, Object> fleetServerRoleDescriptor = (Map<String, Object>) apiKeyDocument.get("limited_by_role_descriptors");
        assertThat(fleetServerRoleDescriptor.size(), equalTo(1));
        assertThat(fleetServerRoleDescriptor, hasKey("elastic/fleet-server"));

        @SuppressWarnings("unchecked")
        final Map<String, ?> descriptor = (Map<String, ?>) fleetServerRoleDescriptor.get("elastic/fleet-server");

        final RoleDescriptor roleDescriptor = RoleDescriptor.parse(
            "elastic/fleet-server",
            XContentTestUtils.convertToXContent(descriptor, XContentType.JSON),
            false,
            XContentType.JSON
        );
        assertThat(roleDescriptor, equalTo(ServiceAccountService.getServiceAccounts().get("elastic/fleet-server").roleDescriptor()));
    }

    public void testGetApiKeyWorksForTheApiKeyItself() {
        final String apiKeyName = randomAlphaOfLength(10);
        final CreateApiKeyResponse createApiKeyResponse = client().execute(
            CreateApiKeyAction.INSTANCE,
            new CreateApiKeyRequest(
                apiKeyName,
                List.of(new RoleDescriptor("x", new String[] { "manage_own_api_key", "manage_token" }, null, null, null, null, null, null)),
                null,
                null
            )
        ).actionGet();

        final String apiKeyId = createApiKeyResponse.getId();
        final String base64ApiKeyKeyValue = Base64.getEncoder()
            .encodeToString((apiKeyId + ":" + createApiKeyResponse.getKey().toString()).getBytes(StandardCharsets.UTF_8));

        // Works for both the API key itself or the token created by it
        final Client clientKey1;
        if (randomBoolean()) {
            clientKey1 = client().filterWithHeader(Collections.singletonMap("Authorization", "ApiKey " + base64ApiKeyKeyValue));
        } else {
            final CreateTokenResponse createTokenResponse = new CreateTokenRequestBuilder(
                client().filterWithHeader(Collections.singletonMap("Authorization", "ApiKey " + base64ApiKeyKeyValue)),
                CreateTokenAction.INSTANCE
            ).setGrantType("client_credentials").get();
            clientKey1 = client().filterWithHeader(Map.of("Authorization", "Bearer " + createTokenResponse.getTokenString()));
        }

        // Can get its own info
        final GetApiKeyResponse getApiKeyResponse = clientKey1.execute(
            GetApiKeyAction.INSTANCE,
            GetApiKeyRequest.usingApiKeyId(apiKeyId, randomBoolean())
        ).actionGet();
        assertThat(getApiKeyResponse.getApiKeyInfos().length, equalTo(1));
        assertThat(getApiKeyResponse.getApiKeyInfos()[0].getId(), equalTo(apiKeyId));

        // Cannot get any other keys
        final ElasticsearchSecurityException e = expectThrows(
            ElasticsearchSecurityException.class,
            () -> clientKey1.execute(GetApiKeyAction.INSTANCE, GetApiKeyRequest.forAllApiKeys()).actionGet()
        );
        assertThat(e.getMessage(), containsString("unauthorized for API key id [" + apiKeyId + "]"));
    }

    private Map<String, Object> getApiKeyDocument(String apiKeyId) {
        final GetResponse getResponse = client().execute(GetAction.INSTANCE, new GetRequest(".security-7", apiKeyId)).actionGet();
        return getResponse.getSource();
    }
}
