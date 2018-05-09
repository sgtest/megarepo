/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.NoShardAvailableActionException;
import org.elasticsearch.action.get.GetAction;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.get.GetRequestBuilder;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.get.MultiGetAction;
import org.elasticsearch.action.get.MultiGetItemResponse;
import org.elasticsearch.action.get.MultiGetRequest;
import org.elasticsearch.action.get.MultiGetRequestBuilder;
import org.elasticsearch.action.get.MultiGetResponse;
import org.elasticsearch.action.index.IndexAction;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.update.UpdateAction;
import org.elasticsearch.action.update.UpdateRequestBuilder;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.collect.MapBuilder;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.node.Node;
import org.elasticsearch.test.ClusterServiceUtils;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.EqualsHashCodeTestUtils;
import org.elasticsearch.threadpool.FixedExecutorBuilder;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.Authentication.RealmRef;
import org.elasticsearch.xpack.core.security.authc.TokenMetaData;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.core.watcher.watch.ClockMock;
import org.elasticsearch.xpack.security.SecurityLifecycleService;
import org.elasticsearch.xpack.security.support.SecurityIndexManager;
import org.junit.AfterClass;
import org.junit.Before;
import org.junit.BeforeClass;

import javax.crypto.SecretKey;

import java.io.IOException;
import java.time.Clock;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Base64;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.function.Consumer;

