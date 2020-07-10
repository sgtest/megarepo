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
package org.elasticsearch.snapshots;

import org.elasticsearch.Version;
import org.elasticsearch.action.admin.cluster.snapshots.create.CreateSnapshotResponse;
import org.elasticsearch.action.admin.cluster.state.ClusterStateResponse;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateObserver;
import org.elasticsearch.cluster.SnapshotDeletionsInProgress;
import org.elasticsearch.cluster.SnapshotsInProgress;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.RepositoriesMetadata;
import org.elasticsearch.cluster.metadata.RepositoryMetadata;
import org.elasticsearch.cluster.routing.allocation.decider.EnableAllocationDecider;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.node.NodeClosedException;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryData;
import org.elasticsearch.repositories.ShardGenerations;
import org.elasticsearch.repositories.blobstore.BlobStoreRepository;
import org.elasticsearch.repositories.blobstore.BlobStoreTestUtil;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.snapshots.mockstore.MockRepository;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.threadpool.ThreadPool;
import org.junit.After;

import java.io.IOException;
import java.nio.file.FileVisitResult;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.SimpleFileVisitor;
import java.nio.file.StandardOpenOption;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Function;
import java.util.function.Predicate;

import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;

public abstract class AbstractSnapshotIntegTestCase extends ESIntegTestCase {

    private static final String OLD_VERSION_SNAPSHOT_PREFIX = "old-version-snapshot-";

