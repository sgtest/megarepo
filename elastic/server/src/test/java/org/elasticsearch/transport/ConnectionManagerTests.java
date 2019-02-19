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

package org.elasticsearch.transport;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.CheckedBiConsumer;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.net.InetAddress;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static org.mockito.Matchers.any;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;

public class ConnectionManagerTests extends ESTestCase {

    private ConnectionManager connectionManager;
    private ThreadPool threadPool;
    private Transport transport;
    private ConnectionProfile connectionProfile;

    @Before
    public void createConnectionManager() {
        Settings settings = Settings.builder()
            .put("node.name", ConnectionManagerTests.class.getSimpleName())
            .build();
        threadPool = new ThreadPool(settings);
        transport = mock(Transport.class);
        connectionManager = new ConnectionManager(settings, transport);
        TimeValue oneSecond = new TimeValue(1000);
        TimeValue oneMinute = TimeValue.timeValueMinutes(1);
        connectionProfile = ConnectionProfile.buildSingleChannelProfile(TransportRequestOptions.Type.REG, oneSecond, oneSecond,
            oneMinute, false);
    }

    @After
    public void stopThreadPool() {
        threadPool.shutdown();
    }

    public void testConnectAndDisconnect() {
        AtomicInteger nodeConnectedCount = new AtomicInteger();
        AtomicInteger nodeDisconnectedCount = new AtomicInteger();
        connectionManager.addListener(new TransportConnectionListener() {
            @Override
            public void onNodeConnected(DiscoveryNode node) {
                nodeConnectedCount.incrementAndGet();
            }

            @Override
            public void onNodeDisconnected(DiscoveryNode node) {
                nodeDisconnectedCount.incrementAndGet();
            }
        });


        DiscoveryNode node = new DiscoveryNode("", new TransportAddress(InetAddress.getLoopbackAddress(), 0), Version.CURRENT);
        Transport.Connection connection = new TestConnect(node);
        doAnswer(invocationOnMock -> {
            ActionListener<Transport.Connection> listener = (ActionListener<Transport.Connection>) invocationOnMock.getArguments()[2];
            listener.onResponse(connection);
            return null;
        }).when(transport).openConnection(eq(node), eq(connectionProfile), any(ActionListener.class));

        assertFalse(connectionManager.nodeConnected(node));

        AtomicReference<Transport.Connection> connectionRef = new AtomicReference<>();
        CheckedBiConsumer<Transport.Connection, ConnectionProfile, IOException> validator = (c, p) -> connectionRef.set(c);
        connectionManager.connectToNode(node, connectionProfile, validator);

        assertFalse(connection.isClosed());
        assertTrue(connectionManager.nodeConnected(node));
        assertSame(connection, connectionManager.getConnection(node));
        assertEquals(1, connectionManager.size());
        assertEquals(1, nodeConnectedCount.get());
        assertEquals(0, nodeDisconnectedCount.get());

        if (randomBoolean()) {
            connectionManager.disconnectFromNode(node);
        } else {
            connection.close();
        }
        assertTrue(connection.isClosed());
        assertEquals(0, connectionManager.size());
        assertEquals(1, nodeConnectedCount.get());
        assertEquals(1, nodeDisconnectedCount.get());
    }

    public void testConnectFails() {
        AtomicInteger nodeConnectedCount = new AtomicInteger();
        AtomicInteger nodeDisconnectedCount = new AtomicInteger();
        connectionManager.addListener(new TransportConnectionListener() {
            @Override
            public void onNodeConnected(DiscoveryNode node) {
                nodeConnectedCount.incrementAndGet();
            }

            @Override
            public void onNodeDisconnected(DiscoveryNode node) {
                nodeDisconnectedCount.incrementAndGet();
            }
        });


        DiscoveryNode node = new DiscoveryNode("", new TransportAddress(InetAddress.getLoopbackAddress(), 0), Version.CURRENT);
        Transport.Connection connection = new TestConnect(node);
        doAnswer(invocationOnMock -> {
            ActionListener<Transport.Connection> listener = (ActionListener<Transport.Connection>) invocationOnMock.getArguments()[2];
            listener.onResponse(connection);
            return null;
        }).when(transport).openConnection(eq(node), eq(connectionProfile), any(ActionListener.class));

        assertFalse(connectionManager.nodeConnected(node));

        CheckedBiConsumer<Transport.Connection, ConnectionProfile, IOException> validator = (c, p) -> {
            throw new ConnectTransportException(node, "");
        };

        expectThrows(ConnectTransportException.class, () -> connectionManager.connectToNode(node, connectionProfile, validator));

        assertTrue(connection.isClosed());
        assertFalse(connectionManager.nodeConnected(node));
        expectThrows(NodeNotConnectedException.class, () -> connectionManager.getConnection(node));
        assertEquals(0, connectionManager.size());
        assertEquals(0, nodeConnectedCount.get());
        assertEquals(0, nodeDisconnectedCount.get());
    }

    private static class TestConnect extends CloseableConnection {

        private final DiscoveryNode node;

        private TestConnect(DiscoveryNode node) {
            this.node = node;
        }

        @Override
        public DiscoveryNode getNode() {
            return node;
        }

        @Override
        public void sendRequest(long requestId, String action, TransportRequest request, TransportRequestOptions options)
            throws TransportException {

        }
    }
}
