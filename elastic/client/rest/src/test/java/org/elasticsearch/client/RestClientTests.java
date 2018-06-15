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
import org.apache.http.HttpHost;
import org.apache.http.impl.nio.client.CloseableHttpAsyncClient;
import org.elasticsearch.client.DeadHostStateTests.ConfigurableTimeSupplier;
import org.elasticsearch.client.RestClient.NodeTuple;

import java.io.IOException;
import java.net.URI;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.TimeUnit;

import static java.util.Collections.singletonList;
import static org.elasticsearch.client.RestClientTestUtil.getHttpMethods;
import static org.hamcrest.Matchers.instanceOf;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThat;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;

public class RestClientTests extends RestClientTestCase {

    public void testCloseIsIdempotent() throws IOException {
        List<Node> nodes = singletonList(new Node(new HttpHost("localhost", 9200)));
        CloseableHttpAsyncClient closeableHttpAsyncClient = mock(CloseableHttpAsyncClient.class);
        RestClient restClient = new RestClient(closeableHttpAsyncClient, 1_000, new Header[0], nodes, null, null);
        restClient.close();
        verify(closeableHttpAsyncClient, times(1)).close();
        restClient.close();
        verify(closeableHttpAsyncClient, times(2)).close();
        restClient.close();
        verify(closeableHttpAsyncClient, times(3)).close();
    }

