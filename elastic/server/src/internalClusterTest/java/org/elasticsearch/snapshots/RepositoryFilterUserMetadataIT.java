/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.snapshots;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.env.Environment;
import org.elasticsearch.indices.recovery.RecoverySettings;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.RepositoryPlugin;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryData;
import org.elasticsearch.repositories.ShardGenerations;
import org.elasticsearch.repositories.SnapshotShardContext;
import org.elasticsearch.repositories.fs.FsRepository;
import org.elasticsearch.test.ESIntegTestCase;

import java.util.Collection;
import java.util.Collections;
import java.util.Map;
import java.util.function.Function;

import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.is;

public class RepositoryFilterUserMetadataIT extends ESIntegTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return Collections.singleton(MetadataFilteringPlugin.class);
    }

    public void testFilteredRepoMetadataIsUsed() {
        final String masterName = internalCluster().getMasterName();
        final String repoName = "test-repo";
        assertAcked(client().admin().cluster().preparePutRepository(repoName).setType(MetadataFilteringPlugin.TYPE).setSettings(
            Settings.builder().put("location", randomRepoPath())
                .put(MetadataFilteringPlugin.MASTER_SETTING_VALUE, masterName)));
        createIndex("test-idx");
        final SnapshotInfo snapshotInfo = client().admin().cluster().prepareCreateSnapshot(repoName, "test-snap")
            .setWaitForCompletion(true).get().getSnapshotInfo();
        assertThat(snapshotInfo.userMetadata(), is(Collections.singletonMap(MetadataFilteringPlugin.MOCK_FILTERED_META, masterName)));
    }

    // Mock plugin that stores the name of the master node that started a snapshot in each snapshot's metadata
    public static final class MetadataFilteringPlugin extends org.elasticsearch.plugins.Plugin implements RepositoryPlugin {

        private static final String MOCK_FILTERED_META = "mock_filtered_meta";

        private static final String MASTER_SETTING_VALUE = "initial_master";

        private static final String TYPE = "mock_meta_filtering";

        @Override
        public Map<String, Repository.Factory> getRepositories(Environment env, NamedXContentRegistry namedXContentRegistry,
                                                               ClusterService clusterService, BigArrays bigArrays,
                                                               RecoverySettings recoverySettings) {
            return Collections.singletonMap("mock_meta_filtering", metadata ->
                new FsRepository(metadata, env, namedXContentRegistry, clusterService, bigArrays, recoverySettings) {

                    // Storing the initially expected metadata value here to verify that #filterUserMetadata is only called once on the
                    // initial master node starting the snapshot
                    private final String initialMetaValue = metadata.settings().get(MASTER_SETTING_VALUE);

                    @Override
                    public void finalizeSnapshot(ShardGenerations shardGenerations, long repositoryStateId,
                                                 Metadata clusterMetadata, SnapshotInfo snapshotInfo, Version repositoryMetaVersion,
                                                 Function<ClusterState, ClusterState> stateTransformer,
                                                 ActionListener<RepositoryData> listener) {
                        super.finalizeSnapshot(shardGenerations, repositoryStateId, clusterMetadata, snapshotInfo,
                            repositoryMetaVersion, stateTransformer, listener);
                    }

                    @Override
                    public void snapshotShard(SnapshotShardContext context) {
                        assertThat(context.userMetadata(), is(Collections.singletonMap(MOCK_FILTERED_META, initialMetaValue)));
                        super.snapshotShard(context);
                    }

                    @Override
                    public Map<String, Object> adaptUserMetadata(Map<String, Object> userMetadata) {
                        return Collections.singletonMap(MOCK_FILTERED_META, clusterService.getNodeName());
                    }
                });
        }
    }
}
