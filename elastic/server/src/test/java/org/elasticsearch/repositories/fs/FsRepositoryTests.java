/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.repositories.fs;

import org.apache.lucene.document.Document;
import org.apache.lucene.document.Field;
import org.apache.lucene.document.SortedDocValuesField;
import org.apache.lucene.document.StringField;
import org.apache.lucene.document.TextField;
import org.apache.lucene.index.CodecReader;
import org.apache.lucene.index.FilterMergePolicy;
import org.apache.lucene.index.IndexCommit;
import org.apache.lucene.index.IndexWriter;
import org.apache.lucene.index.NoMergePolicy;
import org.apache.lucene.index.Term;
import org.apache.lucene.store.Directory;
import org.apache.lucene.tests.analysis.MockAnalyzer;
import org.apache.lucene.tests.util.TestUtil;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.IOSupplier;
import org.elasticsearch.Version;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.RepositoryMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.RecoverySource;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.ShardRoutingHelper;
import org.elasticsearch.cluster.routing.UnassignedInfo;
import org.elasticsearch.common.lucene.Lucene;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.util.MockBigArrays;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.engine.Engine;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.snapshots.IndexShardSnapshotStatus;
import org.elasticsearch.index.store.Store;
import org.elasticsearch.indices.recovery.RecoverySettings;
import org.elasticsearch.indices.recovery.RecoveryState;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.repositories.ShardGeneration;
import org.elasticsearch.repositories.ShardSnapshotResult;
import org.elasticsearch.repositories.SnapshotShardContext;
import org.elasticsearch.repositories.blobstore.BlobStoreTestUtil;
import org.elasticsearch.snapshots.Snapshot;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.test.DummyShardLock;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.IndexSettingsModule;
import org.elasticsearch.xcontent.NamedXContentRegistry;

import java.io.IOException;
import java.nio.file.Path;
import java.util.Collection;
import java.util.Comparator;
import java.util.List;

import static java.util.Collections.emptyMap;
import static java.util.Collections.emptySet;

public class FsRepositoryTests extends ESTestCase {

