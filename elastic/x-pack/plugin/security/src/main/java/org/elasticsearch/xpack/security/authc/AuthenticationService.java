/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc;

import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.logging.log4j.util.Supplier;
import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.component.AbstractComponent;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.node.Node;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportMessage;
import org.elasticsearch.xpack.core.common.IteratingActionListener;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.Authentication.RealmRef;
import org.elasticsearch.xpack.core.security.authc.AuthenticationFailureHandler;
import org.elasticsearch.xpack.core.security.authc.AuthenticationResult;
import org.elasticsearch.xpack.core.security.authc.AuthenticationServiceField;
import org.elasticsearch.xpack.core.security.authc.AuthenticationToken;
import org.elasticsearch.xpack.core.security.authc.Realm;
import org.elasticsearch.xpack.core.security.authz.permission.Role;
import org.elasticsearch.xpack.core.security.support.Exceptions;
import org.elasticsearch.xpack.core.security.user.AnonymousUser;
import org.elasticsearch.xpack.core.security.user.SystemUser;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.security.audit.AuditTrail;
import org.elasticsearch.xpack.security.audit.AuditTrailService;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BiConsumer;
import java.util.function.Consumer;

/**
 * An authentication service that delegates the authentication process to its configured {@link Realm realms}.
 * This service also supports request level caching of authenticated users (i.e. once a user authenticated
 * successfully, it is set on the request context to avoid subsequent redundant authentication process)
 */
public class AuthenticationService extends AbstractComponent {

    private final Realms realms;
    private final AuditTrail auditTrail;
    private final AuthenticationFailureHandler failureHandler;
    private final ThreadContext threadContext;
    private final String nodeName;
    private final AnonymousUser anonymousUser;
    private final TokenService tokenService;
    private final boolean runAsEnabled;
    private final boolean isAnonymousUserEnabled;

    public AuthenticationService(Settings settings, Realms realms, AuditTrailService auditTrail,
                                 AuthenticationFailureHandler failureHandler, ThreadPool threadPool,
                                 AnonymousUser anonymousUser, TokenService tokenService) {
        super(settings);
        this.nodeName = Node.NODE_NAME_SETTING.get(settings);
        this.realms = realms;
        this.auditTrail = auditTrail;
        this.failureHandler = failureHandler;
        this.threadContext = threadPool.getThreadContext();
        this.anonymousUser = anonymousUser;
        this.runAsEnabled = AuthenticationServiceField.RUN_AS_ENABLED.get(settings);
        this.isAnonymousUserEnabled = AnonymousUser.isAnonymousEnabled(settings);
        this.tokenService = tokenService;
    }

    /**
     * Authenticates the user that is associated with the given request. If the user was authenticated successfully (i.e.
     * a user was indeed associated with the request and the credentials were verified to be valid), the method returns
     * the user and that user is then "attached" to the request's context.
     *
     * @param request   The request to be authenticated
     */
    public void authenticate(RestRequest request, ActionListener<Authentication> authenticationListener) {
        createAuthenticator(request, authenticationListener).authenticateAsync();
    }

    /**
     * Authenticates the user that is associated with the given message. If the user was authenticated successfully (i.e.
     * a user was indeed associated with the request and the credentials were verified to be valid), the method returns
     * the user and that user is then "attached" to the message's context. If no user was found to be attached to the given
     * message, then the given fallback user will be returned instead.
     *
     * @param action        The action of the message
     * @param message       The message to be authenticated
     * @param fallbackUser  The default user that will be assumed if no other user is attached to the message. Can be
     *                      {@code null}, in which case there will be no fallback user and the success/failure of the
     *                      authentication will be based on the whether there's an attached user to in the message and
     *                      if there is, whether its credentials are valid.
     */
    public void authenticate(String action, TransportMessage message, User fallbackUser, ActionListener<Authentication> listener) {
        createAuthenticator(action, message, fallbackUser, listener).authenticateAsync();
    }

