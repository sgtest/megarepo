/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.searchablesnapshots;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.CollectionUtils;
import org.elasticsearch.indices.SystemIndexDescriptor;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.SystemIndexPlugin;
import org.elasticsearch.snapshots.SnapshotInfo;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.searchablesnapshots.MountSearchableSnapshotAction;
import org.elasticsearch.xpack.core.searchablesnapshots.MountSearchableSnapshotRequest;

import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class SearchableSnapshotsSystemIndicesIntegTests extends BaseSearchableSnapshotsIntegTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return CollectionUtils.appendToCopy(super.nodePlugins(), TestSystemIndexPlugin.class);
    }

    public void testCannotMountSystemIndex() throws Exception {
        executeTest(TestSystemIndexPlugin.INDEX_NAME, new OriginSettingClient(client(), ClientHelper.SEARCHABLE_SNAPSHOTS_ORIGIN));
    }

    public void testCannotMountSnapshotBlobCacheIndex() throws Exception {
        executeTest(SearchableSnapshotsConstants.SNAPSHOT_BLOB_CACHE_INDEX, client());
    }

    private void executeTest(final String indexName, final Client client) throws Exception {
        final boolean isHidden = randomBoolean();
        createAndPopulateIndex(indexName, Settings.builder().put(IndexMetadata.SETTING_INDEX_HIDDEN, isHidden));

        final String repositoryName = randomAlphaOfLength(10).toLowerCase(Locale.ROOT);
        createRepository(repositoryName, "fs");

        final String snapshotName = randomAlphaOfLength(10).toLowerCase(Locale.ROOT);
        final int numPrimaries = getNumShards(indexName).numPrimaries;
        final SnapshotInfo snapshotInfo = createSnapshot(repositoryName, snapshotName, Collections.singletonList(indexName));
        assertThat(snapshotInfo.successfulShards(), equalTo(numPrimaries));
        assertThat(snapshotInfo.failedShards(), equalTo(0));

        if (randomBoolean()) {
            assertAcked(client.admin().indices().prepareClose(indexName));
        } else {
            assertAcked(client.admin().indices().prepareDelete(indexName));
        }

        final MountSearchableSnapshotRequest mountRequest = new MountSearchableSnapshotRequest(
            indexName,
            repositoryName,
            snapshotName,
            indexName,
            Settings.builder().put(IndexMetadata.SETTING_INDEX_HIDDEN, randomBoolean()).build(),
            Strings.EMPTY_ARRAY,
            true,
            randomFrom(MountSearchableSnapshotRequest.Storage.values())
        );

        final ElasticsearchException exception = expectThrows(
            ElasticsearchException.class,
            () -> client.execute(MountSearchableSnapshotAction.INSTANCE, mountRequest).actionGet()
        );
        assertThat(exception.getMessage(), containsString("system index [" + indexName + "] cannot be mounted as searchable snapshots"));
    }

    public static class TestSystemIndexPlugin extends Plugin implements SystemIndexPlugin {

        static final String INDEX_NAME = ".test-system-index";

        @Override
        public Collection<SystemIndexDescriptor> getSystemIndexDescriptors(Settings settings) {
            return List.of(new SystemIndexDescriptor(INDEX_NAME, "System index for [" + getTestClass().getName() + ']'));
        }
    }
}
