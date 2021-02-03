/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.monitoring.exporter.local;

import org.elasticsearch.Version;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexTemplateMetadata;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ObjectPath;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.ingest.PipelineConfiguration;
import org.elasticsearch.protocol.xpack.watcher.PutWatchRequest;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.xpack.core.monitoring.MonitoredSystem;
import org.elasticsearch.xpack.core.monitoring.exporter.MonitoringTemplateUtils;
import org.elasticsearch.xpack.core.watcher.transport.actions.put.PutWatchAction;
import org.elasticsearch.xpack.monitoring.exporter.ClusterAlertsUtil;
import org.elasticsearch.xpack.monitoring.exporter.MonitoringMigrationCoordinator;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.test.ESIntegTestCase.Scope.TEST;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.notNullValue;

@ESIntegTestCase.ClusterScope(scope = TEST,
                              numDataNodes = 1, numClientNodes = 0, supportsDedicatedMasters = false)
public class LocalExporterResourceIntegTests extends LocalExporterIntegTestCase {

    public LocalExporterResourceIntegTests() {
        super();
    }

    @Override
    protected Settings nodeSettings(int nodeOrdinal) {
        return Settings.builder()
            .put(super.nodeSettings(nodeOrdinal))
            .put("xpack.license.self_generated.type", "trial")
            .build();
    }

    private final MonitoredSystem system = randomFrom(MonitoredSystem.values());

    public void testCreateWhenResourcesNeedToBeAddedOrUpdated() throws Exception {
        // sometimes they need to be added; sometimes they need to be replaced
        if (randomBoolean()) {
            putResources(oldVersion());
        }

        assertResourcesExist();
    }

    public void testCreateWhenResourcesShouldNotBeReplaced() throws Exception {
        putResources(newEnoughVersion());

        assertResourcesExist();

        // these were "newer" or at least the same version, so they shouldn't be replaced
        assertTemplateNotUpdated();
        assertPipelinesNotUpdated();
    }

    public void testRemoveWhenResourcesShouldBeRemoved() throws Exception {
        putResources(newEnoughVersion());

        assertResourcesExist();
        waitNoPendingTasksOnAll();

        Settings exporterSettings = Settings.builder().put(localExporterSettings())
            .put("xpack.monitoring.migration.decommission_alerts", true).build();

        createResources("decommission_local", exporterSettings);
        waitNoPendingTasksOnAll();
        assertBusy(() -> {
            assertTemplatesExist();
            assertPipelinesExist();
            assertNoWatchesExist();
        });
    }

    public void testResourcesBlockedDuringMigration() throws Exception {
        putResources(newEnoughVersion());

        assertResourcesExist();
        waitNoPendingTasksOnAll();

        Settings exporterSettings = Settings.builder().put(localExporterSettings())
            .put("xpack.monitoring.migration.decommission_alerts", true).build();

        MonitoringMigrationCoordinator coordinator = new MonitoringMigrationCoordinator();
        assertTrue(coordinator.tryBlockInstallationTasks());
        assertFalse(coordinator.canInstall());

        assertThat(clusterService().state().version(), not(ClusterState.UNKNOWN_VERSION));
        try (LocalExporter exporter = createLocalExporter("decommission_local", exporterSettings, coordinator)) {
            assertThat(exporter.isExporterReady(), is(false));
        }
    }

    @Override
    protected Settings localExporterSettings() {
        // Override the settings for local exporters created in this test, make sure watcher is enabled so we can test
        // cluster alert creation and decommissioning
        return Settings.builder()
            .put(super.localExporterSettings())
            .put("xpack.monitoring.exporters." + exporterName +  ".cluster_alerts.management.enabled", true)
            .build();
    }

    private void createResources() throws Exception {
        createResources(exporterName, localExporterSettings());
    }

    private void createResources(String exporterName, Settings exporterSettings) throws Exception {
        // wait until the cluster is ready (this is done at the "Exporters" level)
        // this is not a busy assertion because it's checked earlier
        assertThat(clusterService().state().version(), not(ClusterState.UNKNOWN_VERSION));

        try (LocalExporter exporter = createLocalExporter(exporterName, exporterSettings)) {
            assertBusy(() -> assertThat(exporter.isExporterReady(), is(true)));
        }
    }

