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

package org.elasticsearch.index.reindex;

import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.rest.RestRequest.Method;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.test.rest.RestActionTestCase;
import org.junit.Before;

import java.io.IOException;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;

import static java.util.Collections.singletonMap;
import static org.elasticsearch.common.unit.TimeValue.timeValueSeconds;

public class RestReindexActionTests extends RestActionTestCase {

    private RestReindexAction action;

    @Before
    public void setUpAction() {
        action = new RestReindexAction(Settings.EMPTY, controller());
    }

    public void testBuildRemoteInfoNoRemote() throws IOException {
        assertNull(RestReindexAction.buildRemoteInfo(new HashMap<>()));
    }

    public void testBuildRemoteInfoFullyLoaded() throws IOException {
        Map<String, String> headers = new HashMap<>();
        headers.put("first", "a");
        headers.put("second", "b");
        headers.put("third", "");

        Map<String, Object> remote = new HashMap<>();
        remote.put("host", "https://example.com:9200");
        remote.put("username", "testuser");
        remote.put("password", "testpass");
        remote.put("headers", headers);
        remote.put("socket_timeout", "90s");
        remote.put("connect_timeout", "10s");

        Map<String, Object> query = new HashMap<>();
        query.put("a", "b");

        Map<String, Object> source = new HashMap<>();
        source.put("remote", remote);
        source.put("query", query);

        RemoteInfo remoteInfo = RestReindexAction.buildRemoteInfo(source);
        assertEquals("https", remoteInfo.getScheme());
        assertEquals("example.com", remoteInfo.getHost());
        assertEquals(9200, remoteInfo.getPort());
        assertEquals("{\n  \"a\" : \"b\"\n}", remoteInfo.getQuery().utf8ToString());
        assertEquals("testuser", remoteInfo.getUsername());
        assertEquals("testpass", remoteInfo.getPassword());
        assertEquals(headers, remoteInfo.getHeaders());
        assertEquals(timeValueSeconds(90), remoteInfo.getSocketTimeout());
        assertEquals(timeValueSeconds(10), remoteInfo.getConnectTimeout());
    }

    public void testBuildRemoteInfoWithoutAllParts() throws IOException {
        expectThrows(IllegalArgumentException.class, () -> buildRemoteInfoHostTestCase("example.com"));
        expectThrows(IllegalArgumentException.class, () -> buildRemoteInfoHostTestCase(":9200"));
        expectThrows(IllegalArgumentException.class, () -> buildRemoteInfoHostTestCase("http://:9200"));
        expectThrows(IllegalArgumentException.class, () -> buildRemoteInfoHostTestCase("example.com:9200"));
        expectThrows(IllegalArgumentException.class, () -> buildRemoteInfoHostTestCase("http://example.com"));
    }

    public void testBuildRemoteInfoWithAllHostParts() throws IOException {
        RemoteInfo info = buildRemoteInfoHostTestCase("http://example.com:9200");
        assertEquals("http", info.getScheme());
        assertEquals("example.com", info.getHost());
        assertEquals(9200, info.getPort());
        assertNull(info.getPathPrefix());
        assertEquals(RemoteInfo.DEFAULT_SOCKET_TIMEOUT, info.getSocketTimeout()); // Didn't set the timeout so we should get the default
        assertEquals(RemoteInfo.DEFAULT_CONNECT_TIMEOUT, info.getConnectTimeout()); // Didn't set the timeout so we should get the default

        info = buildRemoteInfoHostTestCase("https://other.example.com:9201");
        assertEquals("https", info.getScheme());
        assertEquals("other.example.com", info.getHost());
        assertEquals(9201, info.getPort());
        assertNull(info.getPathPrefix());
        assertEquals(RemoteInfo.DEFAULT_SOCKET_TIMEOUT, info.getSocketTimeout());
        assertEquals(RemoteInfo.DEFAULT_CONNECT_TIMEOUT, info.getConnectTimeout());

        info = buildRemoteInfoHostTestCase("https://[::1]:9201");
        assertEquals("https", info.getScheme());
        assertEquals("[::1]", info.getHost());
        assertEquals(9201, info.getPort());
        assertNull(info.getPathPrefix());
        assertEquals(RemoteInfo.DEFAULT_SOCKET_TIMEOUT, info.getSocketTimeout());
        assertEquals(RemoteInfo.DEFAULT_CONNECT_TIMEOUT, info.getConnectTimeout());

        info = buildRemoteInfoHostTestCase("https://other.example.com:9201/");
        assertEquals("https", info.getScheme());
        assertEquals("other.example.com", info.getHost());
        assertEquals(9201, info.getPort());
        assertEquals("/", info.getPathPrefix());
        assertEquals(RemoteInfo.DEFAULT_SOCKET_TIMEOUT, info.getSocketTimeout());
        assertEquals(RemoteInfo.DEFAULT_CONNECT_TIMEOUT, info.getConnectTimeout());

        info = buildRemoteInfoHostTestCase("https://other.example.com:9201/proxy-path/");
        assertEquals("https", info.getScheme());
        assertEquals("other.example.com", info.getHost());
        assertEquals(9201, info.getPort());
        assertEquals("/proxy-path/", info.getPathPrefix());
        assertEquals(RemoteInfo.DEFAULT_SOCKET_TIMEOUT, info.getSocketTimeout());
        assertEquals(RemoteInfo.DEFAULT_CONNECT_TIMEOUT, info.getConnectTimeout());

        final IllegalArgumentException exception = expectThrows(IllegalArgumentException.class,
            () -> buildRemoteInfoHostTestCase("https"));
        assertEquals("[host] must be of the form [scheme]://[host]:[port](/[pathPrefix])? but was [https]",
            exception.getMessage());
    }

