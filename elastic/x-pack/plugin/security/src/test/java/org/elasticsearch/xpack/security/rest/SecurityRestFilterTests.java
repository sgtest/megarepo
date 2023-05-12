/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.security.rest;

import com.nimbusds.jose.util.StandardCharset;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.http.HttpChannel;
import org.elasticsearch.http.HttpRequest;
import org.elasticsearch.license.TestUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.rest.RestChannel;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestRequestFilter;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.SecuritySettingsSourceField;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.xcontent.DeprecationHandler;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xcontent.json.JsonXContent;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.AuthenticationTestHelper;
import org.elasticsearch.xpack.core.security.authc.support.SecondaryAuthentication;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;
import org.elasticsearch.xpack.security.audit.AuditTrail;
import org.elasticsearch.xpack.security.audit.AuditTrailService;
import org.elasticsearch.xpack.security.authc.AuthenticationService;
import org.elasticsearch.xpack.security.authc.support.SecondaryAuthenticator;
import org.junit.Before;
import org.mockito.ArgumentCaptor;

import java.util.Base64;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;

import static org.elasticsearch.test.ActionListenerUtils.anyActionListener;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.sameInstance;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.verifyNoMoreInteractions;
import static org.mockito.Mockito.when;

public class SecurityRestFilterTests extends ESTestCase {

    private ThreadContext threadContext;
    private AuthenticationService authcService;
    private SecondaryAuthenticator secondaryAuthenticator;
    private RestChannel channel;
    private SecurityRestFilter filter;
    private RestHandler restHandler;

    @Before
    public void init() throws Exception {
        authcService = mock(AuthenticationService.class);
        channel = mock(RestChannel.class);
        restHandler = mock(RestHandler.class);
        threadContext = new ThreadContext(Settings.EMPTY);
        secondaryAuthenticator = new SecondaryAuthenticator(Settings.EMPTY, threadContext, authcService, new AuditTrailService(null, null));
        filter = new SecurityRestFilter(true, threadContext, secondaryAuthenticator, new AuditTrailService(null, null), restHandler);
    }

    public void testProcess() throws Exception {
        RestRequest request = mock(RestRequest.class);
        when(request.getHttpChannel()).thenReturn(mock(HttpChannel.class));
        HttpRequest httpRequest = mock(HttpRequest.class);
        when(request.getHttpRequest()).thenReturn(httpRequest);
        Authentication authentication = AuthenticationTestHelper.builder().build();
        doAnswer((i) -> {
            @SuppressWarnings("unchecked")
            ActionListener<Authentication> callback = (ActionListener<Authentication>) i.getArguments()[1];
            callback.onResponse(authentication);
            return Void.TYPE;
        }).when(authcService).authenticate(eq(httpRequest), anyActionListener());
        filter.handleRequest(request, channel, null);
        verify(restHandler).handleRequest(request, channel, null);
        verifyNoMoreInteractions(channel);
    }

    public void testProcessSecondaryAuthentication() throws Exception {
        RestRequest request = mock(RestRequest.class);
        when(channel.request()).thenReturn(request);
        when(request.getHttpChannel()).thenReturn(mock(HttpChannel.class));
        HttpRequest httpRequest = mock(HttpRequest.class);
        when(request.getHttpRequest()).thenReturn(httpRequest);

        Authentication primaryAuthentication = AuthenticationTestHelper.builder().build();
        doAnswer(i -> {
            final Object[] arguments = i.getArguments();
            @SuppressWarnings("unchecked")
            ActionListener<Authentication> callback = (ActionListener<Authentication>) arguments[arguments.length - 1];
            callback.onResponse(primaryAuthentication);
            return null;
        }).when(authcService).authenticate(eq(httpRequest), anyActionListener());

        Authentication secondaryAuthentication = AuthenticationTestHelper.builder().build();
        doAnswer(i -> {
            final Object[] arguments = i.getArguments();
            @SuppressWarnings("unchecked")
            ActionListener<Authentication> callback = (ActionListener<Authentication>) arguments[arguments.length - 1];
            callback.onResponse(secondaryAuthentication);
            return null;
        }).when(authcService).authenticate(eq(httpRequest), eq(false), anyActionListener());

        SecurityContext securityContext = new SecurityContext(Settings.EMPTY, threadContext);
        AtomicReference<SecondaryAuthentication> secondaryAuthRef = new AtomicReference<>();
        doAnswer(i -> {
            secondaryAuthRef.set(securityContext.getSecondaryAuthentication());
            return null;
        }).when(restHandler).handleRequest(request, channel, null);

        final String credentials = randomAlphaOfLengthBetween(4, 8) + ":" + randomAlphaOfLengthBetween(4, 12);
        threadContext.putHeader(
            SecondaryAuthenticator.SECONDARY_AUTH_HEADER_NAME,
            "Basic " + Base64.getEncoder().encodeToString(credentials.getBytes(StandardCharset.UTF_8))
        );
        filter.handleRequest(request, channel, null);
        verify(restHandler).handleRequest(request, channel, null);
        verifyNoMoreInteractions(channel);

        assertThat(secondaryAuthRef.get(), notNullValue());
        assertThat(secondaryAuthRef.get().getAuthentication(), sameInstance(secondaryAuthentication));
    }