    /**
     * Generates a basic template that loosely represents a monitoring template.
     */
    private static BytesReference generateTemplateSource(final String name, final Integer version) throws IOException {
        final XContentBuilder builder = jsonBuilder().startObject();

        // this would totally break Monitoring UI, but the idea is that it's different from a real template and
        // the version controls that; it also won't break indexing (just searching) so this test can use it blindly
        builder
            .field("index_patterns", name)
            .startObject("settings")
                .field("index.number_of_shards", 1)
                .field("index.number_of_replicas", 0)
            .endObject()
            .startObject("mappings")
                // The internal representation still requires a default type of _doc
                .startObject("_doc")
                    .startObject("_meta")
                        .field("test", true)
                    .endObject()
                    .field("enabled", false)
                .endObject()
            .endObject();

        if (version != null) {
            builder.field("version", version);
        }

        return BytesReference.bytes(builder.endObject());
    }

    private void putResources(final Integer version) throws Exception {
        waitNoPendingTasksOnAll();

        putTemplate(version);
        putPipelines(version);
        putWatches(version);
    }

    private void putTemplate(final Integer version) throws Exception {
        final String templateName = MonitoringTemplateUtils.templateName(system.getSystem());
        final BytesReference source = generateTemplateSource(templateName, version);

        assertAcked(client().admin().indices().preparePutTemplate(templateName).setSource(source, XContentType.JSON).get());
    }

    private void putPipelines(final Integer version) {
        for (final String pipelineId : MonitoringTemplateUtils.PIPELINE_IDS) {
            putPipeline(MonitoringTemplateUtils.pipelineName(pipelineId), version);
        }
    }

    private void putPipeline(final String pipelineName, final Integer version) {
        assertAcked(client().admin().cluster().preparePutPipeline(pipelineName, replaceablePipeline(version), XContentType.JSON).get());
    }

    /**
     * Create a pipeline with nothing in it whose description is literally "test".
     *
     * @param version Version to add to the pipeline, if any
     * @return Never {@code null}.
     */
    private BytesReference replaceablePipeline(final Integer version) {
        try {
            final XContentBuilder builder = XContentBuilder.builder(XContentType.JSON.xContent());

            builder.startObject();

            {
                builder.startArray("processors").endArray();
                // something we can quickly check to ensure we have/have not replaced it
                builder.field("description", getTestName());

                // sometimes give it a version that should be overwritten (and sometimes don't give it a version at all)
                if (version != null) {
                    builder.field("version", version);
                }
            }

            return BytesReference.bytes(builder.endObject());
        } catch (final IOException e) {
            throw new RuntimeException("Failed to create pipeline", e);
        }
    }

    /**
     * Create a cluster alert that does nothing.
     * @param version Version to add to the watch, if any
     */
    private void putWatches(final Integer version) throws Exception {
        for (final String watchId : ClusterAlertsUtil.WATCH_IDS) {
            final String uniqueWatchId = ClusterAlertsUtil.createUniqueWatchId(clusterService(), watchId);
            final BytesReference watch = generateWatchSource(watchId, clusterService().state().metadata().clusterUUID(), version);
            client().execute(PutWatchAction.INSTANCE, new PutWatchRequest(uniqueWatchId, watch, XContentType.JSON))
                .actionGet();
        }
    }

    /**
     * Generates a basic watch that loosely represents a monitoring cluster alert but ultimately does nothing.
     */
    private static BytesReference generateWatchSource(final String id, final String clusterUUID, final Integer version) throws Exception {
        final XContentBuilder builder = jsonBuilder().startObject();
        builder
            .startObject("metadata")
                .startObject("xpack")
                    .field("cluster_uuid", clusterUUID);
                    if(version != null) {
                        builder.field("version_created", Integer.toString(version));
                    }
        builder
                    .field("watch", id)
                .endObject()
            .endObject()
            .startObject("trigger")
                .startObject("schedule")
                    .field("interval", "30m")
                .endObject()
            .endObject()
            .startObject("input")
                .startObject("simple")
                    .field("ignore", "ignore")
                .endObject()
            .endObject()
            .startObject("condition")
                .startObject("never")
                .endObject()
            .endObject()
            .startObject("actions")
            .endObject();

        return BytesReference.bytes(builder.endObject());
    }

    private Integer oldVersion() {
        final int minimumVersion = Math.min(ClusterAlertsUtil.LAST_UPDATED_VERSION, MonitoringTemplateUtils.LAST_UPDATED_VERSION);

        // randomly supply an older version, or no version at all
        return randomBoolean() ? minimumVersion - randomIntBetween(1, 100000) : null;
    }

