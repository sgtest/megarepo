/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.http;

import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.network.NetworkService;
import org.elasticsearch.common.network.NetworkUtils;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.MockBigArrays;
import org.elasticsearch.common.util.MockPageCacheRecycler;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.indices.breaker.NoneCircuitBreakerService;
import org.elasticsearch.rest.RestChannel;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.MockLogAppender;
import org.elasticsearch.test.junit.annotations.TestLogging;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportSettings;
import org.junit.After;
import org.junit.Assert;
import org.junit.Before;

import java.net.InetSocketAddress;
import java.net.UnknownHostException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;

import static java.net.InetAddress.getByName;
import static java.util.Arrays.asList;
import static org.elasticsearch.http.AbstractHttpServerTransport.resolvePublishPort;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;

public class AbstractHttpServerTransportTests extends ESTestCase {

    private NetworkService networkService;
    private ThreadPool threadPool;
    private MockBigArrays bigArrays;

    @Before
    public void setup() throws Exception {
        networkService = new NetworkService(Collections.emptyList());
        threadPool = new TestThreadPool("test");
        bigArrays = new MockBigArrays(new MockPageCacheRecycler(Settings.EMPTY), new NoneCircuitBreakerService());
    }

    @After
    public void shutdown() throws Exception {
        if (threadPool != null) {
            threadPool.shutdownNow();
        }
        threadPool = null;
        networkService = null;
        bigArrays = null;
    }

    public void testHttpPublishPort() throws Exception {
        int boundPort = randomIntBetween(9000, 9100);
        int otherBoundPort = randomIntBetween(9200, 9300);

        int publishPort = resolvePublishPort(Settings.builder().put(HttpTransportSettings.SETTING_HTTP_PUBLISH_PORT.getKey(), 9080).build(),
            randomAddresses(), getByName("127.0.0.2"));
        assertThat("Publish port should be explicitly set to 9080", publishPort, equalTo(9080));

        publishPort = resolvePublishPort(Settings.EMPTY, asList(address("127.0.0.1", boundPort), address("127.0.0.2", otherBoundPort)),
            getByName("127.0.0.1"));
        assertThat("Publish port should be derived from matched address", publishPort, equalTo(boundPort));

        publishPort = resolvePublishPort(Settings.EMPTY, asList(address("127.0.0.1", boundPort), address("127.0.0.2", boundPort)),
            getByName("127.0.0.3"));
        assertThat("Publish port should be derived from unique port of bound addresses", publishPort, equalTo(boundPort));

        final BindHttpException e =
            expectThrows(BindHttpException.class,
                () -> resolvePublishPort(
                    Settings.EMPTY,
                    asList(address("127.0.0.1", boundPort), address("127.0.0.2", otherBoundPort)),
                    getByName("127.0.0.3")));
        assertThat(e.getMessage(), containsString("Failed to auto-resolve http publish port"));

        publishPort = resolvePublishPort(Settings.EMPTY, asList(address("0.0.0.0", boundPort), address("127.0.0.2", otherBoundPort)),
            getByName("127.0.0.1"));
        assertThat("Publish port should be derived from matching wildcard address", publishPort, equalTo(boundPort));

        if (NetworkUtils.SUPPORTS_V6) {
            publishPort = resolvePublishPort(Settings.EMPTY, asList(address("0.0.0.0", boundPort), address("127.0.0.2", otherBoundPort)),
                getByName("::1"));
            assertThat("Publish port should be derived from matching wildcard address", publishPort, equalTo(boundPort));
        }
    }

