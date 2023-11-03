/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.inference.external.huggingface;

import org.apache.http.HttpHeaders;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.inference.InferenceResults;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.http.MockResponse;
import org.elasticsearch.test.http.MockWebServer;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.inference.external.http.HttpClientManager;
import org.elasticsearch.xpack.inference.external.http.sender.HttpRequestSenderFactory;
import org.elasticsearch.xpack.inference.external.http.sender.Sender;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.net.URISyntaxException;
import java.util.Map;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.core.Strings.format;
import static org.elasticsearch.xpack.core.ml.inference.trainedmodel.InferenceConfig.DEFAULT_RESULTS_FIELD;
import static org.elasticsearch.xpack.inference.external.http.Utils.createThreadPool;
import static org.elasticsearch.xpack.inference.external.http.Utils.entityAsMap;
import static org.elasticsearch.xpack.inference.external.http.Utils.getUrl;
import static org.elasticsearch.xpack.inference.external.http.Utils.mockClusterServiceEmpty;
import static org.elasticsearch.xpack.inference.external.request.huggingface.HuggingFaceElserRequestTests.createRequest;
import static org.elasticsearch.xpack.inference.logging.ThrottlerManagerTests.mockThrottlerManager;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doThrow;
import static org.mockito.Mockito.mock;

public class HuggingFaceClientTests extends ESTestCase {
    private static final TimeValue TIMEOUT = new TimeValue(30, TimeUnit.SECONDS);
    private final MockWebServer webServer = new MockWebServer();
    private ThreadPool threadPool;
    private HttpClientManager clientManager;

    @Before
    public void init() throws Exception {
        webServer.start();
        threadPool = createThreadPool(getTestName());
        clientManager = HttpClientManager.create(Settings.EMPTY, threadPool, mockClusterServiceEmpty(), mockThrottlerManager());
    }

    @After
    public void shutdown() throws IOException {
        clientManager.close();
        terminate(threadPool);
        webServer.close();
    }

    public void testSend_SuccessfulResponse() throws IOException, URISyntaxException {
        var senderFactory = new HttpRequestSenderFactory(threadPool, clientManager, mockClusterServiceEmpty(), Settings.EMPTY);

        try (var sender = senderFactory.createSender("test_service")) {
            sender.start();

            String responseJson = """
                [
                    {
                        ".": 0.133155956864357
                    }
                ]
                """;
            webServer.enqueue(new MockResponse().setResponseCode(200).setBody(responseJson));

            HuggingFaceClient huggingFaceClient = new HuggingFaceClient(sender, mockThrottlerManager());

            PlainActionFuture<InferenceResults> listener = new PlainActionFuture<>();
            huggingFaceClient.send(createRequest(getUrl(webServer), "secret", "abc"), listener);

            InferenceResults result = listener.actionGet(TIMEOUT);

            assertThat(result.asMap(), is(Map.of(DEFAULT_RESULTS_FIELD, Map.of(".", 0.13315596f))));

            assertThat(webServer.requests(), hasSize(1));
            assertNull(webServer.requests().get(0).getUri().getQuery());
            assertThat(
                webServer.requests().get(0).getHeader(HttpHeaders.CONTENT_TYPE),
                equalTo(XContentType.JSON.mediaTypeWithoutParameters())
            );
            assertThat(webServer.requests().get(0).getHeader(HttpHeaders.AUTHORIZATION), equalTo("Bearer secret"));

            var requestMap = entityAsMap(webServer.requests().get(0).getBody());
            assertThat(requestMap.size(), is(1));
            assertThat(requestMap.get("inputs"), is("abc"));
        }
    }

    public void testSend_FailsFromInvalidResponseFormat() throws IOException, URISyntaxException {
        var senderFactory = new HttpRequestSenderFactory(threadPool, clientManager, mockClusterServiceEmpty(), Settings.EMPTY);

        try (var sender = senderFactory.createSender("test_service")) {
            sender.start();

            String responseJson = """
                [
                  {
                    "outputs": [
                      [
                        [
                          ".",
                          ".",
                          0.133155956864357
                        ]
                      ]
                    ]
                  }
                ]
                """;
            webServer.enqueue(new MockResponse().setResponseCode(200).setBody(responseJson));

            HuggingFaceClient huggingFaceClient = new HuggingFaceClient(sender, mockThrottlerManager());

            PlainActionFuture<InferenceResults> listener = new PlainActionFuture<>();
            huggingFaceClient.send(createRequest(getUrl(webServer), "secret", "abc"), listener);

            var thrownException = expectThrows(ElasticsearchException.class, () -> listener.actionGet(TIMEOUT));
            assertThat(
                thrownException.getMessage(),
                is(format("Failed to parse the Hugging Face ELSER response for request [POST %s HTTP/1.1]", getUrl(webServer)))
            );

            assertThat(webServer.requests(), hasSize(1));
            assertNull(webServer.requests().get(0).getUri().getQuery());
            assertThat(
                webServer.requests().get(0).getHeader(HttpHeaders.CONTENT_TYPE),
                equalTo(XContentType.JSON.mediaTypeWithoutParameters())
            );
            assertThat(webServer.requests().get(0).getHeader(HttpHeaders.AUTHORIZATION), equalTo("Bearer secret"));

            var requestMap = entityAsMap(webServer.requests().get(0).getBody());
            assertThat(requestMap.size(), is(1));
            assertThat(requestMap.get("inputs"), is("abc"));
        }
    }

    public void testSend_ThrowsException() {
        var sender = mock(Sender.class);
        doThrow(new ElasticsearchException("failed")).when(sender).send(any(), any());

        HuggingFaceClient huggingFaceClient = new HuggingFaceClient(sender, mockThrottlerManager());
        PlainActionFuture<InferenceResults> listener = new PlainActionFuture<>();

        var thrownException = expectThrows(
            ElasticsearchException.class,
            () -> huggingFaceClient.send(createRequest(getUrl(webServer), "secret", "abc"), listener)
        );
        assertThat(thrownException.getMessage(), is("failed"));
    }

}