    public void testPerformAsyncWithUnsupportedMethod() throws Exception {
        final CountDownLatch latch = new CountDownLatch(1);
        try (RestClient restClient = createRestClient()) {
            restClient.performRequestAsync(new Request("unsupported", randomAsciiLettersOfLength(5)), new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    throw new UnsupportedOperationException("onSuccess cannot be called when using a mocked http client");
                }

                @Override
                public void onFailure(Exception exception) {
                    try {
                        assertThat(exception, instanceOf(UnsupportedOperationException.class));
                        assertEquals("http method not supported: unsupported", exception.getMessage());
                    } finally {
                        latch.countDown();
                    }
                }
            });
            assertTrue("time out waiting for request to return", latch.await(1000, TimeUnit.MILLISECONDS));
        }
    }

    /**
     * @deprecated will remove method in 7.0 but needs tests until then. Replaced by {@link #testPerformAsyncWithUnsupportedMethod()}.
     */
    @Deprecated
    public void testPerformAsyncOldStyleWithUnsupportedMethod() throws Exception {
        final CountDownLatch latch = new CountDownLatch(1);
        try (RestClient restClient = createRestClient()) {
            restClient.performRequestAsync("unsupported", randomAsciiLettersOfLength(5), new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    throw new UnsupportedOperationException("onSuccess cannot be called when using a mocked http client");
                }

                @Override
                public void onFailure(Exception exception) {
                    try {
                        assertThat(exception, instanceOf(UnsupportedOperationException.class));
                        assertEquals("http method not supported: unsupported", exception.getMessage());
                    } finally {
                        latch.countDown();
                    }
                }
            });
            assertTrue("time out waiting for request to return", latch.await(1000, TimeUnit.MILLISECONDS));
        }
    }

    /**
     * @deprecated will remove method in 7.0 but needs tests until then. Replaced by {@link RequestTests#testAddParameters()}.
     */
    @Deprecated
    public void testPerformOldStyleAsyncWithNullParams() throws Exception {
        final CountDownLatch latch = new CountDownLatch(1);
        try (RestClient restClient = createRestClient()) {
            restClient.performRequestAsync(randomAsciiLettersOfLength(5), randomAsciiLettersOfLength(5), null, new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    throw new UnsupportedOperationException("onSuccess cannot be called when using a mocked http client");
                }

                @Override
                public void onFailure(Exception exception) {
                    try {
                        assertThat(exception, instanceOf(NullPointerException.class));
                        assertEquals("parameters cannot be null", exception.getMessage());
                    } finally {
                        latch.countDown();
                    }
                }
            });
            assertTrue("time out waiting for request to return", latch.await(1000, TimeUnit.MILLISECONDS));
        }
    }

    /**
     * @deprecated will remove method in 7.0 but needs tests until then. Replaced by {@link RequestTests#testAddHeader()}.
     */
    @Deprecated
    public void testPerformOldStyleAsyncWithNullHeaders() throws Exception {
        final CountDownLatch latch = new CountDownLatch(1);
        try (RestClient restClient = createRestClient()) {
            ResponseListener listener = new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    throw new UnsupportedOperationException("onSuccess cannot be called when using a mocked http client");
                }

                @Override
                public void onFailure(Exception exception) {
                    try {
                        assertThat(exception, instanceOf(NullPointerException.class));
                        assertEquals("header cannot be null", exception.getMessage());
                    } finally {
                        latch.countDown();
                    }
                }
            };
            restClient.performRequestAsync("GET", randomAsciiLettersOfLength(5), listener, (Header) null);
            assertTrue("time out waiting for request to return", latch.await(1000, TimeUnit.MILLISECONDS));
        }
    }

    public void testPerformAsyncWithWrongEndpoint() throws Exception {
        final CountDownLatch latch = new CountDownLatch(1);
        try (RestClient restClient = createRestClient()) {
            restClient.performRequestAsync(new Request("GET", "::http:///"), new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    throw new UnsupportedOperationException("onSuccess cannot be called when using a mocked http client");
                }

                @Override
                public void onFailure(Exception exception) {
                    try {
                        assertThat(exception, instanceOf(IllegalArgumentException.class));
                        assertEquals("Expected scheme name at index 0: ::http:///", exception.getMessage());
                    } finally {
                        latch.countDown();
                    }
                }
            });
            assertTrue("time out waiting for request to return", latch.await(1000, TimeUnit.MILLISECONDS));
        }
    }

    /**
     * @deprecated will remove method in 7.0 but needs tests until then. Replaced by {@link #testPerformAsyncWithWrongEndpoint()}.
     */
    @Deprecated
    public void testPerformAsyncOldStyleWithWrongEndpoint() throws Exception {
        final CountDownLatch latch = new CountDownLatch(1);
        try (RestClient restClient = createRestClient()) {
            restClient.performRequestAsync("GET", "::http:///", new ResponseListener() {
                @Override
                public void onSuccess(Response response) {
                    throw new UnsupportedOperationException("onSuccess cannot be called when using a mocked http client");
                }

                @Override
                public void onFailure(Exception exception) {
                    try {
                        assertThat(exception, instanceOf(IllegalArgumentException.class));
                        assertEquals("Expected scheme name at index 0: ::http:///", exception.getMessage());
                    } finally {
                        latch.countDown();
                    }
                }
            });
            assertTrue("time out waiting for request to return", latch.await(1000, TimeUnit.MILLISECONDS));
        }
    }

    public void testBuildUriLeavesPathUntouched() {
        {
            URI uri = RestClient.buildUri("/foo$bar", "/index/type/id", Collections.<String, String>emptyMap());
            assertEquals("/foo$bar/index/type/id", uri.getPath());
        }
        {
            URI uri = RestClient.buildUri(null, "/foo$bar/ty/pe/i/d", Collections.<String, String>emptyMap());
            assertEquals("/foo$bar/ty/pe/i/d", uri.getPath());
        }
        {
            URI uri = RestClient.buildUri(null, "/index/type/id", Collections.singletonMap("foo$bar", "x/y/z"));
            assertEquals("/index/type/id", uri.getPath());
            assertEquals("foo$bar=x/y/z", uri.getQuery());
        }
    }

    @Deprecated
    public void testSetHostsWrongArguments() throws IOException {
        try (RestClient restClient = createRestClient()) {
            restClient.setHosts((HttpHost[]) null);
            fail("setHosts should have failed");
        } catch (IllegalArgumentException e) {
            assertEquals("hosts must not be null nor empty", e.getMessage());
        }
        try (RestClient restClient = createRestClient()) {
            restClient.setHosts();
            fail("setHosts should have failed");
        } catch (IllegalArgumentException e) {
            assertEquals("hosts must not be null nor empty", e.getMessage());
        }
        try (RestClient restClient = createRestClient()) {
            restClient.setHosts((HttpHost) null);
            fail("setHosts should have failed");
        } catch (IllegalArgumentException e) {
            assertEquals("host cannot be null", e.getMessage());
        }
        try (RestClient restClient = createRestClient()) {
            restClient.setHosts(new HttpHost("localhost", 9200), null, new HttpHost("localhost", 9201));
            fail("setHosts should have failed");
        } catch (IllegalArgumentException e) {
            assertEquals("host cannot be null", e.getMessage());
        }
    }

    public void testSetNodesWrongArguments() throws IOException {
        try (RestClient restClient = createRestClient()) {
            restClient.setNodes(null);
            fail("setNodes should have failed");
        } catch (IllegalArgumentException e) {
            assertEquals("nodes must not be null or empty", e.getMessage());
        }
        try (RestClient restClient = createRestClient()) {
            restClient.setNodes(Collections.<Node>emptyList());
            fail("setNodes should have failed");
        } catch (IllegalArgumentException e) {
            assertEquals("nodes must not be null or empty", e.getMessage());
        }
        try (RestClient restClient = createRestClient()) {
            restClient.setNodes(Collections.singletonList((Node) null));
            fail("setNodes should have failed");
        } catch (NullPointerException e) {
            assertEquals("node cannot be null", e.getMessage());
        }
        try (RestClient restClient = createRestClient()) {
            restClient.setNodes(Arrays.asList(
                new Node(new HttpHost("localhost", 9200)),
                null,
                new Node(new HttpHost("localhost", 9201))));
            fail("setNodes should have failed");
        } catch (NullPointerException e) {
            assertEquals("node cannot be null", e.getMessage());
        }
    }

    public void testSetNodesPreservesOrdering() throws Exception {
        try (RestClient restClient = createRestClient()) {
            List<Node> nodes = randomNodes();
            restClient.setNodes(nodes);
            assertEquals(nodes, restClient.getNodes());
        }
    }

    private static List<Node> randomNodes() {
        int numNodes = randomIntBetween(1, 10);
        List<Node> nodes = new ArrayList<>(numNodes);
        for (int i = 0; i < numNodes; i++) {
            nodes.add(new Node(new HttpHost("host-" + i, 9200)));
        }
        return nodes;
    }

    public void testSetNodesDuplicatedHosts() throws Exception {
        try (RestClient restClient = createRestClient()) {
            int numNodes = randomIntBetween(1, 10);
            List<Node> nodes = new ArrayList<>(numNodes);
            Node node = new Node(new HttpHost("host", 9200));
            for (int i = 0; i < numNodes; i++) {
                nodes.add(node);
            }
            restClient.setNodes(nodes);
            assertEquals(1, restClient.getNodes().size());
            assertEquals(node, restClient.getNodes().get(0));
        }
    }

    /**
     * @deprecated will remove method in 7.0 but needs tests until then. Replaced by {@link RequestTests#testConstructor()}.
     */
    @Deprecated
    public void testNullPath() throws IOException {
        try (RestClient restClient = createRestClient()) {
            for (String method : getHttpMethods()) {
                try {
                    restClient.performRequest(method, null);
                    fail("path set to null should fail!");
                } catch (NullPointerException e) {
                    assertEquals("endpoint cannot be null", e.getMessage());
                }
            }
        }
    }

    public void testSelectHosts() throws IOException {
        Node n1 = new Node(new HttpHost("1"), null, null, "1", null, null);
        Node n2 = new Node(new HttpHost("2"), null, null, "2", null, null);
        Node n3 = new Node(new HttpHost("3"), null, null, "3", null, null);

        NodeSelector not1 = new NodeSelector() {
            @Override
            public void select(Iterable<Node> nodes) {
                for (Iterator<Node> itr = nodes.iterator(); itr.hasNext();) {
                    if ("1".equals(itr.next().getVersion())) {
                        itr.remove();
                    }
                }
            }

            @Override
            public String toString() {
                return "NOT 1";
            }
        };
        NodeSelector noNodes = new NodeSelector() {
            @Override
            public void select(Iterable<Node> nodes) {
                for (Iterator<Node> itr = nodes.iterator(); itr.hasNext();) {
                    itr.next();
                    itr.remove();
                }
            }

            @Override
            public String toString() {
                return "NONE";
            }
        };

        NodeTuple<List<Node>> nodeTuple = new NodeTuple<>(Arrays.asList(n1, n2, n3), null);

        Map<HttpHost, DeadHostState> emptyBlacklist = Collections.emptyMap();

        // Normal cases where the node selector doesn't reject all living nodes
        assertSelectLivingHosts(Arrays.asList(n1, n2, n3), nodeTuple, emptyBlacklist, NodeSelector.ANY);
        assertSelectLivingHosts(Arrays.asList(n2, n3), nodeTuple, emptyBlacklist, not1);

        /*
         * Try a NodeSelector that excludes all nodes. This should
         * throw an exception
         */
        {
            String message = "NodeSelector [NONE] rejected all nodes, living ["
                    + "[host=http://1, version=1], [host=http://2, version=2], "
                    + "[host=http://3, version=3]] and dead []";
            assertEquals(message, assertSelectAllRejected(nodeTuple, emptyBlacklist, noNodes));
        }

        // Mark all the nodes dead for a few test cases
        {
            ConfigurableTimeSupplier timeSupplier = new ConfigurableTimeSupplier();
            Map<HttpHost, DeadHostState> blacklist = new HashMap<>();
            blacklist.put(n1.getHost(), new DeadHostState(timeSupplier));
            blacklist.put(n2.getHost(), new DeadHostState(new DeadHostState(timeSupplier)));
            blacklist.put(n3.getHost(), new DeadHostState(new DeadHostState(new DeadHostState(timeSupplier))));

            /*
             * selectHosts will revive a single host if regardless of
             * blacklist time. It'll revive the node that is closest
             * to being revived that the NodeSelector is ok with.
             */
            assertEquals(singletonList(n1), RestClient.selectHosts(nodeTuple, blacklist, new AtomicInteger(), NodeSelector.ANY));
            assertEquals(singletonList(n2), RestClient.selectHosts(nodeTuple, blacklist, new AtomicInteger(), not1));

            /*
             * Try a NodeSelector that excludes all nodes. This should
             * return a failure, but a different failure than when the
             * blacklist is empty so that the caller knows that all of
             * their nodes are blacklisted AND blocked.
             */
            String message = "NodeSelector [NONE] rejected all nodes, living [] and dead ["
                    + "[host=http://1, version=1], [host=http://2, version=2], "
                    + "[host=http://3, version=3]]";
            assertEquals(message, assertSelectAllRejected(nodeTuple, blacklist, noNodes));

            /*
             * Now lets wind the clock forward, past the timeout for one of
             * the dead nodes. We should return it.
             */
            timeSupplier.nanoTime = new DeadHostState(timeSupplier).getDeadUntilNanos();
            assertSelectLivingHosts(Arrays.asList(n1), nodeTuple, blacklist, NodeSelector.ANY);

            /*
             * But if the NodeSelector rejects that node then we'll pick the
             * first on that the NodeSelector doesn't reject.
             */
            assertSelectLivingHosts(Arrays.asList(n2), nodeTuple, blacklist, not1);

            /*
             * If we wind the clock way into the future, past any of the
             * blacklist timeouts then we function as though the nodes aren't
             * in the blacklist at all.
             */
            timeSupplier.nanoTime += DeadHostState.MAX_CONNECTION_TIMEOUT_NANOS;
            assertSelectLivingHosts(Arrays.asList(n1, n2, n3), nodeTuple, blacklist, NodeSelector.ANY);
            assertSelectLivingHosts(Arrays.asList(n2, n3), nodeTuple, blacklist, not1);
        }
    }

    private void assertSelectLivingHosts(List<Node> expectedNodes, NodeTuple<List<Node>> nodeTuple,
            Map<HttpHost, DeadHostState> blacklist, NodeSelector nodeSelector) throws IOException {
        int iterations = 1000;
        AtomicInteger lastNodeIndex = new AtomicInteger(0);
        assertEquals(expectedNodes, RestClient.selectHosts(nodeTuple, blacklist, lastNodeIndex, nodeSelector));
        // Calling it again rotates the set of results
        for (int i = 1; i < iterations; i++) {
            Collections.rotate(expectedNodes, 1);
            assertEquals("iteration " + i, expectedNodes,
                    RestClient.selectHosts(nodeTuple, blacklist, lastNodeIndex, nodeSelector));
        }
    }

    /**
     * Assert that {@link RestClient#selectHosts} fails on the provided arguments.
     * @return the message in the exception thrown by the failure
     */
    private String assertSelectAllRejected( NodeTuple<List<Node>> nodeTuple,
            Map<HttpHost, DeadHostState> blacklist, NodeSelector nodeSelector) {
        try {
            RestClient.selectHosts(nodeTuple, blacklist, new AtomicInteger(0), nodeSelector);
            throw new AssertionError("expected selectHosts to fail");
        } catch (IOException e) {
            return e.getMessage();
        }
    }

    private static RestClient createRestClient() {
        List<Node> nodes = Collections.singletonList(new Node(new HttpHost("localhost", 9200)));
        return new RestClient(mock(CloseableHttpAsyncClient.class), randomLongBetween(1_000, 30_000),
                new Header[] {}, nodes, null, null);
    }


}
