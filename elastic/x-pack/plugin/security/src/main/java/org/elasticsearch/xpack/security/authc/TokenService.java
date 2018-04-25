/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc;

import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.BytesRefBuilder;
import org.elasticsearch.core.internal.io.IOUtils;
import org.apache.lucene.util.StringHelper;
import org.apache.lucene.util.UnicodeUtil;
import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.DocWriteRequest.OpType;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.get.MultiGetItemResponse;
import org.elasticsearch.action.get.MultiGetRequest;
import org.elasticsearch.action.get.MultiGetResponse;
import org.elasticsearch.action.index.IndexAction;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.ContextPreservingActionListener;
import org.elasticsearch.action.support.WriteRequest.RefreshPolicy;
import org.elasticsearch.action.support.master.AcknowledgedRequest;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.AckedClusterStateUpdateTask;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ack.AckedRequest;
import org.elasticsearch.cluster.ack.ClusterStateUpdateResponse;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.cache.Cache;
import org.elasticsearch.common.cache.CacheBuilder;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.component.AbstractComponent;
import org.elasticsearch.common.hash.MessageDigests;
import org.elasticsearch.common.io.stream.InputStreamStreamInput;
import org.elasticsearch.common.io.stream.OutputStreamStreamOutput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Setting.Property;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.util.iterable.Iterables;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.index.engine.DocumentMissingException;
import org.elasticsearch.index.engine.VersionConflictEngineException;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.security.ScrollHelper;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.KeyAndTimestamp;
import org.elasticsearch.xpack.core.security.authc.TokenMetaData;
import org.elasticsearch.xpack.security.SecurityLifecycleService;

import javax.crypto.Cipher;
import javax.crypto.CipherInputStream;
import javax.crypto.CipherOutputStream;
import javax.crypto.NoSuchPaddingException;
import javax.crypto.SecretKey;
import javax.crypto.SecretKeyFactory;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.PBEKeySpec;
import javax.crypto.spec.SecretKeySpec;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.Closeable;
import java.io.IOException;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.security.SecureRandom;
import java.security.spec.InvalidKeySpecException;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.temporal.ChronoUnit;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Supplier;

import static org.elasticsearch.action.support.TransportActions.isShardNotAvailableException;
import static org.elasticsearch.gateway.GatewayService.STATE_NOT_RECOVERED_BLOCK;
import static org.elasticsearch.xpack.core.ClientHelper.SECURITY_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

/**
 * Service responsible for the creation, validation, and other management of {@link UserToken}
 * objects for authentication
 */
public final class TokenService extends AbstractComponent {

    /**
     * The parameters below are used to generate the cryptographic key that is used to encrypt the
     * values returned by this service. These parameters are based off of the
     * <a href="https://www.owasp.org/index.php/Password_Storage_Cheat_Sheet">OWASP Password Storage
     * Cheat Sheet</a> and the <a href="https://pages.nist.gov/800-63-3/sp800-63b.html#sec5">
     * NIST Digital Identity Guidelines</a>
     */
    private static final int ITERATIONS = 100000;
    private static final String KDF_ALGORITHM = "PBKDF2withHMACSHA512";
    private static final int SALT_BYTES = 32;
    private static final int KEY_BYTES = 64;
    private static final int IV_BYTES = 12;
    private static final int VERSION_BYTES = 4;
    private static final String ENCRYPTION_CIPHER = "AES/GCM/NoPadding";
    private static final String EXPIRED_TOKEN_WWW_AUTH_VALUE = "Bearer realm=\"" + XPackField.SECURITY +
            "\", error=\"invalid_token\", error_description=\"The access token expired\"";
    private static final String MALFORMED_TOKEN_WWW_AUTH_VALUE = "Bearer realm=\"" + XPackField.SECURITY +
            "\", error=\"invalid_token\", error_description=\"The access token is malformed\"";
    private static final String TYPE = "doc";

    public static final String THREAD_POOL_NAME = XPackField.SECURITY + "-token-key";
    public static final Setting<TimeValue> TOKEN_EXPIRATION = Setting.timeSetting("xpack.security.authc.token.timeout",
            TimeValue.timeValueMinutes(20L), TimeValue.timeValueSeconds(1L), Property.NodeScope);
    public static final Setting<TimeValue> DELETE_INTERVAL = Setting.timeSetting("xpack.security.authc.token.delete.interval",
            TimeValue.timeValueMinutes(30L), Property.NodeScope);
    public static final Setting<TimeValue> DELETE_TIMEOUT = Setting.timeSetting("xpack.security.authc.token.delete.timeout",
            TimeValue.MINUS_ONE, Property.NodeScope);

    static final String INVALIDATED_TOKEN_DOC_TYPE = "invalidated-token";
    static final int MINIMUM_BYTES = VERSION_BYTES + SALT_BYTES + IV_BYTES + 1;
    private static final int MINIMUM_BASE64_BYTES = Double.valueOf(Math.ceil((4 * MINIMUM_BYTES) / 3)).intValue();

    private final SecureRandom secureRandom = new SecureRandom();
    private final ClusterService clusterService;
    private final Clock clock;
    private final TimeValue expirationDelay;
    private final TimeValue deleteInterval;
    private final Client client;
    private final SecurityLifecycleService lifecycleService;
    private final ExpiredTokenRemover expiredTokenRemover;
    private final boolean enabled;
    private volatile TokenKeys keyCache;
    private volatile long lastExpirationRunMs;
    private final AtomicLong createdTimeStamps = new AtomicLong(-1);

    /**
     * Creates a new token service
     *
     * @param settings the node settings
     * @param clock    the clock that will be used for comparing timestamps
     * @param client   the client to use when checking for revocations
     */
    public TokenService(Settings settings, Clock clock, Client client,
                        SecurityLifecycleService lifecycleService, ClusterService clusterService) throws GeneralSecurityException {
        super(settings);
        byte[] saltArr = new byte[SALT_BYTES];
        secureRandom.nextBytes(saltArr);

        final SecureString tokenPassphrase = generateTokenKey();
        this.clock = clock.withZone(ZoneOffset.UTC);
        this.expirationDelay = TOKEN_EXPIRATION.get(settings);
        this.client = client;
        this.lifecycleService = lifecycleService;
        this.lastExpirationRunMs = client.threadPool().relativeTimeInMillis();
        this.deleteInterval = DELETE_INTERVAL.get(settings);
        this.enabled = isTokenServiceEnabled(settings);
        this.expiredTokenRemover = new ExpiredTokenRemover(settings, client);
        ensureEncryptionCiphersSupported();
        KeyAndCache keyAndCache = new KeyAndCache(new KeyAndTimestamp(tokenPassphrase, createdTimeStamps.incrementAndGet()),
                new BytesKey(saltArr));
        keyCache = new TokenKeys(Collections.singletonMap(keyAndCache.getKeyHash(), keyAndCache), keyAndCache.getKeyHash());
        this.clusterService = clusterService;
        initialize(clusterService);
        getTokenMetaData();
    }

    public static Boolean isTokenServiceEnabled(Settings settings) {
        return XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.get(settings);
    }

