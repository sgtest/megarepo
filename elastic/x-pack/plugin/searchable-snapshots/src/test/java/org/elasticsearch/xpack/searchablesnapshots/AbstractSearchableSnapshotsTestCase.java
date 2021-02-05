/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.searchablesnapshots;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.cluster.routing.RecoverySource;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.ShardRoutingState;
import org.elasticsearch.cluster.routing.TestShardRouting;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.lucene.store.ESIndexInputTestCase;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.core.internal.io.IOUtils;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.store.cache.CacheFile;
import org.elasticsearch.index.store.cache.CacheKey;
import org.elasticsearch.indices.recovery.RecoveryState;
import org.elasticsearch.indices.recovery.SearchableSnapshotRecoveryState;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.snapshots.Snapshot;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.test.ClusterServiceUtils;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.threadpool.ThreadPoolStats;
import org.elasticsearch.xpack.searchablesnapshots.cache.CacheService;
import org.elasticsearch.xpack.searchablesnapshots.cache.FrozenCacheService;
import org.elasticsearch.xpack.searchablesnapshots.cache.PersistentCache;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.SortedSet;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.index.store.cache.TestUtils.randomPopulateAndReads;

public abstract class AbstractSearchableSnapshotsTestCase extends ESIndexInputTestCase {

    private static final ClusterSettings CLUSTER_SETTINGS = new ClusterSettings(
        Settings.EMPTY,
        Sets.union(
            ClusterSettings.BUILT_IN_CLUSTER_SETTINGS,
            Set.of(
                CacheService.SNAPSHOT_CACHE_SIZE_SETTING,
                CacheService.SNAPSHOT_CACHE_RANGE_SIZE_SETTING,
                CacheService.SNAPSHOT_CACHE_SYNC_INTERVAL_SETTING,
                CacheService.SNAPSHOT_CACHE_MAX_FILES_TO_SYNC_AT_ONCE_SETTING
            )
        )
    );

    protected ThreadPool threadPool;
    protected ClusterService clusterService;
    protected NodeEnvironment nodeEnvironment;
    protected Environment environment;

    @Before
    public void setUpTest() throws Exception {
        final DiscoveryNode node = new DiscoveryNode(
            "node",
            ESTestCase.buildNewFakeTransportAddress(),
            Collections.emptyMap(),
            DiscoveryNodeRole.BUILT_IN_ROLES,
            Version.CURRENT
        );
        threadPool = new TestThreadPool(getTestName(), SearchableSnapshots.executorBuilders());
        clusterService = ClusterServiceUtils.createClusterService(threadPool, node, CLUSTER_SETTINGS);
        nodeEnvironment = newNodeEnvironment();
        environment = newEnvironment();
    }

    @After
    public void tearDownTest() throws Exception {
        IOUtils.close(nodeEnvironment, clusterService);
        assertTrue(ThreadPool.terminate(threadPool, 30L, TimeUnit.SECONDS));
    }

    /**
     * @return a new {@link CacheService} instance configured with default settings
     */
    protected CacheService defaultCacheService() {
        return new CacheService(Settings.EMPTY, clusterService, threadPool, new PersistentCache(nodeEnvironment));
    }

    /**
     * @return a new {@link CacheService} instance configured with random cache size and cache range size settings
     */
    protected CacheService randomCacheService() {
        final Settings.Builder cacheSettings = Settings.builder();
        if (randomBoolean()) {
            cacheSettings.put(CacheService.SNAPSHOT_CACHE_SIZE_SETTING.getKey(), randomCacheSize());
        }
        if (randomBoolean()) {
            cacheSettings.put(CacheService.SNAPSHOT_CACHE_RANGE_SIZE_SETTING.getKey(), randomCacheRangeSize());
        }
        if (randomBoolean()) {
            cacheSettings.put(CacheService.SNAPSHOT_CACHE_RECOVERY_RANGE_SIZE_SETTING.getKey(), randomCacheRangeSize());
        }
        if (randomBoolean()) {
            cacheSettings.put(
                CacheService.SNAPSHOT_CACHE_SYNC_INTERVAL_SETTING.getKey(),
                TimeValue.timeValueSeconds(scaledRandomIntBetween(1, 120))
            );
        }
        return new CacheService(cacheSettings.build(), clusterService, threadPool, new PersistentCache(nodeEnvironment));
    }

