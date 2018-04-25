/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.action.saml;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.GroupedActionListener;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.security.action.saml.SamlInvalidateSessionAction;
import org.elasticsearch.xpack.core.security.action.saml.SamlInvalidateSessionRequest;
import org.elasticsearch.xpack.core.security.action.saml.SamlInvalidateSessionResponse;
import org.elasticsearch.xpack.security.authc.Realms;
import org.elasticsearch.xpack.security.authc.TokenService;
import org.elasticsearch.xpack.security.authc.UserToken;
import org.elasticsearch.xpack.security.authc.saml.SamlLogoutRequestHandler;
import org.elasticsearch.xpack.security.authc.saml.SamlRealm;
import org.elasticsearch.xpack.security.authc.saml.SamlRedirect;
import org.elasticsearch.xpack.security.authc.saml.SamlUtils;
import org.opensaml.saml.saml2.core.LogoutResponse;

import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.stream.Collectors;

import static org.elasticsearch.xpack.security.authc.saml.SamlRealm.findSamlRealms;

/**
 * Transport action responsible for taking a SAML {@code LogoutRequest} and invalidating any associated Security Tokens
 */
public final class TransportSamlInvalidateSessionAction
        extends HandledTransportAction<SamlInvalidateSessionRequest, SamlInvalidateSessionResponse> {

    private final TokenService tokenService;
    private final Realms realms;

    @Inject
    public TransportSamlInvalidateSessionAction(Settings settings, ThreadPool threadPool, TransportService transportService,
                                                ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver,
                                                TokenService tokenService, Realms realms) {
        super(settings, SamlInvalidateSessionAction.NAME, threadPool, transportService, actionFilters, indexNameExpressionResolver,
                SamlInvalidateSessionRequest::new);
        this.tokenService = tokenService;
        this.realms = realms;
    }

    @Override
    protected void doExecute(SamlInvalidateSessionRequest request,
                             ActionListener<SamlInvalidateSessionResponse> listener) {
        List<SamlRealm> realms = findSamlRealms(this.realms, request.getRealmName(), request.getAssertionConsumerServiceURL());
        if (realms.isEmpty()) {
            listener.onFailure(SamlUtils.samlException("Cannot find any matching realm for [{}]", request));
        } else if (realms.size() > 1) {
            listener.onFailure(SamlUtils.samlException("Found multiple matching realms [{}] for [{}]", realms, request));
        } else {
            invalidateSession(realms.get(0), request, listener);
        }
    }

    private void invalidateSession(SamlRealm realm, SamlInvalidateSessionRequest request,
                                   ActionListener<SamlInvalidateSessionResponse> listener) {
        try {
            final SamlLogoutRequestHandler.Result result = realm.getLogoutHandler().parseFromQueryString(request.getQueryString());
            findAndInvalidateTokens(realm, result, ActionListener.wrap(count -> listener.onResponse(
                    new SamlInvalidateSessionResponse(realm.name(), count, buildLogoutResponseUrl(realm, result))
            ), listener::onFailure));
        } catch (ElasticsearchSecurityException e) {
            logger.info("Failed to invalidate SAML session", e);
            listener.onFailure(e);
        }
    }

    private String buildLogoutResponseUrl(SamlRealm realm, SamlLogoutRequestHandler.Result result) {
        final LogoutResponse response = realm.buildLogoutResponse(result.getRequestId());
        return new SamlRedirect(response, realm.getSigningConfiguration()).getRedirectUrl(result.getRelayState());
    }

    private void findAndInvalidateTokens(SamlRealm realm, SamlLogoutRequestHandler.Result result, ActionListener<Integer> listener) {
        final Map<String, Object> tokenMetadata = realm.createTokenMetadata(result.getNameId(), result.getSession());
        if (Strings.hasText((String) tokenMetadata.get(SamlRealm.TOKEN_METADATA_NAMEID_VALUE)) == false) {
            // If we don't have a valid name-id to match against, don't do anything
            logger.debug("Logout request [{}] has no NameID value, so cannot invalidate any sessions", result);
            listener.onResponse(0);
            return;
        }

        tokenService.findActiveTokensForRealm(realm.name(), ActionListener.wrap(tokens -> {
                    List<Tuple<UserToken, String>> sessionTokens = filterTokens(tokens, tokenMetadata);
                    logger.debug("Found [{}] token pairs to invalidate for SAML metadata [{}]", sessionTokens.size(), tokenMetadata);
                    if (sessionTokens.isEmpty()) {
                        listener.onResponse(0);
                    } else {
                        GroupedActionListener<Boolean> groupedListener = new GroupedActionListener<>(
                                ActionListener.wrap(collection -> listener.onResponse(collection.size()), listener::onFailure),
                                sessionTokens.size(), Collections.emptyList()
                        );
                        sessionTokens.forEach(tuple -> invalidateTokenPair(tuple, groupedListener));
                    }
                }, e -> listener.onFailure(e)
        ));
    }

    private void invalidateTokenPair(Tuple<UserToken, String> tokenPair, ActionListener<Boolean> listener) {
        // Invalidate the refresh token first, so the client doesn't trigger a refresh once the access token is invalidated
        tokenService.invalidateRefreshToken(tokenPair.v2(), ActionListener.wrap(ignore -> tokenService.invalidateAccessToken(
                tokenPair.v1(),
                ActionListener.wrap(listener::onResponse, e -> {
                    logger.info("Failed to invalidate SAML access_token [{}] - {}", tokenPair.v1().getId(), e.toString());
                    listener.onFailure(e);
                })), listener::onFailure));
    }

    private List<Tuple<UserToken, String>> filterTokens(Collection<Tuple<UserToken, String>> tokens, Map<String, Object> requiredMetadata) {
        return tokens.stream()
                .filter(tup -> {
                    Map<String, Object> actualMetadata = tup.v1().getMetadata();
                    return requiredMetadata.entrySet().stream().allMatch(e -> Objects.equals(actualMetadata.get(e.getKey()), e.getValue()));
                })
                .collect(Collectors.toList());
    }

}