    /**
     * Create a token based on the provided authentication and metadata.
     * The created token will be stored in the security index.
     */
    public void createUserToken(Authentication authentication, Authentication originatingClientAuth,
                                ActionListener<Tuple<UserToken, String>> listener, Map<String, Object> metadata) throws IOException {
        ensureEnabled();
        if (authentication == null) {
            listener.onFailure(new IllegalArgumentException("authentication must be provided"));
        } else if (originatingClientAuth == null) {
            listener.onFailure(new IllegalArgumentException("originating client authentication must be provided"));
        } else {
            final Instant created = clock.instant();
            final Instant expiration = getExpirationTime(created);
            final Version version = clusterService.state().nodes().getMinNodeVersion();
            final Authentication matchingVersionAuth = version.equals(authentication.getVersion()) ? authentication :
                    new Authentication(authentication.getUser(), authentication.getAuthenticatedBy(), authentication.getLookedUpBy(),
                            version);
            final UserToken userToken = new UserToken(version, matchingVersionAuth, expiration, metadata);
            final String refreshToken = UUIDs.randomBase64UUID();

            try (XContentBuilder builder = XContentFactory.jsonBuilder()) {
                builder.startObject();
                builder.field("doc_type", "token");
                builder.field("creation_time", created.toEpochMilli());
                builder.startObject("refresh_token")
                        .field("token", refreshToken)
                        .field("invalidated", false)
                        .field("refreshed", false)
                        .startObject("client")
                            .field("type", "unassociated_client")
                            .field("user", originatingClientAuth.getUser().principal())
                            .field("realm", originatingClientAuth.getAuthenticatedBy().getName())
                        .endObject()
                        .endObject();
                builder.startObject("access_token")
                        .field("invalidated", false)
                        .field("user_token", userToken)
                        .field("realm", authentication.getAuthenticatedBy().getName())
                        .endObject();
                builder.endObject();
                IndexRequest request =
                        client.prepareIndex(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, getTokenDocumentId(userToken))
                                .setOpType(OpType.CREATE)
                                .setSource(builder)
                                .setRefreshPolicy(RefreshPolicy.WAIT_UNTIL)
                                .request();
                lifecycleService.prepareIndexIfNeededThenExecute(listener::onFailure, () ->
                        executeAsyncWithOrigin(client, SECURITY_ORIGIN, IndexAction.INSTANCE, request,
                                ActionListener.wrap(indexResponse -> listener.onResponse(new Tuple<>(userToken, refreshToken)),
                                        listener::onFailure))
                );
            }
        }
    }

    /**
     * Looks in the context to see if the request provided a header with a user token and if so the
     * token is validated, which includes authenticated decryption and verification that the token
     * has not been revoked or is expired.
     */
    void getAndValidateToken(ThreadContext ctx, ActionListener<UserToken> listener) {
        if (enabled) {
            final String token = getFromHeader(ctx);
            if (token == null) {
                listener.onResponse(null);
            } else {
                try {
                    decodeAndValidateToken(token, ActionListener.wrap(listener::onResponse, e -> {
                        if (e instanceof IOException) {
                            // could happen with a token that is not ours
                            logger.debug("invalid token", e);
                            listener.onResponse(null);
                        } else {
                            listener.onFailure(e);
                        }
                    }));
                } catch (IOException e) {
                    // could happen with a token that is not ours
                    logger.debug("invalid token", e);
                    listener.onResponse(null);
                }
            }
        } else {
            listener.onResponse(null);
        }
    }

    /**
     * Reads the authentication and metadata from the given token.
     * This method does not validate whether the token is expired or not.
     */
    public void getAuthenticationAndMetaData(String token, ActionListener<Tuple<Authentication, Map<String, Object>>> listener)
            throws IOException {
        decodeToken(token, ActionListener.wrap(
                userToken -> {
                    if (userToken == null) {
                        listener.onFailure(new ElasticsearchSecurityException("supplied token is not valid"));
                    } else {
                        listener.onResponse(new Tuple<>(userToken.getAuthentication(), userToken.getMetadata()));
                    }
                },
                listener::onFailure
        ));
    }

    private void decodeAndValidateToken(String token, ActionListener<UserToken> listener) throws IOException {
        decodeToken(token, ActionListener.wrap(userToken -> {
            if (userToken != null) {
                Instant currentTime = clock.instant();
                if (currentTime.isAfter(userToken.getExpirationTime())) {
                    // token expired
                    listener.onFailure(expiredTokenException());
                } else {
                    checkIfTokenIsRevoked(userToken, listener);
                }
            } else {
                listener.onResponse(null);
            }
        }, listener::onFailure));
    }

