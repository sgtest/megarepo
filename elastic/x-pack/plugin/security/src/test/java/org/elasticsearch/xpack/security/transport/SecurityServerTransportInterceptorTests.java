/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.transport;

import org.elasticsearch.Version;
import org.elasticsearch.action.support.DestructiveOperations;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.Transport;
import org.elasticsearch.transport.Transport.Connection;
import org.elasticsearch.transport.TransportException;
import org.elasticsearch.transport.TransportInterceptor.AsyncSender;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.transport.TransportRequestOptions;
import org.elasticsearch.transport.TransportResponse;
import org.elasticsearch.transport.TransportResponse.Empty;
import org.elasticsearch.transport.TransportResponseHandler;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.Authentication.RealmRef;
import org.elasticsearch.xpack.core.security.user.SystemUser;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.security.authc.AuthenticationService;
import org.elasticsearch.xpack.security.authz.AuthorizationService;

import java.util.Collections;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

import static org.mockito.Matchers.any;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.spy;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.verifyNoMoreInteractions;
import static org.mockito.Mockito.verifyZeroInteractions;
import static org.mockito.Mockito.when;

public class SecurityServerTransportInterceptorTests extends ESTestCase {

    private Settings settings;
    private ThreadPool threadPool;
    private ThreadContext threadContext;
    private XPackLicenseState xPackLicenseState;
    private SecurityContext securityContext;

    @Override
    public void setUp() throws Exception {
        super.setUp();
        settings = Settings.builder().put("path.home", createTempDir()).build();
        threadPool = mock(ThreadPool.class);
        threadContext = new ThreadContext(settings);
        when(threadPool.getThreadContext()).thenReturn(threadContext);
        securityContext = spy(new SecurityContext(settings, threadPool.getThreadContext()));
        xPackLicenseState = mock(XPackLicenseState.class);
        when(xPackLicenseState.isAuthAllowed()).thenReturn(true);
        when(xPackLicenseState.isSecurityEnabled()).thenReturn(true);
    }

    public void testSendAsyncUnlicensed() {
        SecurityServerTransportInterceptor interceptor = new SecurityServerTransportInterceptor(settings, threadPool,
                mock(AuthenticationService.class), mock(AuthorizationService.class), xPackLicenseState, mock(SSLService.class),
                securityContext, new DestructiveOperations(Settings.EMPTY, new ClusterSettings(Settings.EMPTY,
                Collections.singleton(DestructiveOperations.REQUIRES_NAME_SETTING))));
        when(xPackLicenseState.isAuthAllowed()).thenReturn(false);
        AtomicBoolean calledWrappedSender = new AtomicBoolean(false);
        AsyncSender sender = interceptor.interceptSender(new AsyncSender() {
            @Override
            public <T extends TransportResponse> void sendRequest(Transport.Connection connection, String action, TransportRequest request,
                                                                  TransportRequestOptions options, TransportResponseHandler<T> handler) {
                if (calledWrappedSender.compareAndSet(false, true) == false) {
                    fail("sender called more than once!");
                }
            }
        });
        sender.sendRequest(null, null, null, null, null);
        assertTrue(calledWrappedSender.get());
        verify(xPackLicenseState).isAuthAllowed();
        verify(xPackLicenseState).isSecurityEnabled();
        verifyNoMoreInteractions(xPackLicenseState);
        verifyZeroInteractions(securityContext);
    }

