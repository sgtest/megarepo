/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.client;

import org.apache.http.Header;
import org.apache.http.HttpEntity;
import org.apache.http.HttpEntityEnclosingRequest;
import org.apache.http.HttpHost;
import org.apache.http.HttpRequest;
import org.apache.http.HttpResponse;
import org.apache.http.ProtocolVersion;
import org.apache.http.StatusLine;
import org.apache.http.client.methods.HttpHead;
import org.apache.http.client.methods.HttpOptions;
import org.apache.http.client.methods.HttpPatch;
import org.apache.http.client.methods.HttpPost;
import org.apache.http.client.methods.HttpPut;
import org.apache.http.client.methods.HttpTrace;
import org.apache.http.client.methods.HttpUriRequest;
import org.apache.http.client.protocol.HttpClientContext;
import org.apache.http.client.utils.URIBuilder;
import org.apache.http.concurrent.FutureCallback;
import org.apache.http.conn.ConnectTimeoutException;
import org.apache.http.entity.ContentType;
import org.apache.http.entity.StringEntity;
import org.apache.http.impl.auth.BasicScheme;
import org.apache.http.impl.nio.client.CloseableHttpAsyncClient;
import org.apache.http.message.BasicHttpResponse;
import org.apache.http.message.BasicStatusLine;
import org.apache.http.nio.protocol.HttpAsyncRequestProducer;
import org.apache.http.nio.protocol.HttpAsyncResponseConsumer;
import org.apache.http.util.EntityUtils;
import org.junit.After;
import org.junit.Before;
import org.mockito.ArgumentCaptor;
import org.mockito.invocation.InvocationOnMock;
import org.mockito.stubbing.Answer;

import java.io.IOException;
import java.net.SocketTimeoutException;
import java.net.URI;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

import static org.elasticsearch.client.RestClientTestUtil.getAllErrorStatusCodes;
import static org.elasticsearch.client.RestClientTestUtil.getHttpMethods;
import static org.elasticsearch.client.RestClientTestUtil.getOkStatusCodes;
import static org.elasticsearch.client.RestClientTestUtil.randomHttpMethod;
import static org.elasticsearch.client.RestClientTestUtil.randomStatusCode;
import static org.elasticsearch.client.SyncResponseListenerTests.assertExceptionStackContainsCallingMethod;
import static org.hamcrest.CoreMatchers.equalTo;
import static org.hamcrest.CoreMatchers.instanceOf;
import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThat;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;
import static org.mockito.Matchers.any;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

/**
 * Tests for basic functionality of {@link RestClient} against one single host: tests http requests being sent, headers,
 * body, different status codes and corresponding responses/exceptions.
 * Relies on a mock http client to intercept requests and return desired responses based on request path.
 */
public class RestClientSingleHostTests extends RestClientTestCase {

    private ExecutorService exec = Executors.newFixedThreadPool(1);
    private RestClient restClient;
    private Header[] defaultHeaders;
    private HttpHost httpHost;
    private CloseableHttpAsyncClient httpClient;
    private HostsTrackingFailureListener failureListener;

    @Before
    @SuppressWarnings("unchecked")
    public void createRestClient() {
        httpClient = mock(CloseableHttpAsyncClient.class);
        when(httpClient.<HttpResponse>execute(any(HttpAsyncRequestProducer.class), any(HttpAsyncResponseConsumer.class),
                any(HttpClientContext.class), any(FutureCallback.class))).thenAnswer(new Answer<Future<HttpResponse>>() {
                    @Override
                    public Future<HttpResponse> answer(InvocationOnMock invocationOnMock) throws Throwable {
                        HttpAsyncRequestProducer requestProducer = (HttpAsyncRequestProducer) invocationOnMock.getArguments()[0];
                        HttpClientContext context = (HttpClientContext) invocationOnMock.getArguments()[2];
                        assertThat(context.getAuthCache().get(httpHost), instanceOf(BasicScheme.class));
                        final FutureCallback<HttpResponse> futureCallback =
                            (FutureCallback<HttpResponse>) invocationOnMock.getArguments()[3];
                        HttpUriRequest request = (HttpUriRequest)requestProducer.generateRequest();
                        //return the desired status code or exception depending on the path
                        if (request.getURI().getPath().equals("/soe")) {
                            futureCallback.failed(new SocketTimeoutException());
                        } else if (request.getURI().getPath().equals("/coe")) {
                            futureCallback.failed(new ConnectTimeoutException());
                        } else {
                            int statusCode = Integer.parseInt(request.getURI().getPath().substring(1));
                            StatusLine statusLine = new BasicStatusLine(new ProtocolVersion("http", 1, 1), statusCode, "");

                            final HttpResponse httpResponse = new BasicHttpResponse(statusLine);
                            //return the same body that was sent
                            if (request instanceof HttpEntityEnclosingRequest) {
                                HttpEntity entity = ((HttpEntityEnclosingRequest) request).getEntity();
                                if (entity != null) {
                                    assertTrue("the entity is not repeatable, cannot set it to the response directly",
                                            entity.isRepeatable());
                                    httpResponse.setEntity(entity);
                                }
                            }
                            //return the same headers that were sent
                            httpResponse.setHeaders(request.getAllHeaders());
                            // Call the callback asynchronous to better simulate how async http client works
                            exec.execute(new Runnable() {
                                @Override
                                public void run() {
                                    futureCallback.completed(httpResponse);
                                }
                            });
                        }
                        return null;
                    }
                });

        defaultHeaders = RestClientTestUtil.randomHeaders(getRandom(), "Header-default");
        httpHost = new HttpHost("localhost", 9200);
        failureListener = new HostsTrackingFailureListener();
        restClient = new RestClient(httpClient, 10000, defaultHeaders, new HttpHost[]{httpHost}, null, failureListener);
    }