    private int newEnoughVersion() {
        final int maximumVersion = Math.max(ClusterAlertsUtil.LAST_UPDATED_VERSION, MonitoringTemplateUtils.LAST_UPDATED_VERSION);

        // randomly supply a newer version or the expected version
        return randomFrom(maximumVersion + randomIntBetween(1, 100000), maximumVersion);
    }

    private void assertTemplatesExist() {
        for (String templateName : monitoringTemplateNames()) {
            assertTemplateInstalled(templateName);
        }
    }

    private void assertPipelinesExist() {
        for (PipelineConfiguration pipeline : client().admin().cluster().prepareGetPipeline("xpack_monitoring_*").get().pipelines()) {
            final Object description = pipeline.getConfigAsMap().get("description");

            // this just ensures that it's set; not who set it
            assertThat(description, notNullValue());
        }
    }

    private void assertWatchesExist() {
        // Check if watches index exists
        if (client().admin().indices().prepareGetIndex().addIndices(".watches").get().getIndices().length == 0) {
            fail("Expected [.watches] index with cluster alerts present, but no [.watches] index was found");
        }

        String clusterUUID = clusterService().state().getMetadata().clusterUUID();
        SearchSourceBuilder searchSource = SearchSourceBuilder.searchSource()
            .query(QueryBuilders.matchQuery("metadata.xpack.cluster_uuid", clusterUUID));
        Set<String> watchIds = new HashSet<>(Arrays.asList(ClusterAlertsUtil.WATCH_IDS));
        for (SearchHit hit : client().prepareSearch(".watches").setSource(searchSource).get().getHits().getHits()) {
            String watchId = ObjectPath.eval("metadata.xpack.watch", hit.getSourceAsMap());
            assertNotNull("Missing watch ID", watchId);
            assertTrue("found unexpected watch id", watchIds.contains(watchId));

            String version = ObjectPath.eval("metadata.xpack.version_created", hit.getSourceAsMap());
            assertNotNull("Missing version from returned watch [" + watchId + "]", version);
            assertTrue(Version.fromId(Integer.parseInt(version)).onOrAfter(Version.fromId(ClusterAlertsUtil.LAST_UPDATED_VERSION)));

            String uuid = ObjectPath.eval("metadata.xpack.cluster_uuid", hit.getSourceAsMap());
            assertNotNull("Missing cluster uuid", uuid);
            assertEquals(clusterUUID, uuid);
        }
    }

    private void assertNoWatchesExist() {
        // Check if watches index exists
        if (client().admin().indices().prepareGetIndex().addIndices(".watches").get().getIndices().length == 0) {
            fail("Expected [.watches] index with cluster alerts present, but no [.watches] index was found");
        }

        String clusterUUID = clusterService().state().getMetadata().clusterUUID();
        SearchSourceBuilder searchSource = SearchSourceBuilder.searchSource()
            .query(QueryBuilders.matchQuery("metadata.xpack.cluster_uuid", clusterUUID));
        SearchResponse searchResponse = client().prepareSearch(".watches").setSource(searchSource).get();
        if (searchResponse.getHits().getTotalHits().value > 0) {
            List<String> invalidWatches = new ArrayList<>();
            for (SearchHit hit : searchResponse.getHits().getHits()) {
                invalidWatches.add(ObjectPath.eval("metadata.xpack.watch", hit.getSourceAsMap()));
            }
            fail("Found [" + searchResponse.getHits().getTotalHits().value + "] invalid watches when none were expected: "
                + invalidWatches);
        }
    }

    private void assertResourcesExist() throws Exception {
        createResources();

        waitNoPendingTasksOnAll();

        assertBusy(() -> {
            assertTemplatesExist();
            assertPipelinesExist();
            assertWatchesExist();
        });
    }

    private void assertTemplateNotUpdated() {
        final String name = MonitoringTemplateUtils.templateName(system.getSystem());

        for (IndexTemplateMetadata template : client().admin().indices().prepareGetTemplates(name).get().getIndexTemplates()) {
            final String docMapping = template.getMappings().toString();

            assertThat(docMapping, notNullValue());
            assertThat(docMapping, containsString("test"));
        }
    }

    private void assertPipelinesNotUpdated() {
        for (PipelineConfiguration pipeline : client().admin().cluster().prepareGetPipeline("xpack_monitoring_*").get().pipelines()) {
            final Object description = pipeline.getConfigAsMap().get("description");

            assertThat(description, equalTo(getTestName()));
        }
    }
}
