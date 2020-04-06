/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.searchablesnapshots;

import org.apache.lucene.index.IndexFileNames;
import org.apache.lucene.search.TotalHits;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.admin.cluster.snapshots.create.CreateSnapshotResponse;
import org.elasticsearch.action.admin.cluster.snapshots.restore.RestoreSnapshotResponse;
import org.elasticsearch.action.admin.indices.recovery.RecoveryResponse;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.concurrent.AtomicArray;
import org.elasticsearch.index.IndexModule;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.indices.recovery.RecoveryState;
import org.elasticsearch.snapshots.SnapshotInfo;
import org.elasticsearch.xpack.core.searchablesnapshots.MountSearchableSnapshotAction;
import org.elasticsearch.xpack.core.searchablesnapshots.MountSearchableSnapshotRequest;
import org.elasticsearch.xpack.core.searchablesnapshots.SearchableSnapshotShardStats;
import org.elasticsearch.xpack.searchablesnapshots.action.SearchableSnapshotsStatsAction;
import org.elasticsearch.xpack.searchablesnapshots.action.SearchableSnapshotsStatsRequest;
import org.elasticsearch.xpack.searchablesnapshots.action.SearchableSnapshotsStatsResponse;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.CountDownLatch;
import java.util.stream.StreamSupport;

import static org.elasticsearch.index.query.QueryBuilders.matchQuery;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshots.SNAPSHOT_DIRECTORY_FACTORY_KEY;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.lessThanOrEqualTo;

public class SearchableSnapshotsIntegTests extends BaseSearchableSnapshotsIntegTestCase {