    public void testSnapshotAndRestore() throws IOException {
        try (Directory directory = newDirectory()) {
            Path repo = createTempDir();
            Settings settings = Settings.builder()
                .put(Environment.PATH_HOME_SETTING.getKey(), createTempDir().toAbsolutePath())
                .put(Environment.PATH_REPO_SETTING.getKey(), repo.toAbsolutePath())
                .putList(Environment.PATH_DATA_SETTING.getKey(), tmpPaths())
                .put("location", repo)
                .put("compress", randomBoolean())
                .put("chunk_size", randomIntBetween(100, 1000), ByteSizeUnit.BYTES)
                .build();

            int numDocs = indexDocs(directory);
            RepositoryMetadata metadata = new RepositoryMetadata("test", "fs", settings);
            FsRepository repository = new FsRepository(
                metadata,
                new Environment(settings, null),
                NamedXContentRegistry.EMPTY,
                BlobStoreTestUtil.mockClusterService(),
                MockBigArrays.NON_RECYCLING_INSTANCE,
                new RecoverySettings(settings, new ClusterSettings(settings, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS))
            );
            repository.start();
            final Settings indexSettings = Settings.builder().put(IndexMetadata.SETTING_INDEX_UUID, "myindexUUID").build();
            IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("myindex", indexSettings);
            ShardId shardId = new ShardId(idxSettings.getIndex(), 1);
            Store store = new Store(shardId, idxSettings, directory, new DummyShardLock(shardId));
            SnapshotId snapshotId = new SnapshotId("test", "test");
            IndexId indexId = new IndexId(idxSettings.getIndex().getName(), idxSettings.getUUID());

            IndexCommit indexCommit = Lucene.getIndexCommit(Lucene.readSegmentInfos(store.directory()), store.directory());
            final PlainActionFuture<ShardSnapshotResult> snapshot1Future = PlainActionFuture.newFuture();
            IndexShardSnapshotStatus snapshotStatus = IndexShardSnapshotStatus.newInitializing(null);
            repository.snapshotShard(
                new SnapshotShardContext(
                    store,
                    null,
                    snapshotId,
                    indexId,
                    new Engine.IndexCommitRef(indexCommit, () -> {}),
                    null,
                    snapshotStatus,
                    Version.CURRENT,
                    randomMillisUpToYear9999(),
                    snapshot1Future
                )
            );
            final ShardGeneration shardGeneration = snapshot1Future.actionGet().getGeneration();
            IndexShardSnapshotStatus.Copy snapshot1StatusCopy = snapshotStatus.asCopy();
            assertEquals(snapshot1StatusCopy.getTotalFileCount(), snapshot1StatusCopy.getIncrementalFileCount());
            Lucene.cleanLuceneIndex(directory);
            expectThrows(org.apache.lucene.index.IndexNotFoundException.class, () -> Lucene.readSegmentInfos(directory));
            DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
            ShardRouting routing = ShardRouting.newUnassigned(
                shardId,
                true,
                new RecoverySource.SnapshotRecoverySource("test", new Snapshot("foo", snapshotId), Version.CURRENT, indexId),
                new UnassignedInfo(UnassignedInfo.Reason.EXISTING_INDEX_RESTORED, ""),
                ShardRouting.Role.DEFAULT
            );
            routing = ShardRoutingHelper.initialize(routing, localNode.getId(), 0);
            RecoveryState state = new RecoveryState(routing, localNode, null);
            final PlainActionFuture<Void> restore1Future = PlainActionFuture.newFuture();
            repository.restoreShard(store, snapshotId, indexId, shardId, state, restore1Future);
            restore1Future.actionGet();

            assertTrue(state.getIndex().recoveredBytes() > 0);
            assertEquals(0, state.getIndex().reusedFileCount());
            assertEquals(indexCommit.getFileNames().size(), state.getIndex().recoveredFileCount());
            assertEquals(numDocs, Lucene.readSegmentInfos(directory).totalMaxDoc());
            deleteRandomDoc(store.directory());
            SnapshotId incSnapshotId = new SnapshotId("test1", "test1");
            IndexCommit incIndexCommit = Lucene.getIndexCommit(Lucene.readSegmentInfos(store.directory()), store.directory());
            Collection<String> commitFileNames = incIndexCommit.getFileNames();
            final PlainActionFuture<ShardSnapshotResult> snapshot2future = PlainActionFuture.newFuture();
            IndexShardSnapshotStatus snapshotStatus2 = IndexShardSnapshotStatus.newInitializing(shardGeneration);
            repository.snapshotShard(
                new SnapshotShardContext(
                    store,
                    null,
                    incSnapshotId,
                    indexId,
                    new Engine.IndexCommitRef(incIndexCommit, () -> {}),
                    null,
                    snapshotStatus2,
                    Version.CURRENT,
                    randomMillisUpToYear9999(),
                    snapshot2future
                )
            );
            snapshot2future.actionGet();
            IndexShardSnapshotStatus.Copy snapshot2statusCopy = snapshotStatus2.asCopy();
            assertEquals(2, snapshot2statusCopy.getIncrementalFileCount());
            assertEquals(commitFileNames.size(), snapshot2statusCopy.getTotalFileCount());

            // roll back to the first snap and then incrementally restore
            RecoveryState firstState = new RecoveryState(routing, localNode, null);
            final PlainActionFuture<Void> restore2Future = PlainActionFuture.newFuture();
            repository.restoreShard(store, snapshotId, indexId, shardId, firstState, restore2Future);
            restore2Future.actionGet();
            assertEquals(
                "should reuse everything except of .liv and .si",
                commitFileNames.size() - 2,
                firstState.getIndex().reusedFileCount()
            );

            RecoveryState secondState = new RecoveryState(routing, localNode, null);
            final PlainActionFuture<Void> restore3Future = PlainActionFuture.newFuture();
            repository.restoreShard(store, incSnapshotId, indexId, shardId, secondState, restore3Future);
            restore3Future.actionGet();
            assertEquals(secondState.getIndex().reusedFileCount(), commitFileNames.size() - 2);
            assertEquals(secondState.getIndex().recoveredFileCount(), 2);
            List<RecoveryState.FileDetail> recoveredFiles = secondState.getIndex()
                .fileDetails()
                .stream()
                .filter(f -> f.reused() == false)
                .sorted(Comparator.comparing(RecoveryState.FileDetail::name))
                .toList();
            assertTrue(recoveredFiles.get(0).name(), recoveredFiles.get(0).name().endsWith(".liv"));
            assertTrue(recoveredFiles.get(1).name(), recoveredFiles.get(1).name().endsWith("segments_" + incIndexCommit.getGeneration()));
        }
    }

    private void deleteRandomDoc(Directory directory) throws IOException {
        try (
            IndexWriter writer = new IndexWriter(
                directory,
                newIndexWriterConfig(random(), new MockAnalyzer(random())).setCodec(TestUtil.getDefaultCodec())
                    .setMergePolicy(new FilterMergePolicy(NoMergePolicy.INSTANCE) {
                        @Override
                        public boolean keepFullyDeletedSegment(IOSupplier<CodecReader> readerIOSupplier) {
                            return true;
                        }

                    })
            )
        ) {
            final int numDocs = writer.getDocStats().numDocs;
            writer.deleteDocuments(new Term("id", "" + randomIntBetween(0, writer.getDocStats().numDocs - 1)));
            writer.commit();
            assertEquals(writer.getDocStats().numDocs, numDocs - 1);
        }
    }

    private int indexDocs(Directory directory) throws IOException {
        try (
            IndexWriter writer = new IndexWriter(
                directory,
                newIndexWriterConfig(random(), new MockAnalyzer(random())).setCodec(TestUtil.getDefaultCodec())
            )
        ) {
            int docs = 1 + random().nextInt(100);
            for (int i = 0; i < docs; i++) {
                Document doc = new Document();
                doc.add(new StringField("id", "" + i, random().nextBoolean() ? Field.Store.YES : Field.Store.NO));
                doc.add(
                    new TextField(
                        "body",
                        TestUtil.randomRealisticUnicodeString(random()),
                        random().nextBoolean() ? Field.Store.YES : Field.Store.NO
                    )
                );
                doc.add(new SortedDocValuesField("dv", new BytesRef(TestUtil.randomRealisticUnicodeString(random()))));
                writer.addDocument(doc);
            }
            writer.commit();
            return docs;
        }
    }

}