    /**
     * Shutdown the executor so we don't leak threads into other test runs.
     */
    @After
    public void shutdownExec() {
        exec.shutdown();
    }

    /**
     * Verifies the content of the {@link HttpRequest} that's internally created and passed through to the http client
     */
    @SuppressWarnings("unchecked")
    public void testInternalHttpRequest() throws Exception {
        ArgumentCaptor<HttpAsyncRequestProducer> requestArgumentCaptor = ArgumentCaptor.forClass(HttpAsyncRequestProducer.class);
        int times = 0;
        for (String httpMethod : getHttpMethods()) {
            HttpUriRequest expectedRequest = performRandomRequest(httpMethod);
            verify(httpClient, times(++times)).<HttpResponse>execute(requestArgumentCaptor.capture(),
                    any(HttpAsyncResponseConsumer.class), any(HttpClientContext.class), any(FutureCallback.class));
            HttpUriRequest actualRequest = (HttpUriRequest)requestArgumentCaptor.getValue().generateRequest();
            assertEquals(expectedRequest.getURI(), actualRequest.getURI());
            assertEquals(expectedRequest.getClass(), actualRequest.getClass());
            assertArrayEquals(expectedRequest.getAllHeaders(), actualRequest.getAllHeaders());
            if (expectedRequest instanceof HttpEntityEnclosingRequest) {
                HttpEntity expectedEntity = ((HttpEntityEnclosingRequest) expectedRequest).getEntity();
                if (expectedEntity != null) {
                    HttpEntity actualEntity = ((HttpEntityEnclosingRequest) actualRequest).getEntity();
                    assertEquals(EntityUtils.toString(expectedEntity), EntityUtils.toString(actualEntity));
                }
            }
        }
    }

    /**
     * End to end test for ok status codes
     */
    public void testOkStatusCodes() throws IOException {
        for (String method : getHttpMethods()) {
            for (int okStatusCode : getOkStatusCodes()) {
                Response response = performRequest(method, "/" + okStatusCode);
                assertThat(response.getStatusLine().getStatusCode(), equalTo(okStatusCode));
            }
        }
        failureListener.assertNotCalled();
    }

    /**
     * End to end test for error status codes: they should cause an exception to be thrown, apart from 404 with HEAD requests
     */
    public void testErrorStatusCodes() throws IOException {
        for (String method : getHttpMethods()) {
            Set<Integer> expectedIgnores = new HashSet<>();
            String ignoreParam = "";
            if (HttpHead.METHOD_NAME.equals(method)) {
                expectedIgnores.add(404);
            }
            if (randomBoolean()) {
                int numIgnores = randomIntBetween(1, 3);
                for (int i = 0; i < numIgnores; i++) {
                    Integer code = randomFrom(getAllErrorStatusCodes());
                    expectedIgnores.add(code);
                    ignoreParam += code;
                    if (i < numIgnores - 1) {
                        ignoreParam += ",";
                    }
                }
            }
            //error status codes should cause an exception to be thrown
            for (int errorStatusCode : getAllErrorStatusCodes()) {
                try {
                    Map<String, String> params;
                    if (ignoreParam.isEmpty()) {
                        params = Collections.emptyMap();
                    } else {
                        params = Collections.singletonMap("ignore", ignoreParam);
                    }
                    Response response = performRequest(method, "/" + errorStatusCode, params);
                    if (expectedIgnores.contains(errorStatusCode)) {
                        //no exception gets thrown although we got an error status code, as it was configured to be ignored
                        assertEquals(errorStatusCode, response.getStatusLine().getStatusCode());
                    } else {
                        fail("request should have failed");
                    }
                } catch(ResponseException e) {
                    if (expectedIgnores.contains(errorStatusCode)) {
                        throw e;
                    }
                    assertEquals(errorStatusCode, e.getResponse().getStatusLine().getStatusCode());
                    assertExceptionStackContainsCallingMethod(e);
                }
                if (errorStatusCode <= 500 || expectedIgnores.contains(errorStatusCode)) {
                    failureListener.assertNotCalled();
                } else {
                    failureListener.assertCalled(httpHost);
                }
            }
        }
    }

