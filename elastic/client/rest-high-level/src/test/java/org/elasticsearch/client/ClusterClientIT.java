/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.client;

import org.apache.http.util.EntityUtils;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.action.admin.cluster.health.ClusterHealthRequest;
import org.elasticsearch.action.admin.cluster.health.ClusterHealthResponse;
import org.elasticsearch.action.admin.cluster.settings.ClusterGetSettingsRequest;
import org.elasticsearch.action.admin.cluster.settings.ClusterGetSettingsResponse;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsRequest;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsResponse;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.cluster.RemoteConnectionInfo;
import org.elasticsearch.client.cluster.RemoteInfoRequest;
import org.elasticsearch.client.cluster.RemoteInfoResponse;
import org.elasticsearch.client.cluster.SniffModeInfo;
import org.elasticsearch.client.indices.ComponentTemplatesExistRequest;
import org.elasticsearch.client.indices.DeleteComponentTemplateRequest;
import org.elasticsearch.client.indices.GetComponentTemplatesRequest;
import org.elasticsearch.client.indices.GetComponentTemplatesResponse;
import org.elasticsearch.client.indices.PutComponentTemplateRequest;
import org.elasticsearch.cluster.health.ClusterHealthStatus;
import org.elasticsearch.cluster.health.ClusterIndexHealth;
import org.elasticsearch.cluster.health.ClusterShardHealth;
import org.elasticsearch.cluster.metadata.AliasMetadata;
import org.elasticsearch.cluster.metadata.ComponentTemplate;
import org.elasticsearch.cluster.metadata.Template;
import org.elasticsearch.cluster.routing.allocation.decider.EnableAllocationDecider;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.indices.recovery.RecoverySettings;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.transport.RemoteClusterService;
import org.elasticsearch.transport.SniffConnectionStrategy;