    public void testSendAsync() throws Exception {
        final User authUser = randomBoolean() ? new User("authenticator") : null;
        final User user = new User("test", randomRoles(), authUser);
        final Authentication authentication = new Authentication(user, new RealmRef("ldap", "foo", "node1"), null);
        authentication.writeToContext(threadContext);
        SecurityServerTransportInterceptor interceptor = new SecurityServerTransportInterceptor(settings, threadPool,
                mock(AuthenticationService.class), mock(AuthorizationService.class), xPackLicenseState, mock(SSLService.class),
                securityContext, new DestructiveOperations(Settings.EMPTY, new ClusterSettings(Settings.EMPTY,
                Collections.singleton(DestructiveOperations.REQUIRES_NAME_SETTING))));

        AtomicBoolean calledWrappedSender = new AtomicBoolean(false);
        AtomicReference<User> sendingUser = new AtomicReference<>();
        AsyncSender sender = interceptor.interceptSender(new AsyncSender() {
            @Override
            public <T extends TransportResponse> void sendRequest(Transport.Connection connection, String action, TransportRequest request,
                                                                  TransportRequestOptions options, TransportResponseHandler<T> handler) {
                if (calledWrappedSender.compareAndSet(false, true) == false) {
                    fail("sender called more than once!");
                }
                sendingUser.set(securityContext.getUser());
            }
        });
        Transport.Connection connection = mock(Transport.Connection.class);
        when(connection.getVersion()).thenReturn(Version.CURRENT);
        sender.sendRequest(connection, "indices:foo", null, null, null);
        assertTrue(calledWrappedSender.get());
        assertEquals(user, sendingUser.get());
        assertEquals(user, securityContext.getUser());
        verify(xPackLicenseState).isAuthAllowed();
        verify(xPackLicenseState).isSecurityEnabled();
        verify(securityContext, never()).executeAsUser(any(User.class), any(Consumer.class), any(Version.class));
        verifyNoMoreInteractions(xPackLicenseState);
    }

    public void testSendAsyncSwitchToSystem() throws Exception {
        final User authUser = randomBoolean() ? new User("authenticator") : null;
        final User user = new User("test", randomRoles(), authUser);
        final Authentication authentication = new Authentication(user, new RealmRef("ldap", "foo", "node1"), null);
        authentication.writeToContext(threadContext);
        threadContext.putTransient(AuthorizationService.ORIGINATING_ACTION_KEY, "indices:foo");

        SecurityServerTransportInterceptor interceptor = new SecurityServerTransportInterceptor(settings, threadPool,
                mock(AuthenticationService.class), mock(AuthorizationService.class), xPackLicenseState, mock(SSLService.class),
                securityContext, new DestructiveOperations(Settings.EMPTY, new ClusterSettings(Settings.EMPTY,
                Collections.singleton(DestructiveOperations.REQUIRES_NAME_SETTING))));

        AtomicBoolean calledWrappedSender = new AtomicBoolean(false);
        AtomicReference<User> sendingUser = new AtomicReference<>();
        AsyncSender sender = interceptor.interceptSender(new AsyncSender() {
            @Override
            public <T extends TransportResponse> void sendRequest(Transport.Connection connection, String action, TransportRequest request,
                                                                  TransportRequestOptions options, TransportResponseHandler<T> handler) {
                if (calledWrappedSender.compareAndSet(false, true) == false) {
                    fail("sender called more than once!");
                }
                sendingUser.set(securityContext.getUser());
            }
        });
        Connection connection = mock(Connection.class);
        when(connection.getVersion()).thenReturn(Version.CURRENT);
        sender.sendRequest(connection, "internal:foo", null, null, null);
        assertTrue(calledWrappedSender.get());
        assertNotEquals(user, sendingUser.get());
        assertEquals(SystemUser.INSTANCE, sendingUser.get());
        assertEquals(user, securityContext.getUser());
        verify(xPackLicenseState).isAuthAllowed();
        verify(xPackLicenseState).isSecurityEnabled();
        verify(securityContext).executeAsUser(any(User.class), any(Consumer.class), eq(Version.CURRENT));
        verifyNoMoreInteractions(xPackLicenseState);
    }

    public void testSendWithoutUser() throws Exception {
        SecurityServerTransportInterceptor interceptor = new SecurityServerTransportInterceptor(settings, threadPool,
                mock(AuthenticationService.class), mock(AuthorizationService.class), xPackLicenseState, mock(SSLService.class),
                securityContext, new DestructiveOperations(Settings.EMPTY, new ClusterSettings(Settings.EMPTY,
                Collections.singleton(DestructiveOperations.REQUIRES_NAME_SETTING)))) {
            @Override
            void assertNoAuthentication(String action) {
            }
        };

        assertNull(securityContext.getUser());
        AsyncSender sender = interceptor.interceptSender(new AsyncSender() {
            @Override
            public <T extends TransportResponse> void sendRequest(Transport.Connection connection, String action, TransportRequest request,
                                                                  TransportRequestOptions options, TransportResponseHandler<T> handler) {
                fail("sender should not be called!");
            }
        });
        Transport.Connection connection = mock(Transport.Connection.class);
        when(connection.getVersion()).thenReturn(Version.CURRENT);
        IllegalStateException e =
                expectThrows(IllegalStateException.class, () -> sender.sendRequest(connection, "indices:foo", null, null, null));
        assertEquals("there should always be a user when sending a message for action [indices:foo]", e.getMessage());
        assertNull(securityContext.getUser());
        verify(xPackLicenseState).isAuthAllowed();
        verify(xPackLicenseState).isSecurityEnabled();
        verify(securityContext, never()).executeAsUser(any(User.class), any(Consumer.class), any(Version.class));
        verifyNoMoreInteractions(xPackLicenseState);
    }