    /**
     * Authenticates the username and password that are provided as parameters. This will not look
     * at the values in the ThreadContext for Authentication.
     *
     * @param action  The action of the message
     * @param message The message that resulted in this authenticate call
     * @param token   The token (credentials) to be authenticated
     */
    public void authenticate(String action, TransportMessage message,
                             AuthenticationToken token, ActionListener<Authentication> listener) {
        new Authenticator(action, message, null, listener).authenticateToken(token);
    }

    // pkg private method for testing
    Authenticator createAuthenticator(RestRequest request, ActionListener<Authentication> listener) {
        return new Authenticator(request, listener);
    }

    // pkg private method for testing
    Authenticator createAuthenticator(String action, TransportMessage message, User fallbackUser, ActionListener<Authentication> listener) {
        return new Authenticator(action, message, fallbackUser, listener);
    }

    /**
     * This class is responsible for taking a request and executing the authentication. The authentication is executed in an asynchronous
     * fashion in order to avoid blocking calls on a network thread. This class also performs the auditing necessary around authentication
     */
    class Authenticator {

        private final AuditableRequest request;
        private final User fallbackUser;

        private final ActionListener<Authentication> listener;
        private RealmRef authenticatedBy = null;
        private RealmRef lookedupBy = null;
        private AuthenticationToken authenticationToken = null;

        Authenticator(RestRequest request, ActionListener<Authentication> listener) {
            this(new AuditableRestRequest(auditTrail, failureHandler, threadContext, request), null, listener);
        }

        Authenticator(String action, TransportMessage message, User fallbackUser, ActionListener<Authentication> listener) {
            this(new AuditableTransportRequest(auditTrail, failureHandler, threadContext, action, message), fallbackUser, listener);
        }

        private Authenticator(AuditableRequest auditableRequest, User fallbackUser, ActionListener<Authentication> listener) {
            this.request = auditableRequest;
            this.fallbackUser = fallbackUser;
            this.listener = listener;
        }

        /**
         * This method starts the authentication process. The authentication process can be broken down into distinct operations. In order,
         * these operations are:
         *
         * <ol>
         *     <li>look for existing authentication {@link #lookForExistingAuthentication(Consumer)}</li>
         *     <li>look for a user token</li>
         *     <li>token extraction {@link #extractToken(Consumer)}</li>
         *     <li>token authentication {@link #consumeToken(AuthenticationToken)}</li>
         *     <li>user lookup for run as if necessary {@link #consumeUser(User, Map)} and
         *     {@link #lookupRunAsUser(User, String, Consumer)}</li>
         *     <li>write authentication into the context {@link #finishAuthentication(User)}</li>
         * </ol>
         */
        private void authenticateAsync() {
            lookForExistingAuthentication((authentication) -> {
                if (authentication != null) {
                    listener.onResponse(authentication);
                } else {
                    tokenService.getAndValidateToken(threadContext, ActionListener.wrap(userToken -> {
                        if (userToken != null) {
                            writeAuthToContext(userToken.getAuthentication());
                        } else {
                            extractToken(this::consumeToken);
                        }
                    }, e -> {
                        if (e instanceof ElasticsearchSecurityException &&
                                tokenService.isExpiredTokenException((ElasticsearchSecurityException) e) == false) {
                            // intentionally ignore the returned exception; we call this primarily
                            // for the auditing as we already have a purpose built exception
                            request.tamperedRequest();
                        }
                        listener.onFailure(e);
                    }));
                }
            });
        }

