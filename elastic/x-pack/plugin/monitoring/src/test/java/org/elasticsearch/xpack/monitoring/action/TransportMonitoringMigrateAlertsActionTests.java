/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.monitoring.action;

import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.elasticsearch.xpack.monitoring.exporter.http.ClusterAlertHttpResource.CLUSTER_ALERT_VERSION_PARAMETERS;
import static org.elasticsearch.xpack.monitoring.exporter.http.WatcherExistsHttpResource.WATCHER_CHECK_PARAMETERS;
import static org.hamcrest.Matchers.endsWith;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.Matchers.startsWith;

import java.io.IOException;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;

import org.elasticsearch.action.admin.indices.template.get.GetIndexTemplatesResponse;
import org.elasticsearch.cluster.metadata.IndexTemplateMetadata;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.http.MockRequest;
import org.elasticsearch.test.http.MockResponse;
import org.elasticsearch.test.http.MockWebServer;
import org.elasticsearch.test.junit.annotations.TestLogging;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.monitoring.action.MonitoringBulkAction;
import org.elasticsearch.xpack.core.monitoring.action.MonitoringBulkRequest;
import org.elasticsearch.xpack.core.monitoring.action.MonitoringBulkResponse;
import org.elasticsearch.xpack.core.monitoring.action.MonitoringMigrateAlertsAction;
import org.elasticsearch.xpack.core.monitoring.action.MonitoringMigrateAlertsRequest;
import org.elasticsearch.xpack.core.monitoring.action.MonitoringMigrateAlertsResponse;
import org.elasticsearch.xpack.core.monitoring.exporter.MonitoringTemplateUtils;
import org.elasticsearch.xpack.core.watcher.transport.actions.get.GetWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.get.GetWatchRequest;
import org.elasticsearch.xpack.core.watcher.watch.Watch;
import org.elasticsearch.xpack.monitoring.Monitoring;
import org.elasticsearch.xpack.monitoring.MonitoringService;
import org.elasticsearch.xpack.monitoring.exporter.ClusterAlertsUtil;
import org.elasticsearch.xpack.monitoring.exporter.http.HttpExporter;
import org.elasticsearch.xpack.monitoring.exporter.local.LocalExporter;
import org.elasticsearch.xpack.monitoring.exporter.local.LocalExporterIntegTests;
import org.elasticsearch.xpack.monitoring.test.MonitoringIntegTestCase;
import org.junit.After;
import org.junit.Before;

@ESIntegTestCase.ClusterScope(numDataNodes = 3)
public class TransportMonitoringMigrateAlertsActionTests extends MonitoringIntegTestCase {

    private MockWebServer webServer;

    private MockWebServer createMockWebServer() throws IOException {
        MockWebServer server = new MockWebServer();
        server.start();
        return server;
    }

    @Before
    public void startWebServer() throws IOException {
        webServer = createMockWebServer();
    }

    @After
    public void stopWebServer() {
        if (webServer != null) {
            webServer.close();
        }
    }

    @Override
    protected Settings nodeSettings(int nodeOrdinal, Settings otherSettings) {
        return Settings.builder()
            // Parent conf
            .put(super.nodeSettings(nodeOrdinal, otherSettings))

            // Disable monitoring
            .put("xpack.monitoring.collection.enabled", false)
            .put("xpack.monitoring.collection.interval", "1s")

            // X-Pack configuration
            .put("xpack.license.self_generated.type", "trial")
            .put(XPackSettings.MACHINE_LEARNING_ENABLED.getKey(), false)
            .build();
    }

