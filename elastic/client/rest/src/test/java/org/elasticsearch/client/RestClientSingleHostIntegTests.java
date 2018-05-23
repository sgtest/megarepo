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

import com.sun.net.httpserver.Headers;
import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpHandler;
import com.sun.net.httpserver.HttpServer;
import org.apache.http.Consts;
import org.apache.http.Header;
import org.apache.http.HttpHost;
import org.apache.http.auth.AuthScope;
import org.apache.http.auth.UsernamePasswordCredentials;
import org.apache.http.entity.ContentType;
import org.apache.http.impl.client.BasicCredentialsProvider;
import org.apache.http.impl.client.TargetAuthenticationStrategy;
import org.apache.http.impl.nio.client.HttpAsyncClientBuilder;
import org.apache.http.message.BasicHeader;
import org.apache.http.nio.entity.NStringEntity;
import org.apache.http.util.EntityUtils;
import org.elasticsearch.mocksocket.MockHttpServer;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.client.RestClientTestUtil.getAllStatusCodes;
import static org.elasticsearch.client.RestClientTestUtil.getHttpMethods;
import static org.elasticsearch.client.RestClientTestUtil.randomStatusCode;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.Matchers.startsWith;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThat;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

/**
 * Integration test to check interaction between {@link RestClient} and {@link org.apache.http.client.HttpClient}.
 * Works against a real http server, one single host.
 */
public class RestClientSingleHostIntegTests extends RestClientTestCase {

    private HttpServer httpServer;
    private RestClient restClient;
    private String pathPrefix;
    private Header[] defaultHeaders;

    @Before
    public void startHttpServer() throws Exception {
        pathPrefix = randomBoolean() ? "/testPathPrefix/" + randomAsciiLettersOfLengthBetween(1, 5) : "";
        httpServer = createHttpServer();
        defaultHeaders = RestClientTestUtil.randomHeaders(getRandom(), "Header-default");
        restClient = createRestClient(false, true);
    }

    private HttpServer createHttpServer() throws Exception {
        HttpServer httpServer = MockHttpServer.createHttp(new InetSocketAddress(InetAddress.getLoopbackAddress(), 0), 0);
        httpServer.start();
        //returns a different status code depending on the path
        for (int statusCode : getAllStatusCodes()) {
            httpServer.createContext(pathPrefix + "/" + statusCode, new ResponseHandler(statusCode));
        }
        return httpServer;
    }

    private static class ResponseHandler implements HttpHandler {
        private final int statusCode;

        ResponseHandler(int statusCode) {
            this.statusCode = statusCode;
        }

        @Override
        public void handle(HttpExchange httpExchange) throws IOException {
            //copy request body to response body so we can verify it was sent
            StringBuilder body = new StringBuilder();
            try (InputStreamReader reader = new InputStreamReader(httpExchange.getRequestBody(), Consts.UTF_8)) {
                char[] buffer = new char[256];
                int read;
                while ((read = reader.read(buffer)) != -1) {
                    body.append(buffer, 0, read);
                }
            }
            //copy request headers to response headers so we can verify they were sent
            Headers requestHeaders = httpExchange.getRequestHeaders();
            Headers responseHeaders = httpExchange.getResponseHeaders();
            for (Map.Entry<String, List<String>> header : requestHeaders.entrySet()) {
                responseHeaders.put(header.getKey(), header.getValue());
            }
            httpExchange.getRequestBody().close();
            httpExchange.sendResponseHeaders(statusCode, body.length() == 0 ? -1 : body.length());
            if (body.length() > 0) {
                try (OutputStream out = httpExchange.getResponseBody()) {
                    out.write(body.toString().getBytes(Consts.UTF_8));
                }
            }
            httpExchange.close();
        }
    }

    private RestClient createRestClient(final boolean useAuth, final boolean usePreemptiveAuth) {
        // provide the username/password for every request
        final BasicCredentialsProvider credentialsProvider = new BasicCredentialsProvider();
        credentialsProvider.setCredentials(AuthScope.ANY, new UsernamePasswordCredentials("user", "pass"));

        final RestClientBuilder restClientBuilder = RestClient.builder(
            new HttpHost(httpServer.getAddress().getHostString(), httpServer.getAddress().getPort())).setDefaultHeaders(defaultHeaders);
        if (pathPrefix.length() > 0) {
            restClientBuilder.setPathPrefix(pathPrefix);
        }

        if (useAuth) {
            restClientBuilder.setHttpClientConfigCallback(new RestClientBuilder.HttpClientConfigCallback() {
                @Override
                public HttpAsyncClientBuilder customizeHttpClient(final HttpAsyncClientBuilder httpClientBuilder) {
                    if (usePreemptiveAuth == false) {
                        // disable preemptive auth by ignoring any authcache
                        httpClientBuilder.disableAuthCaching();
                        // don't use the "persistent credentials strategy"
                        httpClientBuilder.setTargetAuthenticationStrategy(new TargetAuthenticationStrategy());
                    }

                    return httpClientBuilder.setDefaultCredentialsProvider(credentialsProvider);
                }
            });
        }

        return restClientBuilder.build();
    }

