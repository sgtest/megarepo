/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.security.authc;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.DocWriteRequest;
import org.elasticsearch.action.DocWriteResponse;
import org.elasticsearch.action.NoShardAvailableActionException;
import org.elasticsearch.action.UnavailableShardsException;
import org.elasticsearch.action.bulk.BulkAction;
import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.get.GetAction;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.get.GetRequestBuilder;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.index.IndexAction;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchRequestBuilder;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.update.UpdateAction;
import org.elasticsearch.action.update.UpdateRequestBuilder;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.get.GetResult;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.TermQueryBuilder;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.license.XPackLicenseState.Feature;
import org.elasticsearch.node.Node;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.test.ClusterServiceUtils;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.EqualsHashCodeTestUtils;
import org.elasticsearch.test.XContentTestUtils;
import org.elasticsearch.threadpool.FixedExecutorBuilder;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.Authentication.AuthenticationType;
import org.elasticsearch.xpack.core.security.authc.Authentication.RealmRef;
import org.elasticsearch.xpack.core.security.authc.TokenMetadata;
import org.elasticsearch.xpack.core.security.authc.support.TokensInvalidationResult;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.core.watcher.watch.ClockMock;
import org.elasticsearch.xpack.security.authc.TokenService.RefreshTokenStatus;
import org.elasticsearch.xpack.security.support.FeatureNotEnabledException;
import org.elasticsearch.xpack.security.support.SecurityIndexManager;
import org.elasticsearch.xpack.security.test.SecurityMocks;
import org.hamcrest.Matchers;
import org.junit.After;
import org.junit.AfterClass;
import org.junit.Before;
import org.junit.BeforeClass;