    public void testIOExceptions() {
        for (String method : getHttpMethods()) {
            //IOExceptions should be let bubble up
            try {
                performRequest(method, "/coe");
                fail("request should have failed");
            } catch(IOException e) {
                assertThat(e, instanceOf(ConnectTimeoutException.class));
            }
            failureListener.assertCalled(httpHost);
            try {
                performRequest(method, "/soe");
                fail("request should have failed");
            } catch(IOException e) {
                assertThat(e, instanceOf(SocketTimeoutException.class));
            }
            failureListener.assertCalled(httpHost);
        }
    }

    /**
     * End to end test for request and response body. Exercises the mock http client ability to send back
     * whatever body it has received.
     */
    public void testBody() throws IOException {
        String body = "{ \"field\": \"value\" }";
        StringEntity entity = new StringEntity(body, ContentType.APPLICATION_JSON);
        for (String method : Arrays.asList("DELETE", "GET", "PATCH", "POST", "PUT")) {
            for (int okStatusCode : getOkStatusCodes()) {
                Request request = new Request(method, "/" + okStatusCode);
                request.setEntity(entity);
                Response response = restClient.performRequest(request);
                assertThat(response.getStatusLine().getStatusCode(), equalTo(okStatusCode));
                assertThat(EntityUtils.toString(response.getEntity()), equalTo(body));
            }
            for (int errorStatusCode : getAllErrorStatusCodes()) {
                Request request = new Request(method, "/" + errorStatusCode);
                request.setEntity(entity);
                try {
                    restClient.performRequest(request);
                    fail("request should have failed");
                } catch(ResponseException e) {
                    Response response = e.getResponse();
                    assertThat(response.getStatusLine().getStatusCode(), equalTo(errorStatusCode));
                    assertThat(EntityUtils.toString(response.getEntity()), equalTo(body));
                    assertExceptionStackContainsCallingMethod(e);
                }
            }
        }
        for (String method : Arrays.asList("HEAD", "OPTIONS", "TRACE")) {
            Request request = new Request(method, "/" + randomStatusCode(getRandom()));
            request.setEntity(entity);
            try {
                restClient.performRequest(request);
                fail("request should have failed");
            } catch(UnsupportedOperationException e) {
                assertThat(e.getMessage(), equalTo(method + " with body is not supported"));
            }
        }
    }

    /**
     * @deprecated will remove method in 7.0 but needs tests until then. Replaced by {@link RequestTests#testSetHeaders()}.
     */
    @Deprecated
    public void tesPerformRequestOldStyleNullHeaders() throws IOException {
        String method = randomHttpMethod(getRandom());
        int statusCode = randomStatusCode(getRandom());
        try {
            performRequest(method, "/" + statusCode, (Header[])null);
            fail("request should have failed");
        } catch(NullPointerException e) {
            assertEquals("request headers must not be null", e.getMessage());
        }
        try {
            performRequest(method, "/" + statusCode, (Header)null);
            fail("request should have failed");
        } catch(NullPointerException e) {
            assertEquals("request header must not be null", e.getMessage());
        }
    }

    /**
     * @deprecated will remove method in 7.0 but needs tests until then. Replaced by {@link RequestTests#testSetParameters()}.
     */
    @Deprecated
    public void testPerformRequestOldStyleWithNullParams() throws IOException {
        String method = randomHttpMethod(getRandom());
        int statusCode = randomStatusCode(getRandom());
        try {
            restClient.performRequest(method, "/" + statusCode, (Map<String, String>)null);
            fail("request should have failed");
        } catch(NullPointerException e) {
            assertEquals("parameters cannot be null", e.getMessage());
        }
        try {
            restClient.performRequest(method, "/" + statusCode, null, (HttpEntity)null);
            fail("request should have failed");
        } catch(NullPointerException e) {
            assertEquals("parameters cannot be null", e.getMessage());
        }
    }