import java.io.IOException;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static java.util.Collections.emptyMap;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class ClusterClientIT extends ESRestHighLevelClientTestCase {

    public void testClusterPutSettings() throws IOException {
        final String transientSettingKey = RecoverySettings.INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING.getKey();
        final int transientSettingValue = 10;

        final String persistentSettingKey = EnableAllocationDecider.CLUSTER_ROUTING_ALLOCATION_ENABLE_SETTING.getKey();
        final String persistentSettingValue = EnableAllocationDecider.Allocation.NONE.name();

        Settings transientSettings = Settings.builder().put(transientSettingKey, transientSettingValue, ByteSizeUnit.BYTES).build();
        Map<String, Object> map = new HashMap<>();
        map.put(persistentSettingKey, persistentSettingValue);

        ClusterUpdateSettingsRequest setRequest = new ClusterUpdateSettingsRequest();
        setRequest.transientSettings(transientSettings);
        setRequest.persistentSettings(map);

        ClusterUpdateSettingsResponse setResponse = execute(setRequest, highLevelClient().cluster()::putSettings,
                highLevelClient().cluster()::putSettingsAsync);

        assertAcked(setResponse);
        assertThat(setResponse.getTransientSettings().get(transientSettingKey), notNullValue());
        assertThat(setResponse.getTransientSettings().get(persistentSettingKey), nullValue());
        assertThat(setResponse.getTransientSettings().get(transientSettingKey),
                equalTo(transientSettingValue + ByteSizeUnit.BYTES.getSuffix()));
        assertThat(setResponse.getPersistentSettings().get(transientSettingKey), nullValue());
        assertThat(setResponse.getPersistentSettings().get(persistentSettingKey), notNullValue());
        assertThat(setResponse.getPersistentSettings().get(persistentSettingKey), equalTo(persistentSettingValue));

        Map<String, Object> setMap = getAsMap("/_cluster/settings");
        String transientSetValue = (String) XContentMapValues.extractValue("transient." + transientSettingKey, setMap);
        assertThat(transientSetValue, equalTo(transientSettingValue + ByteSizeUnit.BYTES.getSuffix()));
        String persistentSetValue = (String) XContentMapValues.extractValue("persistent." + persistentSettingKey, setMap);
        assertThat(persistentSetValue, equalTo(persistentSettingValue));

        ClusterUpdateSettingsRequest resetRequest = new ClusterUpdateSettingsRequest();
        resetRequest.transientSettings(Settings.builder().putNull(transientSettingKey));
        resetRequest.persistentSettings("{\"" + persistentSettingKey + "\": null }", XContentType.JSON);

        ClusterUpdateSettingsResponse resetResponse = execute(resetRequest, highLevelClient().cluster()::putSettings,
                highLevelClient().cluster()::putSettingsAsync);

        assertThat(resetResponse.getTransientSettings().get(transientSettingKey), equalTo(null));
        assertThat(resetResponse.getPersistentSettings().get(persistentSettingKey), equalTo(null));
        assertThat(resetResponse.getTransientSettings(), equalTo(Settings.EMPTY));
        assertThat(resetResponse.getPersistentSettings(), equalTo(Settings.EMPTY));

        Map<String, Object> resetMap = getAsMap("/_cluster/settings");
        String transientResetValue = (String) XContentMapValues.extractValue("transient." + transientSettingKey, resetMap);
        assertThat(transientResetValue, equalTo(null));
        String persistentResetValue = (String) XContentMapValues.extractValue("persistent." + persistentSettingKey, resetMap);
        assertThat(persistentResetValue, equalTo(null));
    }

    public void testClusterUpdateSettingNonExistent() {
        String setting = "no_idea_what_you_are_talking_about";
        int value = 10;
        ClusterUpdateSettingsRequest clusterUpdateSettingsRequest = new ClusterUpdateSettingsRequest();
        clusterUpdateSettingsRequest.transientSettings(Settings.builder().put(setting, value).build());

        ElasticsearchException exception = expectThrows(ElasticsearchException.class, () -> execute(clusterUpdateSettingsRequest,
                highLevelClient().cluster()::putSettings, highLevelClient().cluster()::putSettingsAsync));
        assertThat(exception.status(), equalTo(RestStatus.BAD_REQUEST));
        assertThat(exception.getMessage(), equalTo(
                "Elasticsearch exception [type=illegal_argument_exception, reason=transient setting [" + setting + "], not recognized]"));
    }

    public void testClusterGetSettings() throws IOException {
        final String transientSettingKey = RecoverySettings.INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING.getKey();
        final int transientSettingValue = 10;

        final String persistentSettingKey = EnableAllocationDecider.CLUSTER_ROUTING_ALLOCATION_ENABLE_SETTING.getKey();
        final String persistentSettingValue = EnableAllocationDecider.Allocation.NONE.name();

        Settings transientSettings =
            Settings.builder().put(transientSettingKey, transientSettingValue, ByteSizeUnit.BYTES).build();
        Settings persistentSettings = Settings.builder().put(persistentSettingKey, persistentSettingValue).build();
        clusterUpdateSettings(persistentSettings, transientSettings);

        ClusterGetSettingsRequest request = new ClusterGetSettingsRequest();
        ClusterGetSettingsResponse response = execute(
            request, highLevelClient().cluster()::getSettings, highLevelClient().cluster()::getSettingsAsync);
        assertEquals(persistentSettings, response.getPersistentSettings());
        assertEquals(transientSettings, response.getTransientSettings());
        assertEquals(0, response.getDefaultSettings().size());
    }

    public void testClusterGetSettingsWithDefault() throws IOException {
        final String transientSettingKey = RecoverySettings.INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING.getKey();
        final int transientSettingValue = 10;

        final String persistentSettingKey = EnableAllocationDecider.CLUSTER_ROUTING_ALLOCATION_ENABLE_SETTING.getKey();
        final String persistentSettingValue = EnableAllocationDecider.Allocation.NONE.name();

        Settings transientSettings =
            Settings.builder().put(transientSettingKey, transientSettingValue, ByteSizeUnit.BYTES).build();
        Settings persistentSettings = Settings.builder().put(persistentSettingKey, persistentSettingValue).build();
        clusterUpdateSettings(persistentSettings, transientSettings);

        ClusterGetSettingsRequest request = new ClusterGetSettingsRequest().includeDefaults(true);
        ClusterGetSettingsResponse response = execute(
            request, highLevelClient().cluster()::getSettings, highLevelClient().cluster()::getSettingsAsync);
        assertEquals(persistentSettings, response.getPersistentSettings());
        assertEquals(transientSettings, response.getTransientSettings());
        assertThat(response.getDefaultSettings().size(), greaterThan(0));
    }

    public void testClusterHealthGreen() throws IOException {
        ClusterHealthRequest request = new ClusterHealthRequest();
        request.timeout("5s");
        ClusterHealthResponse response = execute(request, highLevelClient().cluster()::health, highLevelClient().cluster()::healthAsync);

        assertThat(response, notNullValue());
        assertThat(response.isTimedOut(), equalTo(false));
        assertThat(response.status(), equalTo(RestStatus.OK));
        assertThat(response.getStatus(), equalTo(ClusterHealthStatus.GREEN));
    }

    public void testClusterHealthYellowClusterLevel() throws IOException {
        createIndex("index", Settings.EMPTY);
        createIndex("index2", Settings.EMPTY);
        ClusterHealthRequest request = new ClusterHealthRequest();
        request.timeout("5s");
        ClusterHealthResponse response = execute(request, highLevelClient().cluster()::health, highLevelClient().cluster()::healthAsync);

        logger.info("Shard stats\n{}", EntityUtils.toString(
                client().performRequest(new Request("GET", "/_cat/shards")).getEntity()));
        assertThat(response.getIndices().size(), equalTo(0));
    }

    public void testClusterHealthYellowIndicesLevel() throws IOException {
        String firstIndex = "index";
        String secondIndex = "index2";
        // including another index that we do not assert on, to ensure that we are not
        // accidentally asserting on entire cluster state
        String ignoredIndex = "tasks";
        createIndex(firstIndex, Settings.EMPTY);
        createIndex(secondIndex, Settings.EMPTY);
        if (randomBoolean()) {
            createIndex(ignoredIndex, Settings.EMPTY);
        }
        ClusterHealthRequest request = new ClusterHealthRequest(firstIndex, secondIndex);
        request.timeout("5s");
        request.level(ClusterHealthRequest.Level.INDICES);
        ClusterHealthResponse response = execute(request, highLevelClient().cluster()::health, highLevelClient().cluster()::healthAsync);

        logger.info("Shard stats\n{}", EntityUtils.toString(
                client().performRequest(new Request("GET", "/_cat/shards")).getEntity()));
        assertYellowShards(response);
        assertThat(response.getIndices().size(), equalTo(2));
        for (Map.Entry<String, ClusterIndexHealth> entry : response.getIndices().entrySet()) {
            assertYellowIndex(entry.getKey(), entry.getValue(), true);
        }
    }

    private static void assertYellowShards(ClusterHealthResponse response) {
        assertThat(response, notNullValue());
        assertThat(response.isTimedOut(), equalTo(false));
        assertThat(response.status(), equalTo(RestStatus.OK));
        assertThat(response.getStatus(), equalTo(ClusterHealthStatus.YELLOW));
        assertThat(response.getActivePrimaryShards(), equalTo(2));
        assertThat(response.getNumberOfDataNodes(), equalTo(1));
        assertThat(response.getNumberOfNodes(), equalTo(1));
        assertThat(response.getActiveShards(), equalTo(2));
        assertThat(response.getDelayedUnassignedShards(), equalTo(0));
        assertThat(response.getInitializingShards(), equalTo(0));
        assertThat(response.getUnassignedShards(), equalTo(2));
    }


    public void testClusterHealthYellowSpecificIndex() throws IOException {
        createIndex("index", Settings.EMPTY);
        createIndex("index2", Settings.EMPTY);
        ClusterHealthRequest request = new ClusterHealthRequest("index");
        request.level(ClusterHealthRequest.Level.SHARDS);
        request.timeout("5s");
        ClusterHealthResponse response = execute(request, highLevelClient().cluster()::health, highLevelClient().cluster()::healthAsync);

        assertThat(response, notNullValue());
        assertThat(response.isTimedOut(), equalTo(false));
        assertThat(response.status(), equalTo(RestStatus.OK));
        assertThat(response.getStatus(), equalTo(ClusterHealthStatus.YELLOW));
        assertThat(response.getActivePrimaryShards(), equalTo(1));
        assertThat(response.getNumberOfDataNodes(), equalTo(1));
        assertThat(response.getNumberOfNodes(), equalTo(1));
        assertThat(response.getActiveShards(), equalTo(1));
        assertThat(response.getDelayedUnassignedShards(), equalTo(0));
        assertThat(response.getInitializingShards(), equalTo(0));
        assertThat(response.getUnassignedShards(), equalTo(1));
        assertThat(response.getIndices().size(), equalTo(1));
        Map.Entry<String, ClusterIndexHealth> index = response.getIndices().entrySet().iterator().next();
        assertYellowIndex(index.getKey(), index.getValue(), false);
    }

    private static void assertYellowIndex(String indexName, ClusterIndexHealth indexHealth, boolean emptyShards) {
        assertThat(indexHealth, notNullValue());
        assertThat(indexHealth.getIndex(),equalTo(indexName));
        assertThat(indexHealth.getActivePrimaryShards(),equalTo(1));
        assertThat(indexHealth.getActiveShards(),equalTo(1));
        assertThat(indexHealth.getNumberOfReplicas(),equalTo(1));
        assertThat(indexHealth.getInitializingShards(),equalTo(0));
        assertThat(indexHealth.getUnassignedShards(),equalTo(1));
        assertThat(indexHealth.getRelocatingShards(),equalTo(0));
        assertThat(indexHealth.getStatus(),equalTo(ClusterHealthStatus.YELLOW));
        if (emptyShards) {
            assertThat(indexHealth.getShards().size(), equalTo(0));
        } else {
            assertThat(indexHealth.getShards().size(), equalTo(1));
            for (Map.Entry<Integer, ClusterShardHealth> entry : indexHealth.getShards().entrySet()) {
                assertYellowShard(entry.getKey(), entry.getValue());
            }
        }
    }

    private static void assertYellowShard(int shardId, ClusterShardHealth shardHealth) {
        assertThat(shardHealth, notNullValue());
        assertThat(shardHealth.getShardId(), equalTo(shardId));
        assertThat(shardHealth.getStatus(), equalTo(ClusterHealthStatus.YELLOW));
        assertThat(shardHealth.getActiveShards(), equalTo(1));
        assertThat(shardHealth.getInitializingShards(), equalTo(0));
        assertThat(shardHealth.getUnassignedShards(), equalTo(1));
        assertThat(shardHealth.getRelocatingShards(), equalTo(0));
    }

    private static void assertNoIndices(ClusterHealthResponse response) {
        assertThat(response.getIndices(), equalTo(emptyMap()));
        assertThat(response.getActivePrimaryShards(), equalTo(0));
        assertThat(response.getNumberOfDataNodes(), equalTo(1));
        assertThat(response.getNumberOfNodes(), equalTo(1));
        assertThat(response.getActiveShards(), equalTo(0));
        assertThat(response.getDelayedUnassignedShards(), equalTo(0));
        assertThat(response.getInitializingShards(), equalTo(0));
        assertThat(response.getUnassignedShards(), equalTo(0));
    }

    public void testClusterHealthNotFoundIndex() throws IOException {
        createIndex("index", Settings.EMPTY);
        ClusterHealthRequest request = new ClusterHealthRequest("notexisted-index");
        request.timeout("5s");
        ClusterHealthResponse response = execute(request, highLevelClient().cluster()::health, highLevelClient().cluster()::healthAsync);

        assertThat(response, notNullValue());
        assertThat(response.isTimedOut(), equalTo(true));
        assertThat(response.status(), equalTo(RestStatus.REQUEST_TIMEOUT));
        assertThat(response.getStatus(), equalTo(ClusterHealthStatus.RED));
        assertNoIndices(response);
    }

    public void testRemoteInfo() throws Exception {
        String clusterAlias = "local_cluster";
        setupRemoteClusterConfig(clusterAlias);

        ClusterGetSettingsRequest settingsRequest = new ClusterGetSettingsRequest();
        settingsRequest.includeDefaults(true);
        ClusterGetSettingsResponse settingsResponse = highLevelClient().cluster().getSettings(settingsRequest, RequestOptions.DEFAULT);

        List<String> seeds = SniffConnectionStrategy.REMOTE_CLUSTER_SEEDS
                .getConcreteSettingForNamespace(clusterAlias)
                .get(settingsResponse.getTransientSettings());
        int connectionsPerCluster = SniffConnectionStrategy.REMOTE_CONNECTIONS_PER_CLUSTER
                .get(settingsResponse.getTransientSettings());
        TimeValue initialConnectionTimeout = RemoteClusterService.REMOTE_INITIAL_CONNECTION_TIMEOUT_SETTING
                .get(settingsResponse.getTransientSettings());
        boolean skipUnavailable = RemoteClusterService.REMOTE_CLUSTER_SKIP_UNAVAILABLE
                .getConcreteSettingForNamespace(clusterAlias)
                .get(settingsResponse.getTransientSettings());

        RemoteInfoRequest request = new RemoteInfoRequest();
        RemoteInfoResponse response = execute(request, highLevelClient().cluster()::remoteInfo,
                highLevelClient().cluster()::remoteInfoAsync);

        assertThat(response, notNullValue());
        assertThat(response.getInfos().size(), equalTo(1));
        RemoteConnectionInfo info = response.getInfos().get(0);
        assertThat(info.getClusterAlias(), equalTo(clusterAlias));
        assertThat(info.getInitialConnectionTimeoutString(), equalTo(initialConnectionTimeout.toString()));
        assertThat(info.isSkipUnavailable(), equalTo(skipUnavailable));
        assertThat(info.getModeInfo().modeName(), equalTo(SniffModeInfo.NAME));
        assertThat(info.getModeInfo().isConnected(), equalTo(true));
        SniffModeInfo sniffModeInfo = (SniffModeInfo) info.getModeInfo();
        assertThat(sniffModeInfo.getMaxConnectionsPerCluster(), equalTo(connectionsPerCluster));
        assertThat(sniffModeInfo.getNumNodesConnected(), equalTo(1));
        assertThat(sniffModeInfo.getSeedNodes(), equalTo(seeds));
    }

    public void testComponentTemplates() throws Exception {
        String templateName = "my-template";
        Settings settings = Settings.builder().put("index.number_of_shards", 1).build();
        CompressedXContent mappings = new CompressedXContent("{\"properties\":{\"host_name\":{\"type\":\"keyword\"}}}");
        AliasMetadata alias = AliasMetadata.builder("alias").writeIndex(true).build();
        Template template = new Template(settings, mappings, Map.of("alias", alias));
        ComponentTemplate componentTemplate = new ComponentTemplate(template, 1L, new HashMap<>());
        PutComponentTemplateRequest putComponentTemplateRequest =
            new PutComponentTemplateRequest().name(templateName).create(true).componentTemplate(componentTemplate);

        AcknowledgedResponse response = execute(putComponentTemplateRequest,
            highLevelClient().cluster()::putComponentTemplate, highLevelClient().cluster()::putComponentTemplateAsync);
        assertThat(response.isAcknowledged(), equalTo(true));

        ComponentTemplatesExistRequest componentTemplatesExistRequest = new ComponentTemplatesExistRequest(templateName);
        boolean exist = execute(componentTemplatesExistRequest,
            highLevelClient().cluster()::existsComponentTemplate, highLevelClient().cluster()::existsComponentTemplateAsync);

        assertTrue(exist);

        GetComponentTemplatesRequest getComponentTemplatesRequest = new GetComponentTemplatesRequest(templateName);
        GetComponentTemplatesResponse getResponse = execute(getComponentTemplatesRequest,
            highLevelClient().cluster()::getComponentTemplate, highLevelClient().cluster()::getComponentTemplateAsync);

        assertThat(getResponse.getComponentTemplates().size(), equalTo(1));
        assertThat(getResponse.getComponentTemplates().containsKey(templateName), equalTo(true));
        assertThat(getResponse.getComponentTemplates().get(templateName), equalTo(componentTemplate));

        DeleteComponentTemplateRequest deleteComponentTemplateRequest = new DeleteComponentTemplateRequest(templateName);
        response = execute(deleteComponentTemplateRequest, highLevelClient().cluster()::deleteComponentTemplate,
            highLevelClient().cluster()::deleteComponentTemplateAsync);
        assertThat(response.isAcknowledged(), equalTo(true));

        ElasticsearchStatusException statusException = expectThrows(ElasticsearchStatusException.class,
            () -> execute(getComponentTemplatesRequest,
                highLevelClient().cluster()::getComponentTemplate, highLevelClient().cluster()::getComponentTemplateAsync));

        assertThat(statusException.status(), equalTo(RestStatus.NOT_FOUND));

        exist = execute(componentTemplatesExistRequest,
            highLevelClient().cluster()::existsComponentTemplate, highLevelClient().cluster()::existsComponentTemplateAsync);

        assertFalse(exist);
    }
}