    /**
     * @return a new {@link FrozenCacheService} instance configured with default settings
     */
    protected FrozenCacheService defaultFrozenCacheService() {
        return new FrozenCacheService(environment, threadPool);
    }

    protected FrozenCacheService randomFrozenCacheService() {
        final Settings.Builder cacheSettings = Settings.builder();
        if (randomBoolean()) {
            cacheSettings.put(FrozenCacheService.SNAPSHOT_CACHE_SIZE_SETTING.getKey(), randomFrozenCacheSize());
        }
        if (randomBoolean()) {
            cacheSettings.put(FrozenCacheService.SNAPSHOT_CACHE_REGION_SIZE_SETTING.getKey(), randomFrozenCacheSize());
        }
        if (randomBoolean()) {
            cacheSettings.put(FrozenCacheService.FROZEN_CACHE_RANGE_SIZE_SETTING.getKey(), randomCacheRangeSize());
        }
        if (randomBoolean()) {
            cacheSettings.put(FrozenCacheService.FROZEN_CACHE_RECOVERY_RANGE_SIZE_SETTING.getKey(), randomCacheRangeSize());
        }
        return new FrozenCacheService(newEnvironment(cacheSettings.build()), threadPool);
    }

    /**
     * @return a new {@link CacheService} instance configured with the given cache size and cache range size settings
     */
    protected CacheService createCacheService(final ByteSizeValue cacheSize, final ByteSizeValue cacheRangeSize) {
        return new CacheService(
            Settings.builder()
                .put(CacheService.SNAPSHOT_CACHE_SIZE_SETTING.getKey(), cacheSize)
                .put(CacheService.SNAPSHOT_CACHE_RANGE_SIZE_SETTING.getKey(), cacheRangeSize)
                .build(),
            clusterService,
            threadPool,
            new PersistentCache(nodeEnvironment)
        );
    }

    protected FrozenCacheService createFrozenCacheService(final ByteSizeValue cacheSize, final ByteSizeValue cacheRangeSize) {
        return new FrozenCacheService(
            newEnvironment(
                Settings.builder()
                    .put(FrozenCacheService.SNAPSHOT_CACHE_SIZE_SETTING.getKey(), cacheSize)
                    .put(FrozenCacheService.FROZEN_CACHE_RANGE_SIZE_SETTING.getKey(), cacheRangeSize)
                    .build()
            ),
            threadPool
        );
    }

    /**
     * Returns a random shard data path for the specified {@link ShardId}. The returned path can be located on any of the data node paths.
     */
    protected Path randomShardPath(ShardId shardId) {
        return randomFrom(nodeEnvironment.availableShardPaths(shardId));
    }

    /**
     * @return a random {@link ByteSizeValue} that can be used to set {@link CacheService#SNAPSHOT_CACHE_SIZE_SETTING}.
     * Note that it can return a cache size of 0.
     */
    protected static ByteSizeValue randomCacheSize() {
        return new ByteSizeValue(randomNonNegativeLong());
    }

    protected static ByteSizeValue randomFrozenCacheSize() {
        return new ByteSizeValue(randomLongBetween(0, 10_000_000));
    }

    /**
     * @return a random {@link ByteSizeValue} that can be used to set {@link CacheService#SNAPSHOT_CACHE_RANGE_SIZE_SETTING}
     */
    protected static ByteSizeValue randomCacheRangeSize() {
        return new ByteSizeValue(
            randomLongBetween(CacheService.MIN_SNAPSHOT_CACHE_RANGE_SIZE.getBytes(), CacheService.MAX_SNAPSHOT_CACHE_RANGE_SIZE.getBytes())
        );
    }

    protected static ByteSizeValue randomFrozenCacheRangeSize() {
        return new ByteSizeValue(
            randomLongBetween(
                FrozenCacheService.MIN_SNAPSHOT_CACHE_RANGE_SIZE.getBytes(),
                FrozenCacheService.MAX_SNAPSHOT_CACHE_RANGE_SIZE.getBytes()
            )
        );
    }