    /**
     * End to end test for request and response headers. Exercises the mock http client ability to send back
     * whatever headers it has received.
     */
    public void testHeaders() throws IOException {
        for (String method : getHttpMethods()) {
            final Header[] requestHeaders = RestClientTestUtil.randomHeaders(getRandom(), "Header");
            final int statusCode = randomStatusCode(getRandom());
            Request request = new Request(method, "/" + statusCode);
            request.setHeaders(requestHeaders);
            Response esResponse;
            try {
                esResponse = restClient.performRequest(request);
            } catch(ResponseException e) {
                esResponse = e.getResponse();
            }
            assertThat(esResponse.getStatusLine().getStatusCode(), equalTo(statusCode));
            assertHeaders(defaultHeaders, requestHeaders, esResponse.getHeaders(), Collections.<String>emptySet());
        }
    }

    private HttpUriRequest performRandomRequest(String method) throws Exception {
        String uriAsString = "/" + randomStatusCode(getRandom());
        Request request = new Request(method, uriAsString);
        URIBuilder uriBuilder = new URIBuilder(uriAsString);
        if (randomBoolean()) {
            int numParams = randomIntBetween(1, 3);
            for (int i = 0; i < numParams; i++) {
                String name = "param-" + i;
                String value = randomAsciiAlphanumOfLengthBetween(3, 10);
                request.addParameter(name, value);
                uriBuilder.addParameter(name, value);
            }
        }
        if (randomBoolean()) {
            //randomly add some ignore parameter, which doesn't get sent as part of the request
            String ignore = Integer.toString(randomFrom(RestClientTestUtil.getAllErrorStatusCodes()));
            if (randomBoolean()) {
                ignore += "," + Integer.toString(randomFrom(RestClientTestUtil.getAllErrorStatusCodes()));
            }
            request.addParameter("ignore", ignore);
        }
        URI uri = uriBuilder.build();

        HttpUriRequest expectedRequest;
        switch(method) {
            case "DELETE":
                expectedRequest = new HttpDeleteWithEntity(uri);
                break;
            case "GET":
                expectedRequest = new HttpGetWithEntity(uri);
                break;
            case "HEAD":
                expectedRequest = new HttpHead(uri);
                break;
            case "OPTIONS":
                expectedRequest = new HttpOptions(uri);
                break;
            case "PATCH":
                expectedRequest = new HttpPatch(uri);
                break;
            case "POST":
                expectedRequest = new HttpPost(uri);
                break;
            case "PUT":
                expectedRequest = new HttpPut(uri);
                break;
            case "TRACE":
                expectedRequest = new HttpTrace(uri);
                break;
            default:
                throw new UnsupportedOperationException("method not supported: " + method);
        }

        if (expectedRequest instanceof HttpEntityEnclosingRequest && getRandom().nextBoolean()) {
            HttpEntity entity = new StringEntity(randomAsciiAlphanumOfLengthBetween(10, 100), ContentType.APPLICATION_JSON);
            ((HttpEntityEnclosingRequest) expectedRequest).setEntity(entity);
            request.setEntity(entity);
        }

        final Set<String> uniqueNames = new HashSet<>();
        if (randomBoolean()) {
            Header[] headers = RestClientTestUtil.randomHeaders(getRandom(), "Header");
            request.setHeaders(headers);
            for (Header header : headers) {
                expectedRequest.addHeader(header);
                uniqueNames.add(header.getName());
            }
        }
        for (Header defaultHeader : defaultHeaders) {
            // request level headers override default headers
            if (uniqueNames.contains(defaultHeader.getName()) == false) {
                expectedRequest.addHeader(defaultHeader);
            }
        }

        try {
            restClient.performRequest(request);
        } catch(ResponseException e) {
            //all good
        }
        return expectedRequest;
    }

    /**
     * @deprecated prefer {@link RestClient#performRequest(Request)}.
     */
    @Deprecated
    private Response performRequest(String method, String endpoint, Header... headers) throws IOException {
        return performRequest(method, endpoint, Collections.<String, String>emptyMap(), headers);
    }

    /**
     * @deprecated prefer {@link RestClient#performRequest(Request)}.
     */
    @Deprecated
    private Response performRequest(String method, String endpoint, Map<String, String> params, Header... headers) throws IOException {
        int methodSelector;
        if (params.isEmpty()) {
            methodSelector = randomIntBetween(0, 2);
        } else {
            methodSelector = randomIntBetween(1, 2);
        }
        switch(methodSelector) {
            case 0:
                return restClient.performRequest(method, endpoint, headers);
            case 1:
                return restClient.performRequest(method, endpoint, params, headers);
            case 2:
                return restClient.performRequest(method, endpoint, params, (HttpEntity)null, headers);
            default:
                throw new UnsupportedOperationException();
        }
    }
}