    public void testCreateAndRestoreSearchableSnapshot() throws Exception {
        final String fsRepoName = randomAlphaOfLength(10);
        final String indexName = randomAlphaOfLength(10).toLowerCase(Locale.ROOT);
        final String restoredIndexName = randomBoolean() ? indexName : randomAlphaOfLength(10).toLowerCase(Locale.ROOT);
        final String snapshotName = randomAlphaOfLength(10).toLowerCase(Locale.ROOT);

        final Path repo = randomRepoPath();
        assertAcked(
            client().admin()
                .cluster()
                .preparePutRepository(fsRepoName)
                .setType("fs")
                .setSettings(Settings.builder().put("location", repo).put("chunk_size", randomIntBetween(100, 1000), ByteSizeUnit.BYTES))
        );

        createIndex(indexName);
        final List<IndexRequestBuilder> indexRequestBuilders = new ArrayList<>();
        for (int i = between(10, 10_000); i >= 0; i--) {
            indexRequestBuilders.add(client().prepareIndex(indexName).setSource("foo", randomBoolean() ? "bar" : "baz"));
        }
        // TODO NORELEASE no dummy docs since that includes deletes, yet we always copy the .liv file in peer recovery
        indexRandom(true, false, indexRequestBuilders);
        refresh(indexName);
        assertThat(
            client().admin().indices().prepareForceMerge(indexName).setOnlyExpungeDeletes(true).setFlush(true).get().getFailedShards(),
            equalTo(0)
        );

        final TotalHits originalAllHits = internalCluster().client()
            .prepareSearch(indexName)
            .setTrackTotalHits(true)
            .get()
            .getHits()
            .getTotalHits();
        final TotalHits originalBarHits = internalCluster().client()
            .prepareSearch(indexName)
            .setTrackTotalHits(true)
            .setQuery(matchQuery("foo", "bar"))
            .get()
            .getHits()
            .getTotalHits();
        logger.info("--> [{}] in total, of which [{}] match the query", originalAllHits, originalBarHits);

        expectThrows(
            ResourceNotFoundException.class,
            "Searchable snapshot stats on a non snapshot searchable index should fail",
            () -> client().execute(SearchableSnapshotsStatsAction.INSTANCE, new SearchableSnapshotsStatsRequest()).actionGet()
        );

        CreateSnapshotResponse createSnapshotResponse = client().admin()
            .cluster()
            .prepareCreateSnapshot(fsRepoName, snapshotName)
            .setWaitForCompletion(true)
            .get();
        final SnapshotInfo snapshotInfo = createSnapshotResponse.getSnapshotInfo();
        assertThat(snapshotInfo.successfulShards(), greaterThan(0));
        assertThat(snapshotInfo.successfulShards(), equalTo(snapshotInfo.totalShards()));

        assertAcked(client().admin().indices().prepareDelete(indexName));

        final boolean cacheEnabled = randomBoolean();
        logger.info("--> restoring index [{}] with cache [{}]", restoredIndexName, cacheEnabled ? "enabled" : "disabled");

        Settings.Builder indexSettingsBuilder = Settings.builder()
            .put(SearchableSnapshots.SNAPSHOT_CACHE_ENABLED_SETTING.getKey(), cacheEnabled)
            .put(IndexSettings.INDEX_CHECK_ON_STARTUP.getKey(), Boolean.FALSE.toString());
        final List<String> nonCachedExtensions;
        if (randomBoolean()) {
            nonCachedExtensions = randomSubsetOf(Arrays.asList("fdt", "fdx", "nvd", "dvd", "tip", "cfs", "dim"));
            indexSettingsBuilder.putList(SearchableSnapshots.SNAPSHOT_CACHE_EXCLUDED_FILE_TYPES_SETTING.getKey(), nonCachedExtensions);
        } else {
            nonCachedExtensions = Collections.emptyList();
        }
        if (randomBoolean()) {
            indexSettingsBuilder.put(
                SearchableSnapshots.SNAPSHOT_UNCACHED_CHUNK_SIZE_SETTING.getKey(),
                new ByteSizeValue(randomLongBetween(10, 100_000))
            );
        }
        final MountSearchableSnapshotRequest req = new MountSearchableSnapshotRequest(
            restoredIndexName,
            fsRepoName,
            snapshotInfo.snapshotId().getName(),
            indexName,
            indexSettingsBuilder.build(),
            Strings.EMPTY_ARRAY,
            true
        );

        final RestoreSnapshotResponse restoreSnapshotResponse = client().execute(MountSearchableSnapshotAction.INSTANCE, req).get();
        assertThat(restoreSnapshotResponse.getRestoreInfo().failedShards(), equalTo(0));

        final Settings settings = client().admin()
            .indices()
            .prepareGetSettings(restoredIndexName)
            .get()
            .getIndexToSettings()
            .get(restoredIndexName);
        assertThat(SearchableSnapshots.SNAPSHOT_REPOSITORY_SETTING.get(settings), equalTo(fsRepoName));
        assertThat(SearchableSnapshots.SNAPSHOT_SNAPSHOT_NAME_SETTING.get(settings), equalTo(snapshotName));
        assertThat(IndexModule.INDEX_STORE_TYPE_SETTING.get(settings), equalTo(SNAPSHOT_DIRECTORY_FACTORY_KEY));
        assertTrue(IndexMetadata.INDEX_BLOCKS_WRITE_SETTING.get(settings));
        assertTrue(SearchableSnapshots.SNAPSHOT_SNAPSHOT_ID_SETTING.exists(settings));
        assertTrue(SearchableSnapshots.SNAPSHOT_INDEX_ID_SETTING.exists(settings));

        assertRecovered(restoredIndexName, originalAllHits, originalBarHits);
        assertSearchableSnapshotStats(restoredIndexName, cacheEnabled, nonCachedExtensions);

        internalCluster().fullRestart();
        assertRecovered(restoredIndexName, originalAllHits, originalBarHits);
        assertSearchableSnapshotStats(restoredIndexName, cacheEnabled, nonCachedExtensions);

        internalCluster().ensureAtLeastNumDataNodes(2);

        final DiscoveryNode dataNode = randomFrom(
            StreamSupport.stream(
                client().admin().cluster().prepareState().get().getState().nodes().getDataNodes().values().spliterator(),
                false
            ).map(c -> c.value).toArray(DiscoveryNode[]::new)
        );

        assertAcked(
            client().admin()
                .indices()
                .prepareUpdateSettings(restoredIndexName)
                .setSettings(
                    Settings.builder()
                        .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 0)
                        .put(
                            IndexMetadata.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getConcreteSettingForNamespace("_name").getKey(),
                            dataNode.getName()
                        )
                )
        );

        assertFalse(
            client().admin()
                .cluster()
                .prepareHealth(restoredIndexName)
                .setWaitForNoRelocatingShards(true)
                .setWaitForEvents(Priority.LANGUID)
                .get()
                .isTimedOut()
        );