        /**
         * Looks to see if the request contains an existing {@link Authentication} and if so, that authentication will be used. The
         * consumer is called if no exception was thrown while trying to read the authentication and may be called with a {@code null}
         * value
         */
        private void lookForExistingAuthentication(Consumer<Authentication> authenticationConsumer) {
            Runnable action;
            try {
                final Authentication authentication = Authentication.readFromContext(threadContext);
                if (authentication != null && request instanceof AuditableRestRequest) {
                    action = () -> listener.onFailure(request.tamperedRequest());
                } else {
                    action = () -> authenticationConsumer.accept(authentication);
                }
            } catch (Exception e) {
                logger.error((Supplier<?>)
                        () -> new ParameterizedMessage("caught exception while trying to read authentication from request [{}]", request),
                        e);
                action = () -> listener.onFailure(request.tamperedRequest());
            }

            // While we could place this call in the try block, the issue is that we catch all exceptions and could catch exceptions that
            // have nothing to do with a tampered request.
            action.run();
        }

        /**
         * Attempts to extract an {@link AuthenticationToken} from the request by iterating over the {@link Realms} and calling
         * {@link Realm#token(ThreadContext)}. The first non-null token that is returned will be used. The consumer is only called if
         * no exception was caught during the extraction process and may be called with a {@code null} token.
         */
        // pkg-private accessor testing token extraction with a consumer
        void extractToken(Consumer<AuthenticationToken> consumer) {
            Runnable action = () -> consumer.accept(null);
            try {
                if (authenticationToken != null) {
                    action = () -> consumer.accept(authenticationToken);
                } else {
                    for (Realm realm : realms) {
                        final AuthenticationToken token = realm.token(threadContext);
                        if (token != null) {
                            action = () -> consumer.accept(token);
                            break;
                        }
                    }
                }
            } catch (Exception e) {
                logger.warn("An exception occurred while attempting to find authentication credentials", e);
                action = () -> listener.onFailure(request.exceptionProcessingRequest(e, null));
            }

            action.run();
        }

        /**
         * Consumes the {@link AuthenticationToken} provided by the caller. In the case of a {@code null} token, {@link #handleNullToken()}
         * is called. In the case of a {@code non-null} token, the realms are iterated over and the first realm that returns a non-null
         * {@link User} is the authenticating realm and iteration is stopped. This user is then passed to {@link #consumeUser(User, Map)}
         * if no exception was caught while trying to authenticate the token
         */
        private void consumeToken(AuthenticationToken token) {
            if (token == null) {
                handleNullToken();
            } else {
                authenticationToken = token;
                final List<Realm> realmsList = realms.asList();
                final Map<Realm, Tuple<String, Exception>> messages = new LinkedHashMap<>();
                final BiConsumer<Realm, ActionListener<User>> realmAuthenticatingConsumer = (realm, userListener) -> {
                    if (realm.supports(authenticationToken)) {
                        realm.authenticate(authenticationToken, ActionListener.wrap((result) -> {
                            assert result != null : "Realm " + realm + " produced a null authentication result";
                            if (result.getStatus() == AuthenticationResult.Status.SUCCESS) {
                                // user was authenticated, populate the authenticated by information
                                authenticatedBy = new RealmRef(realm.name(), realm.type(), nodeName);
                                userListener.onResponse(result.getUser());
                            } else {
                                // the user was not authenticated, call this so we can audit the correct event
                                request.realmAuthenticationFailed(authenticationToken, realm.name());
                                if (result.getStatus() == AuthenticationResult.Status.TERMINATE) {
                                    logger.info("Authentication of [{}] was terminated by realm [{}] - {}",
                                            authenticationToken.principal(), realm.name(), result.getMessage());
                                    userListener.onFailure(Exceptions.authenticationError(result.getMessage(), result.getException()));
                                } else {
                                    if (result.getMessage() != null) {
                                        messages.put(realm, new Tuple<>(result.getMessage(), result.getException()));
                                    }
                                    userListener.onResponse(null);
                                }
                            }
                        }, (ex) -> {
                            logger.warn(new ParameterizedMessage(
                                    "An error occurred while attempting to authenticate [{}] against realm [{}]",
                                    authenticationToken.principal(), realm.name()), ex);
                            userListener.onFailure(ex);
                        }));
                    } else {
                        userListener.onResponse(null);
                    }
                };
                final IteratingActionListener<User, Realm> authenticatingListener =
                        new IteratingActionListener<>(ActionListener.wrap(
                                (user) -> consumeUser(user, messages),
                                (e) -> listener.onFailure(request.exceptionProcessingRequest(e, token))),
                        realmAuthenticatingConsumer, realmsList, threadContext);
                try {
                    authenticatingListener.run();
                } catch (Exception e) {
                    listener.onFailure(request.exceptionProcessingRequest(e, token));
                }
            }
        }