import static java.time.Clock.systemUTC;
import static org.elasticsearch.repositories.ESBlobStoreTestCase.randomBytes;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.notNullValue;
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
    private SecurityLifecycleService lifecycleService;
    private SecurityIndexManager securityIndex;
    private ClusterService clusterService;
    private Settings tokenServiceEnabledSettings = Settings.builder()
        .put(XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.getKey(), true).build();

    @Before
    public void setupClient() {
        client = mock(Client.class);
        when(client.threadPool()).thenReturn(threadPool);
        when(client.settings()).thenReturn(settings);
        doAnswer(invocationOnMock -> {
            GetRequestBuilder builder = new GetRequestBuilder(client, GetAction.INSTANCE);
            builder.setIndex((String) invocationOnMock.getArguments()[0])
                    .setType((String) invocationOnMock.getArguments()[1])
                    .setId((String) invocationOnMock.getArguments()[2]);
            return builder;
        }).when(client).prepareGet(anyString(), anyString(), anyString());
        when(client.prepareMultiGet()).thenReturn(new MultiGetRequestBuilder(client, MultiGetAction.INSTANCE));
        doAnswer(invocationOnMock -> {
            ActionListener<MultiGetResponse> listener = (ActionListener<MultiGetResponse>) invocationOnMock.getArguments()[1];
            MultiGetResponse response = mock(MultiGetResponse.class);
            MultiGetItemResponse[] responses = new MultiGetItemResponse[2];
            when(response.getResponses()).thenReturn(responses);

            GetResponse oldGetResponse = mock(GetResponse.class);
            when(oldGetResponse.isExists()).thenReturn(false);
            responses[0] = new MultiGetItemResponse(oldGetResponse, null);

            GetResponse getResponse = mock(GetResponse.class);
            responses[1] = new MultiGetItemResponse(getResponse, null);
            when(getResponse.isExists()).thenReturn(false);
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).multiGet(any(MultiGetRequest.class), any(ActionListener.class));
        when(client.prepareIndex(any(String.class), any(String.class), any(String.class)))
                .thenReturn(new IndexRequestBuilder(client, IndexAction.INSTANCE));
        when(client.prepareUpdate(any(String.class), any(String.class), any(String.class)))
                .thenReturn(new UpdateRequestBuilder(client, UpdateAction.INSTANCE));
        doAnswer(invocationOnMock -> {
            ActionListener<IndexResponse> responseActionListener = (ActionListener<IndexResponse>) invocationOnMock.getArguments()[2];
            responseActionListener.onResponse(new IndexResponse());
            return null;
        }).when(client).execute(eq(IndexAction.INSTANCE), any(IndexRequest.class), any(ActionListener.class));

        // setup lifecycle service
        lifecycleService = mock(SecurityLifecycleService.class);
        securityIndex = mock(SecurityIndexManager.class);
        when(lifecycleService.securityIndex()).thenReturn(securityIndex);
        doAnswer(invocationOnMock -> {
            Runnable runnable = (Runnable) invocationOnMock.getArguments()[1];
            runnable.run();
            return null;
        }).when(securityIndex).prepareIndexIfNeededThenExecute(any(Consumer.class), any(Runnable.class));
        this.clusterService = ClusterServiceUtils.createClusterService(threadPool);
    }

    @BeforeClass
    public static void startThreadPool() throws IOException {
        threadPool = new ThreadPool(settings,
                new FixedExecutorBuilder(settings, TokenService.THREAD_POOL_NAME, 1, 1000, "xpack.security.authc.token.thread_pool"));
        new Authentication(new User("foo"), new RealmRef("realm", "type", "node"), null).writeToContext(threadPool.getThreadContext());
    }

    @AfterClass
    public static void shutdownThreadpool() throws InterruptedException {
        terminate(threadPool);
        threadPool = null;
    }

    public void testAttachAndGetToken() throws Exception {
        TokenService tokenService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        assertNotNull(token);
        mockGetTokenFromId(token);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // verify a second separate token service with its own salt can also verify
            TokenService anotherService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService
                    , clusterService);
            anotherService.refreshMetaData(tokenService.getTokenMetaData());
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            anotherService.getAndValidateToken(requestContext, future);
            UserToken fromOtherService = future.get();
            assertEquals(authentication, fromOtherService.getAuthentication());
        }
    }

    public void testRotateKey() throws Exception {
        TokenService tokenService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        assertNotNull(token);
        mockGetTokenFromId(token);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }
        rotateKeys(tokenService);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }

        PlainActionFuture<Tuple<UserToken, String>> newTokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, newTokenFuture, Collections.emptyMap());
        final UserToken newToken = newTokenFuture.get().v1();
        assertNotNull(newToken);
        assertNotEquals(tokenService.getUserTokenString(newToken), tokenService.getUserTokenString(token));

        requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(newToken));
        mockGetTokenFromId(newToken);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }
    }

    private void rotateKeys(TokenService tokenService) {
        TokenMetaData tokenMetaData = tokenService.generateSpareKey();
        tokenService.refreshMetaData(tokenMetaData);
        tokenMetaData = tokenService.rotateToSpareKey();
        tokenService.refreshMetaData(tokenMetaData);
    }

    public void testKeyExchange() throws Exception {
        TokenService tokenService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        int numRotations = 0;randomIntBetween(1, 5);
        for (int i = 0; i < numRotations; i++) {
            rotateKeys(tokenService);
        }
        TokenService otherTokenService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService,
                clusterService);
        otherTokenService.refreshMetaData(tokenService.getTokenMetaData());
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        assertNotNull(token);
        mockGetTokenFromId(token);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));
        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            otherTokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }

        rotateKeys(tokenService);

        otherTokenService.refreshMetaData(tokenService.getTokenMetaData());

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            otherTokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }
    }

    public void testPruneKeys() throws Exception {
        TokenService tokenService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        assertNotNull(token);
        mockGetTokenFromId(token);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }
        TokenMetaData metaData = tokenService.pruneKeys(randomIntBetween(0, 100));
        tokenService.refreshMetaData(metaData);

        int numIterations = scaledRandomIntBetween(1, 5);
        for (int i = 0; i < numIterations; i++) {
            rotateKeys(tokenService);
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }

        PlainActionFuture<Tuple<UserToken, String>> newTokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, newTokenFuture, Collections.emptyMap());
        final UserToken newToken = newTokenFuture.get().v1();
        assertNotNull(newToken);
        assertNotEquals(tokenService.getUserTokenString(newToken), tokenService.getUserTokenString(token));

        metaData = tokenService.pruneKeys(1);
        tokenService.refreshMetaData(metaData);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertNull(serialized);
        }

        requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(newToken));
        mockGetTokenFromId(newToken);
        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }

    }

    public void testPassphraseWorks() throws Exception {
        TokenService tokenService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        assertNotNull(token);
        mockGetTokenFromId(token);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // verify a second separate token service with its own passphrase cannot verify
            TokenService anotherService = new TokenService(Settings.EMPTY, systemUTC(), client, lifecycleService,
                    clusterService);
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            anotherService.getAndValidateToken(requestContext, future);
            assertNull(future.get());
        }
    }

    public void testGetTokenWhenKeyCacheHasExpired() throws Exception {
        TokenService tokenService = new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);

        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        UserToken token = tokenFuture.get().v1();
        assertThat(tokenService.getUserTokenString(token), notNullValue());

        tokenService.clearActiveKeyCache();
        assertThat(tokenService.getUserTokenString(token), notNullValue());
    }

    public void testInvalidatedToken() throws Exception {
        when(securityIndex.indexExists()).thenReturn(true);
        TokenService tokenService =
            new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        assertNotNull(token);
        doAnswer(invocationOnMock -> {
            ActionListener<MultiGetResponse> listener = (ActionListener<MultiGetResponse>) invocationOnMock.getArguments()[1];
            MultiGetResponse response = mock(MultiGetResponse.class);
            MultiGetItemResponse[] responses = new MultiGetItemResponse[2];
            when(response.getResponses()).thenReturn(responses);

            final boolean newExpired = randomBoolean();
            GetResponse oldGetResponse = mock(GetResponse.class);
            when(oldGetResponse.isExists()).thenReturn(newExpired == false);
            responses[0] = new MultiGetItemResponse(oldGetResponse, null);

            GetResponse getResponse = mock(GetResponse.class);
            responses[1] = new MultiGetItemResponse(getResponse, null);
            when(getResponse.isExists()).thenReturn(newExpired);
            if (newExpired) {
                Map<String, Object> source = MapBuilder.<String, Object>newMapBuilder()
                        .put("access_token", Collections.singletonMap("invalidated", true))
                        .immutableMap();
                when(getResponse.getSource()).thenReturn(source);
            }
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).multiGet(any(MultiGetRequest.class), any(ActionListener.class));

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));
        mockGetTokenFromId(token);

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, future::actionGet);
            final String headerValue = e.getHeader("WWW-Authenticate").get(0);
            assertThat(headerValue, containsString("Bearer realm="));
            assertThat(headerValue, containsString("expired"));
        }
    }

    public void testComputeSecretKeyIsConsistent() throws Exception {
        byte[] saltArr = new byte[32];
        random().nextBytes(saltArr);
        SecretKey key = TokenService.computeSecretKey("some random passphrase".toCharArray(), saltArr);
        SecretKey key2 = TokenService.computeSecretKey("some random passphrase".toCharArray(), saltArr);
        assertArrayEquals(key.getEncoded(), key2.getEncoded());
    }

    public void testTokenExpiry() throws Exception {
        ClockMock clock = ClockMock.frozen();
        TokenService tokenService = new TokenService(tokenServiceEnabledSettings, clock, client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        mockGetTokenFromId(token);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // the clock is still frozen, so the cookie should be valid
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertEquals(authentication, future.get().getAuthentication());
        }

        final TimeValue defaultExpiration = TokenService.TOKEN_EXPIRATION.get(Settings.EMPTY);
        final int fastForwardAmount = randomIntBetween(1, Math.toIntExact(defaultExpiration.getSeconds()));
        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // move the clock forward but don't go to expiry
            clock.fastForwardSeconds(fastForwardAmount);
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertEquals(authentication, future.get().getAuthentication());
        }

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            // move to expiry
            clock.fastForwardSeconds(Math.toIntExact(defaultExpiration.getSeconds()) - fastForwardAmount);
            clock.rewind(TimeValue.timeValueNanos(clock.instant().getNano())); // trim off nanoseconds since don't store them in the index
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertEquals(authentication, future.get().getAuthentication());
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
                Clock.systemUTC(), client, lifecycleService, clusterService);
        IllegalStateException e = expectThrows(IllegalStateException.class, () -> tokenService.createUserToken(null, null, null, null));
        assertEquals("tokens are not enabled", e.getMessage());

        PlainActionFuture<UserToken> future = new PlainActionFuture<>();
        tokenService.getAndValidateToken(null, future);
        assertNull(future.get());

        e = expectThrows(IllegalStateException.class, () -> {
            PlainActionFuture<Boolean> invalidateFuture = new PlainActionFuture<>();
            tokenService.invalidateAccessToken((String) null, invalidateFuture);
            invalidateFuture.actionGet();
        });
        assertEquals("tokens are not enabled", e.getMessage());
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
        TokenService tokenService = new TokenService(Settings.EMPTY, systemUTC(), client, lifecycleService, clusterService);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + Base64.getEncoder().encodeToString(randomBytes));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());
        }
    }

    public void testIndexNotAvailable() throws Exception {
        TokenService tokenService =
            new TokenService(tokenServiceEnabledSettings, systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        PlainActionFuture<Tuple<UserToken, String>> tokenFuture = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, tokenFuture, Collections.emptyMap());
        final UserToken token = tokenFuture.get().v1();
        assertNotNull(token);
        mockGetTokenFromId(token);

        ThreadContext requestContext = new ThreadContext(Settings.EMPTY);
        requestContext.putHeader("Authorization", "Bearer " + tokenService.getUserTokenString(token));

        doAnswer(invocationOnMock -> {
            ActionListener<GetResponse> listener = (ActionListener<GetResponse>) invocationOnMock.getArguments()[1];
            listener.onFailure(new NoShardAvailableActionException(new ShardId(new Index("foo", "uuid"), 0), "shard oh shard"));
            return Void.TYPE;
        }).when(client).multiGet(any(MultiGetRequest.class), any(ActionListener.class));

        try (ThreadContext.StoredContext ignore = requestContext.newStoredContext(true)) {
            PlainActionFuture<UserToken> future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            UserToken serialized = future.get();
            assertEquals(authentication, serialized.getAuthentication());

            when(securityIndex.isAvailable()).thenReturn(false);
            when(securityIndex.indexExists()).thenReturn(true);
            future = new PlainActionFuture<>();
            tokenService.getAndValidateToken(requestContext, future);
            assertNull(future.get());
        }
    }

    public void testGetAuthenticationWorksWithExpiredToken() throws Exception {
        TokenService tokenService =
                new TokenService(tokenServiceEnabledSettings, Clock.systemUTC(), client, lifecycleService, clusterService);
        Authentication authentication = new Authentication(new User("joe", "admin"), new RealmRef("native_realm", "native", "node1"), null);
        UserToken expired = new UserToken(authentication, Instant.now().minus(3L, ChronoUnit.DAYS));
        mockGetTokenFromId(expired);
        String userTokenString = tokenService.getUserTokenString(expired);
        PlainActionFuture<Tuple<Authentication, Map<String, Object>>> authFuture = new PlainActionFuture<>();
        tokenService.getAuthenticationAndMetaData(userTokenString, authFuture);
        Authentication retrievedAuth = authFuture.actionGet().v1();
        assertEquals(authentication, retrievedAuth);
    }

    private void mockGetTokenFromId(UserToken userToken) {
        mockGetTokenFromId(userToken, client);
    }

    public static void mockGetTokenFromId(UserToken userToken, Client client) {
        doAnswer(invocationOnMock -> {
            GetRequest getRequest = (GetRequest) invocationOnMock.getArguments()[0];
            ActionListener<GetResponse> getResponseListener = (ActionListener<GetResponse>) invocationOnMock.getArguments()[1];
            GetResponse getResponse = mock(GetResponse.class);
            if (userToken.getId().equals(getRequest.id().replace("token_", ""))) {
                when(getResponse.isExists()).thenReturn(true);
                Map<String, Object> sourceMap = new HashMap<>();
                try (XContentBuilder builder = XContentBuilder.builder(XContentType.JSON.xContent())) {
                    userToken.toXContent(builder, ToXContent.EMPTY_PARAMS);
                    sourceMap.put("access_token",
                            Collections.singletonMap("user_token",
                                    XContentHelper.convertToMap(XContentType.JSON.xContent(), Strings.toString(builder), false)));
                }
                when(getResponse.getSource()).thenReturn(sourceMap);
            }
            getResponseListener.onResponse(getResponse);
            return Void.TYPE;
        }).when(client).get(any(GetRequest.class), any(ActionListener.class));
    }
}