    public void testSendToNewerVersionSetsCorrectVersion() throws Exception {
        final User authUser = randomBoolean() ? new User("authenticator") : null;
        final User user = new User("joe", randomRoles(), authUser);
        final Authentication authentication = new Authentication(user, new RealmRef("file", "file", "node1"), null);
        authentication.writeToContext(threadContext);
        threadContext.putTransient(AuthorizationService.ORIGINATING_ACTION_KEY, "indices:foo");

        SecurityServerTransportInterceptor interceptor = new SecurityServerTransportInterceptor(settings, threadPool,
                mock(AuthenticationService.class), mock(AuthorizationService.class), xPackLicenseState, mock(SSLService.class),
                securityContext, new DestructiveOperations(Settings.EMPTY, new ClusterSettings(Settings.EMPTY,
                Collections.singleton(DestructiveOperations.REQUIRES_NAME_SETTING))));

        AtomicBoolean calledWrappedSender = new AtomicBoolean(false);
        AtomicReference<User> sendingUser = new AtomicReference<>();
        AtomicReference<Authentication> authRef = new AtomicReference<>();
        AsyncSender intercepted = new AsyncSender() {
            @Override
            public <T extends TransportResponse> void sendRequest(Transport.Connection connection, String action, TransportRequest request,
                                                                  TransportRequestOptions options, TransportResponseHandler<T> handler) {
                if (calledWrappedSender.compareAndSet(false, true) == false) {
                    fail("sender called more than once!");
                }
                sendingUser.set(securityContext.getUser());
                authRef.set(securityContext.getAuthentication());
            }
        };
        AsyncSender sender = interceptor.interceptSender(intercepted);
        final Version connectionVersion = Version.fromId(Version.CURRENT.id + randomIntBetween(100, 100000));
        assertEquals(Version.CURRENT, Version.min(connectionVersion, Version.CURRENT));

        Transport.Connection connection = mock(Transport.Connection.class);
        when(connection.getVersion()).thenReturn(connectionVersion);
        sender.sendRequest(connection, "indices:foo[s]", null, null, null);
        assertTrue(calledWrappedSender.get());
        assertEquals(user, sendingUser.get());
        assertEquals(user, securityContext.getUser());
        assertEquals(Version.CURRENT, authRef.get().getVersion());
        assertEquals(Version.CURRENT, authentication.getVersion());
    }

