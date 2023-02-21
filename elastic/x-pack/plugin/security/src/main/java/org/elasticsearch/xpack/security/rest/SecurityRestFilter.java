/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.security.rest;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.util.Supplier;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.util.Maps;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.http.HttpChannel;
import org.elasticsearch.rest.RestChannel;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestRequest.Method;
import org.elasticsearch.rest.RestRequestFilter;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.xpack.security.authc.AuthenticationService;
import org.elasticsearch.xpack.security.authc.support.SecondaryAuthenticator;
import org.elasticsearch.xpack.security.transport.SSLEngineUtils;

import java.util.List;
import java.util.Map;

import static org.elasticsearch.core.Strings.format;

public class SecurityRestFilter implements RestHandler {

    private static final Logger logger = LogManager.getLogger(SecurityRestFilter.class);

    private final RestHandler restHandler;
    private final AuthenticationService authenticationService;
    private final SecondaryAuthenticator secondaryAuthenticator;
    private final boolean enabled;
    private final ThreadContext threadContext;
    private final boolean extractClientCertificate;

    public enum ActionType {
        Authentication("Authentication"),
        SecondaryAuthentication("Secondary authentication"),
        RequestHandling("Request handling");

        private final String name;

        ActionType(String name) {
            this.name = name;
        }

        @Override
        public String toString() {
            return name;
        }
    }

    public SecurityRestFilter(
        boolean enabled,
        ThreadContext threadContext,
        AuthenticationService authenticationService,
        SecondaryAuthenticator secondaryAuthenticator,
        RestHandler restHandler,
        boolean extractClientCertificate
    ) {
        this.enabled = enabled;
        this.threadContext = threadContext;
        this.authenticationService = authenticationService;
        this.secondaryAuthenticator = secondaryAuthenticator;
        this.restHandler = restHandler;
        this.extractClientCertificate = extractClientCertificate;
    }

    @Override
    public boolean allowSystemIndexAccessByDefault() {
        return restHandler.allowSystemIndexAccessByDefault();
    }

    public RestHandler getConcreteRestHandler() {
        return restHandler.getConcreteRestHandler();
    }

    @Override
    public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
        if (request.method() == Method.OPTIONS) {
            // CORS - allow for preflight unauthenticated OPTIONS request
            restHandler.handleRequest(request, channel, client);
            return;
        }

        if (enabled == false) {
            doHandleRequest(request, channel, client);
            return;
        }

        if (extractClientCertificate) {
            HttpChannel httpChannel = request.getHttpChannel();
            SSLEngineUtils.extractClientCertificates(logger, threadContext, httpChannel);
        }

        authenticationService.authenticate(maybeWrapRestRequest(request), ActionListener.wrap(authentication -> {
            if (authentication == null) {
                logger.trace("No authentication available for REST request [{}]", request.uri());
            } else {
                logger.trace("Authenticated REST request [{}] as {}", request.uri(), authentication);
            }
            secondaryAuthenticator.authenticateAndAttachToContext(request, ActionListener.wrap(secondaryAuthentication -> {
                if (secondaryAuthentication != null) {
                    logger.trace("Found secondary authentication {} in REST request [{}]", secondaryAuthentication, request.uri());
                }
                RemoteHostHeader.process(request, threadContext);
                try {
                    doHandleRequest(request, channel, client);
                } catch (Exception e) {
                    handleException(ActionType.RequestHandling, request, channel, e);
                }
            }, e -> handleException(ActionType.SecondaryAuthentication, request, channel, e)));
        }, e -> handleException(ActionType.Authentication, request, channel, e)));
    }

    private void doHandleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
        threadContext.sanitizeHeaders();
        restHandler.handleRequest(request, channel, client);
    }

    protected void handleException(ActionType actionType, RestRequest request, RestChannel channel, Exception e) {
        logger.debug(() -> format("%s failed for REST request [%s]", actionType, request.uri()), e);
        threadContext.sanitizeHeaders();
        final RestStatus restStatus = ExceptionsHelper.status(e);
        try {
            channel.sendResponse(new RestResponse(channel, restStatus, e) {

                @Override
                protected boolean skipStackTrace() {
                    return restStatus == RestStatus.UNAUTHORIZED;
                }

                @Override
                public Map<String, List<String>> filterHeaders(Map<String, List<String>> headers) {
                    if (actionType != ActionType.RequestHandling
                        || (restStatus == RestStatus.UNAUTHORIZED || restStatus == RestStatus.FORBIDDEN)) {
                        if (headers.containsKey("Warning")) {
                            headers = Maps.copyMapWithRemovedEntry(headers, "Warning");
                        }
                        if (headers.containsKey("X-elastic-product")) {
                            headers = Maps.copyMapWithRemovedEntry(headers, "X-elastic-product");
                        }
                    }
                    return headers;
                }

            });
        } catch (Exception inner) {
            inner.addSuppressed(e);
            logger.error((Supplier<?>) () -> "failed to send failure response for uri [" + request.uri() + "]", inner);
        }
    }

    @Override
    public boolean canTripCircuitBreaker() {
        return restHandler.canTripCircuitBreaker();
    }

    @Override
    public boolean supportsContentStream() {
        return restHandler.supportsContentStream();
    }

    @Override
    public boolean allowsUnsafeBuffers() {
        return restHandler.allowsUnsafeBuffers();
    }

    @Override
    public List<Route> routes() {
        return restHandler.routes();
    }

    private RestRequest maybeWrapRestRequest(RestRequest restRequest) {
        if (restHandler instanceof RestRequestFilter) {
            return ((RestRequestFilter) restHandler).getFilteredRequest(restRequest);
        }
        return restRequest;
    }

    @Override
    public boolean mediaTypesValid(RestRequest request) {
        return restHandler.mediaTypesValid(request);
    }
}