        /**
         * Handles failed extraction of an authentication token. This can happen in a few different scenarios:
         *
         * <ul>
         *     <li>this is an initial request from a client without preemptive authentication, so we must return an authentication
         *     challenge</li>
         *     <li>this is a request made internally within a node and there is a fallback user, which is typically the
         *     {@link SystemUser}</li>
         *     <li>anonymous access is enabled and this will be considered an anonymous request</li>
         * </ul>
         *
         * Regardless of the scenario, this method will call the listener with either failure or success.
         */
        // pkg-private for tests
        void handleNullToken() {
            final Authentication authentication;
            if (fallbackUser != null) {
                RealmRef authenticatedBy = new RealmRef("__fallback", "__fallback", nodeName);
                authentication = new Authentication(fallbackUser, authenticatedBy, null);
            } else if (isAnonymousUserEnabled) {
                RealmRef authenticatedBy = new RealmRef("__anonymous", "__anonymous", nodeName);
                authentication = new Authentication(anonymousUser, authenticatedBy, null);
            } else {
                authentication = null;
            }

            Runnable action;
            if (authentication != null) {
                action = () -> writeAuthToContext(authentication);
            } else {
                action = () -> listener.onFailure(request.anonymousAccessDenied());
            }

            // we assign the listener call to an action to avoid calling the listener within a try block and auditing the wrong thing when
            // an exception bubbles up even after successful authentication
            action.run();
        }

        /**
         * Consumes the {@link User} that resulted from attempting to authenticate a token against the {@link Realms}. When the user is
         * {@code null}, authentication fails and does not proceed. When there is a user, the request is inspected to see if the run as
         * functionality is in use. When run as is not in use, {@link #finishAuthentication(User)} is called, otherwise we try to lookup
         * the run as user in {@link #lookupRunAsUser(User, String, Consumer)}
         */
        private void consumeUser(User user, Map<Realm, Tuple<String, Exception>> messages) {
            if (user == null) {
                messages.forEach((realm, tuple) -> {
                    final String message = tuple.v1();
                    final String cause = tuple.v2() == null ? "" : " (Caused by " + tuple.v2() + ")";
                    logger.warn("Authentication to realm {} failed - {}{}", realm.name(), message, cause);
                });
                listener.onFailure(request.authenticationFailed(authenticationToken));
            } else {
                if (runAsEnabled) {
                    final String runAsUsername = threadContext.getHeader(AuthenticationServiceField.RUN_AS_USER_HEADER);
                    if (runAsUsername != null && runAsUsername.isEmpty() == false) {
                        lookupRunAsUser(user, runAsUsername, this::finishAuthentication);
                    } else if (runAsUsername == null) {
                        finishAuthentication(user);
                    } else {
                        assert runAsUsername.isEmpty() : "the run as username may not be empty";
                        logger.debug("user [{}] attempted to runAs with an empty username", user.principal());
                        listener.onFailure(request.runAsDenied(
                                new Authentication(new User(runAsUsername, null, user), authenticatedBy, lookedupBy), authenticationToken));
                    }
                } else {
                    finishAuthentication(user);
                }
            }
        }