    @Override
    protected Settings nodeSettings(int nodeOrdinal) {
        return Settings.builder().put(super.nodeSettings(nodeOrdinal))
            // Rebalancing is causing some checks after restore to randomly fail
            // due to https://github.com/elastic/elasticsearch/issues/9421
            .put(EnableAllocationDecider.CLUSTER_ROUTING_REBALANCE_ENABLE_SETTING.getKey(), EnableAllocationDecider.Rebalance.NONE)
            .build();
    }

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return Arrays.asList(MockRepository.Plugin.class);
    }

    @After
    public void assertConsistentHistoryInLuceneIndex() throws Exception {
        internalCluster().assertConsistentHistoryBetweenTranslogAndLuceneIndex();
    }

    @After
    public void verifyNoLeakedListeners() throws Exception {
        assertBusy(() -> {
            for (SnapshotsService snapshotsService : internalCluster().getInstances(SnapshotsService.class)) {
                assertTrue(snapshotsService.assertAllListenersResolved());
            }
        }, 30L, TimeUnit.SECONDS);
    }

    private String skipRepoConsistencyCheckReason;

    @After
    public void assertRepoConsistency() {
        if (skipRepoConsistencyCheckReason == null) {
            client().admin().cluster().prepareGetRepositories().get().repositories().forEach(repositoryMetadata -> {
                final String name = repositoryMetadata.name();
                if (repositoryMetadata.settings().getAsBoolean("readonly", false) == false) {
                    client().admin().cluster().prepareDeleteSnapshot(name, OLD_VERSION_SNAPSHOT_PREFIX + "*").get();
                    client().admin().cluster().prepareCleanupRepository(name).get();
                }
                BlobStoreTestUtil.assertRepoConsistency(internalCluster(), name);
            });
        } else {
            logger.info("--> skipped repo consistency checks because [{}]", skipRepoConsistencyCheckReason);
        }
    }

    protected void disableRepoConsistencyCheck(String reason) {
        assertNotNull(reason);
        skipRepoConsistencyCheckReason = reason;
    }

    protected RepositoryData getRepositoryData(String repository) {
        return getRepositoryData(internalCluster().getCurrentMasterNodeInstance(RepositoriesService.class).repository(repository));
    }

    protected RepositoryData getRepositoryData(Repository repository) {
        return PlainActionFuture.get(repository::getRepositoryData);
    }

    public static long getFailureCount(String repository) {
        long failureCount = 0;
        for (RepositoriesService repositoriesService :
            internalCluster().getDataOrMasterNodeInstances(RepositoriesService.class)) {
            MockRepository mockRepository = (MockRepository) repositoriesService.repository(repository);
            failureCount += mockRepository.getFailureCount();
        }
        return failureCount;
    }

    public static void assertFileCount(Path dir, int expectedCount) throws IOException {
        final List<Path> found = new ArrayList<>();
        Files.walkFileTree(dir, new SimpleFileVisitor<Path>() {
            @Override
            public FileVisitResult visitFile(Path file, BasicFileAttributes attrs) {
                found.add(file);
                return FileVisitResult.CONTINUE;
            }
        });
        assertEquals("Unexpected file count, found: [" + found + "].", expectedCount, found.size());
    }

    public static int numberOfFiles(Path dir) throws IOException {
        final AtomicInteger count = new AtomicInteger();
        Files.walkFileTree(dir, new SimpleFileVisitor<Path>() {
            @Override
            public FileVisitResult visitFile(Path file, BasicFileAttributes attrs) throws IOException {
                count.incrementAndGet();
                return FileVisitResult.CONTINUE;
            }
        });
        return count.get();
    }

    public static void stopNode(final String node) throws IOException {
        internalCluster().stopRandomNode(settings -> settings.get("node.name").equals(node));
    }

    public static void waitForBlock(String node, String repository, TimeValue timeout) throws InterruptedException {
        long start = System.currentTimeMillis();
        RepositoriesService repositoriesService = internalCluster().getInstance(RepositoriesService.class, node);
        MockRepository mockRepository = (MockRepository) repositoriesService.repository(repository);
        while (System.currentTimeMillis() - start < timeout.millis()) {
            if (mockRepository.blocked()) {
                return;
            }
            Thread.sleep(100);
        }
        fail("Timeout waiting for node [" + node + "] to be blocked");
    }

    public SnapshotInfo waitForCompletion(String repository, String snapshotName, TimeValue timeout) throws InterruptedException {
        long start = System.currentTimeMillis();
        while (System.currentTimeMillis() - start < timeout.millis()) {
            List<SnapshotInfo> snapshotInfos = client().admin().cluster().prepareGetSnapshots(repository).setSnapshots(snapshotName)
                .get().getSnapshots(repository);
            assertThat(snapshotInfos.size(), equalTo(1));
            if (snapshotInfos.get(0).state().completed()) {
                // Make sure that snapshot clean up operations are finished
                ClusterStateResponse stateResponse = client().admin().cluster().prepareState().get();
                boolean found = false;
                for (SnapshotsInProgress.Entry entry :
                    stateResponse.getState().custom(SnapshotsInProgress.TYPE, SnapshotsInProgress.EMPTY).entries()) {
                    final Snapshot curr = entry.snapshot();
                    if (curr.getRepository().equals(repository) && curr.getSnapshotId().getName().equals(snapshotName)) {
                        found = true;
                        break;
                    }
                }
                if (found == false) {
                    return snapshotInfos.get(0);
                }
            }
            Thread.sleep(100);
        }
        fail("Timeout!!!");
        return null;
    }

    public static String blockMasterFromFinalizingSnapshotOnIndexFile(final String repositoryName) {
        final String masterName = internalCluster().getMasterName();
        ((MockRepository)internalCluster().getInstance(RepositoriesService.class, masterName)
            .repository(repositoryName)).setBlockOnWriteIndexFile(true);
        return masterName;
    }

    public static void blockMasterFromDeletingIndexNFile(String repositoryName) {
        final String masterName = internalCluster().getMasterName();
        ((MockRepository)internalCluster().getInstance(RepositoriesService.class, masterName)
            .repository(repositoryName)).setBlockOnDeleteIndexFile();
    }

    public static String blockMasterFromFinalizingSnapshotOnSnapFile(final String repositoryName) {
        final String masterName = internalCluster().getMasterName();
        ((MockRepository)internalCluster().getInstance(RepositoriesService.class, masterName)
            .repository(repositoryName)).setBlockAndFailOnWriteSnapFiles(true);
        return masterName;
    }

    public static String blockNodeWithIndex(final String repositoryName, final String indexName) {
        for(String node : internalCluster().nodesInclude(indexName)) {
            ((MockRepository)internalCluster().getInstance(RepositoriesService.class, node).repository(repositoryName))
                .blockOnDataFiles(true);
            return node;
        }
        fail("No nodes for the index " + indexName + " found");
        return null;
    }

    public static void blockNodeOnAnyFiles(String repository, String nodeName) {
        ((MockRepository) internalCluster().getInstance(RepositoriesService.class, nodeName)
                .repository(repository)).setBlockOnAnyFiles(true);
    }

    public static void blockDataNode(String repository, String nodeName) {
        ((MockRepository) internalCluster().getInstance(RepositoriesService.class, nodeName)
                .repository(repository)).blockOnDataFiles(true);
    }

    public static void blockAllDataNodes(String repository) {
        for(RepositoriesService repositoriesService : internalCluster().getDataNodeInstances(RepositoriesService.class)) {
            ((MockRepository)repositoriesService.repository(repository)).blockOnDataFiles(true);
        }
    }

    public static void unblockAllDataNodes(String repository) {
        for(RepositoriesService repositoriesService : internalCluster().getDataNodeInstances(RepositoriesService.class)) {
            ((MockRepository)repositoriesService.repository(repository)).unblock();
        }
    }

    public static void waitForBlockOnAnyDataNode(String repository, TimeValue timeout) throws InterruptedException {
        final boolean blocked = waitUntil(() -> {
            for (RepositoriesService repositoriesService : internalCluster().getDataNodeInstances(RepositoriesService.class)) {
                MockRepository mockRepository = (MockRepository) repositoriesService.repository(repository);
                if (mockRepository.blocked()) {
                    return true;
                }
            }
            return false;
        }, timeout.millis(), TimeUnit.MILLISECONDS);

        assertTrue("No repository is blocked waiting on a data node", blocked);
    }

    public void unblockNode(final String repository, final String node) {
        logger.info("--> unblocking [{}] on node [{}]", repository, node);
        ((MockRepository)internalCluster().getInstance(RepositoriesService.class, node).repository(repository)).unblock();
    }

    protected void createRepository(String repoName, String type, Settings.Builder settings) {
        logger.info("--> creating repository [{}] [{}]", repoName, type);
        assertAcked(client().admin().cluster().preparePutRepository(repoName)
            .setType(type).setSettings(settings));
    }

    protected void createRepository(String repoName, String type, Path location) {
        createRepository(repoName, type, Settings.builder().put("location", location));
    }

    protected void createRepository(String repoName, String type) {
        Settings.Builder settings = Settings.builder().put("location", randomRepoPath()).put("compress", randomBoolean());
        if (rarely()) {
            settings = settings.put("chunk_size", randomIntBetween(100, 1000), ByteSizeUnit.BYTES);
        }
        createRepository(repoName, type, settings);
    }

    protected static Settings.Builder indexSettingsNoReplicas(int shards) {
        return Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, shards)
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 0);
    }

    /**
     * Randomly write an empty snapshot of an older version to an empty repository to simulate an older repository metadata format.
     */
    protected void maybeInitWithOldSnapshotVersion(String repoName, Path repoPath) throws IOException {
        if (randomBoolean() && randomBoolean()) {
            initWithSnapshotVersion(repoName, repoPath, VersionUtils.randomIndexCompatibleVersion(random()));
        }
    }

    /**
     * Workaround to simulate BwC situation: taking a snapshot without indices here so that we don't create any new version shard
     * generations (the existence of which would short-circuit checks for the repo containing old version snapshots)
     */
    protected String initWithSnapshotVersion(String repoName, Path repoPath, Version version) throws IOException {
        assertThat("This hack only works on an empty repository", getRepositoryData(repoName).getSnapshotIds(), empty());
        final String oldVersionSnapshot = OLD_VERSION_SNAPSHOT_PREFIX + version.id;
        final CreateSnapshotResponse createSnapshotResponse = client().admin().cluster()
                .prepareCreateSnapshot(repoName, oldVersionSnapshot).setIndices("does-not-exist-for-sure-*")
                .setWaitForCompletion(true).get();
        assertThat(createSnapshotResponse.getSnapshotInfo().totalShards(), is(0));

        logger.info("--> writing downgraded RepositoryData for repository metadata version [{}]", version);
        final RepositoryData repositoryData = getRepositoryData(repoName);
        final XContentBuilder jsonBuilder = JsonXContent.contentBuilder();
        repositoryData.snapshotsToXContent(jsonBuilder, version);
        final RepositoryData downgradedRepoData = RepositoryData.snapshotsFromXContent(JsonXContent.jsonXContent.createParser(
                NamedXContentRegistry.EMPTY,
                DeprecationHandler.THROW_UNSUPPORTED_OPERATION,
                Strings.toString(jsonBuilder).replace(Version.CURRENT.toString(), version.toString())),
                repositoryData.getGenId(), randomBoolean());
        Files.write(repoPath.resolve(BlobStoreRepository.INDEX_FILE_PREFIX + repositoryData.getGenId()),
                BytesReference.toBytes(BytesReference.bytes(
                        downgradedRepoData.snapshotsToXContent(XContentFactory.jsonBuilder(), version))),
                StandardOpenOption.TRUNCATE_EXISTING);
        return oldVersionSnapshot;
    }

    protected SnapshotInfo createFullSnapshot(String repoName, String snapshotName) {
        logger.info("--> creating full snapshot [{}] in [{}]", snapshotName, repoName);
        CreateSnapshotResponse createSnapshotResponse = client().admin().cluster().prepareCreateSnapshot(repoName, snapshotName)
            .setIncludeGlobalState(true)
            .setWaitForCompletion(true)
            .get();
        final SnapshotInfo snapshotInfo = createSnapshotResponse.getSnapshotInfo();
        assertThat(snapshotInfo.successfulShards(), is(snapshotInfo.totalShards()));
        assertThat(snapshotInfo.state(), is(SnapshotState.SUCCESS));
        return snapshotInfo;
    }

    protected void createIndexWithRandomDocs(String indexName, int docCount) throws InterruptedException {
        createIndex(indexName);
        ensureGreen();
        indexRandomDocs(indexName, docCount);
    }

    protected void indexRandomDocs(String index, int numdocs) throws InterruptedException {
        logger.info("--> indexing [{}] documents into [{}]", numdocs, index);
        IndexRequestBuilder[] builders = new IndexRequestBuilder[numdocs];
        for (int i = 0; i < builders.length; i++) {
            builders[i] = client().prepareIndex(index).setId(Integer.toString(i)).setSource("field1", "bar " + i);
        }
        indexRandom(true, builders);
        flushAndRefresh(index);
        assertDocCount(index, numdocs);
    }

    protected long getCountForIndex(String indexName) {
        return client().search(new SearchRequest(new SearchRequest(indexName).source(
            new SearchSourceBuilder().size(0).trackTotalHits(true)))).actionGet().getHits().getTotalHits().value;
    }

    protected void assertDocCount(String index, long count) {
        assertEquals(getCountForIndex(index), count);
    }

    /**
     * Adds a snapshot in state {@link SnapshotState#FAILED} to the given repository.
     *
     * @param repoName     repository to add snapshot to
     * @param snapshotName name for the new failed snapshot
     * @param metadata     snapshot metadata to write (as returned by {@link SnapshotInfo#userMetadata()})
     */
    protected void addBwCFailedSnapshot(String repoName, String snapshotName, Map<String, Object> metadata) throws Exception {
        final ClusterState state = client().admin().cluster().prepareState().get().getState();
        final RepositoriesMetadata repositoriesMetadata = state.metadata().custom(RepositoriesMetadata.TYPE);
        assertNotNull(repositoriesMetadata);
        final RepositoryMetadata initialRepoMetadata = repositoriesMetadata.repository(repoName);
        assertNotNull(initialRepoMetadata);
        assertThat("We can only manually insert a snapshot into a repository that does not have a generation tracked in the CS",
                initialRepoMetadata.generation(), is(RepositoryData.UNKNOWN_REPO_GEN));
        final Repository repo = internalCluster().getCurrentMasterNodeInstance(RepositoriesService.class).repository(repoName);
        final SnapshotId snapshotId = new SnapshotId(snapshotName, UUIDs.randomBase64UUID(random()));
        logger.info("--> adding old version FAILED snapshot [{}] to repository [{}]", snapshotId, repoName);
        final SnapshotInfo snapshotInfo = new SnapshotInfo(snapshotId,
                Collections.emptyList(), Collections.emptyList(),
                SnapshotState.FAILED, "failed on purpose",
                SnapshotsService.OLD_SNAPSHOT_FORMAT, 0L,0L, 0, 0, Collections.emptyList(),
                randomBoolean(), metadata);
        PlainActionFuture.<RepositoryData, Exception>get(f -> repo.finalizeSnapshot(
                ShardGenerations.EMPTY, getRepositoryData(repoName).getGenId(), state.metadata(), snapshotInfo,
                SnapshotsService.OLD_SNAPSHOT_FORMAT, Function.identity(), f));
    }

    protected void awaitNoMoreRunningOperations(String viaNode) throws Exception {
        logger.info("--> verify no more operations in the cluster state");
        awaitClusterState(viaNode, state -> state.custom(SnapshotsInProgress.TYPE, SnapshotsInProgress.EMPTY).entries().isEmpty() &&
                state.custom(SnapshotDeletionsInProgress.TYPE, SnapshotDeletionsInProgress.EMPTY).hasDeletionsInProgress() == false);
    }

    protected void awaitClusterState(String viaNode, Predicate<ClusterState> statePredicate) throws Exception {
        final ClusterService clusterService = internalCluster().getInstance(ClusterService.class, viaNode);
        final ThreadPool threadPool = internalCluster().getInstance(ThreadPool.class, viaNode);
        final ClusterStateObserver observer = new ClusterStateObserver(clusterService, logger, threadPool.getThreadContext());
        if (statePredicate.test(observer.setAndGetObservedState()) == false) {
            final PlainActionFuture<Void> future = PlainActionFuture.newFuture();
            observer.waitForNextChange(new ClusterStateObserver.Listener() {
                @Override
                public void onNewClusterState(ClusterState state) {
                    future.onResponse(null);
                }

                @Override
                public void onClusterServiceClose() {
                    future.onFailure(new NodeClosedException(clusterService.localNode()));
                }

                @Override
                public void onTimeout(TimeValue timeout) {
                    future.onFailure(new TimeoutException());
                }
            }, statePredicate);
            future.get(30L, TimeUnit.SECONDS);
        }
    }
}