    /*
     * Asynchronously decodes the string representation of a {@link UserToken}. The process for
     * this is asynchronous as we may need to compute a key, which can be computationally expensive
     * so this should not block the current thread, which is typically a network thread. A second
     * reason for being asynchronous is that we can restrain the amount of resources consumed by
     * the key computation to a single thread.
     */
    void decodeToken(String token, ActionListener<UserToken> listener) throws IOException {
        // We intentionally do not use try-with resources since we need to keep the stream open if we need to compute a key!
        byte[] bytes = token.getBytes(StandardCharsets.UTF_8);
        StreamInput in = new InputStreamStreamInput(Base64.getDecoder().wrap(new ByteArrayInputStream(bytes)), bytes.length);
        if (in.available() < MINIMUM_BASE64_BYTES) {
            logger.debug("invalid token");
            listener.onResponse(null);
        } else {
            // the token exists and the value is at least as long as we'd expect
            final Version version = Version.readVersion(in);
            in.setVersion(version);
            final BytesKey decodedSalt = new BytesKey(in.readByteArray());
            final BytesKey passphraseHash = new BytesKey(in.readByteArray());
            KeyAndCache keyAndCache = keyCache.get(passphraseHash);
            if (keyAndCache != null) {
                getKeyAsync(decodedSalt, keyAndCache, ActionListener.wrap(decodeKey -> {
                    try {
                        final byte[] iv = in.readByteArray();
                        final Cipher cipher = getDecryptionCipher(iv, decodeKey, version, decodedSalt);
                        if (version.onOrAfter(Version.V_6_2_0)) {
                            // we only have the id and need to get the token from the doc!
                            decryptTokenId(in, cipher, version, ActionListener.wrap(tokenId ->
                                lifecycleService.prepareIndexIfNeededThenExecute(listener::onFailure, () -> {
                                    final GetRequest getRequest =
                                            client.prepareGet(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE,
                                                    getTokenDocumentId(tokenId)).request();
                                    executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN, getRequest,
                                            ActionListener.<GetResponse>wrap(response -> {
                                                if (response.isExists()) {
                                                    Map<String, Object> accessTokenSource =
                                                            (Map<String, Object>) response.getSource().get("access_token");
                                                    if (accessTokenSource == null) {
                                                        listener.onFailure(new IllegalStateException("token document is missing " +
                                                                "the access_token field"));
                                                    } else if (accessTokenSource.containsKey("user_token") == false) {
                                                        listener.onFailure(new IllegalStateException("token document is missing " +
                                                                "the user_token field"));
                                                    } else {
                                                        Map<String, Object> userTokenSource =
                                                                (Map<String, Object>) accessTokenSource.get("user_token");
                                                        listener.onResponse(UserToken.fromSourceMap(userTokenSource));
                                                    }
                                                } else {
                                                    listener.onFailure(
                                                            new IllegalStateException("token document is missing and must be present"));
                                                }
                                            }, e -> {
                                                // if the index or the shard is not there / available we assume that
                                                // the token is not valid
                                                if (isShardNotAvailableException(e)) {
                                                    logger.warn("failed to get token [{}] since index is not available", tokenId);
                                                    listener.onResponse(null);
                                                } else {
                                                    logger.error(new ParameterizedMessage("failed to get token [{}]", tokenId), e);
                                                    listener.onFailure(e);
                                                }
                                            }), client::get);
                                }), listener::onFailure));
                        } else {
                            decryptToken(in, cipher, version, listener);
                        }
                    } catch (GeneralSecurityException e) {
                        // could happen with a token that is not ours
                        logger.warn("invalid token", e);
                        listener.onResponse(null);
                    } finally {
                        in.close();
                    }
                }, e -> {
                    IOUtils.closeWhileHandlingException(in);
                    listener.onFailure(e);
                }));
            } else {
                IOUtils.closeWhileHandlingException(in);
                logger.debug("invalid key {} key: {}", passphraseHash, keyCache.cache.keySet());
                listener.onResponse(null);
            }
        }
    }

    private void getKeyAsync(BytesKey decodedSalt, KeyAndCache keyAndCache, ActionListener<SecretKey> listener) {
        final SecretKey decodeKey = keyAndCache.getKey(decodedSalt);
        if (decodeKey != null) {
            listener.onResponse(decodeKey);
        } else {
            /* As a measure of protected against DOS, we can pass requests requiring a key
             * computation off to a single thread executor. For normal usage, the initial
             * request(s) that require a key computation will be delayed and there will be
             * some additional latency.
             */
            client.threadPool().executor(THREAD_POOL_NAME)
                    .submit(new KeyComputingRunnable(decodedSalt, listener, keyAndCache));
        }
    }

    private static void decryptToken(StreamInput in, Cipher cipher, Version version, ActionListener<UserToken> listener) throws
            IOException {
        try (CipherInputStream cis = new CipherInputStream(in, cipher); StreamInput decryptedInput = new InputStreamStreamInput(cis)) {
            decryptedInput.setVersion(version);
            listener.onResponse(new UserToken(decryptedInput));
        }
    }

    private static void decryptTokenId(StreamInput in, Cipher cipher, Version version, ActionListener<String> listener) throws IOException {
        try (CipherInputStream cis = new CipherInputStream(in, cipher); StreamInput decryptedInput = new InputStreamStreamInput(cis)) {
            decryptedInput.setVersion(version);
            listener.onResponse(decryptedInput.readString());
        }
    }

    /**
     * This method performs the steps necessary to invalidate a token so that it may no longer be
     * used. The process of invalidation involves a step that is needed for backwards compatibility
     * with versions prior to 6.2.0; this step records an entry to indicate that a token with a
     * given id has been expired. The second step is to record the invalidation for tokens that
     * have been created on versions on or after 6.2; this step involves performing an update to
     * the token document and setting the <code>invalidated</code> field to <code>true</code>
     */
    public void invalidateAccessToken(String tokenString, ActionListener<Boolean> listener) {
        ensureEnabled();
        if (Strings.isNullOrEmpty(tokenString)) {
            listener.onFailure(new IllegalArgumentException("token must be provided"));
        } else {
            maybeStartTokenRemover();
            try {
                decodeToken(tokenString, ActionListener.wrap(userToken -> {
                    if (userToken == null) {
                        listener.onFailure(malformedTokenException());
                    } else {
                        final long expirationEpochMilli = getExpirationTime().toEpochMilli();
                        indexBwcInvalidation(userToken, listener, new AtomicInteger(0), expirationEpochMilli);
                    }
                }, listener::onFailure));
            } catch (IOException e) {
                logger.error("received a malformed token as part of a invalidation request", e);
                listener.onFailure(malformedTokenException());
            }
        }
    }

    /**
     * This method performs the steps necessary to invalidate a token so that it may no longer be used.
     *
     * @see #invalidateAccessToken(String, ActionListener)
     */
    public void invalidateAccessToken(UserToken userToken, ActionListener<Boolean> listener) {
        ensureEnabled();
        if (userToken == null) {
            listener.onFailure(new IllegalArgumentException("token must be provided"));
        } else {
            maybeStartTokenRemover();
            final long expirationEpochMilli = getExpirationTime().toEpochMilli();
            indexBwcInvalidation(userToken, listener, new AtomicInteger(0), expirationEpochMilli);
        }
    }

    public void invalidateRefreshToken(String refreshToken, ActionListener<Boolean> listener) {
        ensureEnabled();
        if (Strings.isNullOrEmpty(refreshToken)) {
            listener.onFailure(new IllegalArgumentException("refresh token must be provided"));
        } else {
            maybeStartTokenRemover();
            findTokenFromRefreshToken(refreshToken,
                    ActionListener.wrap(tuple -> {
                        final String docId = tuple.v1().getHits().getAt(0).getId();
                        final long docVersion = tuple.v1().getHits().getAt(0).getVersion();
                        indexInvalidation(docId, Version.CURRENT, listener, tuple.v2(), "refresh_token", docVersion);
                    }, listener::onFailure), new AtomicInteger(0));
        }
    }

    /**
     * Performs the actual bwc invalidation of a token and then kicks off the new invalidation method
     *
     * @param userToken            the token to invalidate
     * @param listener             the listener to notify upon completion
     * @param attemptCount         the number of attempts to invalidate that have already been tried
     * @param expirationEpochMilli the expiration time as milliseconds since the epoch
     */
    private void indexBwcInvalidation(UserToken userToken, ActionListener<Boolean> listener, AtomicInteger attemptCount,
                                      long expirationEpochMilli) {
        if (attemptCount.get() > 5) {
            listener.onFailure(invalidGrantException("failed to invalidate token"));
        } else {
            final String invalidatedTokenId = getInvalidatedTokenDocumentId(userToken);
            IndexRequest indexRequest = client.prepareIndex(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, invalidatedTokenId)
                    .setOpType(OpType.CREATE)
                    .setSource("doc_type", INVALIDATED_TOKEN_DOC_TYPE, "expiration_time", expirationEpochMilli)
                    .setRefreshPolicy(RefreshPolicy.WAIT_UNTIL)
                    .request();
            final String tokenDocId = getTokenDocumentId(userToken);
            final Version version = userToken.getVersion();
            lifecycleService.prepareIndexIfNeededThenExecute(listener::onFailure, () ->
                    executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN, indexRequest,
                            ActionListener.<IndexResponse>wrap(indexResponse -> {
                                ActionListener<Boolean> wrappedListener =
                                        ActionListener.wrap(ignore -> listener.onResponse(true), listener::onFailure);
                                indexInvalidation(tokenDocId, version, wrappedListener, attemptCount, "access_token", 1L);
                            }, e -> {
                                Throwable cause = ExceptionsHelper.unwrapCause(e);
                                if (cause instanceof VersionConflictEngineException) {
                                    // expected since something else could have invalidated
                                    ActionListener<Boolean> wrappedListener =
                                            ActionListener.wrap(ignore -> listener.onResponse(false), listener::onFailure);
                                    indexInvalidation(tokenDocId, version, wrappedListener, attemptCount, "access_token", 1L);
                                } else if (isShardNotAvailableException(e)) {
                                    attemptCount.incrementAndGet();
                                    indexBwcInvalidation(userToken, listener, attemptCount, expirationEpochMilli);
                                } else {
                                    listener.onFailure(e);
                                }
                            }), client::index));
        }
    }

    /**
     * Performs the actual invalidation of a token
     *
     * @param tokenDocId      the id of the token doc to invalidate
     * @param listener        the listener to notify upon completion
     * @param attemptCount    the number of attempts to invalidate that have already been tried
     * @param srcPrefix       the prefix to use when constructing the doc to update
     * @param documentVersion the expected version of the document we will update
     */
    private void indexInvalidation(String tokenDocId, Version version, ActionListener<Boolean> listener, AtomicInteger attemptCount,
                                   String srcPrefix, long documentVersion) {
        if (attemptCount.get() > 5) {
            listener.onFailure(invalidGrantException("failed to invalidate token"));
        } else {
            UpdateRequest request = client.prepareUpdate(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, tokenDocId)
                    .setDoc(srcPrefix, Collections.singletonMap("invalidated", true))
                    .setVersion(documentVersion)
                    .setRefreshPolicy(RefreshPolicy.WAIT_UNTIL)
                    .request();
            lifecycleService.prepareIndexIfNeededThenExecute(listener::onFailure, () ->
                executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN, request,
                        ActionListener.<UpdateResponse>wrap(updateResponse -> {
                            if (updateResponse.getGetResult() != null
                                    && updateResponse.getGetResult().sourceAsMap().containsKey(srcPrefix)
                                    && ((Map<String, Object>) updateResponse.getGetResult().sourceAsMap().get(srcPrefix))
                                        .containsKey("invalidated")) {
                                final boolean prevInvalidated = (boolean)
                                        ((Map<String, Object>) updateResponse.getGetResult().sourceAsMap().get(srcPrefix))
                                                .get("invalidated");
                                listener.onResponse(prevInvalidated == false);
                            } else {
                                listener.onResponse(true);
                            }
                        }, e -> {
                            Throwable cause = ExceptionsHelper.unwrapCause(e);
                            if (cause instanceof DocumentMissingException) {
                                if (version.onOrAfter(Version.V_6_2_0)) {
                                    // the document should always be there!
                                    listener.onFailure(e);
                                } else {
                                    listener.onResponse(false);
                                }
                            } else if (cause instanceof VersionConflictEngineException
                                    || isShardNotAvailableException(cause)) {
                                attemptCount.incrementAndGet();
                                executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN,
                                        client.prepareGet(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, tokenDocId).request(),
                                        ActionListener.<GetResponse>wrap(getResult -> {
                                                    if (getResult.isExists()) {
                                                        Map<String, Object> source = getResult.getSource();
                                                        Map<String, Object> accessTokenSource =
                                                                (Map<String, Object>) source.get("access_token");
                                                        if (accessTokenSource == null) {
                                                            listener.onFailure(new IllegalArgumentException("token document is " +
                                                                    "missing access_token field"));
                                                        } else {
                                                            Boolean invalidated = (Boolean) accessTokenSource.get("invalidated");
                                                            if (invalidated == null) {
                                                                listener.onFailure(new IllegalStateException(
                                                                        "token document missing invalidated value"));
                                                            } else if (invalidated) {
                                                                listener.onResponse(false);
                                                            } else {
                                                                indexInvalidation(tokenDocId, version, listener, attemptCount, srcPrefix,
                                                                        getResult.getVersion());
                                                            }
                                                        }
                                                    } else if (version.onOrAfter(Version.V_6_2_0)) {
                                                        logger.warn("could not find token document [{}] but there should " +
                                                                        "be one as token has version [{}]", tokenDocId, version);
                                                        listener.onFailure(invalidGrantException("could not invalidate the token"));
                                                    } else {
                                                        listener.onResponse(false);
                                                    }
                                                },
                                                e1 -> {
                                                    if (isShardNotAvailableException(e1)) {
                                                        // don't increment count; call again
                                                        indexInvalidation(tokenDocId, version, listener, attemptCount, srcPrefix,
                                                                documentVersion);
                                                    } else {
                                                        listener.onFailure(e1);
                                                    }
                                                }), client::get);
                            } else {
                                listener.onFailure(e);
                            }
                        }), client::update));
        }
    }

    /**
     * Uses the refresh token to refresh its associated token and returns the new token with an
     * updated expiration date to the listener
     */
    public void refreshToken(String refreshToken, ActionListener<Tuple<UserToken, String>> listener) {
        ensureEnabled();
        findTokenFromRefreshToken(refreshToken,
                ActionListener.wrap(tuple -> {
                    final Authentication userAuth = Authentication.readFromContext(client.threadPool().getThreadContext());
                    final String tokenDocId = tuple.v1().getHits().getHits()[0].getId();
                    innerRefresh(tokenDocId, userAuth, listener, tuple.v2());
                }, listener::onFailure),
                new AtomicInteger(0));
    }

    private void findTokenFromRefreshToken(String refreshToken, ActionListener<Tuple<SearchResponse, AtomicInteger>> listener,
                                           AtomicInteger attemptCount) {
        if (attemptCount.get() > 5) {
            listener.onFailure(invalidGrantException("could not refresh the requested token"));
        } else {
            SearchRequest request = client.prepareSearch(SecurityLifecycleService.SECURITY_INDEX_NAME)
                    .setQuery(QueryBuilders.boolQuery()
                            .filter(QueryBuilders.termQuery("doc_type", "token"))
                            .filter(QueryBuilders.termQuery("refresh_token.token", refreshToken)))
                    .setVersion(true)
                    .request();

            lifecycleService.prepareIndexIfNeededThenExecute(listener::onFailure, () ->
                    executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN, request,
                            ActionListener.<SearchResponse>wrap(searchResponse -> {
                                if (searchResponse.isTimedOut()) {
                                    attemptCount.incrementAndGet();
                                    findTokenFromRefreshToken(refreshToken, listener, attemptCount);
                                } else if (searchResponse.getHits().getHits().length < 1) {
                                    logger.info("could not find token document with refresh_token [{}]", refreshToken);
                                    listener.onFailure(invalidGrantException("could not refresh the requested token"));
                                } else if (searchResponse.getHits().getHits().length > 1) {
                                    listener.onFailure(new IllegalStateException("multiple tokens share the same refresh token"));
                                } else {
                                    listener.onResponse(new Tuple<>(searchResponse, attemptCount));
                                }
                            }, e -> {
                                if (isShardNotAvailableException(e)) {
                                    logger.debug("failed to search for token document, retrying", e);
                                    attemptCount.incrementAndGet();
                                    findTokenFromRefreshToken(refreshToken, listener, attemptCount);
                                } else {
                                    listener.onFailure(e);
                                }
                            }),
                            client::search));
        }
    }

    /**
     * Performs the actual refresh of the token with retries in case of certain exceptions that
     * may be recoverable. The refresh involves retrieval of the token document and then
     * updating the token document to indicate that the document has been refreshed.
     */
    private void innerRefresh(String tokenDocId, Authentication userAuth, ActionListener<Tuple<UserToken, String>> listener,
                              AtomicInteger attemptCount) {
        if (attemptCount.getAndIncrement() > 5) {
            listener.onFailure(invalidGrantException("could not refresh the requested token"));
        } else {
            GetRequest getRequest = client.prepareGet(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, tokenDocId).request();
            executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN, getRequest,
                    ActionListener.<GetResponse>wrap(response -> {
                        if (response.isExists()) {
                            final Map<String, Object> source = response.getSource();
                            final Optional<ElasticsearchSecurityException> invalidSource = checkTokenDocForRefresh(source, userAuth);

                            if (invalidSource.isPresent()) {
                                listener.onFailure(invalidSource.get());
                            } else {
                                final Map<String, Object> userTokenSource = (Map<String, Object>)
                                        ((Map<String, Object>) source.get("access_token")).get("user_token");
                                final String authString = (String) userTokenSource.get("authentication");
                                final Integer version = (Integer) userTokenSource.get("version");
                                final Map<String, Object> metadata = (Map<String, Object>) userTokenSource.get("metadata");

                                Version authVersion = Version.fromId(version);
                                try (StreamInput in = StreamInput.wrap(Base64.getDecoder().decode(authString))) {
                                    in.setVersion(authVersion);
                                    Authentication authentication = new Authentication(in);
                                    UpdateRequest updateRequest =
                                            client.prepareUpdate(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, tokenDocId)
                                                    .setVersion(response.getVersion())
                                                    .setDoc("refresh_token", Collections.singletonMap("refreshed", true))
                                                    .setRefreshPolicy(RefreshPolicy.WAIT_UNTIL)
                                                    .request();
                                    executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN, updateRequest,
                                            ActionListener.<UpdateResponse>wrap(
                                                    updateResponse -> createUserToken(authentication, userAuth, listener, metadata),
                                                    e -> {
                                                        Throwable cause = ExceptionsHelper.unwrapCause(e);
                                                        if (cause instanceof VersionConflictEngineException ||
                                                                isShardNotAvailableException(e)) {
                                                            innerRefresh(tokenDocId, userAuth,
                                                                    listener, attemptCount);
                                                        } else {
                                                            listener.onFailure(e);
                                                        }
                                                    }),
                                            client::update);
                                }
                            }
                        } else {
                            logger.info("could not find token document [{}] for refresh", tokenDocId);
                            listener.onFailure(invalidGrantException("could not refresh the requested token"));
                        }
                    }, e -> {
                        if (isShardNotAvailableException(e)) {
                            innerRefresh(tokenDocId, userAuth, listener, attemptCount);
                        } else {
                            listener.onFailure(e);
                        }
                    }), client::get);
        }
    }

    /**
     * Performs checks on the retrieved source and returns an {@link Optional} with the exception
     * if there is an issue
     */
    private Optional<ElasticsearchSecurityException> checkTokenDocForRefresh(Map<String, Object> source, Authentication userAuth) {
        final Map<String, Object> refreshTokenSrc = (Map<String, Object>) source.get("refresh_token");
        final Map<String, Object> accessTokenSrc = (Map<String, Object>) source.get("access_token");
        if (refreshTokenSrc == null || refreshTokenSrc.isEmpty()) {
            return Optional.of(invalidGrantException("token document is missing the refresh_token object"));
        } else if (accessTokenSrc == null || accessTokenSrc.isEmpty()) {
            return Optional.of(invalidGrantException("token document is missing the access_token object"));
        } else {
            final Boolean refreshed = (Boolean) refreshTokenSrc.get("refreshed");
            final Boolean invalidated = (Boolean) refreshTokenSrc.get("invalidated");
            final Long creationEpochMilli = (Long) source.get("creation_time");
            final Instant creationTime = creationEpochMilli == null ? null : Instant.ofEpochMilli(creationEpochMilli);
            final Map<String, Object> userTokenSrc = (Map<String, Object>) accessTokenSrc.get("user_token");
            if (refreshed == null) {
                return Optional.of(invalidGrantException("token document is missing refreshed value"));
            } else if (invalidated == null) {
                return Optional.of(invalidGrantException("token document is missing invalidated value"));
            } else if (creationEpochMilli == null) {
                return Optional.of(invalidGrantException("token document is missing creation time value"));
            } else if (refreshed) {
                return Optional.of(invalidGrantException("token has already been refreshed"));
            } else if (invalidated) {
                return Optional.of(invalidGrantException("token has been invalidated"));
            } else if (clock.instant().isAfter(creationTime.plus(24L, ChronoUnit.HOURS))) {
                return Optional.of(invalidGrantException("refresh token is expired"));
            } else if (userTokenSrc == null || userTokenSrc.isEmpty()) {
                return Optional.of(invalidGrantException("token document is missing the user token info"));
            } else if (userTokenSrc.get("authentication") == null) {
                return Optional.of(invalidGrantException("token is missing authentication info"));
            } else if (userTokenSrc.get("version") == null) {
                return Optional.of(invalidGrantException("token is missing version value"));
            } else if (userTokenSrc.get("metadata") == null) {
                return Optional.of(invalidGrantException("token is missing metadata"));
            } else {
                return checkClient(refreshTokenSrc, userAuth);
            }
        }
    }

    private Optional<ElasticsearchSecurityException> checkClient(Map<String, Object> refreshTokenSource, Authentication userAuth) {
        Map<String, Object> clientInfo = (Map<String, Object>) refreshTokenSource.get("client");
        if (clientInfo == null) {
            return Optional.of(invalidGrantException("token is missing client information"));
        } else if (userAuth.getUser().principal().equals(clientInfo.get("user")) == false) {
            return Optional.of(invalidGrantException("tokens must be refreshed by the creating client"));
        } else if (userAuth.getAuthenticatedBy().getName().equals(clientInfo.get("realm")) == false) {
            return Optional.of(invalidGrantException("tokens must be refreshed by the creating client"));
        } else {
            return Optional.empty();
        }
    }

    /**
     * Find all stored refresh and access tokens that have not been invalidated or expired, and were issued against
     *  the specified realm.
     */
    public void findActiveTokensForRealm(String realmName, ActionListener<Collection<Tuple<UserToken, String>>> listener) {
        ensureEnabled();

        if (Strings.isNullOrEmpty(realmName)) {
            listener.onFailure(new IllegalArgumentException("Realm name is required"));
            return;
        }

        final Instant now = clock.instant();
        final BoolQueryBuilder boolQuery = QueryBuilders.boolQuery()
                .filter(QueryBuilders.termQuery("doc_type", "token"))
                .filter(QueryBuilders.termQuery("access_token.realm", realmName))
                .filter(QueryBuilders.boolQuery()
                        .should(QueryBuilders.boolQuery()
                                .must(QueryBuilders.termQuery("access_token.invalidated", false))
                                .must(QueryBuilders.rangeQuery("access_token.user_token.expiration_time").gte(now.toEpochMilli()))
                        )
                        .should(QueryBuilders.termQuery("refresh_token.invalidated", false))
                );

        final SearchRequest request = client.prepareSearch(SecurityLifecycleService.SECURITY_INDEX_NAME)
                .setScroll(TimeValue.timeValueSeconds(10L))
                .setQuery(boolQuery)
                .setVersion(false)
                .setSize(1000)
                .setFetchSource(true)
                .request();

        final Supplier<ThreadContext.StoredContext> supplier = client.threadPool().getThreadContext().newRestorableContext(false);
        lifecycleService.prepareIndexIfNeededThenExecute(listener::onFailure, () ->
            ScrollHelper.fetchAllByEntity(client, request, new ContextPreservingActionListener<>(supplier, listener), this::parseHit));
    }

    private Tuple<UserToken, String> parseHit(SearchHit hit) {
        final Map<String, Object> source = hit.getSourceAsMap();
        if (source == null) {
            throw new IllegalStateException("token document did not have source but source should have been fetched");
        }

        try {
            return parseTokensFromDocument(source);
        } catch (IOException e) {
            throw invalidGrantException("cannot read token from document");
        }
    }

    /**
     * @return A {@link Tuple} of access-token and refresh-token-id
     */
    private Tuple<UserToken, String> parseTokensFromDocument(Map<String, Object> source) throws IOException {
        final String refreshToken = (String) ((Map<String, Object>) source.get("refresh_token")).get("token");

        final Map<String, Object> userTokenSource = (Map<String, Object>)
                ((Map<String, Object>) source.get("access_token")).get("user_token");
        final String id = (String) userTokenSource.get("id");
        final Integer version = (Integer) userTokenSource.get("version");
        final String authString = (String) userTokenSource.get("authentication");
        final Long expiration = (Long) userTokenSource.get("expiration_time");
        final Map<String, Object> metadata = (Map<String, Object>) userTokenSource.get("metadata");

        Version authVersion = Version.fromId(version);
        try (StreamInput in = StreamInput.wrap(Base64.getDecoder().decode(authString))) {
            in.setVersion(authVersion);
            Authentication authentication = new Authentication(in);
            return new Tuple<>(new UserToken(id, Version.fromId(version), authentication, Instant.ofEpochMilli(expiration), metadata),
                    refreshToken);
        }
    }

    private static String getInvalidatedTokenDocumentId(UserToken userToken) {
        return getInvalidatedTokenDocumentId(userToken.getId());
    }

    private static String getInvalidatedTokenDocumentId(String id) {
        return INVALIDATED_TOKEN_DOC_TYPE + "_" + id;
    }

    private static String getTokenDocumentId(UserToken userToken) {
        return getTokenDocumentId(userToken.getId());
    }

    private static String getTokenDocumentId(String id) {
        return "token_" + id;
    }

    private void ensureEnabled() {
        if (enabled == false) {
            throw new IllegalStateException("tokens are not enabled");
        }
    }

    /**
     * Checks if the token has been stored as a revoked token to ensure we do not allow tokens that
     * have been explicitly cleared.
     */
    private void checkIfTokenIsRevoked(UserToken userToken, ActionListener<UserToken> listener) {
        if (lifecycleService.isSecurityIndexExisting() == false) {
            // index doesn't exist so the token is considered valid.
            listener.onResponse(userToken);
        } else {
            lifecycleService.prepareIndexIfNeededThenExecute(listener::onFailure, () -> {
                MultiGetRequest mGetRequest = client.prepareMultiGet()
                        .add(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, getInvalidatedTokenDocumentId(userToken))
                        .add(SecurityLifecycleService.SECURITY_INDEX_NAME, TYPE, getTokenDocumentId(userToken))
                        .request();
                executeAsyncWithOrigin(client.threadPool().getThreadContext(), SECURITY_ORIGIN,
                        mGetRequest,
                        new ActionListener<MultiGetResponse>() {

                            @Override
                            public void onResponse(MultiGetResponse response) {
                                MultiGetItemResponse[] itemResponse = response.getResponses();
                                if (itemResponse[0].isFailed()) {
                                    onFailure(itemResponse[0].getFailure().getFailure());
                                } else if (itemResponse[0].getResponse().isExists()) {
                                    listener.onFailure(expiredTokenException());
                                } else if (itemResponse[1].isFailed()) {
                                    onFailure(itemResponse[1].getFailure().getFailure());
                                } else if (itemResponse[1].getResponse().isExists()) {
                                    Map<String, Object> source = itemResponse[1].getResponse().getSource();
                                    Map<String, Object> accessTokenSource = (Map<String, Object>) source.get("access_token");
                                    if (accessTokenSource == null) {
                                        listener.onFailure(new IllegalStateException("token document is missing access_token field"));
                                    } else {
                                        Boolean invalidated = (Boolean) accessTokenSource.get("invalidated");
                                        if (invalidated == null) {
                                            listener.onFailure(new IllegalStateException("token document is missing invalidated field"));
                                        } else if (invalidated) {
                                            listener.onFailure(expiredTokenException());
                                        } else {
                                            listener.onResponse(userToken);
                                        }
                                    }
                                } else if (userToken.getVersion().onOrAfter(Version.V_6_2_0)) {
                                    listener.onFailure(new IllegalStateException("token document is missing and must be present"));
                                } else {
                                    listener.onResponse(userToken);
                                }
                            }

                            @Override
                            public void onFailure(Exception e) {
                                // if the index or the shard is not there / available we assume that
                                // the token is not valid
                                if (isShardNotAvailableException(e)) {
                                    logger.warn("failed to get token [{}] since index is not available", userToken.getId());
                                    listener.onResponse(null);
                                } else {
                                    logger.error(new ParameterizedMessage("failed to get token [{}]", userToken.getId()), e);
                                    listener.onFailure(e);
                                }
                            }
                        }, client::multiGet);
            });
        }
    }


    public TimeValue getExpirationDelay() {
        return expirationDelay;
    }

    private Instant getExpirationTime() {
        return getExpirationTime(clock.instant());
    }

    private Instant getExpirationTime(Instant now) {
        return now.plusSeconds(expirationDelay.getSeconds());
    }

    private void maybeStartTokenRemover() {
        if (lifecycleService.isSecurityIndexAvailable()) {
            if (client.threadPool().relativeTimeInMillis() - lastExpirationRunMs > deleteInterval.getMillis()) {
                expiredTokenRemover.submit(client.threadPool());
                lastExpirationRunMs = client.threadPool().relativeTimeInMillis();
            }
        }
    }

    /**
     * Gets the token from the <code>Authorization</code> header if the header begins with
     * <code>Bearer </code>
     */
    private String getFromHeader(ThreadContext threadContext) {
        String header = threadContext.getHeader("Authorization");
        if (Strings.hasLength(header) && header.startsWith("Bearer ")
                && header.length() > "Bearer ".length()) {
            return header.substring("Bearer ".length());
        }
        return null;
    }

    /**
     * Serializes a token to a String containing an encrypted representation of the token
     */
    public String getUserTokenString(UserToken userToken) throws IOException, GeneralSecurityException {
        // we know that the minimum length is larger than the default of the ByteArrayOutputStream so set the size to this explicitly
        try (ByteArrayOutputStream os = new ByteArrayOutputStream(MINIMUM_BASE64_BYTES);
             OutputStream base64 = Base64.getEncoder().wrap(os);
             StreamOutput out = new OutputStreamStreamOutput(base64)) {
            out.setVersion(userToken.getVersion());
            KeyAndCache keyAndCache = keyCache.activeKeyCache;
            Version.writeVersion(userToken.getVersion(), out);
            out.writeByteArray(keyAndCache.getSalt().bytes);
            out.writeByteArray(keyAndCache.getKeyHash().bytes);
            final byte[] initializationVector = getNewInitializationVector();
            out.writeByteArray(initializationVector);
            try (CipherOutputStream encryptedOutput =
                         new CipherOutputStream(out, getEncryptionCipher(initializationVector, keyAndCache, userToken.getVersion()));
                 StreamOutput encryptedStreamOutput = new OutputStreamStreamOutput(encryptedOutput)) {
                encryptedStreamOutput.setVersion(userToken.getVersion());
                if (userToken.getVersion().onOrAfter(Version.V_6_2_0)) {
                    encryptedStreamOutput.writeString(userToken.getId());
                } else {
                    userToken.writeTo(encryptedStreamOutput);
                }
                encryptedStreamOutput.close();
                return new String(os.toByteArray(), StandardCharsets.UTF_8);
            }
        }
    }

    private void ensureEncryptionCiphersSupported() throws NoSuchPaddingException, NoSuchAlgorithmException {
        Cipher.getInstance(ENCRYPTION_CIPHER);
        SecretKeyFactory.getInstance(KDF_ALGORITHM);
    }

    private Cipher getEncryptionCipher(byte[] iv, KeyAndCache keyAndCache, Version version) throws GeneralSecurityException {
        Cipher cipher = Cipher.getInstance(ENCRYPTION_CIPHER);
        BytesKey salt = keyAndCache.getSalt();
        try {
            cipher.init(Cipher.ENCRYPT_MODE, keyAndCache.getOrComputeKey(salt), new GCMParameterSpec(128, iv), secureRandom);
        } catch (ExecutionException e) {
            throw new ElasticsearchSecurityException("Failed to compute secret key for active salt", e);
        }
        cipher.updateAAD(ByteBuffer.allocate(4).putInt(version.id).array());
        cipher.updateAAD(salt.bytes);
        return cipher;
    }

    private Cipher getDecryptionCipher(byte[] iv, SecretKey key, Version version,
                                       BytesKey salt) throws GeneralSecurityException {
        Cipher cipher = Cipher.getInstance(ENCRYPTION_CIPHER);
        cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(128, iv), secureRandom);
        cipher.updateAAD(ByteBuffer.allocate(4).putInt(version.id).array());
        cipher.updateAAD(salt.bytes);
        return cipher;
    }

    private byte[] getNewInitializationVector() {
        final byte[] initializationVector = new byte[IV_BYTES];
        secureRandom.nextBytes(initializationVector);
        return initializationVector;
    }

    /**
     * Generates a secret key based off of the provided password and salt.
     * This method is computationally expensive.
     */
    static SecretKey computeSecretKey(char[] rawPassword, byte[] salt)
            throws NoSuchAlgorithmException, InvalidKeySpecException {
        SecretKeyFactory secretKeyFactory = SecretKeyFactory.getInstance(KDF_ALGORITHM);
        PBEKeySpec keySpec = new PBEKeySpec(rawPassword, salt, ITERATIONS, 128);
        SecretKey tmp = secretKeyFactory.generateSecret(keySpec);
        return new SecretKeySpec(tmp.getEncoded(), "AES");
    }

    /**
     * Creates an {@link ElasticsearchSecurityException} that indicates the token was expired. It
     * is up to the client to re-authenticate and obtain a new token. The format for this response
     * is defined in <a href="https://tools.ietf.org/html/rfc6750#section-3.1"></a>
     */
    private static ElasticsearchSecurityException expiredTokenException() {
        ElasticsearchSecurityException e =
                new ElasticsearchSecurityException("token expired", RestStatus.UNAUTHORIZED);
        e.addHeader("WWW-Authenticate", EXPIRED_TOKEN_WWW_AUTH_VALUE);
        return e;
    }

    /**
     * Creates an {@link ElasticsearchSecurityException} that indicates the token was expired. It
     * is up to the client to re-authenticate and obtain a new token. The format for this response
     * is defined in <a href="https://tools.ietf.org/html/rfc6750#section-3.1"></a>
     */
    private static ElasticsearchSecurityException malformedTokenException() {
        ElasticsearchSecurityException e =
                new ElasticsearchSecurityException("token malformed", RestStatus.UNAUTHORIZED);
        e.addHeader("WWW-Authenticate", MALFORMED_TOKEN_WWW_AUTH_VALUE);
        return e;
    }

    /**
     * Creates an {@link ElasticsearchSecurityException} that indicates the request contained an invalid grant
     */
    private static ElasticsearchSecurityException invalidGrantException(String detail) {
        ElasticsearchSecurityException e =
                new ElasticsearchSecurityException("invalid_grant", RestStatus.BAD_REQUEST);
        e.addHeader("error_description", detail);
        return e;
    }

    boolean isExpiredTokenException(ElasticsearchSecurityException e) {
        final List<String> headers = e.getHeader("WWW-Authenticate");
        return headers != null && headers.stream().anyMatch(EXPIRED_TOKEN_WWW_AUTH_VALUE::equals);
    }

    boolean isExpirationInProgress() {
        return expiredTokenRemover.isExpirationInProgress();
    }

    private class KeyComputingRunnable extends AbstractRunnable {

        private final BytesKey decodedSalt;
        private final ActionListener<SecretKey> listener;
        private final KeyAndCache keyAndCache;

        KeyComputingRunnable(BytesKey decodedSalt, ActionListener<SecretKey> listener, KeyAndCache keyAndCache) {
            this.decodedSalt = decodedSalt;
            this.listener = listener;
            this.keyAndCache = keyAndCache;
        }

        @Override
        protected void doRun() {
            try {
                final SecretKey computedKey = keyAndCache.getOrComputeKey(decodedSalt);
                listener.onResponse(computedKey);
            } catch (ExecutionException e) {
                if (e.getCause() != null &&
                        (e.getCause() instanceof GeneralSecurityException || e.getCause() instanceof IOException
                                || e.getCause() instanceof IllegalArgumentException)) {
                    // this could happen if another realm supports the Bearer token so we should
                    // see if another realm can use this token!
                    logger.debug("unable to decode bearer token", e);
                    listener.onResponse(null);
                } else {
                    listener.onFailure(e);
                }
            }
        }

        @Override
        public void onFailure(Exception e) {
            listener.onFailure(e);
        }
    }

    /**
     * Creates a new key unless present that is newer than the current active key and returns the corresponding metadata. Note:
     * this method doesn't modify the metadata used in this token service. See {@link #refreshMetaData(TokenMetaData)}
     */
    synchronized TokenMetaData generateSpareKey() {
        KeyAndCache maxKey = keyCache.cache.values().stream().max(Comparator.comparingLong(v -> v.keyAndTimestamp.getTimestamp())).get();
        KeyAndCache currentKey = keyCache.activeKeyCache;
        if (currentKey == maxKey) {
            long timestamp = createdTimeStamps.incrementAndGet();
            while (true) {
                byte[] saltArr = new byte[SALT_BYTES];
                secureRandom.nextBytes(saltArr);
                SecureString tokenKey = generateTokenKey();
                KeyAndCache keyAndCache = new KeyAndCache(new KeyAndTimestamp(tokenKey, timestamp), new BytesKey(saltArr));
                if (keyCache.cache.containsKey(keyAndCache.getKeyHash())) {
                    continue; // collision -- generate a new key
                }
                return newTokenMetaData(keyCache.currentTokenKeyHash, Iterables.concat(keyCache.cache.values(),
                        Collections.singletonList(keyAndCache)));
            }
        }
        return newTokenMetaData(keyCache.currentTokenKeyHash, keyCache.cache.values());
    }

    /**
     * Rotate the current active key to the spare key created in the previous {@link #generateSpareKey()} call.
     */
    synchronized TokenMetaData rotateToSpareKey() {
        KeyAndCache maxKey = keyCache.cache.values().stream().max(Comparator.comparingLong(v -> v.keyAndTimestamp.getTimestamp())).get();
        if (maxKey == keyCache.activeKeyCache) {
            throw new IllegalStateException("call generateSpareKey first");
        }
        return newTokenMetaData(maxKey.getKeyHash(), keyCache.cache.values());
    }

    /**
     * Prunes the keys and keeps up to the latest N keys around
     *
     * @param numKeysToKeep the number of keys to keep.
     */
    synchronized TokenMetaData pruneKeys(int numKeysToKeep) {
        if (keyCache.cache.size() <= numKeysToKeep) {
            return getTokenMetaData(); // nothing to do
        }
        Map<BytesKey, KeyAndCache> map = new HashMap<>(keyCache.cache.size() + 1);
        KeyAndCache currentKey = keyCache.get(keyCache.currentTokenKeyHash);
        ArrayList<KeyAndCache> entries = new ArrayList<>(keyCache.cache.values());
        Collections.sort(entries,
                (left, right) ->  Long.compare(right.keyAndTimestamp.getTimestamp(), left.keyAndTimestamp.getTimestamp()));
        for (KeyAndCache value : entries) {
            if (map.size() < numKeysToKeep || value.keyAndTimestamp.getTimestamp() >= currentKey
                    .keyAndTimestamp.getTimestamp()) {
                logger.debug("keeping key {} ", value.getKeyHash());
                map.put(value.getKeyHash(), value);
            } else {
                logger.debug("prune key {} ", value.getKeyHash());
            }
        }
        assert map.isEmpty() == false;
        assert map.containsKey(keyCache.currentTokenKeyHash);
        return newTokenMetaData(keyCache.currentTokenKeyHash, map.values());
    }

    /**
     * Returns the current in-use metdata of this {@link TokenService}
     */
    public synchronized TokenMetaData getTokenMetaData() {
        return newTokenMetaData(keyCache.currentTokenKeyHash, keyCache.cache.values());
    }

    private TokenMetaData newTokenMetaData(BytesKey activeTokenKey, Iterable<KeyAndCache> iterable) {
        List<KeyAndTimestamp> list = new ArrayList<>();
        for (KeyAndCache v : iterable) {
            list.add(v.keyAndTimestamp);
        }
        return new TokenMetaData(list, activeTokenKey.bytes);
    }

    /**
     * Refreshes the current in-use metadata.
     */
    synchronized void refreshMetaData(TokenMetaData metaData) {
        BytesKey currentUsedKeyHash = new BytesKey(metaData.getCurrentKeyHash());
        byte[] saltArr = new byte[SALT_BYTES];
        Map<BytesKey, KeyAndCache> map = new HashMap<>(metaData.getKeys().size());
        long maxTimestamp = createdTimeStamps.get();
        for (KeyAndTimestamp key : metaData.getKeys()) {
            secureRandom.nextBytes(saltArr);
            KeyAndCache keyAndCache = new KeyAndCache(key, new BytesKey(saltArr));
            maxTimestamp = Math.max(keyAndCache.keyAndTimestamp.getTimestamp(), maxTimestamp);
            if (keyCache.cache.containsKey(keyAndCache.getKeyHash()) == false) {
                map.put(keyAndCache.getKeyHash(), keyAndCache);
            } else {
                map.put(keyAndCache.getKeyHash(), keyCache.get(keyAndCache.getKeyHash())); // maintain the cache we already have
            }
        }
        if (map.containsKey(currentUsedKeyHash) == false) {
            // this won't leak any secrets it's only exposing the current set of hashes
            throw new IllegalStateException("Current key is not in the map: " + map.keySet() + " key: " + currentUsedKeyHash);
        }
        createdTimeStamps.set(maxTimestamp);
        keyCache = new TokenKeys(Collections.unmodifiableMap(map), currentUsedKeyHash);
        logger.debug("refreshed keys current: {}, keys: {}", currentUsedKeyHash, keyCache.cache.keySet());
    }

    private SecureString generateTokenKey() {
        byte[] keyBytes = new byte[KEY_BYTES];
        byte[] encode = new byte[0];
        char[] ref = new char[0];
        try {
            secureRandom.nextBytes(keyBytes);
            encode = Base64.getUrlEncoder().withoutPadding().encode(keyBytes);
            ref = new char[encode.length];
            int len = UnicodeUtil.UTF8toUTF16(encode, 0, encode.length, ref);
            return new SecureString(Arrays.copyOfRange(ref, 0, len));
        } finally {
            Arrays.fill(keyBytes, (byte) 0x00);
            Arrays.fill(encode, (byte) 0x00);
            Arrays.fill(ref, (char) 0x00);
        }
    }

    synchronized String getActiveKeyHash() {
        return new BytesRef(Base64.getUrlEncoder().withoutPadding().encode(this.keyCache.currentTokenKeyHash.bytes)).utf8ToString();
    }

    void rotateKeysOnMaster(ActionListener<ClusterStateUpdateResponse> listener) {
        logger.info("rotate keys on master");
        TokenMetaData tokenMetaData = generateSpareKey();
        clusterService.submitStateUpdateTask("publish next key to prepare key rotation",
                new TokenMetadataPublishAction(
                        ActionListener.wrap((res) -> {
                            if (res.isAcknowledged()) {
                                TokenMetaData metaData = rotateToSpareKey();
                                clusterService.submitStateUpdateTask("publish next key to prepare key rotation",
                                        new TokenMetadataPublishAction(listener, metaData));
                            } else {
                                listener.onFailure(new IllegalStateException("not acked"));
                            }
                        }, listener::onFailure), tokenMetaData));
    }

    private final class TokenMetadataPublishAction extends AckedClusterStateUpdateTask<ClusterStateUpdateResponse> {

        private final TokenMetaData tokenMetaData;

        protected TokenMetadataPublishAction(ActionListener<ClusterStateUpdateResponse> listener, TokenMetaData tokenMetaData) {
            super(new AckedRequest() {
                @Override
                public TimeValue ackTimeout() {
                    return AcknowledgedRequest.DEFAULT_ACK_TIMEOUT;
                }

                @Override
                public TimeValue masterNodeTimeout() {
                    return AcknowledgedRequest.DEFAULT_MASTER_NODE_TIMEOUT;
                }
            }, listener);
            this.tokenMetaData = tokenMetaData;
        }

        @Override
        public ClusterState execute(ClusterState currentState) throws Exception {
            if (tokenMetaData.equals(currentState.custom(TokenMetaData.TYPE))) {
                return currentState;
            }
            return ClusterState.builder(currentState).putCustom(TokenMetaData.TYPE, tokenMetaData).build();
        }

        @Override
        protected ClusterStateUpdateResponse newResponse(boolean acknowledged) {
            return new ClusterStateUpdateResponse(acknowledged);
        }

    }

    private void initialize(ClusterService clusterService) {
        clusterService.addListener(event -> {
            ClusterState state = event.state();
            if (state.getBlocks().hasGlobalBlock(STATE_NOT_RECOVERED_BLOCK)) {
                return;
            }

            TokenMetaData custom = event.state().custom(TokenMetaData.TYPE);
            if (custom != null && custom.equals(getTokenMetaData()) == false) {
                logger.info("refresh keys");
                try {
                    refreshMetaData(custom);
                } catch (Exception e) {
                    logger.warn("refreshing metadata failed", e);
                }
                logger.info("refreshed keys");
            }
        });
    }

    /**
     * For testing
     */
    void clearActiveKeyCache() {
        this.keyCache.activeKeyCache.keyCache.invalidateAll();
    }

    static final class KeyAndCache implements Closeable {
        private final KeyAndTimestamp keyAndTimestamp;
        private final Cache<BytesKey, SecretKey> keyCache;
        private final BytesKey salt;
        private final BytesKey keyHash;

        private KeyAndCache(KeyAndTimestamp keyAndTimestamp, BytesKey salt) {
            this.keyAndTimestamp = keyAndTimestamp;
            keyCache = CacheBuilder.<BytesKey, SecretKey>builder()
                    .setExpireAfterAccess(TimeValue.timeValueMinutes(60L))
                    .setMaximumWeight(500L)
                    .build();
            try {
                SecretKey secretKey = computeSecretKey(keyAndTimestamp.getKey().getChars(), salt.bytes);
                keyCache.put(salt, secretKey);
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
            this.salt = salt;
            this.keyHash = calculateKeyHash(keyAndTimestamp.getKey());
        }

        private SecretKey getKey(BytesKey salt) {
            return keyCache.get(salt);
        }

        public SecretKey getOrComputeKey(BytesKey decodedSalt) throws ExecutionException {
            return keyCache.computeIfAbsent(decodedSalt, (salt) -> {
                try (SecureString closeableChars = keyAndTimestamp.getKey().clone()) {
                    return computeSecretKey(closeableChars.getChars(), salt.bytes);
                }
            });
        }

        @Override
        public void close() throws IOException {
            keyAndTimestamp.getKey().close();
        }

        BytesKey getKeyHash() {
            return keyHash;
        }

        private static BytesKey calculateKeyHash(SecureString key) {
            MessageDigest messageDigest = MessageDigests.sha256();
            BytesRefBuilder b = new BytesRefBuilder();
            try {
                b.copyChars(key);
                BytesRef bytesRef = b.toBytesRef();
                try {
                    messageDigest.update(bytesRef.bytes, bytesRef.offset, bytesRef.length);
                    return new BytesKey(Arrays.copyOfRange(messageDigest.digest(), 0, 8));
                } finally {
                    Arrays.fill(bytesRef.bytes, (byte) 0x00);
                }
            } finally {
                Arrays.fill(b.bytes(), (byte) 0x00);
            }
        }

        BytesKey getSalt() {
            return salt;
        }
    }


    private static final class TokenKeys {
        final Map<BytesKey, KeyAndCache> cache;
        final BytesKey currentTokenKeyHash;
        final KeyAndCache activeKeyCache;

        private TokenKeys(Map<BytesKey, KeyAndCache> cache, BytesKey currentTokenKeyHash) {
            this.cache = cache;
            this.currentTokenKeyHash = currentTokenKeyHash;
            this.activeKeyCache = cache.get(currentTokenKeyHash);
        }

        KeyAndCache get(BytesKey passphraseHash) {
            return cache.get(passphraseHash);
        }
    }

}
