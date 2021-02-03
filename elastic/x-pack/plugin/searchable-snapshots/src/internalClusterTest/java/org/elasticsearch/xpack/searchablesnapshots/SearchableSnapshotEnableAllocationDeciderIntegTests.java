/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.searchablesnapshots;

import org.elasticsearch.action.admin.cluster.health.ClusterHealthResponse;
import org.elasticsearch.client.Requests;
import org.elasticsearch.cluster.routing.allocation.decider.EnableAllocationDecider;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.snapshots.mockstore.MockRepository;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.xpack.searchablesnapshots.cache.CacheService;
import org.hamcrest.Matchers;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Set;

import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;

@ESIntegTestCase.ClusterScope(scope = ESIntegTestCase.Scope.TEST)
public class SearchableSnapshotEnableAllocationDeciderIntegTests extends BaseSearchableSnapshotsIntegTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        List<Class<? extends Plugin>> plugins = new ArrayList<>(super.nodePlugins());
        plugins.add(MockRepository.Plugin.class);
        return plugins;
    }

    @Override
    protected Settings nodeSettings(int nodeOrdinal) {
        return Settings.builder()
            .put(super.nodeSettings(nodeOrdinal))
            // Use an unbound cache so we can recover the searchable snapshot completely all the times
            .put(CacheService.SNAPSHOT_CACHE_SIZE_SETTING.getKey(), new ByteSizeValue(Long.MAX_VALUE, ByteSizeUnit.BYTES))
            .build();
    }

    public void testAllocationDisabled() throws Exception {
        final String restoredIndexName = setupMountedIndex();
        int numPrimaries = getNumShards(restoredIndexName).numPrimaries;
        setEnableAllocation(EnableAllocationDecider.Allocation.PRIMARIES);
        if (randomBoolean()) {
            setAllocateOnRollingRestart(false);
        }
        Set<String> indexNodes = internalCluster().nodesInclude(restoredIndexName);
        for (String indexNode : indexNodes) {
            internalCluster().restartNode(indexNode);
        }

        ClusterHealthResponse response = client().admin().cluster().health(Requests.clusterHealthRequest(restoredIndexName)).actionGet();
        assertThat(response.getUnassignedShards(), Matchers.equalTo(numPrimaries));

        setAllocateOnRollingRestart(true);
        ensureGreen(restoredIndexName);
    }

    public void testAllocateOnRollingRestartEnabled() throws Exception {
        final String restoredIndexName = setupMountedIndex();
        if (randomBoolean()) {
            setEnableAllocation(EnableAllocationDecider.Allocation.PRIMARIES);
        }
        setAllocateOnRollingRestart(true);
        Set<String> indexNodes = internalCluster().nodesInclude(restoredIndexName);
        for (String indexNode : indexNodes) {
            internalCluster().restartNode(indexNode);
        }

        ensureGreen(restoredIndexName);
    }

    private String setupMountedIndex() throws Exception {
        final String indexName = randomAlphaOfLength(10).toLowerCase(Locale.ROOT);
        createAndPopulateIndex(indexName, Settings.builder());

        final String repositoryName = randomAlphaOfLength(10).toLowerCase(Locale.ROOT);
        createRepository(repositoryName, "mock");

        final SnapshotId snapshotId = createSnapshot(repositoryName, "snapshot-1", List.of(indexName)).snapshotId();
        assertAcked(client().admin().indices().prepareDelete(indexName));
        return mountSnapshot(repositoryName, snapshotId.getName(), indexName, Settings.EMPTY);
    }

    public void setEnableAllocation(EnableAllocationDecider.Allocation allocation) {
        setSetting(EnableAllocationDecider.CLUSTER_ROUTING_ALLOCATION_ENABLE_SETTING, allocation.name());
    }

    public void setAllocateOnRollingRestart(boolean allocateOnRollingRestart) {
        setSetting(
            SearchableSnapshotEnableAllocationDecider.SEARCHABLE_SNAPSHOTS_ALLOCATE_ON_ROLLING_RESTART,
            Boolean.toString(allocateOnRollingRestart)
        );
    }

    private void setSetting(Setting<?> setting, String value) {
        logger.info("--> setting [{}={}]", setting.getKey(), value);
        assertAcked(
            client().admin()
                .cluster()
                .prepareUpdateSettings()
                .setPersistentSettings(Settings.builder().put(setting.getKey(), value).build())
                .get()
        );
    }
}
