/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.rest;

import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.breaker.CircuitBreaker;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.component.AbstractLifecycleComponent;
import org.elasticsearch.common.io.stream.BytesStream;
import org.elasticsearch.common.io.stream.RecyclerBytesStreamOutput;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.BoundTransportAddress;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.MockPageCacheRecycler;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.core.IOUtils;
import org.elasticsearch.core.RestApiVersion;
import org.elasticsearch.http.HttpInfo;
import org.elasticsearch.http.HttpRequest;
import org.elasticsearch.http.HttpResponse;
import org.elasticsearch.http.HttpServerTransport;
import org.elasticsearch.http.HttpStats;
import org.elasticsearch.indices.breaker.HierarchyCircuitBreakerService;
import org.elasticsearch.rest.RestHandler.Route;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.client.NoOpNodeClient;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.tracing.Tracer;
import org.elasticsearch.transport.BytesRefRecycler;
import org.elasticsearch.usage.UsageService;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xcontent.yaml.YamlXContent;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

import static org.elasticsearch.rest.RestController.ELASTIC_INTERNAL_ORIGIN_HTTP_HEADER;
import static org.elasticsearch.rest.RestRequest.Method.GET;
import static org.elasticsearch.rest.RestRequest.Method.OPTIONS;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyMap;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.doCallRealMethod;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.spy;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class RestControllerTests extends ESTestCase {

    private static final ByteSizeValue BREAKER_LIMIT = ByteSizeValue.ofBytes(20);
    private CircuitBreaker inFlightRequestsBreaker;
    private RestController restController;
    private HierarchyCircuitBreakerService circuitBreakerService;
    private UsageService usageService;
    private NodeClient client;
    private Tracer tracer;

    @Before
    public void setup() {
        circuitBreakerService = new HierarchyCircuitBreakerService(
            Settings.builder()
                .put(HierarchyCircuitBreakerService.IN_FLIGHT_REQUESTS_CIRCUIT_BREAKER_LIMIT_SETTING.getKey(), BREAKER_LIMIT)
                // We want to have reproducible results in this test, hence we disable real memory usage accounting
                .put(HierarchyCircuitBreakerService.USE_REAL_MEMORY_USAGE_SETTING.getKey(), false)
                .build(),
            Collections.emptyList(),
            new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS)
        );
        usageService = new UsageService();
        // we can do this here only because we know that we don't adjust breaker settings dynamically in the test
        inFlightRequestsBreaker = circuitBreakerService.getBreaker(CircuitBreaker.IN_FLIGHT_REQUESTS);

        HttpServerTransport httpServerTransport = new TestHttpServerTransport();
        client = new NoOpNodeClient(this.getTestName());
        tracer = mock(Tracer.class);
        restController = new RestController(Collections.emptySet(), null, client, circuitBreakerService, usageService, tracer, false);
        restController.registerHandler(
            new Route(GET, "/"),
            (request, channel, client) -> channel.sendResponse(
                new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY)
            )
        );
        restController.registerHandler(
            new Route(GET, "/error"),
            (request, channel, client) -> { throw new IllegalArgumentException("test error"); }
        );
        httpServerTransport.start();
    }

    @After
    public void teardown() throws IOException {
        IOUtils.close(client);
    }

    public void testApplyRelevantHeaders() throws Exception {
        final ThreadContext threadContext = client.threadPool().getThreadContext();
        Set<RestHeaderDefinition> headers = new HashSet<>(
            Arrays.asList(new RestHeaderDefinition("header.1", true), new RestHeaderDefinition("header.2", true))
        );
        final RestController restController = new RestController(headers, null, null, circuitBreakerService, usageService, tracer, false);
        Map<String, List<String>> restHeaders = new HashMap<>();
        restHeaders.put("header.1", Collections.singletonList("true"));
        restHeaders.put("header.2", Collections.singletonList("true"));
        restHeaders.put("header.3", Collections.singletonList("false"));
        RestRequest fakeRequest = new FakeRestRequest.Builder(xContentRegistry()).withHeaders(restHeaders).build();
        AssertingChannel channel = new AssertingChannel(fakeRequest, false, RestStatus.BAD_REQUEST);
        restController.dispatchRequest(fakeRequest, channel, threadContext);
        // the rest controller relies on the caller to stash the context, so we should expect these values here as we didn't stash the
        // context in this test
        assertEquals("true", threadContext.getHeader("header.1"));
        assertEquals("true", threadContext.getHeader("header.2"));
        assertNull(threadContext.getHeader("header.3"));
        List<String> expectedProductResponseHeader = new ArrayList<>();
        expectedProductResponseHeader.add(RestController.ELASTIC_PRODUCT_HTTP_HEADER_VALUE);
        assertEquals(
            expectedProductResponseHeader,
            threadContext.getResponseHeaders().getOrDefault(RestController.ELASTIC_PRODUCT_HTTP_HEADER, null)
        );
    }

    public void testRequestWithDisallowedMultiValuedHeader() {
        final ThreadContext threadContext = client.threadPool().getThreadContext();
        Set<RestHeaderDefinition> headers = new HashSet<>(
            Arrays.asList(new RestHeaderDefinition("header.1", true), new RestHeaderDefinition("header.2", false))
        );
        final RestController restController = new RestController(headers, null, null, circuitBreakerService, usageService, tracer, false);
        Map<String, List<String>> restHeaders = new HashMap<>();
        restHeaders.put("header.1", Collections.singletonList("boo"));
        restHeaders.put("header.2", List.of("foo", "bar"));
        RestRequest fakeRequest = new FakeRestRequest.Builder(xContentRegistry()).withHeaders(restHeaders).build();
        AssertingChannel channel = new AssertingChannel(fakeRequest, false, RestStatus.BAD_REQUEST);
        restController.dispatchRequest(fakeRequest, channel, threadContext);
        assertTrue(channel.getSendResponseCalled());
    }

    /**
     * Check that dispatching a request causes a trace span to be started.
     */
    public void testDispatchStartsTrace() {
        final ThreadContext threadContext = client.threadPool().getThreadContext();
        final RestController restController = new RestController(Set.of(), null, null, circuitBreakerService, usageService, tracer, false);
        RestRequest fakeRequest = new FakeRestRequest.Builder(xContentRegistry()).build();
        final RestController spyRestController = spy(restController);
        when(spyRestController.getAllHandlers(null, fakeRequest.rawPath())).thenReturn(new Iterator<>() {
            @Override
            public boolean hasNext() {
                return false;
            }

            @Override
            public MethodHandlers next() {
                return new MethodHandlers("/").addMethod(GET, RestApiVersion.current(), (request, channel, client) -> {});
            }
        });
        AssertingChannel channel = new AssertingChannel(fakeRequest, false, RestStatus.BAD_REQUEST);
        restController.dispatchRequest(fakeRequest, channel, threadContext);
        verify(tracer).startTrace(
            eq(threadContext),
            eq("rest-" + channel.request().getRequestId()),
            eq("GET /"),
            eq(Map.of("http.method", "GET", "http.flavour", "1.1", "http.url", "/"))
        );
    }

    /**
     * Check that the REST controller picks up and propagates W3C trace context headers via the {@link ThreadContext}.
     * @see <a href="https://www.w3.org/TR/trace-context/">Trace Context - W3C Recommendation</a>
     */
    public void testTraceParentAndTraceId() {
        final ThreadContext threadContext = client.threadPool().getThreadContext();
        Set<RestHeaderDefinition> headers = Set.of(new RestHeaderDefinition(Task.TRACE_PARENT_HTTP_HEADER, false));
        final RestController restController = new RestController(headers, null, null, circuitBreakerService, usageService, tracer, false);
        Map<String, List<String>> restHeaders = new HashMap<>();
        final String traceParentValue = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        restHeaders.put(Task.TRACE_PARENT_HTTP_HEADER, Collections.singletonList(traceParentValue));
        RestRequest fakeRequest = new FakeRestRequest.Builder(xContentRegistry()).withHeaders(restHeaders).build();
        AssertingChannel channel = new AssertingChannel(fakeRequest, false, RestStatus.BAD_REQUEST);

        restController.dispatchRequest(fakeRequest, channel, threadContext);

        // the rest controller relies on the caller to stash the context, so we should expect these values here as we didn't stash the
        // context in this test
        assertThat(threadContext.getHeader(Task.TRACE_ID), equalTo("0af7651916cd43dd8448eb211c80319c"));
        assertThat(threadContext.getHeader(Task.TRACE_PARENT_HTTP_HEADER), nullValue());
        assertThat(threadContext.getTransient("parent_" + Task.TRACE_PARENT_HTTP_HEADER), equalTo(traceParentValue));
    }

    public void testRequestWithDisallowedMultiValuedHeaderButSameValues() {
        final ThreadContext threadContext = client.threadPool().getThreadContext();
        Set<RestHeaderDefinition> headers = new HashSet<>(
            Arrays.asList(new RestHeaderDefinition("header.1", true), new RestHeaderDefinition("header.2", false))
        );
        final RestController restController = new RestController(headers, null, client, circuitBreakerService, usageService, tracer, false);
        Map<String, List<String>> restHeaders = new HashMap<>();
        restHeaders.put("header.1", Collections.singletonList("boo"));
        restHeaders.put("header.2", List.of("foo", "foo"));
        RestRequest fakeRequest = new FakeRestRequest.Builder(xContentRegistry()).withHeaders(restHeaders).withPath("/bar").build();
        restController.registerHandler(
            new Route(GET, "/bar"),
            (request, channel, client) -> channel.sendResponse(
                new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY)
            )
        );
        AssertingChannel channel = new AssertingChannel(fakeRequest, false, RestStatus.OK);
        restController.dispatchRequest(fakeRequest, channel, threadContext);
        assertTrue(channel.getSendResponseCalled());
    }

    public void testRegisterAsDeprecatedHandler() {
        RestController controller = mock(RestController.class);

        RestRequest.Method method = randomFrom(RestRequest.Method.values());
        String path = "/_" + randomAlphaOfLengthBetween(1, 6);
        RestHandler handler = (request, channel, client) -> {};
        String deprecationMessage = randomAlphaOfLengthBetween(1, 10);
        RestApiVersion deprecatedInVersion = RestApiVersion.current();

        Route route = Route.builder(method, path).deprecated(deprecationMessage, deprecatedInVersion).build();

        // don't want to test everything -- just that it actually wraps the handler
        doCallRealMethod().when(controller).registerHandler(route, handler);
        doCallRealMethod().when(controller)
            .registerAsDeprecatedHandler(method, path, deprecatedInVersion, handler, deprecationMessage, null);

        controller.registerHandler(route, handler);

        verify(controller).registerHandler(eq(method), eq(path), eq(deprecatedInVersion), any(DeprecationRestHandler.class));
    }

    public void testRegisterAsReplacedHandler() {
        final RestController controller = mock(RestController.class);

        final RestRequest.Method method = randomFrom(RestRequest.Method.values());
        final String path = "/_" + randomAlphaOfLengthBetween(1, 6);
        final RestHandler handler = (request, channel, client) -> {};
        final RestRequest.Method replacedMethod = randomFrom(RestRequest.Method.values());
        final String replacedPath = "/_" + randomAlphaOfLengthBetween(1, 6);
        final RestApiVersion current = RestApiVersion.current();
        final RestApiVersion previous = RestApiVersion.previous();
        final String deprecationMessage = "["
            + replacedMethod.name()
            + " "
            + replacedPath
            + "] is deprecated! Use ["
            + method.name()
            + " "
            + path
            + "] instead.";

        final Route route = Route.builder(method, path).replaces(replacedMethod, replacedPath, previous).build();

        // don't want to test everything -- just that it actually wraps the handlers
        doCallRealMethod().when(controller).registerHandler(route, handler);
        doCallRealMethod().when(controller)
            .registerAsReplacedHandler(method, path, current, handler, replacedMethod, replacedPath, previous);

        controller.registerHandler(route, handler);

        verify(controller).registerHandler(method, path, current, handler);
        verify(controller).registerAsDeprecatedHandler(replacedMethod, replacedPath, previous, handler, deprecationMessage);
    }

    public void testRegisterSecondMethodWithDifferentNamedWildcard() {
        final RestController restController = new RestController(null, null, null, circuitBreakerService, usageService, tracer, false);

        RestRequest.Method firstMethod = randomFrom(RestRequest.Method.values());
        RestRequest.Method secondMethod = randomFrom(Arrays.stream(RestRequest.Method.values()).filter(m -> m != firstMethod).toList());

        final String path = "/_" + randomAlphaOfLengthBetween(1, 6);

        RestHandler handler = (request, channel, client) -> {};

        restController.registerHandler(new Route(firstMethod, path + "/{wildcard1}"), handler);

        IllegalArgumentException exception = expectThrows(
            IllegalArgumentException.class,
            () -> restController.registerHandler(new Route(secondMethod, path + "/{wildcard2}"), handler)
        );

        assertThat(exception.getMessage(), equalTo("Trying to use conflicting wildcard names for same path: wildcard1 and wildcard2"));
    }

    public void testRestHandlerWrapper() throws Exception {
        AtomicBoolean handlerCalled = new AtomicBoolean(false);
        AtomicBoolean wrapperCalled = new AtomicBoolean(false);
        final RestHandler handler = (RestRequest request, RestChannel channel, NodeClient client) -> handlerCalled.set(true);
        final HttpServerTransport httpServerTransport = new TestHttpServerTransport();
        final RestController restController = new RestController(Collections.emptySet(), h -> {
            assertSame(handler, h);
            return (RestRequest request, RestChannel channel, NodeClient client) -> wrapperCalled.set(true);
        }, client, circuitBreakerService, usageService, tracer, false);
        restController.registerHandler(new Route(GET, "/wrapped"), handler);
        RestRequest request = testRestRequest("/wrapped", "{}", XContentType.JSON);
        AssertingChannel channel = new AssertingChannel(request, true, RestStatus.BAD_REQUEST);
        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());
        httpServerTransport.start();
        assertTrue(wrapperCalled.get());
        assertFalse(handlerCalled.get());
    }

    public void testDispatchRequestAddsAndFreesBytesOnSuccess() {
        int contentLength = BREAKER_LIMIT.bytesAsInt();
        String content = randomAlphaOfLength((int) Math.round(contentLength / inFlightRequestsBreaker.getOverhead()));
        RestRequest request = testRestRequest("/", content, XContentType.JSON);
        AssertingChannel channel = new AssertingChannel(request, true, RestStatus.OK);

        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());

        assertEquals(0, inFlightRequestsBreaker.getTrippedCount());
        assertEquals(0, inFlightRequestsBreaker.getUsed());
    }

    public void testDispatchRequestAddsAndFreesBytesOnError() {
        int contentLength = BREAKER_LIMIT.bytesAsInt();
        String content = randomAlphaOfLength((int) Math.round(contentLength / inFlightRequestsBreaker.getOverhead()));
        RestRequest request = testRestRequest("/error", content, XContentType.JSON);
        AssertingChannel channel = new AssertingChannel(request, true, RestStatus.BAD_REQUEST);

        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());

        assertEquals(0, inFlightRequestsBreaker.getTrippedCount());
        assertEquals(0, inFlightRequestsBreaker.getUsed());
    }

    public void testDispatchRequestAddsAndFreesBytesOnlyOnceOnError() {
        int contentLength = BREAKER_LIMIT.bytesAsInt();
        String content = randomAlphaOfLength((int) Math.round(contentLength / inFlightRequestsBreaker.getOverhead()));
        // we will produce an error in the rest handler and one more when sending the error response
        RestRequest request = testRestRequest("/error", content, XContentType.JSON);
        ExceptionThrowingChannel channel = new ExceptionThrowingChannel(request, true);

        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());

        assertEquals(0, inFlightRequestsBreaker.getTrippedCount());
        assertEquals(0, inFlightRequestsBreaker.getUsed());
    }

    public void testDispatchRequestAddsAndFreesBytesOnlyOnceOnErrorDuringSend() {
        int contentLength = Math.toIntExact(BREAKER_LIMIT.getBytes());
        String content = randomAlphaOfLength((int) Math.round(contentLength / inFlightRequestsBreaker.getOverhead()));
        // use a real recycler that tracks leaks and create some content bytes in the test handler to check for leaks
        final BytesRefRecycler recycler = new BytesRefRecycler(new MockPageCacheRecycler(Settings.EMPTY));
        restController.registerHandler(
            new Route(GET, "/foo"),
            (request, c, client) -> new RestToXContentListener<>(c).onResponse((b, p) -> b.startObject().endObject())
        );
        // we will produce an error in the rest handler and one more when sending the error response
        RestRequest request = testRestRequest("/foo", content, XContentType.JSON);
        ExceptionThrowingChannel channel = new ExceptionThrowingChannel(request, true) {
            @Override
            protected BytesStream newBytesOutput() {
                return new RecyclerBytesStreamOutput(recycler);
            }
        };

        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());

        assertEquals(0, inFlightRequestsBreaker.getTrippedCount());
        assertEquals(0, inFlightRequestsBreaker.getUsed());
    }

    public void testDispatchRequestLimitsBytes() {
        int contentLength = BREAKER_LIMIT.bytesAsInt() + 1;
        String content = randomAlphaOfLength((int) Math.round(contentLength / inFlightRequestsBreaker.getOverhead()));
        RestRequest request = testRestRequest("/", content, XContentType.JSON);
        AssertingChannel channel = new AssertingChannel(request, true, RestStatus.TOO_MANY_REQUESTS);

        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());

        assertEquals(1, inFlightRequestsBreaker.getTrippedCount());
        assertEquals(0, inFlightRequestsBreaker.getUsed());
    }

    public void testDispatchRequiresContentTypeForRequestsWithContent() {
        String content = randomAlphaOfLength((int) Math.round(BREAKER_LIMIT.getBytes() / inFlightRequestsBreaker.getOverhead()));
        RestRequest request = testRestRequest("/", content, null);
        AssertingChannel channel = new AssertingChannel(request, true, RestStatus.NOT_ACCEPTABLE);
        restController = new RestController(Collections.emptySet(), null, null, circuitBreakerService, usageService, tracer, false);
        restController.registerHandler(
            new Route(GET, "/"),
            (r, c, client) -> c.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY))
        );

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchDoesNotRequireContentTypeForRequestsWithoutContent() {
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).build();
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.OK);

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchFailsWithPlainText() {
        String content = randomAlphaOfLength((int) Math.round(BREAKER_LIMIT.getBytes() / inFlightRequestsBreaker.getOverhead()));
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(new BytesArray(content), null)
            .withPath("/foo")
            .withHeaders(Collections.singletonMap("Content-Type", Collections.singletonList("text/plain")))
            .build();
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.NOT_ACCEPTABLE);
        restController.registerHandler(
            new Route(GET, "/foo"),
            (request, channel1, client) -> channel1.sendResponse(
                new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY)
            )
        );

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchUnsupportedContentType() {
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(new BytesArray("{}"), null)
            .withPath("/")
            .withHeaders(Collections.singletonMap("Content-Type", Collections.singletonList("application/x-www-form-urlencoded")))
            .build();
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.NOT_ACCEPTABLE);

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchWorksWithNewlineDelimitedJson() {
        final String mediaType = "application/x-ndjson";
        String content = randomAlphaOfLength((int) Math.round(BREAKER_LIMIT.getBytes() / inFlightRequestsBreaker.getOverhead()));
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(new BytesArray(content), null)
            .withPath("/foo")
            .withHeaders(Collections.singletonMap("Content-Type", Collections.singletonList(mediaType)))
            .build();
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.OK);
        restController.registerHandler(new Route(GET, "/foo"), new RestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                assertThat(request.contentParser().getRestApiVersion(), is(RestApiVersion.current()));

                channel.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
            }

            @Override
            public boolean supportsContentStream() {
                return true;
            }
        });

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchWithContentStream() {
        final String mediaType = randomFrom("application/json", "application/smile");
        String content = randomAlphaOfLength((int) Math.round(BREAKER_LIMIT.getBytes() / inFlightRequestsBreaker.getOverhead()));
        final List<String> contentTypeHeader = Collections.singletonList(mediaType);
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(
            new BytesArray(content),
            RestRequest.parseContentType(contentTypeHeader)
        ).withPath("/foo").withHeaders(Collections.singletonMap("Content-Type", contentTypeHeader)).build();
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.OK);
        restController.registerHandler(new Route(GET, "/foo"), new RestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                assertThat(request.contentParser().getRestApiVersion(), is(RestApiVersion.current()));

                channel.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
            }

            @Override
            public boolean supportsContentStream() {
                return true;
            }
        });

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchWithContentStreamNoContentType() {
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(new BytesArray("{}"), null)
            .withPath("/foo")
            .build();
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.NOT_ACCEPTABLE);
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        restController.registerHandler(new Route(GET, "/foo"), new RestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                channel.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
            }

            @Override
            public boolean supportsContentStream() {
                return true;
            }
        });

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testNonStreamingXContentCausesErrorResponse() throws IOException {
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(
            BytesReference.bytes(YamlXContent.contentBuilder().startObject().endObject()),
            XContentType.YAML
        ).withPath("/foo").build();
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.NOT_ACCEPTABLE);
        restController.registerHandler(new Route(GET, "/foo"), new RestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                channel.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
            }

            @Override
            public boolean supportsContentStream() {
                return true;
            }
        });
        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testUnknownContentWithContentStream() {
        RestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(
            new BytesArray("aaaabbbbb"),
            null
        ).withPath("/foo").withHeaders(Collections.singletonMap("Content-Type", Collections.singletonList("foo/bar"))).build();
        if (randomBoolean()) {
            fakeRestRequest = new RestRequest(fakeRestRequest);
        }
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.NOT_ACCEPTABLE);
        restController.registerHandler(new Route(GET, "/foo"), new RestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                channel.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
            }

            @Override
            public boolean supportsContentStream() {
                return true;
            }
        });
        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchBadRequest() {
        final FakeRestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).build();
        final AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.BAD_REQUEST);
        restController.dispatchBadRequest(
            channel,
            client.threadPool().getThreadContext(),
            randomBoolean() ? new IllegalStateException("bad request") : new Throwable("bad request")
        );
        assertTrue(channel.getSendResponseCalled());
        assertThat(channel.getRestResponse().content().utf8ToString(), containsString("bad request"));
    }

    public void testDoesNotConsumeContent() throws Exception {
        final RestRequest.Method method = randomFrom(RestRequest.Method.values());
        restController.registerHandler(new Route(method, "/notconsumed"), new RestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                channel.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
            }

            @Override
            public boolean canTripCircuitBreaker() {
                return false;
            }
        });

        final XContentBuilder content = XContentBuilder.builder(randomFrom(XContentType.values()).xContent())
            .startObject()
            .field("field", "value")
            .endObject();
        final FakeRestRequest restRequest = new FakeRestRequest.Builder(xContentRegistry()).withPath("/notconsumed")
            .withMethod(method)
            .withContent(BytesReference.bytes(content), content.contentType())
            .build();

        final AssertingChannel channel = new AssertingChannel(restRequest, true, RestStatus.OK);
        assertFalse(channel.getSendResponseCalled());
        assertFalse(restRequest.isContentConsumed());

        restController.dispatchRequest(restRequest, channel, client.threadPool().getThreadContext());

        assertTrue(channel.getSendResponseCalled());
        assertFalse("RestController must not consume request content", restRequest.isContentConsumed());
    }

    public void testDispatchBadRequestUnknownCause() {
        final FakeRestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).build();
        final AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.BAD_REQUEST);
        restController.dispatchBadRequest(channel, client.threadPool().getThreadContext(), null);
        assertTrue(channel.getSendResponseCalled());
        assertThat(channel.getRestResponse().content().utf8ToString(), containsString("unknown cause"));
    }

    public void testFavicon() {
        final FakeRestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withMethod(GET)
            .withPath("/favicon.ico")
            .build();
        final AssertingChannel channel = new AssertingChannel(fakeRestRequest, false, RestStatus.OK);
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
        assertThat(channel.getRestResponse().contentType(), containsString("image/x-icon"));
    }

    public void testFaviconWithWrongHttpMethod() {
        final FakeRestRequest fakeRestRequest = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withMethod(
            randomValueOtherThanMany(m -> m == GET || m == OPTIONS, () -> randomFrom(RestRequest.Method.values()))
        ).withPath("/favicon.ico").build();
        final AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.METHOD_NOT_ALLOWED);
        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
        assertThat(channel.getRestResponse().getHeaders().containsKey("Allow"), equalTo(true));
        assertThat(channel.getRestResponse().getHeaders().get("Allow"), hasItem(equalTo(GET.toString())));
    }

    public void testDispatchUnsupportedHttpMethod() {
        final boolean hasContent = randomBoolean();
        final RestRequest request = RestRequest.request(parserConfig(), new HttpRequest() {
            @Override
            public RestRequest.Method method() {
                throw new IllegalArgumentException("test");
            }

            @Override
            public String uri() {
                return "/";
            }

            @Override
            public BytesReference content() {
                if (hasContent) {
                    return new BytesArray("test");
                }
                return BytesArray.EMPTY;
            }

            @Override
            public Map<String, List<String>> getHeaders() {
                Map<String, List<String>> headers = new HashMap<>();
                if (hasContent) {
                    headers.put("Content-Type", Collections.singletonList("text/plain"));
                }
                return headers;
            }

            @Override
            public List<String> strictCookies() {
                return null;
            }

            @Override
            public HttpVersion protocolVersion() {
                return randomFrom(HttpVersion.values());
            }

            @Override
            public HttpRequest removeHeader(String header) {
                return this;
            }

            @Override
            public HttpResponse createResponse(RestStatus status, BytesReference content) {
                return null;
            }

            @Override
            public HttpResponse createResponse(RestStatus status, ChunkedRestResponseBody content) {
                throw new AssertionError("should not be called");
            }

            @Override
            public void release() {}

            @Override
            public HttpRequest releaseAndCopy() {
                return this;
            }

            @Override
            public Exception getInboundException() {
                return null;
            }
        }, null);

        final AssertingChannel channel = new AssertingChannel(request, true, RestStatus.METHOD_NOT_ALLOWED);
        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
        assertThat(channel.getRestResponse().getHeaders().containsKey("Allow"), equalTo(true));
        assertThat(channel.getRestResponse().getHeaders().get("Allow"), hasItem(equalTo(GET.toString())));
    }

    /**
     * Check that when dispatching a request, if an IllegalArgumentException is thrown, then a trace span is started
     * and the exception is captured in the span.
     */
    public void testDispatchUnsupportedHttpMethodTracesException() {
        final RestRequest request = new FakeRestRequest() {
            @Override
            public Method method() {
                throw new IllegalArgumentException("test");
            }
        };

        final AssertingChannel channel = new AssertingChannel(request, true, RestStatus.METHOD_NOT_ALLOWED);
        restController.dispatchRequest(request, channel, client.threadPool().getThreadContext());
        verify(tracer).startTrace(any(), anyString(), anyString(), anyMap());
        verify(tracer).addError(any(), any(IllegalArgumentException.class));
    }

    public void testDispatchCompatibleHandler() {

        RestController restController = new RestController(
            Collections.emptySet(),
            null,
            client,
            circuitBreakerService,
            usageService,
            tracer,
            false
        );

        final RestApiVersion version = RestApiVersion.minimumSupported();

        final String mediaType = randomCompatibleMediaType(version);
        FakeRestRequest fakeRestRequest = requestWithContent(mediaType);
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.OK);

        // dispatch to a compatible handler
        restController.registerHandler(GET, "/foo", RestApiVersion.minimumSupported(), (request, channel1, client) -> {
            // in real use case we will use exact version RestApiVersion.V_7
            XContentBuilder xContentBuilder = channel1.newBuilder();
            assertThat(xContentBuilder.getRestApiVersion(), equalTo(RestApiVersion.minimumSupported()));
            assertThat(request.contentParser().getRestApiVersion(), equalTo(RestApiVersion.minimumSupported()));
            channel1.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
        });

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, new ThreadContext(Settings.EMPTY));
        assertTrue(channel.getSendResponseCalled());
    }

    public void testDispatchCompatibleRequestToNewlyAddedHandler() {

        RestController restController = new RestController(
            Collections.emptySet(),
            null,
            client,
            circuitBreakerService,
            usageService,
            tracer,
            false
        );

        final RestApiVersion version = RestApiVersion.minimumSupported();

        final String mediaType = randomCompatibleMediaType(version);
        FakeRestRequest fakeRestRequest = requestWithContent(mediaType);
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.OK);

        // dispatch to a CURRENT newly added handler
        restController.registerHandler(new Route(GET, "/foo"), (request, channel1, client) -> {
            XContentBuilder xContentBuilder = channel1.newBuilder();
            // even though the handler is CURRENT, the xContentBuilder has the version requested by a client.
            // This allows to implement the compatible logic within the serialisation without introducing V7 (compatible) handler
            // when only response shape has changed
            assertThat(xContentBuilder.getRestApiVersion(), equalTo(RestApiVersion.minimumSupported()));
            assertThat(request.contentParser().getRestApiVersion(), equalTo(RestApiVersion.minimumSupported()));

            channel1.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
        });

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, new ThreadContext(Settings.EMPTY));
        assertTrue(channel.getSendResponseCalled());
    }

    private FakeRestRequest requestWithContent(String mediaType) {
        String content = randomAlphaOfLength((int) Math.round(BREAKER_LIMIT.getBytes() / inFlightRequestsBreaker.getOverhead()));
        final List<String> mediaTypeList = Collections.singletonList(mediaType);
        return new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY).withContent(
            new BytesArray(content),
            RestRequest.parseContentType(mediaTypeList)
        ).withPath("/foo").withHeaders(Map.of("Content-Type", mediaTypeList, "Accept", mediaTypeList)).build();
    }

    public void testCurrentVersionVNDMediaTypeIsNotUsingCompatibility() {
        RestController restController = new RestController(
            Collections.emptySet(),
            null,
            client,
            circuitBreakerService,
            usageService,
            tracer,
            false
        );

        final RestApiVersion version = RestApiVersion.current();

        final String mediaType = randomCompatibleMediaType(version);
        FakeRestRequest fakeRestRequest = requestWithContent(mediaType);
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.OK);

        // dispatch to a CURRENT newly added handler
        restController.registerHandler(new Route(GET, "/foo"), (request, channel1, client) -> {
            // the media type is in application/vnd.elasticsearch form but with compatible-with=CURRENT.
            // Hence compatibility is not used.

            XContentBuilder xContentBuilder = channel1.newBuilder();
            assertThat(request.contentParser().getRestApiVersion(), equalTo(RestApiVersion.current()));
            assertThat(xContentBuilder.getRestApiVersion(), equalTo(RestApiVersion.current()));
            channel1.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
        });

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, new ThreadContext(Settings.EMPTY));
        assertTrue(channel.getSendResponseCalled());
    }

    public void testCustomMediaTypeValidation() {
        RestController restController = new RestController(
            Collections.emptySet(),
            null,
            client,
            circuitBreakerService,
            usageService,
            tracer,
            false
        );

        final String mediaType = "application/x-protobuf";
        FakeRestRequest fakeRestRequest = requestWithContent(mediaType);
        AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.OK);

        // register handler that handles custom media type validation
        restController.registerHandler(new Route(GET, "/foo"), new RestHandler() {
            @Override
            public boolean mediaTypesValid(RestRequest request) {
                return request.getXContentType() == null
                    && request.getParsedContentType().mediaTypeWithoutParameters().equals("application/x-protobuf");
            }

            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {
                channel.sendResponse(new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY));
            }
        });

        assertFalse(channel.getSendResponseCalled());
        restController.dispatchRequest(fakeRestRequest, channel, new ThreadContext(Settings.EMPTY));
        assertTrue(channel.getSendResponseCalled());
    }

    public void testBrowserSafelistedContentTypesAreRejected() {
        RestController restController = new RestController(
            Collections.emptySet(),
            null,
            client,
            circuitBreakerService,
            usageService,
            tracer,
            false
        );

        final String mediaType = randomFrom(RestController.SAFELISTED_MEDIA_TYPES);
        FakeRestRequest fakeRestRequest = requestWithContent(mediaType);

        final AssertingChannel channel = new AssertingChannel(fakeRestRequest, true, RestStatus.NOT_ACCEPTABLE);

        restController.registerHandler(new Route(GET, "/foo"), new RestHandler() {
            @Override
            public void handleRequest(RestRequest request, RestChannel channel, NodeClient client) throws Exception {}
        });

        restController.dispatchRequest(fakeRestRequest, channel, client.threadPool().getThreadContext());
        assertTrue(channel.getSendResponseCalled());
        assertThat(
            channel.getRestResponse().content().utf8ToString(),
            containsString("Content-Type header [" + mediaType + "] is not supported")
        );
    }

    public void testRegisterWithReservedPath() {
        final RestController restController = new RestController(
            Set.of(),
            null,
            client,
            circuitBreakerService,
            usageService,
            tracer,
            false
        );
        for (String path : RestController.RESERVED_PATHS) {
            IllegalArgumentException iae = expectThrows(IllegalArgumentException.class, () -> {
                restController.registerHandler(
                    new Route(randomFrom(RestRequest.Method.values()), path),
                    (request, channel, client) -> channel.sendResponse(
                        new RestResponse(RestStatus.OK, RestResponse.TEXT_CONTENT_TYPE, BytesArray.EMPTY)
                    )
                );
            });
            assertThat(iae.getMessage(), containsString("path [" + path + "] is a reserved path and may not be registered"));
        }
    }

    /*
     * Test that when serverless is disabled, all endpoints are available regardless of ServerlessScope annotations.
     */
    public void testApiProtectionWithServerlessDisabled() {
        final RestController restController = new RestController(
            Set.of(),
            null,
            client,
            circuitBreakerService,
            new UsageService(),
            tracer,
            false
        );
        restController.registerHandler(new PublicRestHandler());
        restController.registerHandler(new InternalRestHandler());
        restController.registerHandler(new HiddenRestHandler());
        List<String> accessiblePaths = List.of("/public", "/internal", "/hidden");
        accessiblePaths.forEach(path -> {
            RestRequest request = new FakeRestRequest.Builder(xContentRegistry()).withPath(path).build();
            AssertingChannel channel = new AssertingChannel(request, false, RestStatus.OK);
            restController.dispatchRequest(request, channel, new ThreadContext(Settings.EMPTY));
        });
    }

    /*
     * Test that when serverless is enabled, a normal user not using the X-elastic-internal-origin header can only access endpoints
     * annotated with a PUBLIC scope.
     */
    public void testApiProtectionWithServerlessEnabledAsEndUser() {
        final RestController restController = new RestController(
            Set.of(),
            null,
            client,
            circuitBreakerService,
            new UsageService(),
            tracer,
            true
        );
        restController.registerHandler(new PublicRestHandler());
        restController.registerHandler(new InternalRestHandler());
        restController.registerHandler(new HiddenRestHandler());
        List<String> accessiblePaths = List.of("/public");
        accessiblePaths.forEach(path -> {
            RestRequest request = new FakeRestRequest.Builder(xContentRegistry()).withPath(path).build();
            AssertingChannel channel = new AssertingChannel(request, false, RestStatus.OK);
            restController.dispatchRequest(request, channel, new ThreadContext(Settings.EMPTY));
        });
        List<String> inaccessiblePaths = List.of("/internal", "/hidden");
        inaccessiblePaths.forEach(path -> {
            RestRequest request = new FakeRestRequest.Builder(xContentRegistry()).withPath(path).build();
            AssertingChannel channel = new AssertingChannel(request, false, RestStatus.BAD_REQUEST);
            restController.dispatchRequest(request, channel, new ThreadContext(Settings.EMPTY));
        });
    }

    /*
     * Test that when serverless is enabled, a system user using the X-elastic-internal-origin header can only access endpoints
     * annotated with a PUBLIC or INTERNAL scope.
     */
    public void testApiProtectionWithServerlessEnabledAsInternalUser() {
        final RestController restController = new RestController(
            Set.of(),
            null,
            client,
            circuitBreakerService,
            new UsageService(),
            tracer,
            true
        );
        restController.registerHandler(new PublicRestHandler());
        restController.registerHandler(new InternalRestHandler());
        restController.registerHandler(new HiddenRestHandler());
        Map<String, List<String>> headers = new HashMap<>();
        headers.put(ELASTIC_INTERNAL_ORIGIN_HTTP_HEADER, Collections.singletonList("true"));
        List<String> accessiblePaths = List.of("/public", "/internal");
        accessiblePaths.forEach(path -> {
            RestRequest request = new FakeRestRequest.Builder(xContentRegistry()).withHeaders(headers).withPath(path).build();
            AssertingChannel channel = new AssertingChannel(request, false, RestStatus.OK);
            restController.dispatchRequest(request, channel, new ThreadContext(Settings.EMPTY));
        });
        List<String> inaccessiblePaths = List.of("/hidden");
        inaccessiblePaths.forEach(path -> {
            RestRequest request = new FakeRestRequest.Builder(xContentRegistry()).withHeaders(headers).withPath(path).build();
            AssertingChannel channel = new AssertingChannel(request, false, RestStatus.BAD_REQUEST);
            restController.dispatchRequest(request, channel, new ThreadContext(Settings.EMPTY));
        });
    }

    @ServerlessScope(Scope.PUBLIC)
    private static final class PublicRestHandler extends BaseRestHandler {
        @Override
        public String getName() {
            return "publicRestHandler";
        }

        @Override
        public List<Route> routes() {
            return List.of(new Route(GET, "/public"));
        }

        @Override
        protected BaseRestHandler.RestChannelConsumer prepareRequest(RestRequest request, NodeClient client) {
            return restChannel -> { restChannel.sendResponse(new RestResponse(RestStatus.OK, "ok")); };
        }
    }

    @ServerlessScope(Scope.INTERNAL)
    private static final class InternalRestHandler extends BaseRestHandler {
        @Override
        public String getName() {
            return "internalRestHandler";
        }

        @Override
        public List<Route> routes() {
            return List.of(new Route(GET, "/internal"));
        }

        @Override
        protected RestChannelConsumer prepareRequest(RestRequest request, NodeClient client) {
            return restChannel -> { restChannel.sendResponse(new RestResponse(RestStatus.OK, "ok")); };
        }
    }

    private static final class HiddenRestHandler extends BaseRestHandler {
        @Override
        public String getName() {
            return "hiddenRestHandler";
        }

        @Override
        public List<Route> routes() {
            return List.of(new Route(GET, "/hidden"));
        }

        @Override
        protected BaseRestHandler.RestChannelConsumer prepareRequest(RestRequest request, NodeClient client) {
            return restChannel -> { restChannel.sendResponse(new RestResponse(RestStatus.OK, "ok")); };
        }
    }

    private static final class TestHttpServerTransport extends AbstractLifecycleComponent implements HttpServerTransport {

        TestHttpServerTransport() {}

        @Override
        protected void doStart() {}

        @Override
        protected void doStop() {}

        @Override
        protected void doClose() {}

        @Override
        public BoundTransportAddress boundAddress() {
            TransportAddress transportAddress = buildNewFakeTransportAddress();
            return new BoundTransportAddress(new TransportAddress[] { transportAddress }, transportAddress);
        }

        @Override
        public HttpInfo info() {
            return null;
        }

        @Override
        public HttpStats stats() {
            return null;
        }
    }

    public static final class AssertingChannel extends AbstractRestChannel {

        private final RestStatus expectedStatus;
        private final AtomicReference<RestResponse> responseReference = new AtomicReference<>();

        public AssertingChannel(RestRequest request, boolean detailedErrorsEnabled, RestStatus expectedStatus) {
            super(request, detailedErrorsEnabled);
            this.expectedStatus = expectedStatus;
        }

        @Override
        public void sendResponse(RestResponse response) {
            assertEquals(expectedStatus, response.status());
            responseReference.set(response);
        }

        RestResponse getRestResponse() {
            return responseReference.get();
        }

        boolean getSendResponseCalled() {
            return getRestResponse() != null;
        }
    }

    private static class ExceptionThrowingChannel extends AbstractRestChannel {

        protected ExceptionThrowingChannel(RestRequest request, boolean detailedErrorsEnabled) {
            super(request, detailedErrorsEnabled);
        }

        @Override
        public void sendResponse(RestResponse response) {
            try {
                throw new IllegalStateException("always throwing an exception for testing");
            } finally {
                // the production implementation in DefaultRestChannel always releases the output buffer, so we must too
                releaseOutputBuffer();
            }
        }
    }

    private static RestRequest testRestRequest(String path, String content, XContentType xContentType) {
        FakeRestRequest.Builder builder = new FakeRestRequest.Builder(NamedXContentRegistry.EMPTY);
        builder.withPath(path);
        builder.withContent(new BytesArray(content), xContentType);
        if (randomBoolean()) {
            return builder.build();
        } else {
            return new RestRequest(builder.build());
        }
    }
}
