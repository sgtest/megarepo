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
import org.elasticsearch.action.admin.cluster.state.ClusterStateResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.transport.MockTransportService;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;

import java.util.Collections;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.transport.RemoteClusterConnectionTests.startTransport;

public class RemoteClusterClientTests extends ESTestCase {
    private final ThreadPool threadPool = new TestThreadPool(getClass().getName());

    @Override
    public void tearDown() throws Exception {
        super.tearDown();
        ThreadPool.terminate(threadPool, 10, TimeUnit.SECONDS);
    }

    public void testConnectAndExecuteRequest() throws Exception {
        Settings remoteSettings = Settings.builder().put(ClusterName.CLUSTER_NAME_SETTING.getKey(), "foo_bar_cluster").build();
        try (MockTransportService remoteTransport = startTransport("remote_node", Collections.emptyList(), Version.CURRENT, threadPool,
            remoteSettings)) {
            DiscoveryNode remoteNode = remoteTransport.getLocalDiscoNode();

            Settings localSettings = Settings.builder()
                .put(RemoteClusterService.ENABLE_REMOTE_CLUSTERS.getKey(), true)
                .put("search.remote.test.seeds", remoteNode.getAddress().getAddress() + ":" + remoteNode.getAddress().getPort()).build();
            try (MockTransportService service = MockTransportService.createNewService(localSettings, Version.CURRENT, threadPool, null)) {
                service.start();
                service.acceptIncomingRequests();
                RemoteClusterService remoteClusterService = service.getRemoteClusterService();
                assertTrue(remoteClusterService.isRemoteNodeConnected("test", remoteNode));
                Client client = remoteClusterService.getRemoteClusterClient(threadPool, "test");
                ClusterStateResponse clusterStateResponse = client.admin().cluster().prepareState().execute().get();
                assertNotNull(clusterStateResponse);
                assertEquals("foo_bar_cluster", clusterStateResponse.getState().getClusterName().value());
                // also test a failure, there is no handler for search registered
                ActionNotFoundTransportException ex = expectThrows(ActionNotFoundTransportException.class,
                    () -> client.prepareSearch().get());
                assertEquals("No handler for action [indices:data/read/search]", ex.getMessage());
            }
        }
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/29547")
    public void testEnsureWeReconnect() throws Exception {
        Settings remoteSettings = Settings.builder().put(ClusterName.CLUSTER_NAME_SETTING.getKey(), "foo_bar_cluster").build();
        try (MockTransportService remoteTransport = startTransport("remote_node", Collections.emptyList(), Version.CURRENT, threadPool,
            remoteSettings)) {
            DiscoveryNode remoteNode = remoteTransport.getLocalDiscoNode();
            Settings localSettings = Settings.builder()
                .put(RemoteClusterService.ENABLE_REMOTE_CLUSTERS.getKey(), true)
                .put("search.remote.test.seeds", remoteNode.getAddress().getAddress() + ":" + remoteNode.getAddress().getPort()).build();
            try (MockTransportService service = MockTransportService.createNewService(localSettings, Version.CURRENT, threadPool, null)) {
                service.start();
                service.acceptIncomingRequests();
                service.disconnectFromNode(remoteNode);
                RemoteClusterService remoteClusterService = service.getRemoteClusterService();
                assertBusy(() -> assertFalse(remoteClusterService.isRemoteNodeConnected("test", remoteNode)));
                Client client = remoteClusterService.getRemoteClusterClient(threadPool, "test");
                ClusterStateResponse clusterStateResponse = client.admin().cluster().prepareState().execute().get();
                assertNotNull(clusterStateResponse);
                assertEquals("foo_bar_cluster", clusterStateResponse.getState().getClusterName().value());
            }
        }
    }

}
