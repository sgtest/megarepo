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

package org.elasticsearch.client.documentation;

import org.apache.http.Header;
import org.apache.http.HttpEntity;
import org.apache.http.HttpHost;
import org.apache.http.RequestLine;
import org.apache.http.auth.AuthScope;
import org.apache.http.auth.UsernamePasswordCredentials;
import org.apache.http.client.CredentialsProvider;
import org.apache.http.client.config.RequestConfig;
import org.apache.http.entity.BasicHttpEntity;
import org.apache.http.entity.ContentType;
import org.apache.http.entity.StringEntity;
import org.apache.http.impl.client.BasicCredentialsProvider;
import org.apache.http.impl.nio.client.HttpAsyncClientBuilder;
import org.apache.http.impl.nio.reactor.IOReactorConfig;
import org.apache.http.message.BasicHeader;
import org.apache.http.nio.entity.NStringEntity;
import org.apache.http.ssl.SSLContextBuilder;
import org.apache.http.ssl.SSLContexts;
import org.apache.http.util.EntityUtils;
import org.elasticsearch.client.HttpAsyncResponseConsumerFactory;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseListener;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.client.RestClientBuilder;

import javax.net.ssl.SSLContext;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.KeyStore;
import java.util.Collections;
import java.util.Map;
import java.util.concurrent.CountDownLatch;

/**
 * This class is used to generate the Java low-level REST client documentation.
 * You need to wrap your code between two tags like:
 * // tag::example[]
 * // end::example[]
 *
 * Where example is your tag name.
 *
 * Then in the documentation, you can extract what is between tag and end tags with
 * ["source","java",subs="attributes,callouts,macros"]
 * --------------------------------------------------
 * include-tagged::{doc-tests}/RestClientDocumentation.java[example]
 * --------------------------------------------------
 *
 * Note that this is not a test class as we are only interested in testing that docs snippets compile. We don't want
 * to send requests to a node and we don't even have the tools to do it.
 */
@SuppressWarnings("unused")
public class RestClientDocumentation {