    private void stopMonitoring() {
        // Clean up any transient settings we have added
        assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(Settings.builder()
            .putNull(MonitoringService.ENABLED.getKey())
            .putNull("xpack.monitoring.elasticsearch.collection.enabled")
            .putNull("xpack.monitoring.exporters._local.type")
            .putNull("xpack.monitoring.exporters._local.enabled")
            .putNull("xpack.monitoring.exporters._local.cluster_alerts.management.enabled")
            .putNull("xpack.monitoring.exporters.remoteCluster.type")
            .putNull("xpack.monitoring.exporters.remoteCluster.enabled")
            .putNull("xpack.monitoring.exporters.remoteCluster.host")
            .putNull("xpack.monitoring.exporters.remoteCluster.cluster_alerts.management.enabled")
        ));
        // Make sure to clean up the migration setting if it is set
        assertAcked(client().admin().cluster().prepareUpdateSettings().setPersistentSettings(Settings.builder()
            .putNull(Monitoring.MIGRATION_DECOMMISSION_ALERTS.getKey())
        ));
    }

    @TestLogging(
        value = "org.elasticsearch.xpack.monitoring.exporter.local:trace",
        reason = "to ensure we log local exporter on trace level")
    public void testLocalAlertsRemoval() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                .put("xpack.monitoring.exporters._local.type", LocalExporter.TYPE)
                .put("xpack.monitoring.exporters._local.enabled", true)
                .put("xpack.monitoring.exporters._local.cluster_alerts.management.enabled", true);

            // enable local exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // ensure resources exist
            ensureInitialLocalResources();

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check response
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("_local"));
            assertThat(localExporterResult.getType(), is(LocalExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(true));
            assertThat(localExporterResult.getReason(), nullValue());

            // ensure no watches
            assertWatchesExist(false);
        } finally {
            stopMonitoring();
        }
    }

    @TestLogging(
        value = "org.elasticsearch.xpack.monitoring.exporter.local:trace",
        reason = "to ensure we log local exporter on trace level")
    public void testRepeatedLocalAlertsRemoval() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                .put("xpack.monitoring.exporters._local.type", LocalExporter.TYPE)
                .put("xpack.monitoring.exporters._local.enabled", true)
                .put("xpack.monitoring.exporters._local.cluster_alerts.management.enabled", true);

            // enable local exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // ensure resources exist
            ensureInitialLocalResources();

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check response
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("_local"));
            assertThat(localExporterResult.getType(), is(LocalExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(true));
            assertThat(localExporterResult.getReason(), nullValue());

            // ensure no watches
            assertWatchesExist(false);

            // call migration api again
            response = client().execute(MonitoringMigrateAlertsAction.INSTANCE, new MonitoringMigrateAlertsRequest()).actionGet();

            // check second response
            assertThat(response.getExporters().size(), is(1));
            localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("_local"));
            assertThat(localExporterResult.getType(), is(LocalExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(true));
            assertThat(localExporterResult.getReason(), nullValue());
        } finally {
            stopMonitoring();
        }
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/66586")
    public void testDisabledLocalExporterAlertsRemoval() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                .put("xpack.monitoring.exporters._local.type", LocalExporter.TYPE)
                .put("xpack.monitoring.exporters._local.enabled", true)
                .put("xpack.monitoring.exporters._local.cluster_alerts.management.enabled", true);

            // enable local exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // ensure resources exist
            ensureInitialLocalResources();

            // new disable local exporter
            final Settings.Builder disableSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                .put("xpack.monitoring.exporters._local.type", LocalExporter.TYPE)
                .put("xpack.monitoring.exporters._local.enabled", false)
                .put("xpack.monitoring.exporters._local.cluster_alerts.management.enabled", true);
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(disableSettings));

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check response
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("_local"));
            assertThat(localExporterResult.getType(), is(LocalExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(true));
            assertThat(localExporterResult.getReason(), nullValue());

            // ensure no watches
            assertWatchesExist(false);
        } finally {
            stopMonitoring();
        }
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/66586")
    public void testLocalExporterWithAlertingDisabled() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                .put("xpack.monitoring.exporters._local.type", LocalExporter.TYPE)
                .put("xpack.monitoring.exporters._local.enabled", true)
                .put("xpack.monitoring.exporters._local.cluster_alerts.management.enabled", true);

            // enable local exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // ensure resources exist
            ensureInitialLocalResources();

            // new disable local exporter's cluster alerts
            final Settings.Builder disableSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                .put("xpack.monitoring.exporters._local.type", LocalExporter.TYPE)
                .put("xpack.monitoring.exporters._local.enabled", true)
                .put("xpack.monitoring.exporters._local.cluster_alerts.management.enabled", false);
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(disableSettings));

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check response
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("_local"));
            assertThat(localExporterResult.getType(), is(LocalExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(false));
            assertThat(localExporterResult.getReason(), notNullValue());
            assertThat(localExporterResult.getReason().getMessage(), is("cannot manage cluster alerts because alerting is disabled"));
        } finally {
            stopMonitoring();
        }
    }

    public void testRemoteAlertsRemoval() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                // Make sure to not collect ES stats in background. Our web server expects requests in a particular order.
                .put("xpack.monitoring.elasticsearch.collection.enabled", false)
                .put("xpack.monitoring.exporters.remoteCluster.type", HttpExporter.TYPE)
                .put("xpack.monitoring.exporters.remoteCluster.enabled", true)
                .put("xpack.monitoring.exporters.remoteCluster.host", webServer.getHostName() + ":" + webServer.getPort())
                .put("xpack.monitoring.exporters.remoteCluster.cluster_alerts.management.enabled", true);

            // enable http exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // enqueue delete request expectations for alerts
            enqueueWatcherResponses(webServer, true);

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check that all "remote watches" were deleted by the exporter
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("remoteCluster"));
            assertThat(localExporterResult.getType(), is(HttpExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(true));
            assertThat(localExporterResult.getReason(), nullValue());

            // ensure no watches
            assertMonitorWatches(webServer, true);
        } finally {
            stopMonitoring();
            webServer.clearRequests();
        }
    }

    public void testDisabledRemoteAlertsRemoval() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                // Make sure to not collect ES stats in background. Our web server expects requests in a particular order.
                .put("xpack.monitoring.elasticsearch.collection.enabled", false)
                .put("xpack.monitoring.exporters.remoteCluster.type", HttpExporter.TYPE)
                .put("xpack.monitoring.exporters.remoteCluster.enabled", false)
                .put("xpack.monitoring.exporters.remoteCluster.host", webServer.getHostName() + ":" + webServer.getPort())
                .put("xpack.monitoring.exporters.remoteCluster.cluster_alerts.management.enabled", true);

            // configure disabled http exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // enqueue delete request expectations for alerts
            enqueueWatcherResponses(webServer, true);

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check that the disabled http exporter was enabled this one time in order to remove watches
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("remoteCluster"));
            assertThat(localExporterResult.getType(), is(HttpExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(true));
            assertThat(localExporterResult.getReason(), nullValue());

            // ensure no watches
            assertMonitorWatches(webServer, true);
        } finally {
            stopMonitoring();
            webServer.clearRequests();
        }
    }

    public void testRemoteAlertsRemovalWhenOriginalMonitoringClusterIsGone() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                // Make sure to not collect ES stats in background. Our web server expects requests in a particular order.
                .put("xpack.monitoring.elasticsearch.collection.enabled", false)
                .put("xpack.monitoring.exporters.remoteCluster.type", HttpExporter.TYPE)
                .put("xpack.monitoring.exporters.remoteCluster.enabled", false)
                .put("xpack.monitoring.exporters.remoteCluster.host", webServer.getHostName() + ":" + webServer.getPort())
                .put("xpack.monitoring.exporters.remoteCluster.cluster_alerts.management.enabled", true);

            // create a disabled http exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check that migration failed due to monitoring cluster not responding
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("remoteCluster"));
            assertThat(localExporterResult.getType(), is(HttpExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(false));
            // this might be a messier exception in practice like connection refused, but hey, testability
            assertThat(localExporterResult.getReason().getMessage(), is("Connection is closed"));
        } finally {
            stopMonitoring();
            webServer.clearRequests();
        }
    }

    public void testRemoteAlertsRemovalFailure() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                // Make sure to not collect ES stats in background. Our web server expects requests in a particular order.
                .put("xpack.monitoring.elasticsearch.collection.enabled", false)
                .put("xpack.monitoring.exporters.remoteCluster.type", HttpExporter.TYPE)
                .put("xpack.monitoring.exporters.remoteCluster.enabled", true)
                .put("xpack.monitoring.exporters.remoteCluster.host", webServer.getHostName() + ":" + webServer.getPort())
                .put("xpack.monitoring.exporters.remoteCluster.cluster_alerts.management.enabled", true);

            // enable http exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // enqueue a "watcher available" response, but then a "failure to delete watch" response
            enqueueResponse(webServer, 200, "{\"features\":{\"watcher\":{\"available\":true,\"enabled\":true}}}");
            enqueueResponse(webServer, 500, "{\"error\":{}}");

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // check that an error is reported while trying to remove a remote watch
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("remoteCluster"));
            assertThat(localExporterResult.getType(), is(HttpExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(false));
            assertThat(localExporterResult.getReason().getMessage(), startsWith("method [DELETE], host ["));
            assertThat(localExporterResult.getReason().getMessage(),
                endsWith("status line [HTTP/1.1 500 Internal Server Error]\n{\"error\":{}}"));

        } finally {
            stopMonitoring();
            webServer.clearRequests();
        }
    }

    public void testRemoteAlertsRemoteDisallowsWatcher() throws Exception {
        try {
            // start monitoring service
            final Settings.Builder exporterSettings = Settings.builder()
                .put(MonitoringService.ENABLED.getKey(), true)
                // Make sure to not collect ES stats in background. Our web server expects requests in a particular order.
                .put("xpack.monitoring.elasticsearch.collection.enabled", false)
                .put("xpack.monitoring.exporters.remoteCluster.type", HttpExporter.TYPE)
                .put("xpack.monitoring.exporters.remoteCluster.enabled", true)
                .put("xpack.monitoring.exporters.remoteCluster.host", webServer.getHostName() + ":" + webServer.getPort())
                .put("xpack.monitoring.exporters.remoteCluster.cluster_alerts.management.enabled", true);

            // enable http exporter
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(exporterSettings));

            // enqueue a "watcher available" response, but then a "failure to delete watch" response
            enqueueWatcherResponses(webServer, false);

            // call migration api
            MonitoringMigrateAlertsResponse response = client().execute(MonitoringMigrateAlertsAction.INSTANCE,
                new MonitoringMigrateAlertsRequest()).actionGet();

            // Migration is marked as complete since watcher is disabled on remote cluster.
            assertThat(response.getExporters().size(), is(1));
            MonitoringMigrateAlertsResponse.ExporterMigrationResult localExporterResult = response.getExporters().get(0);
            assertThat(localExporterResult.getName(), is("remoteCluster"));
            assertThat(localExporterResult.getType(), is(HttpExporter.TYPE));
            assertThat(localExporterResult.isMigrationComplete(), is(true));

            // ensure responses
            assertMonitorWatches(webServer, false);
        } finally {
            stopMonitoring();
            webServer.clearRequests();
        }
    }

    private void ensureInitialLocalResources() throws Exception {
        // Should trigger setting up alert watches via LocalExporter#openBulk(...) and
        // then eventually to LocalExporter#setupIfElectedMaster(...)
        // Sometimes this last method doesn't install watches, because elected master node doesn't export monitor documents.
        // and then these assertions here fail.
        {
            MonitoringBulkRequest request = new MonitoringBulkRequest();
            request.add(LocalExporterIntegTests.createMonitoringBulkDoc());
            String masterNode = internalCluster().getMasterName();
            MonitoringBulkResponse response = client(masterNode).execute(MonitoringBulkAction.INSTANCE, request).actionGet();
            assertThat(response.status(), equalTo(RestStatus.OK));
        }

        waitForWatcherIndices();
        assertBusy(() -> {
            assertThat(indexExists(".monitoring-*"), is(true));
            ensureYellowAndNoInitializingShards(".monitoring-*");
            checkMonitoringTemplates();
            assertWatchesExist(true);
        }, 20, TimeUnit.SECONDS); // Watcher can be slow to allocate all watches required
    }

    /**
     * Checks that the monitoring templates have been created by the local exporter
     */
    private void checkMonitoringTemplates() {
        final Set<String> templates = new HashSet<>();
        templates.add(".monitoring-alerts-7");
        templates.add(".monitoring-es");
        templates.add(".monitoring-kibana");
        templates.add(".monitoring-logstash");
        templates.add(".monitoring-beats");

        GetIndexTemplatesResponse response = client().admin().indices().prepareGetTemplates(".monitoring-*").get();
        Set<String> actualTemplates = response.getIndexTemplates().stream().map(IndexTemplateMetadata::getName).collect(Collectors.toSet());
        assertEquals(templates, actualTemplates);
    }

    private void assertWatchesExist(boolean exist) {
        // Check if watches index exists
        if (client().admin().indices().prepareGetIndex().addIndices(".watches").get().getIndices().length == 0) {
            fail("Expected [.watches] index with cluster alerts present, but no [.watches] index was found");
        }

        Arrays.stream(ClusterAlertsUtil.WATCH_IDS)
            .map(n -> ClusterAlertsUtil.createUniqueWatchId(clusterService(), n))
            .map(watch -> client().execute(GetWatchAction.INSTANCE, new GetWatchRequest(watch)).actionGet())
            .filter(r -> r.isFound() != exist)
            .findAny()
            .ifPresent(r -> fail((exist ? "missing" : "found") + " watch [" + r.getId() + "]"));
    }

    protected List<String> monitoringTemplateNames() {
        return Arrays.stream(MonitoringTemplateUtils.TEMPLATE_IDS)
            .map(MonitoringTemplateUtils::templateName)
            .collect(Collectors.toList());
    }

    private void enqueueWatcherResponses(final MockWebServer webServer, final boolean remoteClusterAllowsWatcher) throws IOException {
        // if the remote cluster doesn't allow watcher, then we only check for it and we're done
        if (remoteClusterAllowsWatcher) {
            // X-Pack exists and Watcher can be used
            enqueueResponse(webServer, 200, "{\"features\":{\"watcher\":{\"available\":true,\"enabled\":true}}}");

            // add delete responses
            enqueueDeleteClusterAlertResponses(webServer);
        } else {
            // X-Pack exists but Watcher just cannot be used
            if (randomBoolean()) {
                final String responseBody = randomFrom(
                    "{\"features\":{\"watcher\":{\"available\":false,\"enabled\":true}}}",
                    "{\"features\":{\"watcher\":{\"available\":true,\"enabled\":false}}}",
                    "{}"
                );

                enqueueResponse(webServer, 200, responseBody);
            } else {
                // X-Pack is not installed
                enqueueResponse(webServer, 404, "{}");
            }
        }
    }

    private void enqueueDeleteClusterAlertResponses(final MockWebServer webServer) throws IOException {
        for (final String watchId : ClusterAlertsUtil.WATCH_IDS) {
            enqueueDeleteClusterAlertResponse(webServer, watchId);
        }
    }

    private void enqueueDeleteClusterAlertResponse(final MockWebServer webServer, final String watchId) throws IOException {
        if (randomBoolean()) {
            enqueueResponse(webServer, 404, "watch [" + watchId + "] did not exist");
        } else {
            enqueueResponse(webServer, 200, "watch [" + watchId + "] deleted");
        }
    }

    private void enqueueResponse(MockWebServer mockWebServer, int responseCode, String body) throws IOException {
        mockWebServer.enqueue(new MockResponse().setResponseCode(responseCode).setBody(body));
    }

    private String watcherCheckQueryString() {
        return "filter_path=" + WATCHER_CHECK_PARAMETERS.get("filter_path");
    }

    private String resourceClusterAlertQueryString() {
        return "filter_path=" + CLUSTER_ALERT_VERSION_PARAMETERS.get("filter_path");
    }

    private void assertMonitorWatches(final MockWebServer webServer, final boolean remoteClusterAllowsWatcher) {
        MockRequest request  = webServer.takeRequest();

        // GET /_xpack
        assertThat(request.getMethod(), equalTo("GET"));
        assertThat(request.getUri().getPath(), equalTo("/_xpack"));
        assertThat(request.getUri().getQuery(), equalTo(watcherCheckQueryString()));

        if (remoteClusterAllowsWatcher) {
            for (final Tuple<String, String> watch : monitoringWatches()) {
                final String uniqueWatchId = ClusterAlertsUtil.createUniqueWatchId(clusterService(), watch.v1());

                request = webServer.takeRequest();

                // GET / PUT if we are allowed to use it
                assertThat(request.getMethod(), equalTo("DELETE"));
                assertThat(request.getUri().getPath(), equalTo("/_watcher/watch/" + uniqueWatchId));
                assertThat(request.getUri().getQuery(), equalTo(resourceClusterAlertQueryString()));
            }
        }
    }

    protected void waitForWatcherIndices() throws Exception {
        awaitIndexExists(Watch.INDEX);
        assertBusy(() -> ensureYellowAndNoInitializingShards(Watch.INDEX));
    }
}
