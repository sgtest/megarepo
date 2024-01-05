/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security;

import org.apache.lucene.search.TotalHits;
import org.elasticsearch.TransportVersion;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.node.reload.NodesReloadSecureSettingsRequest;
import org.elasticsearch.action.admin.cluster.node.reload.NodesReloadSecureSettingsResponse;
import org.elasticsearch.action.admin.cluster.node.reload.TransportNodesReloadSecureSettingsAction;
import org.elasticsearch.action.admin.cluster.remote.RemoteClusterNodesAction;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsRequest;
import org.elasticsearch.action.admin.cluster.state.ClusterStateAction;
import org.elasticsearch.action.admin.cluster.state.ClusterStateRequest;
import org.elasticsearch.action.admin.cluster.state.ClusterStateResponse;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchShardsRequest;
import org.elasticsearch.action.search.SearchShardsResponse;
import org.elasticsearch.action.search.ShardSearchFailure;
import org.elasticsearch.action.search.TransportSearchAction;
import org.elasticsearch.action.search.TransportSearchShardsAction;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.VersionInformation;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.KeyStoreWrapper;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.common.util.concurrent.EsExecutors;
import org.elasticsearch.env.Environment;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.search.aggregations.InternalAggregations;
import org.elasticsearch.test.SecuritySingleNodeTestCase;
import org.elasticsearch.test.transport.MockTransportService;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.RemoteClusterCredentialsManager;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.security.authc.ApiKeyService;
import org.elasticsearch.xpack.security.authc.CrossClusterAccessHeaders;
import org.junit.BeforeClass;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class ReloadRemoteClusterCredentialsIT extends SecuritySingleNodeTestCase {
    private static final String CLUSTER_ALIAS = "my_remote_cluster";

    @BeforeClass
    public static void disableInFips() {
        assumeFalse(
            "Cannot run in FIPS mode since the keystore will be password protected and sending a password in the reload"
                + "settings api call, require TLS to be configured for the transport layer",
            inFipsJvm()
        );
    }

    @Override
    public String configRoles() {
        return org.elasticsearch.core.Strings.format("""
            user:
              cluster: [ "ALL" ]
              indices:
                - names: '*'
                  privileges: [ "ALL" ]
              remote_indices:
                - names: '*'
                  privileges: [ "ALL" ]
                  clusters: ["*"]
            """);
    }

    @Override
    public void tearDown() throws Exception {
        try {
            clearRemoteCluster();
            super.tearDown();
        } finally {
            ThreadPool.terminate(threadPool, 10, TimeUnit.SECONDS);
        }
    }

    private final ThreadPool threadPool = new TestThreadPool(getClass().getName());

    public void testReloadRemoteClusterCredentials() throws Exception {
        final String credentials = randomAlphaOfLength(42);
        writeCredentialsToKeyStore(credentials);
        final RemoteClusterCredentialsManager clusterCredentialsManager = getInstanceFromNode(TransportService.class)
            .getRemoteClusterService()
            .getRemoteClusterCredentialsManager();
        // Until we reload, credentials written to keystore are not loaded into the credentials manager
        assertThat(clusterCredentialsManager.hasCredentials(CLUSTER_ALIAS), is(false));
        reloadSecureSettings();
        assertThat(clusterCredentialsManager.resolveCredentials(CLUSTER_ALIAS), equalTo(credentials));

        // Check that credentials get used for a remote connection, once we configure it
        final BlockingQueue<Map<String, String>> capturedHeaders = ConcurrentCollections.newBlockingQueue();
        try (MockTransportService remoteTransport = startTransport("remoteNodeA", threadPool, capturedHeaders)) {
            final TransportAddress remoteAddress = remoteTransport.getOriginalTransport()
                .profileBoundAddresses()
                .get("_remote_cluster")
                .publishAddress();

            configureRemoteCluster(remoteAddress);

            // Run search to trigger header capturing on the receiving side
            client().search(new SearchRequest(CLUSTER_ALIAS + ":index-a")).get().decRef();

            assertHeadersContainCredentialsThenClear(credentials, capturedHeaders);

            // Update credentials and ensure they are used
            final String updatedCredentials = randomAlphaOfLength(41);
            writeCredentialsToKeyStore(updatedCredentials);
            reloadSecureSettings();

            client().search(new SearchRequest(CLUSTER_ALIAS + ":index-a")).get().decRef();

            assertHeadersContainCredentialsThenClear(updatedCredentials, capturedHeaders);
        }
    }

    private void assertHeadersContainCredentialsThenClear(String credentials, BlockingQueue<Map<String, String>> capturedHeaders) {
        assertThat(capturedHeaders, is(not(empty())));
        for (Map<String, String> actualHeaders : capturedHeaders) {
            assertThat(actualHeaders, hasKey(CrossClusterAccessHeaders.CROSS_CLUSTER_ACCESS_CREDENTIALS_HEADER_KEY));
            assertThat(
                actualHeaders.get(CrossClusterAccessHeaders.CROSS_CLUSTER_ACCESS_CREDENTIALS_HEADER_KEY),
                equalTo(ApiKeyService.withApiKeyPrefix(credentials))
            );
        }
        capturedHeaders.clear();
        assertThat(capturedHeaders, is(empty()));
    }

    private void clearRemoteCluster() throws InterruptedException, ExecutionException {
        final var builder = Settings.builder()
            .putNull("cluster.remote." + CLUSTER_ALIAS + ".mode")
            .putNull("cluster.remote." + CLUSTER_ALIAS + ".seeds")
            .putNull("cluster.remote." + CLUSTER_ALIAS + ".proxy_address");
        clusterAdmin().updateSettings(new ClusterUpdateSettingsRequest().persistentSettings(builder)).get();
    }

    @Override
    protected Settings nodeSettings() {
        return Settings.builder().put(super.nodeSettings()).put("xpack.security.remote_cluster_client.ssl.enabled", false).build();
    }

    private void configureRemoteCluster(TransportAddress remoteAddress) throws InterruptedException, ExecutionException {
        final Settings.Builder builder = Settings.builder();
        if (randomBoolean()) {
            builder.put("cluster.remote." + CLUSTER_ALIAS + ".mode", "sniff")
                .put("cluster.remote." + CLUSTER_ALIAS + ".seeds", remoteAddress.toString())
                .putNull("cluster.remote." + CLUSTER_ALIAS + ".proxy_address");
        } else {
            builder.put("cluster.remote." + CLUSTER_ALIAS + ".mode", "proxy")
                .put("cluster.remote." + CLUSTER_ALIAS + ".proxy_address", remoteAddress.toString())
                .putNull("cluster.remote." + CLUSTER_ALIAS + ".seeds");
        }
        clusterAdmin().updateSettings(new ClusterUpdateSettingsRequest().persistentSettings(builder)).get();
    }

    private void writeCredentialsToKeyStore(String credentials) throws Exception {
        final Environment environment = getInstanceFromNode(Environment.class);
        final KeyStoreWrapper keyStoreWrapper = KeyStoreWrapper.create();
        keyStoreWrapper.setString("cluster.remote." + CLUSTER_ALIAS + ".credentials", credentials.toCharArray());
        keyStoreWrapper.save(environment.configFile(), new char[0], false);
    }

    public static MockTransportService startTransport(
        final String nodeName,
        final ThreadPool threadPool,
        final BlockingQueue<Map<String, String>> capturedHeaders
    ) {
        boolean success = false;
        final Settings settings = Settings.builder()
            .put("node.name", nodeName)
            .put("remote_cluster_server.enabled", "true")
            .put("remote_cluster.port", "0")
            .put("xpack.security.remote_cluster_server.ssl.enabled", "false")
            .build();
        final MockTransportService service = MockTransportService.createNewService(
            settings,
            VersionInformation.CURRENT,
            TransportVersion.current(),
            threadPool,
            null
        );
        try {
            service.registerRequestHandler(
                ClusterStateAction.NAME,
                EsExecutors.DIRECT_EXECUTOR_SERVICE,
                ClusterStateRequest::new,
                (request, channel, task) -> {
                    capturedHeaders.add(Map.copyOf(threadPool.getThreadContext().getHeaders()));
                    channel.sendResponse(
                        new ClusterStateResponse(ClusterName.DEFAULT, ClusterState.builder(ClusterName.DEFAULT).build(), false)
                    );
                }
            );
            service.registerRequestHandler(
                RemoteClusterNodesAction.TYPE.name(),
                EsExecutors.DIRECT_EXECUTOR_SERVICE,
                RemoteClusterNodesAction.Request::new,
                (request, channel, task) -> {
                    capturedHeaders.add(Map.copyOf(threadPool.getThreadContext().getHeaders()));
                    channel.sendResponse(new RemoteClusterNodesAction.Response(List.of()));
                }
            );
            service.registerRequestHandler(
                TransportSearchShardsAction.TYPE.name(),
                EsExecutors.DIRECT_EXECUTOR_SERVICE,
                SearchShardsRequest::new,
                (request, channel, task) -> {
                    capturedHeaders.add(Map.copyOf(threadPool.getThreadContext().getHeaders()));
                    channel.sendResponse(new SearchShardsResponse(List.of(), List.of(), Collections.emptyMap()));
                }
            );
            service.registerRequestHandler(
                TransportSearchAction.TYPE.name(),
                EsExecutors.DIRECT_EXECUTOR_SERVICE,
                SearchRequest::new,
                (request, channel, task) -> {
                    capturedHeaders.add(Map.copyOf(threadPool.getThreadContext().getHeaders()));
                    channel.sendResponse(
                        new SearchResponse(
                            SearchHits.empty(new TotalHits(0, TotalHits.Relation.EQUAL_TO), Float.NaN),
                            InternalAggregations.EMPTY,
                            null,
                            false,
                            null,
                            null,
                            1,
                            null,
                            1,
                            1,
                            0,
                            100,
                            ShardSearchFailure.EMPTY_ARRAY,
                            SearchResponse.Clusters.EMPTY
                        )
                    );
                }
            );
            service.start();
            service.acceptIncomingRequests();
            success = true;
            return service;
        } finally {
            if (success == false) {
                service.close();
            }
        }
    }

    private void reloadSecureSettings() {
        final AtomicReference<AssertionError> reloadSettingsError = new AtomicReference<>();
        final CountDownLatch latch = new CountDownLatch(1);
        final SecureString emptyPassword = randomBoolean() ? new SecureString(new char[0]) : null;

        final var request = new NodesReloadSecureSettingsRequest();
        try {
            request.nodesIds(Strings.EMPTY_ARRAY);
            request.setSecureStorePassword(emptyPassword);
            client().execute(TransportNodesReloadSecureSettingsAction.TYPE, request, new ActionListener<>() {
                @Override
                public void onResponse(NodesReloadSecureSettingsResponse nodesReloadResponse) {
                    try {
                        assertThat(nodesReloadResponse, notNullValue());
                        final Map<String, NodesReloadSecureSettingsResponse.NodeResponse> nodesMap = nodesReloadResponse.getNodesMap();
                        assertThat(nodesMap.size(), equalTo(1));
                        for (final NodesReloadSecureSettingsResponse.NodeResponse nodeResponse : nodesReloadResponse.getNodes()) {
                            assertThat(nodeResponse.reloadException(), nullValue());
                        }
                    } catch (final AssertionError e) {
                        reloadSettingsError.set(e);
                    } finally {
                        latch.countDown();
                    }
                }

                @Override
                public void onFailure(Exception e) {
                    reloadSettingsError.set(new AssertionError("Nodes request failed", e));
                    latch.countDown();
                }
            });
        } finally {
            request.decRef();
        }
        safeAwait(latch);
        if (reloadSettingsError.get() != null) {
            throw reloadSettingsError.get();
        }
    }
}