    @SuppressWarnings("unused")
    public void testUsage() throws IOException, InterruptedException {

        //tag::rest-client-init
        RestClient restClient = RestClient.builder(
                new HttpHost("localhost", 9200, "http"),
                new HttpHost("localhost", 9201, "http")).build();
        //end::rest-client-init

        //tag::rest-client-close
        restClient.close();
        //end::rest-client-close

        {
            //tag::rest-client-init-default-headers
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200, "http"));
            Header[] defaultHeaders = new Header[]{new BasicHeader("header", "value")};
            builder.setDefaultHeaders(defaultHeaders); // <1>
            //end::rest-client-init-default-headers
        }
        {
            //tag::rest-client-init-max-retry-timeout
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200, "http"));
            builder.setMaxRetryTimeoutMillis(10000); // <1>
            //end::rest-client-init-max-retry-timeout
        }
        {
            //tag::rest-client-init-failure-listener
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200, "http"));
            builder.setFailureListener(new RestClient.FailureListener() {
                @Override
                public void onFailure(HttpHost host) {
                    // <1>
                }
            });
            //end::rest-client-init-failure-listener
        }
        {
            //tag::rest-client-init-request-config-callback
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200, "http"));
            builder.setRequestConfigCallback(new RestClientBuilder.RequestConfigCallback() {
                @Override
                public RequestConfig.Builder customizeRequestConfig(RequestConfig.Builder requestConfigBuilder) {
                    return requestConfigBuilder.setSocketTimeout(10000); // <1>
                }
            });
            //end::rest-client-init-request-config-callback
        }
        {
            //tag::rest-client-init-client-config-callback
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200, "http"));
            builder.setHttpClientConfigCallback(new RestClientBuilder.HttpClientConfigCallback() {
                @Override
                public HttpAsyncClientBuilder customizeHttpClient(HttpAsyncClientBuilder httpClientBuilder) {
                    return httpClientBuilder.setProxy(new HttpHost("proxy", 9000, "http"));  // <1>
                }
            });
            //end::rest-client-init-client-config-callback
        }

        {
            //tag::rest-client-sync
            Request request = new Request(
                "GET",  // <1>
                "/");   // <2>
            Response response = restClient.performRequest(request);
            //end::rest-client-sync
        }
        {
            //tag::rest-client-async
            Request request = new Request(
                "GET",  // <1>
                "/");   // <2>
            restClient.performRequestAsync(request, new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    // <3>
                }

                @Override
                public void onFailure(Exception exception) {
                    // <4>
                }
            });
            //end::rest-client-async
        }
        {
            Request request = new Request("GET", "/");
            //tag::rest-client-parameters
            request.addParameter("pretty", "true");
            //end::rest-client-parameters
            //tag::rest-client-body
            request.setEntity(new StringEntity(
                    "{\"json\":\"text\"}",
                    ContentType.APPLICATION_JSON));
            //end::rest-client-body
            //tag::rest-client-headers
            request.setHeaders(
                    new BasicHeader("Accept", "text/plain"),
                    new BasicHeader("Cache-Control", "no-cache"));
            //end::rest-client-headers
            //tag::rest-client-response-consumer
            request.setHttpAsyncResponseConsumerFactory(
                    new HttpAsyncResponseConsumerFactory.HeapBufferedResponseConsumerFactory(30 * 1024 * 1024));
            //end::rest-client-response-consumer
        }
        {
            HttpEntity[] documents = new HttpEntity[10];
            //tag::rest-client-async-example
            final CountDownLatch latch = new CountDownLatch(documents.length);
            for (int i = 0; i < documents.length; i++) {
                Request request = new Request("PUT", "/posts/doc/" + i);
                //let's assume that the documents are stored in an HttpEntity array
                request.setEntity(documents[i]);
                restClient.performRequestAsync(
                        request,
                        new ResponseListener() {
                            @Override
                            public void onSuccess(Response response) {
                                // <1>
                                latch.countDown();
                            }

                            @Override
                            public void onFailure(Exception exception) {
                                // <2>
                                latch.countDown();
                            }
                        }
                );
            }
            latch.await();
            //end::rest-client-async-example
        }
        {
            //tag::rest-client-response2
            Response response = restClient.performRequest("GET", "/");
            RequestLine requestLine = response.getRequestLine(); // <1>
            HttpHost host = response.getHost(); // <2>
            int statusCode = response.getStatusLine().getStatusCode(); // <3>
            Header[] headers = response.getHeaders(); // <4>
            String responseBody = EntityUtils.toString(response.getEntity()); // <5>
            //end::rest-client-response2
        }
    }

    @SuppressWarnings("unused")
    public void testCommonConfiguration() throws Exception {
        {
            //tag::rest-client-config-timeouts
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200))
                    .setRequestConfigCallback(new RestClientBuilder.RequestConfigCallback() {
                        @Override
                        public RequestConfig.Builder customizeRequestConfig(RequestConfig.Builder requestConfigBuilder) {
                            return requestConfigBuilder.setConnectTimeout(5000)
                                    .setSocketTimeout(60000);
                        }
                    })
                    .setMaxRetryTimeoutMillis(60000);
            //end::rest-client-config-timeouts
        }
        {
            //tag::rest-client-config-threads
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200))
                    .setHttpClientConfigCallback(new RestClientBuilder.HttpClientConfigCallback() {
                        @Override
                        public HttpAsyncClientBuilder customizeHttpClient(HttpAsyncClientBuilder httpClientBuilder) {
                            return httpClientBuilder.setDefaultIOReactorConfig(
                                    IOReactorConfig.custom().setIoThreadCount(1).build());
                        }
                    });
            //end::rest-client-config-threads
        }
        {
            //tag::rest-client-config-basic-auth
            final CredentialsProvider credentialsProvider = new BasicCredentialsProvider();
            credentialsProvider.setCredentials(AuthScope.ANY,
                    new UsernamePasswordCredentials("user", "password"));

            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200))
                    .setHttpClientConfigCallback(new RestClientBuilder.HttpClientConfigCallback() {
                        @Override
                        public HttpAsyncClientBuilder customizeHttpClient(HttpAsyncClientBuilder httpClientBuilder) {
                            return httpClientBuilder.setDefaultCredentialsProvider(credentialsProvider);
                        }
                    });
            //end::rest-client-config-basic-auth
        }
        {
            //tag::rest-client-config-disable-preemptive-auth
            final CredentialsProvider credentialsProvider = new BasicCredentialsProvider();
            credentialsProvider.setCredentials(AuthScope.ANY,
                    new UsernamePasswordCredentials("user", "password"));

            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200))
                    .setHttpClientConfigCallback(new RestClientBuilder.HttpClientConfigCallback() {
                        @Override
                        public HttpAsyncClientBuilder customizeHttpClient(HttpAsyncClientBuilder httpClientBuilder) {
                            httpClientBuilder.disableAuthCaching(); // <1>
                            return httpClientBuilder.setDefaultCredentialsProvider(credentialsProvider);
                        }
                    });
            //end::rest-client-config-disable-preemptive-auth
        }
        {
            Path keyStorePath = Paths.get("");
            String keyStorePass = "";
            //tag::rest-client-config-encrypted-communication
            KeyStore truststore = KeyStore.getInstance("jks");
            try (InputStream is = Files.newInputStream(keyStorePath)) {
                truststore.load(is, keyStorePass.toCharArray());
            }
            SSLContextBuilder sslBuilder = SSLContexts.custom().loadTrustMaterial(truststore, null);
            final SSLContext sslContext = sslBuilder.build();
            RestClientBuilder builder = RestClient.builder(new HttpHost("localhost", 9200, "https"))
                    .setHttpClientConfigCallback(new RestClientBuilder.HttpClientConfigCallback() {
                        @Override
                        public HttpAsyncClientBuilder customizeHttpClient(HttpAsyncClientBuilder httpClientBuilder) {
                            return httpClientBuilder.setSSLContext(sslContext);
                        }
                    });
            //end::rest-client-config-encrypted-communication
        }
    }
}