import javax.crypto.SecretKey;
import java.io.IOException;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.time.Clock;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Base64;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static java.time.Clock.systemUTC;
import static org.elasticsearch.repositories.blobstore.ESBlobStoreRepositoryIntegTestCase.randomBytes;
import static org.elasticsearch.test.ClusterServiceUtils.setState;
import static org.elasticsearch.test.TestMatchers.throwableWithMessage;
import static org.hamcrest.CoreMatchers.is;
import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyString;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TokenServiceTests extends ESTestCase {

    private static ThreadPool threadPool;
    private static final Settings settings = Settings.builder().put(Node.NODE_NAME_SETTING.getKey(), "TokenServiceTests")
        .put(XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.getKey(), true).build();

    private Client client;
    private SecurityIndexManager securityMainIndex;
    private SecurityIndexManager securityTokensIndex;
    private ClusterService clusterService;
    private DiscoveryNode oldNode;
    private Settings tokenServiceEnabledSettings = Settings.builder()
        .put(XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.getKey(), true).build();
    private XPackLicenseState licenseState;
    private SecurityContext securityContext;

    @Before
    public void setupClient() {
        client = mock(Client.class);
        when(client.threadPool()).thenReturn(threadPool);
        when(client.settings()).thenReturn(settings);
        doAnswer(invocationOnMock -> {
            GetRequestBuilder builder = new GetRequestBuilder(client, GetAction.INSTANCE);
            builder.setIndex((String) invocationOnMock.getArguments()[0])
                .setId((String) invocationOnMock.getArguments()[1]);
            return builder;
        }).when(client).prepareGet(anyString(), anyString());
        when(client.prepareIndex(any(String.class)))
            .thenReturn(new IndexRequestBuilder(client, IndexAction.INSTANCE));
        when(client.prepareBulk())
            .thenReturn(new BulkRequestBuilder(client, BulkAction.INSTANCE));
        when(client.prepareUpdate(any(String.class), any(String.class)))
            .thenAnswer(inv -> {
                final String index = (String) inv.getArguments()[0];
                final String id = (String) inv.getArguments()[1];
                return new UpdateRequestBuilder(client, UpdateAction.INSTANCE).setIndex(index).setId(id);
            });
        when(client.prepareSearch(any(String.class)))
            .thenReturn(new SearchRequestBuilder(client, SearchAction.INSTANCE));
        doAnswer(invocationOnMock -> {
            ActionListener<IndexResponse> responseActionListener = (ActionListener<IndexResponse>) invocationOnMock.getArguments()[2];
            responseActionListener.onResponse(new IndexResponse(new ShardId(".security", UUIDs.randomBase64UUID(), randomInt()),
                randomAlphaOfLength(4), randomNonNegativeLong(), randomNonNegativeLong(), randomNonNegativeLong(), true));
            return null;
        }).when(client).execute(eq(IndexAction.INSTANCE), any(IndexRequest.class), any(ActionListener.class));
        doAnswer(invocationOnMock -> {
            BulkRequest request = (BulkRequest) invocationOnMock.getArguments()[0];
            ActionListener<BulkResponse> responseActionListener = (ActionListener<BulkResponse>) invocationOnMock.getArguments()[1];
            BulkItemResponse[] responses = new BulkItemResponse[request.requests().size()];
            final String indexUUID = randomAlphaOfLength(22);
            for (int i = 0; i < responses.length; i++) {
                var shardId = new ShardId(securityTokensIndex.aliasName(), indexUUID, 1);
                var docId = request.requests().get(i).id();
                var result = new GetResult(shardId.getIndexName(), docId, 1, 1, 1, true, null, null, null);
                final UpdateResponse response = new UpdateResponse(shardId, result.getId(), result.getSeqNo(), result.getPrimaryTerm(),
                    result.getVersion() + 1, DocWriteResponse.Result.UPDATED);
                response.setGetResult(result);
                responses[i] = new BulkItemResponse(i, DocWriteRequest.OpType.UPDATE, response);
            }
            responseActionListener.onResponse(new BulkResponse(responses, randomLongBetween(1, 500)));
            return null;
        }).when(client).bulk(any(BulkRequest.class), any(ActionListener.class));

        this.securityContext = new SecurityContext(settings, threadPool.getThreadContext());
        // setup lifecycle service
        this.securityMainIndex = SecurityMocks.mockSecurityIndexManager();
        this.securityTokensIndex = SecurityMocks.mockSecurityIndexManager();
        this.clusterService = ClusterServiceUtils.createClusterService(threadPool);

        // License state (enabled by default)
        licenseState = mock(XPackLicenseState.class);
        when(licenseState.isSecurityEnabled()).thenReturn(true);
        when(licenseState.checkFeature(Feature.SECURITY_TOKEN_SERVICE)).thenReturn(true);

        // version 7.2 was an "inflection" point in the Token Service development (access_tokens as UUIDS, multiple concurrent refreshes,
        // tokens docs on a separate index), let's test the TokenService works in a mixed cluster with nodes with versions prior to these
        // developments
        if (randomBoolean()) {
            oldNode = addAnotherDataNodeWithVersion(this.clusterService, randomFrom(Version.V_7_0_0, Version.V_7_1_0));
        }
    }

    @After
    public void tearDown() throws Exception {
        super.tearDown();
        clusterService.close();
    }

    @BeforeClass
    public static void startThreadPool() throws IOException {
        threadPool = new ThreadPool(settings,
            new FixedExecutorBuilder(settings, TokenService.THREAD_POOL_NAME, 1, 1000, "xpack.security.authc.token.thread_pool",
                false));
        new Authentication(new User("foo"), new RealmRef("realm", "type", "node"), null).writeToContext(threadPool.getThreadContext());
    }

    @AfterClass
    public static void shutdownThreadpool() throws InterruptedException {
        terminate(threadPool);
        threadPool = null;
    }

    public void testAttachAndGetToken() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        assertNotNull(accessToken);
        mockGetTokenFromId(tokenService, userTokenId, authentication, false);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", randomFrom("Bearer ", "BEARER ", "bearer ") + accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // verify a second separate token service with its own salt can also verify
            TokenService anotherService = createTokenService(tokenServiceEnabledSettings, systemUTC());
            anotherService.refreshMetadata(tokenService.getTokenMetadata());
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            anotherService.getAndValidateToken(requestContext, future);
            UserToken fromOtherService = future.get();
            assertAuthentication(authentication, fromOtherService.getAuthentication());
        }
    }

    public void testInvalidAuthorizationHeader() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        String token = randomFrom("", "          ");
        String authScheme = randomFrom("Bearer ", "BEARER ", "bearer ", "Basic ");
        requestContext.putHeader("Authorization", authScheme + token);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertThat(serialized, nullValue());
        }
    }

    public void testRotateKey() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // This test only makes sense in mixed clusters with pre v7.2.0 nodes where the Key is actually used
        if (null == oldNode) {
            oldNode = addAnotherDataNodeWithVersion(this.clusterService, randomFrom(Version.V_7_0_0, Version.V_7_1_0));
        }
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        assertNotNull(accessToken);
        mockGetTokenFromId(tokenService, userTokenId, authentication, false);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }
        rotateKeys(tokenService);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }

        PlainActionFuture<TokenService.CreateTokenResult> newTokenFuture = new PlainActionFuture<>();
        final String newUserTokenId = UUIDs.randomBase64UUID();
        final String newRefreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(newUserTokenId, newRefreshToken, authentication, authentication, Collections.emptyMap(),
            newTokenFuture);
        final String newAccessToken = newTokenFuture.get().getAccessToken();
        assertNotNull(newAccessToken);
        assertNotEquals(newAccessToken, accessToken);

        requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, newAccessToken);
        mockGetTokenFromId(tokenService, newUserTokenId, authentication, false);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }
    }

    private void rotateKeys(TokenService tokenService) {
        TokenMetadata tokenMetadata = tokenService.generateSpareKey();
        tokenService.refreshMetadata(tokenMetadata);
        tokenMetadata = tokenService.rotateToSpareKey();
        tokenService.refreshMetadata(tokenMetadata);
    }

    public void testKeyExchange() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // This test only makes sense in mixed clusters with pre v7.2.0 nodes where the Key is actually used
        if (null == oldNode) {
            oldNode = addAnotherDataNodeWithVersion(this.clusterService, randomFrom(Version.V_7_0_0, Version.V_7_1_0));
        }
        int numRotations = randomIntBetween(1, 5);
        for (int i = 0; i < numRotations; i++) {
            rotateKeys(tokenService);
        }
        TokenService otherTokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        otherTokenService.refreshMetadata(tokenService.getTokenMetadata());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        assertNotNull(accessToken);
        mockGetTokenFromId(tokenService, userTokenId, authentication, false);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);
        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            otherTokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(serialized.getAuthentication(), authentication);
        }

        rotateKeys(tokenService);

        otherTokenService.refreshMetadata(tokenService.getTokenMetadata());

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            otherTokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(serialized.getAuthentication(), authentication);
        }
    }

    public void testPruneKeys() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // This test only makes sense in mixed clusters with pre v7.2.0 nodes where the Key is actually used
        if (null == oldNode) {
            oldNode = addAnotherDataNodeWithVersion(this.clusterService, randomFrom(Version.V_7_0_0, Version.V_7_1_0));
        }
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        assertNotNull(accessToken);
        mockGetTokenFromId(tokenService, userTokenId, authentication, false);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }
        TokenMetadata metadata = tokenService.pruneKeys(randomIntBetween(0, 100));
        tokenService.refreshMetadata(metadata);

        int numIterations = scaledRandomIntBetween(1, 5);
        for (int i = 0; i < numIterations; i++) {
            rotateKeys(tokenService);
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }

        PlainActionFuture<TokenService.CreateTokenResult> newTokenFuture = new PlainActionFuture<>();
        final String newUserTokenId = UUIDs.randomBase64UUID();
        final String newRefreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(newUserTokenId, newRefreshToken, authentication, authentication, Collections.emptyMap(),
            newTokenFuture);
        final String newAccessToken = newTokenFuture.get().getAccessToken();
        assertNotNull(newAccessToken);
        assertNotEquals(newAccessToken, accessToken);

        metadata = tokenService.pruneKeys(1);
        tokenService.refreshMetadata(metadata);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertNull(serialized);
        }

        requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, newAccessToken);
        mockGetTokenFromId(tokenService, newUserTokenId, authentication, false);
        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }

    }

    public void testPassphraseWorks() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // This test only makes sense in mixed clusters with pre v7.1.0 nodes where the Key is actually used
        if (null == oldNode) {
            oldNode = addAnotherDataNodeWithVersion(this.clusterService, randomFrom(Version.V_7_0_0, Version.V_7_1_0));
        }
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        assertNotNull(accessToken);
        mockGetTokenFromId(tokenService, userTokenId, authentication, false);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertAuthentication(authentication, serialized.getAuthentication());
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // verify a second separate token service with its own passphrase cannot verify
            TokenService anotherService = createTokenService(tokenServiceEnabledSettings, systemUTC());
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            anotherService.getAndValidateToken(requestContext, future);
            assertNull(future.get());
        }
    }

    public void testGetTokenWhenKeyCacheHasExpired() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // This test only makes sense in mixed clusters with pre v7.1.0 nodes where the Key is actually used
        if (null == oldNode) {
            oldNode = addAnotherDataNodeWithVersion(this.clusterService, randomFrom(Version.V_7_0_0, Version.V_7_1_0));
        }
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);

        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        String accessToken = tokenFuture.get().getAccessToken();
        assertThat(accessToken, notNullValue());

        tokenService.clearActiveKeyCache();

        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        accessToken = tokenFuture.get().getAccessToken();
        assertThat(accessToken, notNullValue());
    }

    public void testInvalidatedToken() throws Exception {
        when(securityMainIndex.indexExists()).thenReturn(true);
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        assertNotNull(accessToken);
        mockGetTokenFromId(tokenService, userTokenId, authentication, true);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, future::actionGet);
            final String headerValue = e.getHeader("WWW-Authenticate").get(0);
            assertThat(headerValue, containsString("Bearer realm="));
            assertThat(headerValue, containsString("expired"));
        }
    }

    public void testInvalidateRefreshToken() throws Exception {
        when(securityMainIndex.indexExists()).thenReturn(true);
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String rawRefreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, rawRefreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        final String clientRefreshToken = tokenFuture.get().getRefreshToken();
        assertNotNull(accessToken);
        mockFindTokenFromRefreshToken(rawRefreshToken, buildUserToken(tokenService, userTokenId, authentication), null);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<TokensInvalidationResult> future = new PlainActionFuture<>();
            tokenService.invalidateRefreshToken(clientRefreshToken, future);
            final TokensInvalidationResult result = future.get();
            assertThat(result.getInvalidatedTokens(), hasSize(1));
            assertThat(result.getPreviouslyInvalidatedTokens(), empty());
            assertThat(result.getErrors(), empty());
        }
    }

    public void testInvalidateRefreshTokenThatIsAlreadyInvalidated() throws Exception {
        when(securityMainIndex.indexExists()).thenReturn(true);
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String rawRefreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, rawRefreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        final String clientRefreshToken = tokenFuture.get().getRefreshToken();
        assertNotNull(accessToken);
        mockFindTokenFromRefreshToken(rawRefreshToken, buildUserToken(tokenService, userTokenId, authentication),
            new RefreshTokenStatus(true, randomAlphaOfLength(12), randomAlphaOfLength(6), false, null, null, null, null)
        );

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<TokensInvalidationResult> future = new PlainActionFuture<>();
            tokenService.invalidateRefreshToken(clientRefreshToken, future);
            final TokensInvalidationResult result = future.get();
            assertThat(result.getPreviouslyInvalidatedTokens(), hasSize(1));
            assertThat(result.getInvalidatedTokens(), empty());
            assertThat(result.getErrors(), empty());
        }
    }

    private void storeTokenHeader(ThreadContext requestContext, String tokenString) throws IOException, GeneralSecurityException {
        requestContext.putHeader("Authorization", "Bearer " + tokenString);
    }

    public void testComputeSecretKeyIsConsistent() throws Exception {
        byte[] saltArr = new byte[32];
        random().nextBytes(saltArr);
        SecretKey key =
            TokenService.computeSecretKey("some random passphrase".toCharArray(), saltArr, TokenService.TOKEN_SERVICE_KEY_ITERATIONS);
        SecretKey key2 =
            TokenService.computeSecretKey("some random passphrase".toCharArray(), saltArr, TokenService.TOKEN_SERVICE_KEY_ITERATIONS);
        assertArrayEquals(key.getEncoded(), key2.getEncoded());
    }

    public void testTokenExpiryConfig() {
        TimeValue expiration = TokenService.TOKEN_EXPIRATION.get(tokenServiceEnabledSettings);
        assertThat(expiration, equalTo(TimeValue.timeValueMinutes(20L)));
        // Configure Minimum expiration
        tokenServiceEnabledSettings = Settings.builder().put(TokenService.TOKEN_EXPIRATION.getKey(), "1s").build();
        expiration = TokenService.TOKEN_EXPIRATION.get(tokenServiceEnabledSettings);
        assertThat(expiration, equalTo(TimeValue.timeValueSeconds(1L)));
        // Configure Maximum expiration
        tokenServiceEnabledSettings = Settings.builder().put(TokenService.TOKEN_EXPIRATION.getKey(), "60m").build();
        expiration = TokenService.TOKEN_EXPIRATION.get(tokenServiceEnabledSettings);
        assertThat(expiration, equalTo(TimeValue.timeValueHours(1L)));
        // Outside range should fail
        tokenServiceEnabledSettings = Settings.builder().put(TokenService.TOKEN_EXPIRATION.getKey(), "1ms").build();
        IllegalArgumentException ile = expectThrows(IllegalArgumentException.class,
            () -> TokenService.TOKEN_EXPIRATION.get(tokenServiceEnabledSettings));
        assertThat(ile.getMessage(),
            containsString("failed to parse value [1ms] for setting [xpack.security.authc.token.timeout], must be >= [1s]"));
        tokenServiceEnabledSettings = Settings.builder().put(TokenService.TOKEN_EXPIRATION.getKey(), "120m").build();
        ile = expectThrows(IllegalArgumentException.class, () -> TokenService.TOKEN_EXPIRATION.get(tokenServiceEnabledSettings));
        assertThat(ile.getMessage(),
            containsString("failed to parse value [120m] for setting [xpack.security.authc.token.timeout], must be <= [1h]"));
    }

    public void testTokenExpiry() throws Exception {
        ClockMock clock = ClockMock.frozen();
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, clock);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        final String userTokenId = UUIDs.randomBase64UUID();
        UserToken userToken = new UserToken(userTokenId, tokenService.getTokenVersionCompatibility(), authentication,
            tokenService.getExpirationTime(), Collections.emptyMap());
        mockGetTokenFromId(userToken, false);
        final String accessToken = tokenService.prependVersionAndEncodeAccessToken(tokenService.getTokenVersionCompatibility(), userTokenId
        );

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // the clock is still frozen, so the cookie should be valid
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertAuthentication(authentication, future.get().getAuthentication());
        }

        final TimeValue defaultExpiration = TokenService.TOKEN_EXPIRATION.get(Settings.EMPTY);
        final int fastForwardAmount = randomIntBetween(1, Math.toIntExact(defaultExpiration.getSeconds()) - 5);
        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // move the clock forward but don't go to expiry
            clock.fastForwardSeconds(fastForwardAmount);
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertAuthentication(authentication, future.get().getAuthentication());
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // move to expiry, stripping nanoseconds, as we don't store them in the security-tokens index
            clock.setTime(userToken.getExpirationTime().truncatedTo(ChronoUnit.MILLIS).atZone(clock.getZone()));
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertAuthentication(authentication, future.get().getAuthentication());
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // move one second past expiry
            clock.fastForwardSeconds(1);
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, future::actionGet);
            final String headerValue = e.getHeader("WWW-Authenticate").get(0);
            assertThat(headerValue, containsString("Bearer realm="));
            assertThat(headerValue, containsString("expired"));
        }
    }

    public void testTokenServiceDisabled() throws Exception {
        TokenService tokenService = new TokenService(Settings.builder()
            .put(XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.getKey(), false)
            .build(),
            Clock.systemUTC(), client, licenseState, securityContext, securityMainIndex, securityTokensIndex, clusterService);
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
            () -> tokenService.createOAuth2Tokens(null, null, null, true, null));
        assertThat(e, throwableWithMessage("security tokens are not enabled"));
        assertThat(e, instanceOf(FeatureNotEnabledException.class));
        // Client can check the metadata for this value, and depend on an exact string match:
        assertThat(e.getMetadata(FeatureNotEnabledException.DISABLED_FEATURE_METADATA), contains("security_tokens"));

        PlainActionFuture<UserToken> future = new PlainActionFuture<>();
        tokenService.getAndValidateToken(null, future);
        assertNull(future.get());

        PlainActionFuture<TokensInvalidationResult> invalidateFuture = new PlainActionFuture<>();
        e = expectThrows(ElasticsearchException.class, () -> tokenService.invalidateAccessToken((String) null, invalidateFuture));
        assertThat(e, throwableWithMessage("security tokens are not enabled"));
        assertThat(e, instanceOf(FeatureNotEnabledException.class));
        // Client can check the metadata for this value, and depend on an exact string match:
        assertThat(e.getMetadata(FeatureNotEnabledException.DISABLED_FEATURE_METADATA), contains("security_tokens"));
    }

    public void testBytesKeyEqualsHashCode() {
        final int dataLength = randomIntBetween(2, 32);
        final byte[] data = randomBytes(dataLength);
        BytesKey bytesKey = new BytesKey(data);
        EqualsHashCodeTestUtils.checkEqualsAndHashCode(bytesKey, (b) -> new BytesKey(b.bytes.clone()), (b) -> {
            final byte[] copy = b.bytes.clone();
            final int randomlyChangedValue = randomIntBetween(0, copy.length - 1);
            final byte original = copy[randomlyChangedValue];
            boolean loop;
            do {
                byte value = randomByte();
                if (value == original) {
                    loop = true;
                } else {
                    loop = false;
                    copy[randomlyChangedValue] = value;
                }
            } while (loop);
            return new BytesKey(copy);
        });
    }

    public void testMalformedToken() throws Exception {
        final int numBytes = randomIntBetween(1, TokenService.MINIMUM_BYTES + 32);
        final byte[] randomBytes = new byte[numBytes];
        random().nextBytes(randomBytes);
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // mock another random token so that we don't find a token in TokenService#getUserTokenFromId
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        mockGetTokenFromId(tokenService, UUIDs.randomBase64UUID(), authentication, false);
        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, Base64.getEncoder().encodeToString(randomBytes));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());
        }
    }

    public void testNotValidPre72Tokens() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // mock another random token so that we don't find a token in TokenService#getUserTokenFromId
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        mockGetTokenFromId(tokenService, UUIDs.randomBase64UUID(), authentication, false);
        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, generateAccessToken(tokenService, Version.V_7_1_0));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());
        }
    }

    public void testNotValidAfter72Tokens() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        // mock another random token so that we don't find a token in TokenService#getUserTokenFromId
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        mockGetTokenFromId(tokenService, UUIDs.randomBase64UUID(), authentication, false);
        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, generateAccessToken(tokenService, randomFrom(Version.V_7_2_0, Version.V_7_3_2)));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());
        }
    }

    public void testIndexNotAvailable() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String userTokenId = UUIDs.randomBase64UUID();
        final String refreshToken = UUIDs.randomBase64UUID();
        tokenService.createOAuth2Tokens(userTokenId, refreshToken, authentication, authentication, Collections.emptyMap(), tokenFuture);
        final String accessToken = tokenFuture.get().getAccessToken();
        assertNotNull(accessToken);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(requestContext, accessToken);

        doAnswer(invocationOnMock -> {
            ActionListener<GetResponse> listener = (ActionListener<GetResponse>) invocationOnMock.getArguments()[1];
            listener.onFailure(new NoShardAvailableActionException(new ShardId(new Index("foo", "uuid"), 0), "shard oh shard"));
            return Void.TYPE;
        }).when(client).get(any(GetRequest.class), any(ActionListener.class));

        final SecurityIndexManager tokensIndex;
        if (oldNode != null) {
            tokensIndex = securityMainIndex;
            when(securityTokensIndex.isAvailable()).thenReturn(false);
            when(securityTokensIndex.indexExists()).thenReturn(false);
            when(securityTokensIndex.freeze()).thenReturn(securityTokensIndex);
        } else {
            tokensIndex = securityTokensIndex;
            when(securityMainIndex.isAvailable()).thenReturn(false);
            when(securityMainIndex.indexExists()).thenReturn(false);
            when(securityMainIndex.freeze()).thenReturn(securityMainIndex);
        }
        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());

            when(tokensIndex.isAvailable()).thenReturn(false);
            when(tokensIndex.getUnavailableReason()).thenReturn(new UnavailableShardsException(null, "unavailable"));
            when(tokensIndex.indexExists()).thenReturn(true);
            future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());

            when(tokensIndex.indexExists()).thenReturn(false);
            future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());

            when(tokensIndex.isAvailable()).thenReturn(true);
            when(tokensIndex.indexExists()).thenReturn(true);
            mockGetTokenFromId(tokenService, userTokenId, authentication, false);
            future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertAuthentication(future.get().getAuthentication(), authentication);
        }
    }

    public void testGetAuthenticationWorksWithExpiredUserToken() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, Clock.systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        final String userTokenId = UUIDs.randomBase64UUID();
        UserToken expired = new UserToken(userTokenId, tokenService.getTokenVersionCompatibility(), authentication,
            Instant.now().minus(3L, ChronoUnit.DAYS), Collections.emptyMap());
        mockGetTokenFromId(expired, false);
        final String accessToken = tokenService.prependVersionAndEncodeAccessToken(tokenService.getTokenVersionCompatibility(), userTokenId
        );
        PlainActionFuture<Tuple<Authentication, Map<String, Object>>> authFuture = new PlainActionFuture<>();
        tokenService.getAuthenticationAndMetadata(accessToken, authFuture);
        Authentication retrievedAuth = authFuture.actionGet().v1();
        assertAuthentication(authentication, retrievedAuth);
    }

    public void testSupercedingTokenEncryption() throws Exception {
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, Clock.systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<TokenService.CreateTokenResult> tokenFuture = new PlainActionFuture<>();
        final String refrehToken = UUIDs.randomBase64UUID();
        final String newAccessToken = UUIDs.randomBase64UUID();
        final String newRefreshToken = UUIDs.randomBase64UUID();
        final byte[] iv = tokenService.getRandomBytes(TokenService.IV_BYTES);
        final byte[] salt = tokenService.getRandomBytes(TokenService.SALT_BYTES);
        final Version version = tokenService.getTokenVersionCompatibility();
        String encryptedTokens = tokenService.encryptSupersedingTokens(newAccessToken, newRefreshToken, refrehToken, iv,
            salt);
        RefreshTokenStatus refreshTokenStatus = new RefreshTokenStatus(false,
            authentication.getUser().principal(), authentication.getAuthenticatedBy().getName(), true,
            Instant.now().minusSeconds(5L), encryptedTokens, Base64.getEncoder().encodeToString(iv),
            Base64.getEncoder().encodeToString(salt));
        refreshTokenStatus.setVersion(version);
        mockGetTokenAsyncForDecryptedToken(newAccessToken);
        tokenService.decryptAndReturnSupersedingTokens(refrehToken, refreshTokenStatus, securityTokensIndex, authentication, tokenFuture);
        if (version.onOrAfter(TokenService.VERSION_ACCESS_TOKENS_AS_UUIDS)) {
            // previous versions serialized the access token encrypted and the cipher text was different each time (due to different IVs)
            assertThat(tokenService.prependVersionAndEncodeAccessToken(version, newAccessToken),
                equalTo(tokenFuture.get().getAccessToken()));
        }
        assertThat(TokenService.prependVersionAndEncodeRefreshToken(version, newRefreshToken),
            equalTo(tokenFuture.get().getRefreshToken()));
    }

    public void testCannotValidateTokenIfLicenseDoesNotAllowTokens() throws Exception {
        when(licenseState.checkFeature(Feature.SECURITY_TOKEN_SERVICE)).thenReturn(true);
        TokenService tokenService = createTokenService(tokenServiceEnabledSettings, Clock.systemUTC());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        final String userTokenId = UUIDs.randomBase64UUID();
        UserToken token = new UserToken(userTokenId, tokenService.getTokenVersionCompatibility(), authentication,
            Instant.now().plusSeconds(180), Collections.emptyMap());
        mockGetTokenFromId(token, false);
        final String accessToken = tokenService.prependVersionAndEncodeAccessToken(tokenService.getTokenVersionCompatibility(), userTokenId
        );
        final ThreadContext threadContext = new ThreadContext(Settings.EMPTY);
        storeTokenHeader(threadContext, tokenService.prependVersionAndEncodeAccessToken(token.getVersion(), accessToken));

        PlainActionFuture<UserToken> authFuture = new PlainActionFuture<>();
        when(licenseState.checkFeature(Feature.SECURITY_TOKEN_SERVICE)).thenReturn(false);
        tokenService.getAndValidateToken(threadContext, authFuture);
        UserToken authToken = authFuture.actionGet();
        assertThat(authToken, Matchers.nullValue());
    }

    public void testHashedTokenIsUrlSafe() {
        final String hashedId = TokenService.hashTokenString(UUIDs.randomBase64UUID());
        assertEquals(hashedId, URLEncoder.encode(hashedId, StandardCharsets.UTF_8));
    }

    private TokenService createTokenService(Settings settings, Clock clock) throws GeneralSecurityException {
        return new TokenService(settings, clock, client, licenseState, securityContext, securityMainIndex, securityTokensIndex,
            clusterService);
    }

    private void mockGetTokenFromId(TokenService tokenService, String accessToken, Authentication authentication, boolean isExpired) {
        mockGetTokenFromId(tokenService, accessToken, authentication, isExpired, client);
    }

    public static void mockGetTokenFromId(TokenService tokenService, String userTokenId, Authentication authentication, boolean isExpired,
                                          Client client) {
        doAnswer(invocationOnMock -> {
            GetRequest request = (GetRequest) invocationOnMock.getArguments()[0];
            ActionListener<GetResponse> listener = (ActionListener<GetResponse>) invocationOnMock.getArguments()[1];
            GetResponse response = mock(GetResponse.class);
            Version tokenVersion = tokenService.getTokenVersionCompatibility();
            final String possiblyHashedUserTokenId;
            if (tokenVersion.onOrAfter(TokenService.VERSION_ACCESS_TOKENS_AS_UUIDS)) {
                possiblyHashedUserTokenId = TokenService.hashTokenString(userTokenId);
            } else {
                possiblyHashedUserTokenId = userTokenId;
            }
            if (possiblyHashedUserTokenId.equals(request.id().replace("token_", ""))) {
                when(response.isExists()).thenReturn(true);
                Map<String, Object> sourceMap = new HashMap<>();
                final UserToken userToken = buildUserToken(tokenService, userTokenId, authentication);
                try (XContentBuilder builder = XContentBuilder.builder(XContentType.JSON.xContent())) {
                    userToken.toXContent(builder, ToXContent.EMPTY_PARAMS);
                    Map<String, Object> accessTokenMap = new HashMap<>();
                    accessTokenMap.put("user_token",
                        XContentHelper.convertToMap(XContentType.JSON.xContent(), Strings.toString(builder), false));
                    accessTokenMap.put("invalidated", isExpired);
                    sourceMap.put("access_token", accessTokenMap);
                }
                when(response.getSource()).thenReturn(sourceMap);
            }
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).get(any(GetRequest.class), any(ActionListener.class));
    }

    protected static UserToken buildUserToken(TokenService tokenService, String userTokenId, Authentication authentication) {
        final Version tokenVersion = tokenService.getTokenVersionCompatibility();
        final String possiblyHashedUserTokenId;
        if (tokenVersion.onOrAfter(TokenService.VERSION_ACCESS_TOKENS_AS_UUIDS)) {
            possiblyHashedUserTokenId = TokenService.hashTokenString(userTokenId);
        } else {
            possiblyHashedUserTokenId = userTokenId;
        }

        final Authentication tokenAuth = new Authentication(authentication.getUser(), authentication.getAuthenticatedBy(),
            authentication.getLookedUpBy(), tokenVersion, AuthenticationType.TOKEN, authentication.getMetadata());
        final UserToken userToken = new UserToken(possiblyHashedUserTokenId, tokenVersion, tokenAuth,
            tokenService.getExpirationTime(), authentication.getMetadata());
        return userToken;
    }

    private void mockGetTokenFromId(UserToken userToken, boolean isExpired) {
        doAnswer(invocationOnMock -> {
            GetRequest request = (GetRequest) invocationOnMock.getArguments()[0];
            ActionListener<GetResponse> listener = (ActionListener<GetResponse>) invocationOnMock.getArguments()[1];
            GetResponse response = mock(GetResponse.class);
            final String possiblyHashedUserTokenId;
            if (userToken.getVersion().onOrAfter(TokenService.VERSION_ACCESS_TOKENS_AS_UUIDS)) {
                possiblyHashedUserTokenId = TokenService.hashTokenString(userToken.getId());
            } else {
                possiblyHashedUserTokenId = userToken.getId();
            }
            if (possiblyHashedUserTokenId.equals(request.id().replace("token_", ""))) {
                when(response.isExists()).thenReturn(true);
                Map<String, Object> sourceMap = new HashMap<>();
                try (XContentBuilder builder = XContentBuilder.builder(XContentType.JSON.xContent())) {
                    userToken.toXContent(builder, ToXContent.EMPTY_PARAMS);
                    Map<String, Object> accessTokenMap = new HashMap<>();
                    Map<String, Object> userTokenMap = XContentHelper.convertToMap(XContentType.JSON.xContent(),
                        Strings.toString(builder), false);
                    userTokenMap.put("id", possiblyHashedUserTokenId);
                    accessTokenMap.put("user_token", userTokenMap);
                    accessTokenMap.put("invalidated", isExpired);
                    sourceMap.put("access_token", accessTokenMap);
                }
                when(response.getSource()).thenReturn(sourceMap);
            }
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).get(any(GetRequest.class), any(ActionListener.class));
    }

    private void mockFindTokenFromRefreshToken(String refreshToken, UserToken userToken, @Nullable RefreshTokenStatus refreshTokenStatus) {
        String storedRefreshToken;
        if (userToken.getVersion().onOrAfter(TokenService.VERSION_HASHED_TOKENS)) {
            storedRefreshToken = TokenService.hashTokenString(refreshToken);
        } else {
            storedRefreshToken = refreshToken;
        }
        doAnswer(invocationOnMock -> {
            final SearchRequest request = (SearchRequest) invocationOnMock.getArguments()[0];
            final ActionListener<SearchResponse> listener = (ActionListener<SearchResponse>) invocationOnMock.getArguments()[1];
            final SearchResponse response = mock(SearchResponse.class);

            assertThat(request.source().query(), instanceOf(BoolQueryBuilder.class));
            BoolQueryBuilder bool = (BoolQueryBuilder) request.source().query();
            assertThat(bool.filter(), hasSize(2));

            assertThat(bool.filter().get(0), instanceOf(TermQueryBuilder.class));
            TermQueryBuilder docType = (TermQueryBuilder) bool.filter().get(0);
            assertThat(docType.fieldName(), is("doc_type"));
            assertThat(docType.value(), is("token"));

            assertThat(bool.filter().get(1), instanceOf(TermQueryBuilder.class));
            TermQueryBuilder refreshFilter = (TermQueryBuilder) bool.filter().get(1);
            assertThat(refreshFilter.fieldName(), is("refresh_token.token"));
            assertThat(refreshFilter.value(), is(storedRefreshToken));

            final RealmRef realmRef = new RealmRef(
                refreshTokenStatus == null ? randomAlphaOfLength(6) : refreshTokenStatus.getAssociatedRealm(),
                "test",
                randomAlphaOfLength(12));
            final Authentication clientAuthentication = new Authentication(
                new User(refreshTokenStatus == null ? randomAlphaOfLength(8) : refreshTokenStatus.getAssociatedUser()),
                realmRef, realmRef);

            final SearchHit hit = new SearchHit(randomInt(), "token_" + TokenService.hashTokenString(userToken.getId()), null, null);
            BytesReference source = TokenService.createTokenDocument(userToken, storedRefreshToken, clientAuthentication, Instant.now());
            if (refreshTokenStatus != null) {
                var sourceAsMap = XContentHelper.convertToMap(source, false, XContentType.JSON).v2();
                var refreshTokenSource = (Map<String, Object>) sourceAsMap.get("refresh_token");
                refreshTokenSource.put("invalidated", refreshTokenStatus.isInvalidated());
                refreshTokenSource.put("refreshed", refreshTokenStatus.isRefreshed());
                source = XContentTestUtils.convertToXContent(sourceAsMap, XContentType.JSON);
            }
            hit.sourceRef(source);

            final SearchHits hits = new SearchHits(new SearchHit[]{hit}, null, 1);
            when(response.getHits()).thenReturn(hits);
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).search(any(SearchRequest.class), any(ActionListener.class));
    }

    private void mockGetTokenAsyncForDecryptedToken(String accessToken) {
        doAnswer(invocationOnMock -> {
            GetRequest request = (GetRequest) invocationOnMock.getArguments()[0];
            ActionListener<GetResponse> listener = (ActionListener<GetResponse>) invocationOnMock.getArguments()[1];
            GetResponse response = mock(GetResponse.class);
            if (request.id().replace("token_", "").equals(TokenService.hashTokenString(accessToken))) {
                when(response.isExists()).thenReturn(true);
            }
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).get(any(GetRequest.class), any(ActionListener.class));
    }

    public static void assertAuthentication(Authentication result, Authentication expected) {
        assertEquals(expected.getUser(), result.getUser());
        assertEquals(expected.getAuthenticatedBy(), result.getAuthenticatedBy());
        assertEquals(expected.getLookedUpBy(), result.getLookedUpBy());
        assertEquals(expected.getMetadata(), result.getMetadata());
    }

    private DiscoveryNode addAnotherDataNodeWithVersion(ClusterService clusterService, Version version) {
        final ClusterState currentState = clusterService.state();
        final DiscoveryNodes.Builder discoBuilder = DiscoveryNodes.builder(currentState.getNodes());
        final DiscoveryNode anotherDataNode = new DiscoveryNode("another_data_node#" + version, buildNewFakeTransportAddress(),
            Collections.emptyMap(), Collections.singleton(DiscoveryNodeRole.DATA_ROLE), version);
        discoBuilder.add(anotherDataNode);
        final ClusterState.Builder newStateBuilder = ClusterState.builder(currentState);
        newStateBuilder.nodes(discoBuilder);
        setState(clusterService, newStateBuilder.build());
        return anotherDataNode;
    }

    private String generateAccessToken(TokenService tokenService, Version version) throws Exception {
        String accessTokenString = UUIDs.randomBase64UUID();
        if (version.onOrAfter(TokenService.VERSION_ACCESS_TOKENS_AS_UUIDS)) {
            accessTokenString = TokenService.hashTokenString(accessTokenString);
        }
        return tokenService.prependVersionAndEncodeAccessToken(version, accessTokenString);
    }

}