    public void testProcessWithSecurityDisabled() throws Exception {
        filter = new SecurityRestFilter(false, threadContext, secondaryAuthenticator, mock(AuditTrailService.class), restHandler);
        RestRequest request = mock(RestRequest.class);
        filter.handleRequest(request, channel, null);
        verify(restHandler).handleRequest(request, channel, null);
        verifyNoMoreInteractions(channel, authcService);
    }

    public void testProcessOptionsMethod() throws Exception {
        FakeRestRequest request = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withMethod(RestRequest.Method.OPTIONS).build();
        when(channel.request()).thenReturn(request);
        when(channel.newErrorBuilder()).thenReturn(JsonXContent.contentBuilder());
        filter.handleRequest(request, channel, null);
        verifyNoMoreInteractions(restHandler);
        verifyNoMoreInteractions(authcService);
        ArgumentCaptor<RestResponse> responseArgumentCaptor = ArgumentCaptor.forClass(RestResponse.class);
        verify(channel).sendResponse(responseArgumentCaptor.capture());
        RestResponse restResponse = responseArgumentCaptor.getValue();
        assertThat(restResponse.status(), is(RestStatus.INTERNAL_SERVER_ERROR));
        assertThat(restResponse.content().utf8ToString(), containsString("Cannot dispatch OPTIONS request, as they are not authenticated"));
    }

    public void testProcessFiltersBodyCorrectly() throws Exception {
        FakeRestRequest restRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(
            new BytesArray("{\"password\": \"" + SecuritySettingsSourceField.TEST_PASSWORD + "\", \"foo\": \"bar\"}"),
            XContentType.JSON
        ).build();
        when(channel.request()).thenReturn(restRequest);
        SetOnce<RestRequest> handlerRequest = new SetOnce<>();
        restHandler = new FilteredRestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                handlerRequest.set(request);
            }

            @Override
            public Set<String> getFilteredFields() {
                return Collections.singleton("password");
            }
        };
        AuditTrail auditTrail = mock(AuditTrail.class);
        XPackLicenseState licenseState = TestUtils.newTestLicenseState();
        SetOnce<RestRequest> auditTrailRequest = new SetOnce<>();
        doAnswer((i) -> {
            auditTrailRequest.set((RestRequest) i.getArguments()[0]);
            return Void.TYPE;
        }).when(auditTrail).authenticationSuccess(any(RestRequest.class));
        filter = new SecurityRestFilter(
            true,
            threadContext,
            secondaryAuthenticator,
            new AuditTrailService(auditTrail, licenseState),
            restHandler
        );

        filter.handleRequest(restRequest, channel, null);

        assertEquals(restRequest, handlerRequest.get());
        assertEquals(restRequest.content(), handlerRequest.get().content());
        Map<String, Object> original = XContentType.JSON.xContent()
            .createParser(
                NamedXContentRegistry.EMPTY,
                DeprecationHandler.THROW_UNSUPPORTED_OPERATION,
                handlerRequest.get().content().streamInput()
            )
            .map();
        assertEquals(2, original.size());
        assertEquals(SecuritySettingsSourceField.TEST_PASSWORD, original.get("password"));
        assertEquals("bar", original.get("foo"));

        assertNotEquals(restRequest, auditTrailRequest.get());
        assertNotEquals(restRequest.content(), auditTrailRequest.get().content());

        Map<String, Object> map = XContentType.JSON.xContent()
            .createParser(
                NamedXContentRegistry.EMPTY,
                DeprecationHandler.THROW_UNSUPPORTED_OPERATION,
                auditTrailRequest.get().content().streamInput()
            )
            .map();
        assertEquals(1, map.size());
        assertEquals("bar", map.get("foo"));
    }

    public void testSanitizeHeaders() throws Exception {
        for (boolean failRequest : List.of(true, false)) {
            threadContext.putHeader(UsernamePasswordToken.BASIC_AUTH_HEADER, randomAlphaOfLengthBetween(1, 10));
            RestRequest request = mock(RestRequest.class);
            when(request.getHttpChannel()).thenReturn(mock(HttpChannel.class));
            HttpRequest httpRequest = mock(HttpRequest.class);
            when(request.getHttpRequest()).thenReturn(httpRequest);
            Authentication authentication = AuthenticationTestHelper.builder().build();
            doAnswer((i) -> {
                @SuppressWarnings("unchecked")
                ActionListener<Authentication> callback = (ActionListener<Authentication>) i.getArguments()[1];
                if (failRequest) {
                    callback.onFailure(new RuntimeException());
                } else {
                    callback.onResponse(authentication);
                }
                return Void.TYPE;
            }).when(authcService).authenticate(eq(httpRequest), anyActionListener());
            Set<String> foundKeys = threadContext.getHeaders().keySet();
            assertThat(foundKeys, hasItem(UsernamePasswordToken.BASIC_AUTH_HEADER));

            filter.handleRequest(request, channel, null);

            foundKeys = threadContext.getHeaders().keySet();
            assertThat(foundKeys, not(hasItem(UsernamePasswordToken.BASIC_AUTH_HEADER)));
        }
    }

    private interface FilteredRestHandler extends RestHandler, RestRequestFilter {}
}