        /**
         * Iterates over the realms and attempts to lookup the run as user by the given username. The consumer will be called regardless of
         * if the user is found or not, with a non-null user. We do not fail requests if the run as user is not found as that can leak the
         * names of users that exist using a timing attack
         */
        private void lookupRunAsUser(final User user, String runAsUsername, Consumer<User> userConsumer) {
            final List<Realm> realmsList = realms.asList();
            final BiConsumer<Realm, ActionListener<User>> realmLookupConsumer = (realm, lookupUserListener) ->
                    realm.lookupUser(runAsUsername, ActionListener.wrap((lookedupUser) -> {
                        if (lookedupUser != null) {
                            lookedupBy = new RealmRef(realm.name(), realm.type(), nodeName);
                            lookupUserListener.onResponse(lookedupUser);
                        } else {
                            lookupUserListener.onResponse(null);
                        }
                    }, lookupUserListener::onFailure));

            final IteratingActionListener<User, Realm> userLookupListener =
                    new IteratingActionListener<>(ActionListener.wrap((lookupUser) -> {
                                if (lookupUser == null) {
                                    // the user does not exist, but we still create a User object, which will later be rejected by authz
                                    userConsumer.accept(new User(runAsUsername, null, user));
                                } else {
                                    userConsumer.accept(new User(lookupUser, user));
                                }
                            },
                            (e) -> listener.onFailure(request.exceptionProcessingRequest(e, authenticationToken))),
                            realmLookupConsumer, realmsList, threadContext);
            try {
                userLookupListener.run();
            } catch (Exception e) {
                listener.onFailure(request.exceptionProcessingRequest(e, authenticationToken));
            }
        }

        /**
         * Finishes the authentication process by ensuring the returned user is enabled and that the run as user is enabled if there is
         * one. If authentication is successful, this method also ensures that the authentication is written to the ThreadContext
         */
        void finishAuthentication(User finalUser) {
            if (finalUser.enabled() == false || finalUser.authenticatedUser().enabled() == false) {
                // TODO: these should be different log messages if the runas vs auth user is disabled?
                logger.debug("user [{}] is disabled. failing authentication", finalUser);
                listener.onFailure(request.authenticationFailed(authenticationToken));
            } else {
                final Authentication finalAuth = new Authentication(finalUser, authenticatedBy, lookedupBy);
                writeAuthToContext(finalAuth);
            }
        }

        /**
         * Writes the authentication to the {@link ThreadContext} and then calls the listener if
         * successful
         */
        void writeAuthToContext(Authentication authentication) {
            request.authenticationSuccess(authentication.getAuthenticatedBy().getName(), authentication.getUser());
            Runnable action = () -> listener.onResponse(authentication);
            try {
                authentication.writeToContext(threadContext);
            } catch (Exception e) {
                action = () -> listener.onFailure(request.exceptionProcessingRequest(e, authenticationToken));
            }

            // we assign the listener call to an action to avoid calling the listener within a try block and auditing the wrong thing
            // when an exception bubbles up even after successful authentication
            action.run();
        }

        private void authenticateToken(AuthenticationToken token) {
            this.consumeToken(token);
        }
    }

    abstract static class AuditableRequest {

        final AuditTrail auditTrail;
        final AuthenticationFailureHandler failureHandler;
        final ThreadContext threadContext;

        AuditableRequest(AuditTrail auditTrail, AuthenticationFailureHandler failureHandler, ThreadContext threadContext) {
            this.auditTrail = auditTrail;
            this.failureHandler = failureHandler;
            this.threadContext = threadContext;
        }

        abstract void realmAuthenticationFailed(AuthenticationToken token, String realm);

        abstract ElasticsearchSecurityException tamperedRequest();

        abstract ElasticsearchSecurityException exceptionProcessingRequest(Exception e, @Nullable AuthenticationToken token);

        abstract ElasticsearchSecurityException authenticationFailed(AuthenticationToken token);

        abstract ElasticsearchSecurityException anonymousAccessDenied();

        abstract ElasticsearchSecurityException runAsDenied(Authentication authentication, AuthenticationToken token);

        abstract void authenticationSuccess(String realm, User user);

    }

    static class AuditableTransportRequest extends AuditableRequest {

        private final String action;
        private final TransportMessage message;