        assertRecovered(restoredIndexName, originalAllHits, originalBarHits);
        assertSearchableSnapshotStats(restoredIndexName, cacheEnabled, nonCachedExtensions);
    }

    private void assertRecovered(String indexName, TotalHits originalAllHits, TotalHits originalBarHits) throws Exception {
        final Thread[] threads = new Thread[between(1, 5)];
        final AtomicArray<TotalHits> allHits = new AtomicArray<>(threads.length);
        final AtomicArray<TotalHits> barHits = new AtomicArray<>(threads.length);

        final CountDownLatch latch = new CountDownLatch(1);
        for (int i = 0; i < threads.length; i++) {
            int t = i;
            threads[i] = new Thread(() -> {
                try {
                    latch.await();
                } catch (InterruptedException e) {
                    throw new RuntimeException(e);
                }
                allHits.set(t, client().prepareSearch(indexName).setTrackTotalHits(true).get().getHits().getTotalHits());
                barHits.set(
                    t,
                    client().prepareSearch(indexName)
                        .setTrackTotalHits(true)
                        .setQuery(matchQuery("foo", "bar"))
                        .get()
                        .getHits()
                        .getTotalHits()
                );
            });
            threads[i].start();
        }

        ensureGreen(indexName);
        latch.countDown();

        final RecoveryResponse recoveryResponse = client().admin().indices().prepareRecoveries(indexName).get();
        for (List<RecoveryState> recoveryStates : recoveryResponse.shardRecoveryStates().values()) {
            for (RecoveryState recoveryState : recoveryStates) {
                logger.info("Checking {}[{}]", recoveryState.getShardId(), recoveryState.getPrimary() ? "p" : "r");
                assertThat(recoveryState.getIndex().recoveredFileCount(), lessThanOrEqualTo(1)); // we make a new commit so we write a new
                                                                                                 // `segments_n` file
            }
        }

        for (int i = 0; i < threads.length; i++) {
            threads[i].join();

            final TotalHits allTotalHits = allHits.get(i);
            final TotalHits barTotalHits = barHits.get(i);

            logger.info("--> thread #{} has [{}] hits in total, of which [{}] match the query", i, allTotalHits, barTotalHits);
            assertThat(allTotalHits, equalTo(originalAllHits));
            assertThat(barTotalHits, equalTo(originalBarHits));
        }
    }

    private void assertSearchableSnapshotStats(String indexName, boolean cacheEnabled, List<String> nonCachedExtensions) {
        final SearchableSnapshotsStatsResponse statsResponse = client().execute(
            SearchableSnapshotsStatsAction.INSTANCE,
            new SearchableSnapshotsStatsRequest(indexName)
        ).actionGet();
        final NumShards restoredNumShards = getNumShards(indexName);
        assertThat(statsResponse.getStats(), hasSize(restoredNumShards.totalNumShards));

        for (SearchableSnapshotShardStats stats : statsResponse.getStats()) {
            final ShardRouting shardRouting = stats.getShardRouting();
            assertThat(stats.getShardRouting().getIndexName(), equalTo(indexName));
            if (shardRouting.started()) {
                assertThat("Expecting stats to exist for at least 1 Lucene file", stats.getStats().size(), greaterThan(0));
                for (SearchableSnapshotShardStats.CacheIndexInputStats indexInputStats : stats.getStats()) {
                    final String fileName = indexInputStats.getFileName();
                    assertThat(
                        "Unexpected open count for " + fileName + " of shard " + shardRouting,
                        indexInputStats.getOpenCount(),
                        greaterThan(0L)
                    );
                    assertThat(
                        "Unexpected close count for " + fileName + " of shard " + shardRouting,
                        indexInputStats.getCloseCount(),
                        lessThanOrEqualTo(indexInputStats.getOpenCount())
                    );
                    assertThat(
                        "Unexpected file length for " + fileName + " of shard " + shardRouting,
                        indexInputStats.getFileLength(),
                        greaterThan(0L)
                    );

                    if (cacheEnabled == false || nonCachedExtensions.contains(IndexFileNames.getExtension(fileName))) {
                        assertThat(
                            "Expected at least 1 optimized or direct read for " + fileName + " of shard " + shardRouting,
                            Math.max(indexInputStats.getOptimizedBytesRead().getCount(), indexInputStats.getDirectBytesRead().getCount()),
                            greaterThan(0L)
                        );
                        assertThat(
                            "Expected no cache read or write for " + fileName + " of shard " + shardRouting,
                            Math.max(indexInputStats.getCachedBytesRead().getCount(), indexInputStats.getCachedBytesWritten().getCount()),
                            equalTo(0L)
                        );
                    } else {
                        assertThat(
                            "Expected at least 1 cache read or write for " + fileName + " of shard " + shardRouting,
                            Math.max(indexInputStats.getCachedBytesRead().getCount(), indexInputStats.getCachedBytesWritten().getCount()),
                            greaterThan(0L)
                        );
                        assertThat(
                            "Expected no optimized read for " + fileName + " of shard " + shardRouting,
                            indexInputStats.getOptimizedBytesRead().getCount(),
                            equalTo(0L)
                        );
                    }
                }
            }
        }
    }
}