    public void testSendToOlderVersionSetsCorrectVersion() throws Exception {
        final User authUser = randomBoolean() ? new User("authenticator") : null;
        final User user = new User("joe", randomRoles(), authUser);
        final Authentication authentication = new Authentication(user, new RealmRef("file", "file", "node1"), null);
        authentication.writeToContext(threadContext);
        threadContext.putTransient(AuthorizationService.ORIGINATING_ACTION_KEY, "indices:foo");

        SecurityServerTransportInterceptor interceptor = new SecurityServerTransportInterceptor(settings, threadPool,
                mock(AuthenticationService.class), mock(AuthorizationService.class), xPackLicenseState, mock(SSLService.class),
                securityContext, new DestructiveOperations(Settings.EMPTY, new ClusterSettings(Settings.EMPTY,
                Collections.singleton(DestructiveOperations.REQUIRES_NAME_SETTING))));

        AtomicBoolean calledWrappedSender = new AtomicBoolean(false);
        AtomicReference<User> sendingUser = new AtomicReference<>();
        AtomicReference<Authentication> authRef = new AtomicReference<>();
        AsyncSender intercepted = new AsyncSender() {
            @Override
            public <T extends TransportResponse> void sendRequest(Transport.Connection connection, String action, TransportRequest request,
                                                                  TransportRequestOptions options, TransportResponseHandler<T> handler) {
                if (calledWrappedSender.compareAndSet(false, true) == false) {
                    fail("sender called more than once!");
                }
                sendingUser.set(securityContext.getUser());
                authRef.set(securityContext.getAuthentication());
            }
        };
        AsyncSender sender = interceptor.interceptSender(intercepted);
        final Version connectionVersion = Version.fromId(Version.CURRENT.id - randomIntBetween(100, 100000));
        assertEquals(connectionVersion, Version.min(connectionVersion, Version.CURRENT));

        Transport.Connection connection = mock(Transport.Connection.class);
        when(connection.getVersion()).thenReturn(connectionVersion);
        sender.sendRequest(connection, "indices:foo[s]", null, null, null);
        assertTrue(calledWrappedSender.get());
        assertEquals(user, sendingUser.get());
        assertEquals(user, securityContext.getUser());
        assertEquals(connectionVersion, authRef.get().getVersion());
        assertEquals(Version.CURRENT, authentication.getVersion());
    }

    public void testContextRestoreResponseHandler() throws Exception {
        ThreadContext threadContext = new ThreadContext(Settings.EMPTY);

        threadContext.putTransient("foo", "bar");
        threadContext.putHeader("key", "value");
        try (ThreadContext.StoredContext storedContext = threadContext.stashContext()) {
            threadContext.putTransient("foo", "different_bar");
            threadContext.putHeader("key", "value2");
            TransportResponseHandler<Empty> handler = new TransportService.ContextRestoreResponseHandler<>(
                    threadContext.wrapRestorable(storedContext), new TransportResponseHandler<Empty>() {

                @Override
                public Empty newInstance() {
                    return Empty.INSTANCE;
                }

                @Override
                public void handleResponse(Empty response) {
                    assertEquals("bar", threadContext.getTransient("foo"));
                    assertEquals("value", threadContext.getHeader("key"));
                }

                @Override
                public void handleException(TransportException exp) {
                    assertEquals("bar", threadContext.getTransient("foo"));
                    assertEquals("value", threadContext.getHeader("key"));
                }

                @Override
                public String executor() {
                    return null;
                }
            });

            handler.handleResponse(null);
            handler.handleException(null);
        }
    }

    public void testContextRestoreResponseHandlerRestoreOriginalContext() throws Exception {
        try (ThreadContext threadContext = new ThreadContext(Settings.EMPTY)) {
            threadContext.putTransient("foo", "bar");
            threadContext.putHeader("key", "value");
            TransportResponseHandler<Empty> handler;
            try (ThreadContext.StoredContext ignore = threadContext.stashContext()) {
                threadContext.putTransient("foo", "different_bar");
                threadContext.putHeader("key", "value2");
                handler = new TransportService.ContextRestoreResponseHandler<>(threadContext.newRestorableContext(true),
                        new TransportResponseHandler<Empty>() {

                            @Override
                            public Empty newInstance() {
                                return Empty.INSTANCE;
                            }

                            @Override
                            public void handleResponse(Empty response) {
                                assertEquals("different_bar", threadContext.getTransient("foo"));
                                assertEquals("value2", threadContext.getHeader("key"));
                            }

                            @Override
                            public void handleException(TransportException exp) {
                                assertEquals("different_bar", threadContext.getTransient("foo"));
                                assertEquals("value2", threadContext.getHeader("key"));
                            }

                            @Override
                            public String executor() {
                                return null;
                            }
                        });
            }

            assertEquals("bar", threadContext.getTransient("foo"));
            assertEquals("value", threadContext.getHeader("key"));
            handler.handleResponse(null);

            assertEquals("bar", threadContext.getTransient("foo"));
            assertEquals("value", threadContext.getHeader("key"));
            handler.handleException(null);

            assertEquals("bar", threadContext.getTransient("foo"));
            assertEquals("value", threadContext.getHeader("key"));
        }
    }

    private String[] randomRoles() {
        return generateRandomStringArray(3, 10, false, true);
    }


}