        AuditableTransportRequest(AuditTrail auditTrail, AuthenticationFailureHandler failureHandler, ThreadContext threadContext,
                                  String action, TransportMessage message) {
            super(auditTrail, failureHandler, threadContext);
            this.action = action;
            this.message = message;
        }

        @Override
        void authenticationSuccess(String realm, User user) {
            auditTrail.authenticationSuccess(realm, user, action, message);
        }

        @Override
        void realmAuthenticationFailed(AuthenticationToken token, String realm) {
            auditTrail.authenticationFailed(realm, token, action, message);
        }

        @Override
        ElasticsearchSecurityException tamperedRequest() {
            auditTrail.tamperedRequest(action, message);
            return new ElasticsearchSecurityException("failed to verify signed authentication information");
        }

        @Override
        ElasticsearchSecurityException exceptionProcessingRequest(Exception e, @Nullable AuthenticationToken token) {
            if (token != null) {
                auditTrail.authenticationFailed(token, action, message);
            } else {
                auditTrail.authenticationFailed(action, message);
            }
            return failureHandler.exceptionProcessingRequest(message, action, e, threadContext);
        }

        @Override
        ElasticsearchSecurityException authenticationFailed(AuthenticationToken token) {
            auditTrail.authenticationFailed(token, action, message);
            return failureHandler.failedAuthentication(message, token, action, threadContext);
        }

        @Override
        ElasticsearchSecurityException anonymousAccessDenied() {
            auditTrail.anonymousAccessDenied(action, message);
            return failureHandler.missingToken(message, action, threadContext);
        }

        @Override
        ElasticsearchSecurityException runAsDenied(Authentication authentication, AuthenticationToken token) {
            auditTrail.runAsDenied(authentication, action, message, Role.EMPTY.names());
            return failureHandler.failedAuthentication(message, token, action, threadContext);
        }

        @Override
        public String toString() {
            return "transport request action [" + action + "]";
        }

    }

    static class AuditableRestRequest extends AuditableRequest {

        private final RestRequest request;

        @SuppressWarnings("unchecked")
        AuditableRestRequest(AuditTrail auditTrail, AuthenticationFailureHandler failureHandler, ThreadContext threadContext,
                             RestRequest request) {
            super(auditTrail, failureHandler, threadContext);
            this.request = request;
        }

        @Override
        void authenticationSuccess(String realm, User user) {
            auditTrail.authenticationSuccess(realm, user, request);
        }

        @Override
        void realmAuthenticationFailed(AuthenticationToken token, String realm) {
            auditTrail.authenticationFailed(realm, token, request);
        }

        @Override
        ElasticsearchSecurityException tamperedRequest() {
            auditTrail.tamperedRequest(request);
            return new ElasticsearchSecurityException("rest request attempted to inject a user");
        }

        @Override
        ElasticsearchSecurityException exceptionProcessingRequest(Exception e, @Nullable AuthenticationToken token) {
            if (token != null) {
                auditTrail.authenticationFailed(token, request);
            } else {
                auditTrail.authenticationFailed(request);
            }
            return failureHandler.exceptionProcessingRequest(request, e, threadContext);
        }

        @Override
        ElasticsearchSecurityException authenticationFailed(AuthenticationToken token) {
            auditTrail.authenticationFailed(token, request);
            return failureHandler.failedAuthentication(request, token, threadContext);
        }

        @Override
        ElasticsearchSecurityException anonymousAccessDenied() {
            auditTrail.anonymousAccessDenied(request);
            return failureHandler.missingToken(request, threadContext);
        }

        @Override
        ElasticsearchSecurityException runAsDenied(Authentication authentication, AuthenticationToken token) {
            auditTrail.runAsDenied(authentication, request, Role.EMPTY.names());
            return failureHandler.failedAuthentication(request, token, threadContext);
        }

        @Override
        public String toString() {
            return "rest request uri [" + request.uri() + "]";
        }
    }

    public static void addSettings(List<Setting<?>> settings) {
        settings.add(AuthenticationServiceField.RUN_AS_ENABLED);
    }
}