    protected static SearchableSnapshotRecoveryState createRecoveryState(boolean finalizedDone) {
        ShardRouting shardRouting = TestShardRouting.newShardRouting(
            new ShardId(randomAlphaOfLength(10), randomAlphaOfLength(10), 0),
            randomAlphaOfLength(10),
            true,
            ShardRoutingState.INITIALIZING,
            new RecoverySource.SnapshotRecoverySource(
                UUIDs.randomBase64UUID(),
                new Snapshot("repo", new SnapshotId(randomAlphaOfLength(8), UUIDs.randomBase64UUID())),
                Version.CURRENT,
                new IndexId("some_index", UUIDs.randomBase64UUID(random()))
            )
        );
        DiscoveryNode targetNode = new DiscoveryNode("local", buildNewFakeTransportAddress(), Version.CURRENT);
        SearchableSnapshotRecoveryState recoveryState = new SearchableSnapshotRecoveryState(shardRouting, targetNode, null);

        recoveryState.setStage(RecoveryState.Stage.INIT)
            .setStage(RecoveryState.Stage.INDEX)
            .setStage(RecoveryState.Stage.VERIFY_INDEX)
            .setStage(RecoveryState.Stage.TRANSLOG);
        recoveryState.getIndex().setFileDetailsComplete();
        if (finalizedDone) {
            recoveryState.setStage(RecoveryState.Stage.FINALIZE).setStage(RecoveryState.Stage.DONE);
        }
        return recoveryState;
    }

    /**
     * Wait for all operations on the threadpool to complete
     */
    protected static void assertThreadPoolNotBusy(ThreadPool threadPool) throws Exception {
        assertBusy(() -> {
            for (ThreadPoolStats.Stats stat : threadPool.stats()) {
                assertEquals(stat.getActive(), 0);
                assertEquals(stat.getQueue(), 0);
            }
        }, 30L, TimeUnit.SECONDS);
    }

    /**
     * Generates one or more cache files using the specified {@link CacheService}. Each cache files have been written at least once.
     */
    protected List<CacheFile> randomCacheFiles(CacheService cacheService) throws Exception {
        final byte[] buffer = new byte[1024];
        Arrays.fill(buffer, (byte) 0xff);

        final List<CacheFile> cacheFiles = new ArrayList<>();
        for (int snapshots = 0; snapshots < between(1, 2); snapshots++) {
            final String snapshotUUID = UUIDs.randomBase64UUID(random());
            for (int indices = 0; indices < between(1, 2); indices++) {
                IndexId indexId = new IndexId(randomAlphaOfLength(5).toLowerCase(Locale.ROOT), UUIDs.randomBase64UUID(random()));
                for (int shards = 0; shards < between(1, 2); shards++) {
                    ShardId shardId = new ShardId(indexId.getName(), indexId.getId(), shards);

                    final Path cacheDir = Files.createDirectories(
                        CacheService.resolveSnapshotCache(randomShardPath(shardId)).resolve(snapshotUUID)
                    );

                    for (int files = 0; files < between(1, 2); files++) {
                        final CacheKey cacheKey = new CacheKey(snapshotUUID, indexId.getName(), shardId, "file_" + files);
                        final CacheFile cacheFile = cacheService.get(cacheKey, randomLongBetween(100L, buffer.length), cacheDir);

                        final CacheFile.EvictionListener listener = evictedCacheFile -> {};
                        cacheFile.acquire(listener);
                        try {
                            SortedSet<Tuple<Long, Long>> ranges = Collections.emptySortedSet();
                            while (ranges.isEmpty()) {
                                ranges = randomPopulateAndReads(cacheFile, (channel, from, to) -> {
                                    try {
                                        channel.write(ByteBuffer.wrap(buffer, Math.toIntExact(from), Math.toIntExact(to)));
                                    } catch (IOException e) {
                                        throw new AssertionError(e);
                                    }
                                });
                            }
                            cacheFiles.add(cacheFile);
                        } finally {
                            cacheFile.release(listener);
                        }
                    }
                }
            }
        }
        return cacheFiles;
    }
}