    @After
    public void stopHttpServers() throws IOException {
        restClient.close();
        restClient = null;
        httpServer.stop(0);
        httpServer = null;
    }

    /**
     * Tests sending a bunch of async requests works well (e.g. no TimeoutException from the leased pool)
     * See https://github.com/elastic/elasticsearch/issues/24069
     */
    public void testManyAsyncRequests() throws Exception {
        int iters = randomIntBetween(500, 1000);
        final CountDownLatch latch = new CountDownLatch(iters);
        final List<Exception> exceptions = new CopyOnWriteArrayList<>();
        for (int i = 0; i < iters; i++) {
            Request request = new Request("PUT", "/200");
            request.setEntity(new NStringEntity("{}", ContentType.APPLICATION_JSON));
            restClient.performRequestAsync(request, new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    latch.countDown();
                }

                @Override
                public void onFailure(Exception exception) {
                    exceptions.add(exception);
                    latch.countDown();
                }
            });
        }

        assertTrue("timeout waiting for requests to be sent", latch.await(10, TimeUnit.SECONDS));
        if (exceptions.isEmpty() == false) {
            AssertionError error = new AssertionError("expected no failures but got some. see suppressed for first 10 of ["
                + exceptions.size() + "] failures");
            for (Exception exception : exceptions.subList(0, Math.min(10, exceptions.size()))) {
                error.addSuppressed(exception);
            }
            throw error;
        }
    }

    /**
     * End to end test for headers. We test it explicitly against a real http client as there are different ways
     * to set/add headers to the {@link org.apache.http.client.HttpClient}.
     * Exercises the test http server ability to send back whatever headers it received.
     */
    public void testHeaders() throws IOException {
        for (String method : getHttpMethods()) {
            final Set<String> standardHeaders = new HashSet<>(Arrays.asList("Connection", "Host", "User-agent", "Date"));
            if (method.equals("HEAD") == false) {
                standardHeaders.add("Content-length");
            }
            final Header[] requestHeaders = RestClientTestUtil.randomHeaders(getRandom(), "Header");
            final int statusCode = randomStatusCode(getRandom());
            Response esResponse;
            try {
                esResponse = restClient.performRequest(method, "/" + statusCode, Collections.<String, String>emptyMap(), requestHeaders);
            } catch (ResponseException e) {
                esResponse = e.getResponse();
            }

            assertEquals(method, esResponse.getRequestLine().getMethod());
            assertEquals(statusCode, esResponse.getStatusLine().getStatusCode());
            assertEquals(pathPrefix + "/" + statusCode, esResponse.getRequestLine().getUri());
            assertHeaders(defaultHeaders, requestHeaders, esResponse.getHeaders(), standardHeaders);
            for (final Header responseHeader : esResponse.getHeaders()) {
                String name = responseHeader.getName();
                if (name.startsWith("Header") == false) {
                    assertTrue("unknown header was returned " + name, standardHeaders.remove(name));
                }
            }
            assertTrue("some expected standard headers weren't returned: " + standardHeaders, standardHeaders.isEmpty());
        }
    }

    /**
     * End to end test for delete with body. We test it explicitly as it is not supported
     * out of the box by {@link org.apache.http.client.HttpClient}.
     * Exercises the test http server ability to send back whatever body it received.
     */
    public void testDeleteWithBody() throws IOException {
        bodyTest("DELETE");
    }

    /**
     * End to end test for get with body. We test it explicitly as it is not supported
     * out of the box by {@link org.apache.http.client.HttpClient}.
     * Exercises the test http server ability to send back whatever body it received.
     */
    public void testGetWithBody() throws IOException {
        bodyTest("GET");
    }

    public void testEncodeParams() throws IOException {
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "this/is/the/routing"));
            assertEquals(pathPrefix + "/200?routing=this%2Fis%2Fthe%2Frouting", response.getRequestLine().getUri());
        }
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "this|is|the|routing"));
            assertEquals(pathPrefix + "/200?routing=this%7Cis%7Cthe%7Crouting", response.getRequestLine().getUri());
        }
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "routing#1"));
            assertEquals(pathPrefix + "/200?routing=routing%231", response.getRequestLine().getUri());
        }
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "中文"));
            assertEquals(pathPrefix + "/200?routing=%E4%B8%AD%E6%96%87", response.getRequestLine().getUri());
        }
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "foo bar"));
            assertEquals(pathPrefix + "/200?routing=foo+bar", response.getRequestLine().getUri());
        }
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "foo+bar"));
            assertEquals(pathPrefix + "/200?routing=foo%2Bbar", response.getRequestLine().getUri());
        }
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "foo/bar"));
            assertEquals(pathPrefix + "/200?routing=foo%2Fbar", response.getRequestLine().getUri());
        }
        {
            Response response = restClient.performRequest("PUT", "/200", Collections.singletonMap("routing", "foo^bar"));
            assertEquals(pathPrefix + "/200?routing=foo%5Ebar", response.getRequestLine().getUri());
        }
    }

    /**
     * Verify that credentials are sent on the first request with preemptive auth enabled (default when provided with credentials).
     */
    public void testPreemptiveAuthEnabled() throws IOException {
        final String[] methods = {"POST", "PUT", "GET", "DELETE"};

        try (RestClient restClient = createRestClient(true, true)) {
            for (final String method : methods) {
                final Response response = bodyTest(restClient, method);

                assertThat(response.getHeader("Authorization"), startsWith("Basic"));
            }
        }
    }

    /**
     * Verify that credentials are <em>not</em> sent on the first request with preemptive auth disabled.
     */
    public void testPreemptiveAuthDisabled() throws IOException {
        final String[] methods = {"POST", "PUT", "GET", "DELETE"};

        try (RestClient restClient = createRestClient(true, false)) {
            for (final String method : methods) {
                final Response response = bodyTest(restClient, method);

                assertThat(response.getHeader("Authorization"), nullValue());
            }
        }
    }

    /**
     * Verify that credentials continue to be sent even if a 401 (Unauthorized) response is received
     */
    public void testAuthCredentialsAreNotClearedOnAuthChallenge() throws IOException {
        final String[] methods = {"POST", "PUT", "GET", "DELETE"};

        try (RestClient restClient = createRestClient(true, true)) {
            for (final String method : methods) {
                Header realmHeader = new BasicHeader("WWW-Authenticate", "Basic realm=\"test\"");
                final Response response401 = bodyTest(restClient, method, 401, new Header[]{realmHeader});
                assertThat(response401.getHeader("Authorization"), startsWith("Basic"));

                final Response response200 = bodyTest(restClient, method, 200, new Header[0]);
                assertThat(response200.getHeader("Authorization"), startsWith("Basic"));
            }
        }

    }

    public void testUrlWithoutLeadingSlash() throws Exception {
        if (pathPrefix.length() == 0) {
            try {
                restClient.performRequest("GET", "200");
                fail("request should have failed");
            } catch (ResponseException e) {
                assertEquals(404, e.getResponse().getStatusLine().getStatusCode());
            }
        } else {
            {
                Response response = restClient.performRequest("GET", "200");
                //a trailing slash gets automatically added if a pathPrefix is configured
                assertEquals(200, response.getStatusLine().getStatusCode());
            }
            {
                //pathPrefix is not required to start with '/', will be added automatically
                try (RestClient restClient = RestClient.builder(
                    new HttpHost(httpServer.getAddress().getHostString(), httpServer.getAddress().getPort()))
                    .setPathPrefix(pathPrefix.substring(1)).build()) {
                    Response response = restClient.performRequest("GET", "200");
                    //a trailing slash gets automatically added if a pathPrefix is configured
                    assertEquals(200, response.getStatusLine().getStatusCode());
                }
            }
        }
    }

    private Response bodyTest(final String method) throws IOException {
        return bodyTest(restClient, method);
    }

    private Response bodyTest(final RestClient restClient, final String method) throws IOException {
        int statusCode = randomStatusCode(getRandom());
        return bodyTest(restClient, method, statusCode, new Header[0]);
    }

    private Response bodyTest(RestClient restClient, String method, int statusCode, Header[] headers) throws IOException {
        String requestBody = "{ \"field\": \"value\" }";
        Request request = new Request(method, "/" + statusCode);
        request.setJsonEntity(requestBody);
        for (Header header : headers) {
            request.addHeader(header.getName(), header.getValue());
        }
        Response esResponse;
        try {
            esResponse = restClient.performRequest(request);
        } catch(ResponseException e) {
            esResponse = e.getResponse();
        }
        assertEquals(method, esResponse.getRequestLine().getMethod());
        assertEquals(statusCode, esResponse.getStatusLine().getStatusCode());
        assertEquals(pathPrefix + "/" + statusCode, esResponse.getRequestLine().getUri());
        assertEquals(requestBody, EntityUtils.toString(esResponse.getEntity()));

        return esResponse;
    }
}