    public void testReindexFromRemoteRequestParsing() throws IOException {
        BytesReference request;
        try (XContentBuilder b = JsonXContent.contentBuilder()) {
            b.startObject(); {
                b.startObject("source"); {
                    b.startObject("remote"); {
                        b.field("host", "http://localhost:9200");
                    }
                    b.endObject();
                    b.field("index", "source");
                }
                b.endObject();
                b.startObject("dest"); {
                    b.field("index", "dest");
                }
                b.endObject();
            }
            b.endObject();
            request = BytesReference.bytes(b);
        }
        try (XContentParser p = createParser(JsonXContent.jsonXContent, request)) {
            ReindexRequest r = new ReindexRequest();
            RestReindexAction.PARSER.parse(p, r, null);
            assertEquals("localhost", r.getRemoteInfo().getHost());
            assertArrayEquals(new String[] {"source"}, r.getSearchRequest().indices());
        }
    }

    public void testPipelineQueryParameterIsError() throws IOException {
        FakeRestRequest.Builder request = new FakeRestRequest.Builder(xContentRegistry());
        try (XContentBuilder body = JsonXContent.contentBuilder().prettyPrint()) {
            body.startObject(); {
                body.startObject("source"); {
                    body.field("index", "source");
                }
                body.endObject();
                body.startObject("dest"); {
                    body.field("index", "dest");
                }
                body.endObject();
            }
            body.endObject();
            request.withContent(BytesReference.bytes(body), body.contentType());
        }
        request.withParams(singletonMap("pipeline", "doesn't matter"));
        Exception e = expectThrows(IllegalArgumentException.class, () -> action.buildRequest(request.build()));

        assertEquals("_reindex doesn't support [pipeline] as a query parameter. Specify it in the [dest] object instead.", e.getMessage());
    }

    public void testSetScrollTimeout() throws IOException {
        {
            FakeRestRequest.Builder requestBuilder = new FakeRestRequest.Builder(xContentRegistry());
            requestBuilder.withContent(new BytesArray("{}"), XContentType.JSON);
            ReindexRequest request = action.buildRequest(requestBuilder.build());
            assertEquals(AbstractBulkByScrollRequest.DEFAULT_SCROLL_TIMEOUT, request.getScrollTime());
        }
        {
            FakeRestRequest.Builder requestBuilder = new FakeRestRequest.Builder(xContentRegistry());
            requestBuilder.withParams(singletonMap("scroll", "10m"));
            requestBuilder.withContent(new BytesArray("{}"), XContentType.JSON);
            ReindexRequest request = action.buildRequest(requestBuilder.build());
            assertEquals("10m", request.getScrollTime().toString());
        }
    }

    private RemoteInfo buildRemoteInfoHostTestCase(String hostInRest) throws IOException {
        Map<String, Object> remote = new HashMap<>();
        remote.put("host", hostInRest);

        Map<String, Object> source = new HashMap<>();
        source.put("remote", remote);

        return RestReindexAction.buildRemoteInfo(source);
    }

    /**
     * test deprecation is logged if one or more types are used in source search request inside reindex
     */
    public void testTypeInSource() throws IOException {
        FakeRestRequest.Builder requestBuilder = new FakeRestRequest.Builder(xContentRegistry())
                .withMethod(Method.POST)
                .withPath("/_reindex");
        XContentBuilder b = JsonXContent.contentBuilder().startObject();
        {
            b.startObject("source");
            {
                b.field("type", randomFrom(Arrays.asList("\"t1\"", "[\"t1\", \"t2\"]", "\"_doc\"")));
            }
            b.endObject();
        }
        b.endObject();
        requestBuilder.withContent(new BytesArray(BytesReference.bytes(b).toBytesRef()), XContentType.JSON);
        dispatchRequest(requestBuilder.build());
        assertWarnings(RestReindexAction.TYPES_DEPRECATION_MESSAGE);
    }

    /**
     * test deprecation is logged if a type is used in the destination index request inside reindex
     */
    public void testTypeInDestination() throws IOException {
        FakeRestRequest.Builder requestBuilder = new FakeRestRequest.Builder(xContentRegistry())
                .withMethod(Method.POST)
                .withPath("/_reindex");
        XContentBuilder b = JsonXContent.contentBuilder().startObject();
        {
            b.startObject("dest");
            {
                b.field("type", (randomBoolean() ? "_doc" : randomAlphaOfLength(4)));
            }
            b.endObject();
        }
        b.endObject();
        requestBuilder.withContent(new BytesArray(BytesReference.bytes(b).toBytesRef()), XContentType.JSON);
        dispatchRequest(requestBuilder.build());
        assertWarnings(RestReindexAction.TYPES_DEPRECATION_MESSAGE);
    }
}