    public void testDispatchDoesNotModifyThreadContext() {
        final HttpServerTransport.Dispatcher dispatcher = new HttpServerTransport.Dispatcher() {

            @Override
            public void dispatchRequest(final RestRequest request, final RestChannel channel, final ThreadContext threadContext) {
                threadContext.putHeader("foo", "bar");
                threadContext.putTransient("bar", "baz");
            }

            @Override
            public void dispatchBadRequest(final RestChannel channel,
                                           final ThreadContext threadContext,
                                           final Throwable cause) {
                threadContext.putHeader("foo_bad", "bar");
                threadContext.putTransient("bar_bad", "baz");
            }

        };

        try (AbstractHttpServerTransport transport =
                 new AbstractHttpServerTransport(Settings.EMPTY, networkService, bigArrays, threadPool, xContentRegistry(), dispatcher,
                     new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS)) {

                     @Override
                     protected HttpServerChannel bind(InetSocketAddress hostAddress) {
                         return null;
                     }

                     @Override
                     protected void doStart() {

                     }

                     @Override
                     protected void stopInternal() {

                     }

                     @Override
                     public HttpStats stats() {
                         return null;
                     }
                 }) {

            transport.dispatchRequest(null, null, null);
            assertNull(threadPool.getThreadContext().getHeader("foo"));
            assertNull(threadPool.getThreadContext().getTransient("bar"));

            transport.dispatchRequest(null, null, new Exception());
            assertNull(threadPool.getThreadContext().getHeader("foo_bad"));
            assertNull(threadPool.getThreadContext().getTransient("bar_bad"));
        }
    }

    public void testIncorrectHeaderHandling() {

        final ClusterSettings clusterSettings = new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS);
        try (AbstractHttpServerTransport transport =
                 failureAssertingtHttpServerTransport(clusterSettings, "Accept")) {


            Map<String, List<String>> headers = new HashMap<>();
            headers.put("Accept", Collections.singletonList("incorrectheader"));

            FakeRestRequest.FakeHttpRequest fakeHttpRequest =
                new FakeRestRequest.FakeHttpRequest(RestRequest.Method.GET, "/", null, headers);

            transport.incomingRequest(fakeHttpRequest, null);
        }
        try (AbstractHttpServerTransport transport =
                 failureAssertingtHttpServerTransport(clusterSettings, "Content-Type")) {
            Map<String, List<String>> headers = new HashMap<>();
            headers.put("Accept", Collections.singletonList("application/json"));
            headers.put("Content-Type", Collections.singletonList("incorrectheader"));

            FakeRestRequest.FakeHttpRequest fakeHttpRequest =
                new FakeRestRequest.FakeHttpRequest(RestRequest.Method.GET, "/", null, headers);

            transport.incomingRequest(fakeHttpRequest, null);
        }
    }

    private AbstractHttpServerTransport failureAssertingtHttpServerTransport(ClusterSettings clusterSettings,
                                                                             final String failedHeaderName) {
        return new AbstractHttpServerTransport(Settings.EMPTY, networkService, bigArrays, threadPool, xContentRegistry(),
            new HttpServerTransport.Dispatcher() {
                @Override
                public void dispatchRequest(RestRequest request, RestChannel channel, ThreadContext threadContext) {
                    Assert.fail();
                }

                @Override
                public void dispatchBadRequest(RestChannel channel, ThreadContext threadContext, Throwable cause) {
                    assertThat(cause, instanceOf(RestRequest.MediaTypeHeaderException.class));
                    RestRequest.MediaTypeHeaderException mediaTypeHeaderException = (RestRequest.MediaTypeHeaderException) cause;
                    assertThat(mediaTypeHeaderException.getFailedHeaderName(), equalTo(failedHeaderName));
                    assertThat(mediaTypeHeaderException.getMessage(),
                        equalTo("Invalid media-type value on header [" + failedHeaderName + "]"));
                }
            }, clusterSettings) {
            @Override
            protected HttpServerChannel bind(InetSocketAddress hostAddress) {
                return null;
            }

            @Override
            protected void doStart() {
            }

            @Override
            protected void stopInternal() {
            }

            @Override
            public HttpStats stats() {
                return null;
            }
        };
    }

    @TestLogging(
        value = "org.elasticsearch.http.HttpTracer:trace",
        reason = "to ensure we log REST requests on TRACE level")
    public void testTracerLog() throws Exception {
        final String includeSettings;
        final String excludeSettings;
        if (randomBoolean()) {
            includeSettings = randomBoolean() ? "*" : "";
        } else {
            includeSettings = "/internal/test";
        }
        excludeSettings = "/internal/testNotSeen";

        final ClusterSettings clusterSettings = new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS);
        try (AbstractHttpServerTransport transport =
                 new AbstractHttpServerTransport(Settings.EMPTY, networkService, bigArrays, threadPool, xContentRegistry(),
                     new HttpServerTransport.Dispatcher() {
                         @Override
                         public void dispatchRequest(RestRequest request, RestChannel channel, ThreadContext threadContext) {
                             channel.sendResponse(emptyResponse(RestStatus.OK));
                         }

                         @Override
                         public void dispatchBadRequest(RestChannel channel, ThreadContext threadContext, Throwable cause) {
                             channel.sendResponse(emptyResponse(RestStatus.BAD_REQUEST));
                         }
                     }, clusterSettings) {
                     @Override
                     protected HttpServerChannel bind(InetSocketAddress hostAddress) {
                         return null;
                     }

                     @Override
                     protected void doStart() {

                     }

                     @Override
                     protected void stopInternal() {

                     }

                     @Override
                     public HttpStats stats() {
                         return null;
                     }
                 }) {
            clusterSettings.applySettings(Settings.builder()
                .put(HttpTransportSettings.SETTING_HTTP_TRACE_LOG_INCLUDE.getKey(), includeSettings)
                .put(HttpTransportSettings.SETTING_HTTP_TRACE_LOG_EXCLUDE.getKey(), excludeSettings)
                .build());
            MockLogAppender appender = new MockLogAppender();
            final String traceLoggerName = "org.elasticsearch.http.HttpTracer";
            try {
                appender.start();
                Loggers.addAppender(LogManager.getLogger(traceLoggerName), appender);

                final String opaqueId = UUIDs.randomBase64UUID(random());
                appender.addExpectation(
                    new MockLogAppender.PatternSeenEventExpectation(
                        "received request", traceLoggerName, Level.TRACE,
                        "\\[\\d+\\]\\[" + opaqueId + "\\]\\[OPTIONS\\]\\[/internal/test\\] received request from \\[.*"));

                final boolean badRequest = randomBoolean();

                appender.addExpectation(
                    new MockLogAppender.PatternSeenEventExpectation(
                        "sent response", traceLoggerName, Level.TRACE,
                        "\\[\\d+\\]\\[" + opaqueId + "\\]\\[" +
                            (badRequest ? "BAD_REQUEST" : "OK")
                            + "\\]\\[null\\]\\[0\\] sent response to \\[.*"));

                appender.addExpectation(
                    new MockLogAppender.UnseenEventExpectation(
                        "received other request", traceLoggerName, Level.TRACE,
                        "\\[\\d+\\]\\[" + opaqueId + "\\]\\[OPTIONS\\]\\[/internal/testNotSeen\\] received request from \\[.*"));

                final Exception inboundException;
                if (badRequest) {
                    inboundException = new RuntimeException();
                } else {
                    inboundException = null;
                }

                final FakeRestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY)
                    .withMethod(RestRequest.Method.OPTIONS)
                    .withPath("/internal/test")
                    .withHeaders(Collections.singletonMap(Task.X_OPAQUE_ID, Collections.singletonList(opaqueId)))
                    .withInboundException(inboundException)
                    .build();

                transport.incomingRequest(fakeRestRequest.getHttpRequest(), fakeRestRequest.getHttpChannel());

                final Exception inboundExceptionExcludedPath;
                if (randomBoolean()) {
                    inboundExceptionExcludedPath = new RuntimeException();
                } else {
                    inboundExceptionExcludedPath = null;
                }

                final FakeRestRequest fakeRestRequestExcludedPath = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY)
                    .withMethod(RestRequest.Method.OPTIONS)
                    .withPath("/internal/testNotSeen")
                    .withHeaders(Collections.singletonMap(Task.X_OPAQUE_ID, Collections.singletonList(opaqueId)))
                    .withInboundException(inboundExceptionExcludedPath)
                    .build();

                transport.incomingRequest(fakeRestRequestExcludedPath.getHttpRequest(), fakeRestRequestExcludedPath.getHttpChannel());
                appender.assertAllExpectationsMatched();
            } finally {
                Loggers.removeAppender(LogManager.getLogger(traceLoggerName), appender);
                appender.stop();
            }
        }
    }

    public void testLogsSlowInboundProcessing() throws Exception {
        final MockLogAppender mockAppender = new MockLogAppender();
        mockAppender.start();
        final String opaqueId = UUIDs.randomBase64UUID(random());
        final String path = "/internal/test";
        final RestRequest.Method method = randomFrom(RestRequest.Method.values());
        mockAppender.addExpectation(
                new MockLogAppender.SeenEventExpectation(
                        "expected message",
                        AbstractHttpServerTransport.class.getCanonicalName(),
                        Level.WARN,
                        "handling request [" + opaqueId + "][" + method + "][" + path + "]"));
        final Logger inboundHandlerLogger = LogManager.getLogger(AbstractHttpServerTransport.class);
        Loggers.addAppender(inboundHandlerLogger, mockAppender);
        final ClusterSettings clusterSettings = new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS);
        final Settings settings =
                Settings.builder().put(TransportSettings.SLOW_OPERATION_THRESHOLD_SETTING.getKey(), TimeValue.timeValueMillis(5)).build();
        try (AbstractHttpServerTransport transport =
                     new AbstractHttpServerTransport(settings, networkService, bigArrays, threadPool, xContentRegistry(),
                             new HttpServerTransport.Dispatcher() {
                                 @Override
                                 public void dispatchRequest(RestRequest request, RestChannel channel, ThreadContext threadContext) {
                                     try {
                                         TimeUnit.SECONDS.sleep(1L);
                                     } catch (InterruptedException e) {
                                         throw new AssertionError(e);
                                     }
                                     channel.sendResponse(emptyResponse(RestStatus.OK));
                                 }

                                 @Override
                                 public void dispatchBadRequest(RestChannel channel, ThreadContext threadContext, Throwable cause) {
                                     channel.sendResponse(emptyResponse(RestStatus.BAD_REQUEST));
                                 }
                             }, clusterSettings) {
                         @Override
                         protected HttpServerChannel bind(InetSocketAddress hostAddress) {
                             return null;
                         }

                         @Override
                         protected void doStart() {

                         }

                         @Override
                         protected void stopInternal() {

                         }

                         @Override
                         public HttpStats stats() {
                             return null;
                         }
                     }) {

            final FakeRestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY)
                    .withMethod(method)
                    .withPath(path)
                    .withHeaders(Collections.singletonMap(Task.X_OPAQUE_ID, Collections.singletonList(opaqueId)))
                    .build();
            transport.incomingRequest(fakeRestRequest.getHttpRequest(), fakeRestRequest.getHttpChannel());
            mockAppender.assertAllExpectationsMatched();
        } finally {
            Loggers.removeAppender(inboundHandlerLogger, mockAppender);
            mockAppender.stop();
        }
    }

    private static RestResponse emptyResponse(RestStatus status) {
        return new RestResponse() {
            @Override
            public String contentType() {
                return null;
            }

            @Override
            public BytesReference content() {
                return BytesArray.EMPTY;
            }

            @Override
            public RestStatus status() {
                return status;
            }
        };
    }

    private TransportAddress address(String host, int port) throws UnknownHostException {
        return new TransportAddress(getByName(host), port);
    }

    private TransportAddress randomAddress() throws UnknownHostException {
        return address("127.0.0." + randomIntBetween(1, 100), randomIntBetween(9200, 9300));
    }

    private List<TransportAddress> randomAddresses() throws UnknownHostException {
        List<TransportAddress> addresses = new ArrayList<>();
        for (int i = 0; i < randomIntBetween(1, 5); i++) {
            addresses.add(randomAddress());
        }
        return addresses;
    }
}
