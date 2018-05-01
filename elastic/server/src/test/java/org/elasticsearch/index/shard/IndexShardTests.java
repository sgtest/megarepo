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
package org.elasticsearch.index.shard;

import org.apache.logging.log4j.Logger;
import org.apache.lucene.index.CorruptIndexException;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexCommit;
import org.apache.lucene.index.Term;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.search.TopDocs;
import org.apache.lucene.store.AlreadyClosedException;
import org.apache.lucene.store.IOContext;
import org.apache.lucene.util.Constants;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.flush.FlushRequest;
import org.elasticsearch.action.admin.indices.forcemerge.ForceMergeRequest;
import org.elasticsearch.action.admin.indices.stats.CommonStats;
import org.elasticsearch.action.admin.indices.stats.CommonStatsFlags;
import org.elasticsearch.action.admin.indices.stats.ShardStats;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MappingMetaData;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.metadata.RepositoryMetaData;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.AllocationId;
import org.elasticsearch.cluster.routing.IndexShardRoutingTable;
import org.elasticsearch.cluster.routing.RecoverySource;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.ShardRoutingHelper;
import org.elasticsearch.cluster.routing.ShardRoutingState;
import org.elasticsearch.cluster.routing.TestShardRouting;
import org.elasticsearch.cluster.routing.UnassignedInfo;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.breaker.CircuitBreaker;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.component.AbstractLifecycleComponent;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.lease.Releasable;
import org.elasticsearch.common.lease.Releasables;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.core.internal.io.IOUtils;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.VersionType;
import org.elasticsearch.index.engine.Engine;
import org.elasticsearch.index.engine.EngineException;
import org.elasticsearch.index.engine.InternalEngine;
import org.elasticsearch.index.engine.Segment;
import org.elasticsearch.index.engine.SegmentsStats;
import org.elasticsearch.index.fielddata.FieldDataStats;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.IndexFieldDataCache;
import org.elasticsearch.index.fielddata.IndexFieldDataService;
import org.elasticsearch.index.mapper.IdFieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.SourceToParse;
import org.elasticsearch.index.mapper.Uid;
import org.elasticsearch.index.seqno.SequenceNumbers;
import org.elasticsearch.index.snapshots.IndexShardSnapshotStatus;
import org.elasticsearch.index.store.Store;
import org.elasticsearch.index.store.StoreStats;
import org.elasticsearch.index.translog.TestTranslog;
import org.elasticsearch.index.translog.Translog;
import org.elasticsearch.index.translog.TranslogTests;
import org.elasticsearch.indices.IndicesQueryCache;
import org.elasticsearch.indices.breaker.NoneCircuitBreakerService;
import org.elasticsearch.indices.fielddata.cache.IndicesFieldDataCache;
import org.elasticsearch.indices.recovery.RecoveryState;
import org.elasticsearch.indices.recovery.RecoveryTarget;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryData;
import org.elasticsearch.snapshots.Snapshot;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.snapshots.SnapshotInfo;
import org.elasticsearch.snapshots.SnapshotShardFailure;
import org.elasticsearch.test.DummyShardLock;
import org.elasticsearch.test.FieldMaskingReader;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.threadpool.ThreadPool;

import java.io.IOException;
import java.nio.charset.Charset;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.BrokenBarrierException;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.CyclicBarrier;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.Semaphore;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import java.util.function.LongFunction;
import java.util.stream.Collectors;
import java.util.stream.IntStream;

import static java.util.Collections.emptyMap;
import static java.util.Collections.emptySet;
import static org.elasticsearch.cluster.routing.TestShardRouting.newShardRouting;
import static org.elasticsearch.common.lucene.Lucene.cleanLuceneIndex;
import static org.elasticsearch.common.xcontent.ToXContent.EMPTY_PARAMS;
import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.repositories.RepositoryData.EMPTY_REPO_GEN;
import static org.elasticsearch.test.hamcrest.RegexMatcher.matches;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.hasToString;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.lessThan;
import static org.hamcrest.Matchers.lessThanOrEqualTo;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

/**
 * Simple unit-test IndexShard related operations.
 */
public class IndexShardTests extends IndexShardTestCase {

    public static ShardStateMetaData load(Logger logger, Path... shardPaths) throws IOException {
        return ShardStateMetaData.FORMAT.loadLatestState(logger, NamedXContentRegistry.EMPTY, shardPaths);
    }

    public static void write(ShardStateMetaData shardStateMetaData,
                             Path... shardPaths) throws IOException {
        ShardStateMetaData.FORMAT.write(shardStateMetaData, shardPaths);
    }

    public static Engine getEngineFromShard(IndexShard shard) {
        return shard.getEngineOrNull();
    }

    public void testWriteShardState() throws Exception {
        try (NodeEnvironment env = newNodeEnvironment()) {
            ShardId id = new ShardId("foo", "fooUUID", 1);
            boolean primary = randomBoolean();
            AllocationId allocationId = randomBoolean() ? null : randomAllocationId();
            ShardStateMetaData state1 = new ShardStateMetaData(primary, "fooUUID", allocationId);
            write(state1, env.availableShardPaths(id));
            ShardStateMetaData shardStateMetaData = load(logger, env.availableShardPaths(id));
            assertEquals(shardStateMetaData, state1);

            ShardStateMetaData state2 = new ShardStateMetaData(primary, "fooUUID", allocationId);
            write(state2, env.availableShardPaths(id));
            shardStateMetaData = load(logger, env.availableShardPaths(id));
            assertEquals(shardStateMetaData, state1);

            ShardStateMetaData state3 = new ShardStateMetaData(primary, "fooUUID", allocationId);
            write(state3, env.availableShardPaths(id));
            shardStateMetaData = load(logger, env.availableShardPaths(id));
            assertEquals(shardStateMetaData, state3);
            assertEquals("fooUUID", state3.indexUUID);
        }
    }

    public void testPersistenceStateMetadataPersistence() throws Exception {
        IndexShard shard = newStartedShard();
        final Path shardStatePath = shard.shardPath().getShardStatePath();
        ShardStateMetaData shardStateMetaData = load(logger, shardStatePath);
        assertEquals(getShardStateMetadata(shard), shardStateMetaData);
        ShardRouting routing = shard.shardRouting;
        IndexShardTestCase.updateRoutingEntry(shard, routing);

        shardStateMetaData = load(logger, shardStatePath);
        assertEquals(shardStateMetaData, getShardStateMetadata(shard));
        assertEquals(shardStateMetaData,
            new ShardStateMetaData(routing.primary(), shard.indexSettings().getUUID(), routing.allocationId()));

        routing = TestShardRouting.relocate(shard.shardRouting, "some node", 42L);
        IndexShardTestCase.updateRoutingEntry(shard, routing);
        shardStateMetaData = load(logger, shardStatePath);
        assertEquals(shardStateMetaData, getShardStateMetadata(shard));
        assertEquals(shardStateMetaData,
            new ShardStateMetaData(routing.primary(), shard.indexSettings().getUUID(), routing.allocationId()));
        closeShards(shard);
    }

    public void testFailShard() throws Exception {
        IndexShard shard = newStartedShard();
        final ShardPath shardPath = shard.shardPath();
        assertNotNull(shardPath);
        // fail shard
        shard.failShard("test shard fail", new CorruptIndexException("", ""));
        closeShards(shard);
        // check state file still exists
        ShardStateMetaData shardStateMetaData = load(logger, shardPath.getShardStatePath());
        assertEquals(shardStateMetaData, getShardStateMetadata(shard));
        // but index can't be opened for a failed shard
        assertThat("store index should be corrupted", Store.canOpenIndex(logger, shardPath.resolveIndex(), shard.shardId(),
            (shardId, lockTimeoutMS) -> new DummyShardLock(shardId)),
            equalTo(false));
    }

    ShardStateMetaData getShardStateMetadata(IndexShard shard) {
        ShardRouting shardRouting = shard.routingEntry();
        if (shardRouting == null) {
            return null;
        } else {
            return new ShardStateMetaData(shardRouting.primary(), shard.indexSettings().getUUID(), shardRouting.allocationId());
        }
    }

    private AllocationId randomAllocationId() {
        AllocationId allocationId = AllocationId.newInitializing();
        if (randomBoolean()) {
            allocationId = AllocationId.newRelocation(allocationId);
        }
        return allocationId;
    }

    public void testShardStateMetaHashCodeEquals() {
        AllocationId allocationId = randomBoolean() ? null : randomAllocationId();
        ShardStateMetaData meta = new ShardStateMetaData(randomBoolean(),
            randomRealisticUnicodeOfCodepointLengthBetween(1, 10), allocationId);

        assertEquals(meta, new ShardStateMetaData(meta.primary, meta.indexUUID, meta.allocationId));
        assertEquals(meta.hashCode(),
            new ShardStateMetaData(meta.primary, meta.indexUUID, meta.allocationId).hashCode());

        assertFalse(meta.equals(new ShardStateMetaData(!meta.primary, meta.indexUUID, meta.allocationId)));
        assertFalse(meta.equals(new ShardStateMetaData(!meta.primary, meta.indexUUID + "foo", meta.allocationId)));
        assertFalse(meta.equals(new ShardStateMetaData(!meta.primary, meta.indexUUID + "foo", randomAllocationId())));
        Set<Integer> hashCodes = new HashSet<>();
        for (int i = 0; i < 30; i++) { // just a sanity check that we impl hashcode
            allocationId = randomBoolean() ? null : randomAllocationId();
            meta = new ShardStateMetaData(randomBoolean(),
                randomRealisticUnicodeOfCodepointLengthBetween(1, 10), allocationId);
            hashCodes.add(meta.hashCode());
        }
        assertTrue("more than one unique hashcode expected but got: " + hashCodes.size(), hashCodes.size() > 1);

    }

    public void testClosesPreventsNewOperations() throws InterruptedException, ExecutionException, IOException {
        IndexShard indexShard = newStartedShard();
        closeShards(indexShard);
        assertThat(indexShard.getActiveOperationsCount(), equalTo(0));
        try {
            indexShard.acquirePrimaryOperationPermit(null, ThreadPool.Names.WRITE, "");
            fail("we should not be able to increment anymore");
        } catch (IndexShardClosedException e) {
            // expected
        }
        try {
            indexShard.acquireReplicaOperationPermit(indexShard.getPrimaryTerm(), SequenceNumbers.UNASSIGNED_SEQ_NO, null,
                ThreadPool.Names.WRITE, "");
            fail("we should not be able to increment anymore");
        } catch (IndexShardClosedException e) {
            // expected
        }
    }

    public void testRejectOperationPermitWithHigherTermWhenNotStarted() throws IOException {
        IndexShard indexShard = newShard(false);
        expectThrows(IndexShardNotStartedException.class, () ->
            indexShard.acquireReplicaOperationPermit(indexShard.getPrimaryTerm() + randomIntBetween(1, 100),
                SequenceNumbers.UNASSIGNED_SEQ_NO, null, ThreadPool.Names.WRITE, ""));
        closeShards(indexShard);
    }

    public void testPrimaryPromotionDelaysOperations() throws IOException, BrokenBarrierException, InterruptedException {
        final IndexShard indexShard = newStartedShard(false);

        final int operations = scaledRandomIntBetween(1, 64);
        final CyclicBarrier barrier = new CyclicBarrier(1 + operations);
        final CountDownLatch latch = new CountDownLatch(operations);
        final CountDownLatch operationLatch = new CountDownLatch(1);
        final List<Thread> threads = new ArrayList<>();
        for (int i = 0; i < operations; i++) {
            final String id = "t_" + i;
            final Thread thread = new Thread(() -> {
                try {
                    barrier.await();
                } catch (final BrokenBarrierException | InterruptedException e) {
                    throw new RuntimeException(e);
                }
                indexShard.acquireReplicaOperationPermit(
                        indexShard.getPrimaryTerm(),
                        indexShard.getGlobalCheckpoint(),
                        new ActionListener<Releasable>() {
                            @Override
                            public void onResponse(Releasable releasable) {
                                latch.countDown();
                                try {
                                    operationLatch.await();
                                } catch (final InterruptedException e) {
                                    throw new RuntimeException(e);
                                }
                                releasable.close();
                            }

                            @Override
                            public void onFailure(Exception e) {
                                throw new RuntimeException(e);
                            }
                        },
                        ThreadPool.Names.WRITE, id);
            });
            thread.start();
            threads.add(thread);
        }

        barrier.await();
        latch.await();

        // promote the replica
        final ShardRouting replicaRouting = indexShard.routingEntry();
        final ShardRouting primaryRouting =
                newShardRouting(
                        replicaRouting.shardId(),
                        replicaRouting.currentNodeId(),
                        null,
                        true,
                        ShardRoutingState.STARTED,
                        replicaRouting.allocationId());
        indexShard.updateShardState(primaryRouting, indexShard.getPrimaryTerm() + 1, (shard, listener) -> {},
            0L, Collections.singleton(primaryRouting.allocationId().getId()),
            new IndexShardRoutingTable.Builder(primaryRouting.shardId()).addShard(primaryRouting).build(),
            Collections.emptySet());

        final int delayedOperations = scaledRandomIntBetween(1, 64);
        final CyclicBarrier delayedOperationsBarrier = new CyclicBarrier(1 + delayedOperations);
        final CountDownLatch delayedOperationsLatch = new CountDownLatch(delayedOperations);
        final AtomicLong counter = new AtomicLong();
        final List<Thread> delayedThreads = new ArrayList<>();
        for (int i = 0; i < delayedOperations; i++) {
            final String id = "d_" + i;
            final Thread thread = new Thread(() -> {
                try {
                    delayedOperationsBarrier.await();
                } catch (final BrokenBarrierException | InterruptedException e) {
                    throw new RuntimeException(e);
                }
                indexShard.acquirePrimaryOperationPermit(
                        new ActionListener<Releasable>() {
                            @Override
                            public void onResponse(Releasable releasable) {
                                counter.incrementAndGet();
                                releasable.close();
                                delayedOperationsLatch.countDown();
                            }

                            @Override
                            public void onFailure(Exception e) {
                                throw new RuntimeException(e);
                            }
                        },
                        ThreadPool.Names.WRITE, id);
            });
            thread.start();
            delayedThreads.add(thread);
        }

        delayedOperationsBarrier.await();

        assertThat(counter.get(), equalTo(0L));

        operationLatch.countDown();
        for (final Thread thread : threads) {
            thread.join();
        }

        delayedOperationsLatch.await();

        assertThat(counter.get(), equalTo((long) delayedOperations));

        for (final Thread thread : delayedThreads) {
            thread.join();
        }

        closeShards(indexShard);
    }

    /**
     * This test makes sure that people can use the shard routing entry to check whether a shard was already promoted to
     * a primary. Concretely this means, that when we publish the routing entry via {@link IndexShard#routingEntry()} the following
     * should have happened
     * 1) Internal state (ala ReplicationTracker) have been updated
     * 2) Primary term is set to the new term
     */
    public void testPublishingOrderOnPromotion() throws IOException, BrokenBarrierException, InterruptedException {
        final IndexShard indexShard = newStartedShard(false);
        final long promotedTerm = indexShard.getPrimaryTerm() + 1;
        final CyclicBarrier barrier = new CyclicBarrier(2);
        final AtomicBoolean stop = new AtomicBoolean();
        final Thread thread = new Thread(() -> {
            try {
                barrier.await();
            } catch (final BrokenBarrierException | InterruptedException e) {
                throw new RuntimeException(e);
            }
            while(stop.get() == false) {
                if (indexShard.routingEntry().primary()) {
                    assertThat(indexShard.getPrimaryTerm(), equalTo(promotedTerm));
                    assertThat(indexShard.getReplicationGroup(), notNullValue());
                }
            }
        });
        thread.start();

        final ShardRouting replicaRouting = indexShard.routingEntry();
        final ShardRouting primaryRouting = newShardRouting(replicaRouting.shardId(), replicaRouting.currentNodeId(), null, true,
            ShardRoutingState.STARTED, replicaRouting.allocationId());


        final Set<String> inSyncAllocationIds = Collections.singleton(primaryRouting.allocationId().getId());
        final IndexShardRoutingTable routingTable =
            new IndexShardRoutingTable.Builder(primaryRouting.shardId()).addShard(primaryRouting).build();
        barrier.await();
        // promote the replica
        indexShard.updateShardState(primaryRouting, promotedTerm, (shard, listener) -> {}, 0L, inSyncAllocationIds, routingTable,
            Collections.emptySet());

        stop.set(true);
        thread.join();
        closeShards(indexShard);
    }


    public void testPrimaryFillsSeqNoGapsOnPromotion() throws Exception {
        final IndexShard indexShard = newStartedShard(false);

        // most of the time this is large enough that most of the time there will be at least one gap
        final int operations = 1024 - scaledRandomIntBetween(0, 1024);
        final Result result = indexOnReplicaWithGaps(indexShard, operations, Math.toIntExact(SequenceNumbers.NO_OPS_PERFORMED));

        final int maxSeqNo = result.maxSeqNo;
        final boolean gap = result.gap;

        // promote the replica
        final ShardRouting replicaRouting = indexShard.routingEntry();
        final ShardRouting primaryRouting =
                newShardRouting(
                        replicaRouting.shardId(),
                        replicaRouting.currentNodeId(),
                        null,
                        true,
                        ShardRoutingState.STARTED,
                        replicaRouting.allocationId());
        indexShard.updateShardState(primaryRouting, indexShard.getPrimaryTerm() + 1, (shard, listener) -> {},
            0L, Collections.singleton(primaryRouting.allocationId().getId()),
            new IndexShardRoutingTable.Builder(primaryRouting.shardId()).addShard(primaryRouting).build(), Collections.emptySet());

        /*
         * This operation completing means that the delay operation executed as part of increasing the primary term has completed and the
         * gaps are filled.
         */
        final CountDownLatch latch = new CountDownLatch(1);
        indexShard.acquirePrimaryOperationPermit(
                new ActionListener<Releasable>() {
                    @Override
                    public void onResponse(Releasable releasable) {
                        releasable.close();
                        latch.countDown();
                    }

                    @Override
                    public void onFailure(Exception e) {
                        throw new RuntimeException(e);
                    }
                },
                ThreadPool.Names.GENERIC, "");

        latch.await();
        assertThat(indexShard.getLocalCheckpoint(), equalTo((long) maxSeqNo));
        closeShards(indexShard);
    }

    public void testPrimaryPromotionRollsGeneration() throws Exception {
        final IndexShard indexShard = newStartedShard(false);

        final long currentTranslogGeneration = getTranslog(indexShard).getGeneration().translogFileGeneration;

        // promote the replica
        final ShardRouting replicaRouting = indexShard.routingEntry();
        final long newPrimaryTerm = indexShard.getPrimaryTerm() + between(1, 10000);
        final ShardRouting primaryRouting =
                newShardRouting(
                        replicaRouting.shardId(),
                        replicaRouting.currentNodeId(),
                        null,
                        true,
                        ShardRoutingState.STARTED,
                        replicaRouting.allocationId());
        indexShard.updateShardState(primaryRouting, newPrimaryTerm, (shard, listener) -> {},
                0L, Collections.singleton(primaryRouting.allocationId().getId()),
                new IndexShardRoutingTable.Builder(primaryRouting.shardId()).addShard(primaryRouting).build(), Collections.emptySet());

        /*
         * This operation completing means that the delay operation executed as part of increasing the primary term has completed and the
         * translog generation has rolled.
         */
        final CountDownLatch latch = new CountDownLatch(1);
        indexShard.acquirePrimaryOperationPermit(
                new ActionListener<Releasable>() {
                    @Override
                    public void onResponse(Releasable releasable) {
                        releasable.close();
                        latch.countDown();
                    }

                    @Override
                    public void onFailure(Exception e) {
                        throw new RuntimeException(e);
                    }
                },
                ThreadPool.Names.GENERIC, "");

        latch.await();
        assertThat(getTranslog(indexShard).getGeneration().translogFileGeneration, equalTo(currentTranslogGeneration + 1));
        assertThat(TestTranslog.getCurrentTerm(getTranslog(indexShard)), equalTo(newPrimaryTerm));

        closeShards(indexShard);
    }

    public void testOperationPermitsOnPrimaryShards() throws InterruptedException, ExecutionException, IOException {
        final ShardId shardId = new ShardId("test", "_na_", 0);
        final IndexShard indexShard;

        if (randomBoolean()) {
            // relocation target
            indexShard = newShard(newShardRouting(shardId, "local_node", "other node",
                true, ShardRoutingState.INITIALIZING, AllocationId.newRelocation(AllocationId.newInitializing())));
        } else if (randomBoolean()) {
            // simulate promotion
            indexShard = newStartedShard(false);
            ShardRouting replicaRouting = indexShard.routingEntry();
            ShardRouting primaryRouting = newShardRouting(replicaRouting.shardId(), replicaRouting.currentNodeId(), null,
                true, ShardRoutingState.STARTED, replicaRouting.allocationId());
            final long newPrimaryTerm = indexShard.getPrimaryTerm() + between(1, 1000);
            indexShard.updateShardState(primaryRouting, newPrimaryTerm, (shard, listener) -> {
                    assertThat(TestTranslog.getCurrentTerm(getTranslog(indexShard)), equalTo(newPrimaryTerm));
                }, 0L,
                Collections.singleton(indexShard.routingEntry().allocationId().getId()),
                new IndexShardRoutingTable.Builder(indexShard.shardId()).addShard(primaryRouting).build(),
                Collections.emptySet());
        } else {
            indexShard = newStartedShard(true);
        }
        final long primaryTerm = indexShard.getPrimaryTerm();
        assertEquals(0, indexShard.getActiveOperationsCount());
        if (indexShard.routingEntry().isRelocationTarget() == false) {
            try {
                indexShard.acquireReplicaOperationPermit(primaryTerm, indexShard.getGlobalCheckpoint(), null, ThreadPool.Names.WRITE, "");
                fail("shard shouldn't accept operations as replica");
            } catch (IllegalStateException ignored) {

            }
        }
        Releasable operation1 = acquirePrimaryOperationPermitBlockingly(indexShard);
        assertEquals(1, indexShard.getActiveOperationsCount());
        Releasable operation2 = acquirePrimaryOperationPermitBlockingly(indexShard);
        assertEquals(2, indexShard.getActiveOperationsCount());

        Releasables.close(operation1, operation2);
        assertEquals(0, indexShard.getActiveOperationsCount());

        closeShards(indexShard);
    }

    private Releasable acquirePrimaryOperationPermitBlockingly(IndexShard indexShard) throws ExecutionException, InterruptedException {
        PlainActionFuture<Releasable> fut = new PlainActionFuture<>();
        indexShard.acquirePrimaryOperationPermit(fut, ThreadPool.Names.WRITE, "");
        return fut.get();
    }

    private Releasable acquireReplicaOperationPermitBlockingly(IndexShard indexShard, long opPrimaryTerm)
        throws ExecutionException, InterruptedException {
        PlainActionFuture<Releasable> fut = new PlainActionFuture<>();
        indexShard.acquireReplicaOperationPermit(opPrimaryTerm, indexShard.getGlobalCheckpoint(), fut, ThreadPool.Names.WRITE, "");
        return fut.get();
    }

    public void testOperationPermitOnReplicaShards() throws Exception {
        final ShardId shardId = new ShardId("test", "_na_", 0);
        final IndexShard indexShard;
        final boolean engineClosed;
        switch (randomInt(2)) {
            case 0:
                // started replica
                indexShard = newStartedShard(false);
                engineClosed = false;
                break;
            case 1: {
                // initializing replica / primary
                final boolean relocating = randomBoolean();
                ShardRouting routing = newShardRouting(shardId, "local_node",
                    relocating ? "sourceNode" : null,
                    relocating ? randomBoolean() : false,
                    ShardRoutingState.INITIALIZING,
                    relocating ? AllocationId.newRelocation(AllocationId.newInitializing()) : AllocationId.newInitializing());
                indexShard = newShard(routing);
                engineClosed = true;
                break;
            }
            case 2: {
                // relocation source
                indexShard = newStartedShard(true);
                ShardRouting routing = indexShard.routingEntry();
                routing = newShardRouting(routing.shardId(), routing.currentNodeId(), "otherNode",
                    true, ShardRoutingState.RELOCATING, AllocationId.newRelocation(routing.allocationId()));
                IndexShardTestCase.updateRoutingEntry(indexShard, routing);
                indexShard.relocated(primaryContext -> {});
                engineClosed = false;
                break;
            }
            default:
                throw new UnsupportedOperationException("get your numbers straight");

        }
        final ShardRouting shardRouting = indexShard.routingEntry();
        logger.info("shard routing to {}", shardRouting);

        assertEquals(0, indexShard.getActiveOperationsCount());
        if (shardRouting.primary() == false) {
            final IllegalStateException e =
                    expectThrows(IllegalStateException.class,
                        () -> indexShard.acquirePrimaryOperationPermit(null, ThreadPool.Names.WRITE, ""));
            assertThat(e, hasToString(containsString("shard " + shardRouting + " is not a primary")));
        }

        final long primaryTerm = indexShard.getPrimaryTerm();
        final long translogGen = engineClosed ? -1 : getTranslog(indexShard).getGeneration().translogFileGeneration;

        final Releasable operation1;
        final Releasable operation2;
        if (engineClosed == false) {
            operation1 = acquireReplicaOperationPermitBlockingly(indexShard, primaryTerm);
            assertEquals(1, indexShard.getActiveOperationsCount());
            operation2 = acquireReplicaOperationPermitBlockingly(indexShard, primaryTerm);
            assertEquals(2, indexShard.getActiveOperationsCount());
        } else {
            operation1 = null;
            operation2 = null;
        }

        {
            final AtomicBoolean onResponse = new AtomicBoolean();
            final AtomicBoolean onFailure = new AtomicBoolean();
            final AtomicReference<Exception> onFailureException = new AtomicReference<>();
            ActionListener<Releasable> onLockAcquired = new ActionListener<Releasable>() {
                @Override
                public void onResponse(Releasable releasable) {
                    onResponse.set(true);
                }

                @Override
                public void onFailure(Exception e) {
                    onFailure.set(true);
                    onFailureException.set(e);
                }
            };

            indexShard.acquireReplicaOperationPermit(primaryTerm - 1, SequenceNumbers.UNASSIGNED_SEQ_NO, onLockAcquired,
                ThreadPool.Names.WRITE, "");

            assertFalse(onResponse.get());
            assertTrue(onFailure.get());
            assertThat(onFailureException.get(), instanceOf(IllegalStateException.class));
            assertThat(
                    onFailureException.get(), hasToString(containsString("operation primary term [" + (primaryTerm - 1) + "] is too old")));
        }

        {
            final AtomicBoolean onResponse = new AtomicBoolean();
            final AtomicReference<Exception> onFailure = new AtomicReference<>();
            final CyclicBarrier barrier = new CyclicBarrier(2);
            final long newPrimaryTerm = primaryTerm + 1 + randomInt(20);
            if (engineClosed == false) {
                assertThat(indexShard.getLocalCheckpoint(), equalTo(SequenceNumbers.NO_OPS_PERFORMED));
                assertThat(indexShard.getGlobalCheckpoint(), equalTo(SequenceNumbers.NO_OPS_PERFORMED));
            }
            final long newGlobalCheckPoint;
            if (engineClosed || randomBoolean()) {
                newGlobalCheckPoint = SequenceNumbers.NO_OPS_PERFORMED;
            } else {
                long localCheckPoint = indexShard.getGlobalCheckpoint() + randomInt(100);
                // advance local checkpoint
                for (int i = 0; i <= localCheckPoint; i++) {
                    indexShard.markSeqNoAsNoop(i, "dummy doc");
                }
                newGlobalCheckPoint = randomIntBetween((int) indexShard.getGlobalCheckpoint(), (int) localCheckPoint);
            }
            final long expectedLocalCheckpoint;
            if (newGlobalCheckPoint == SequenceNumbers.UNASSIGNED_SEQ_NO) {
                expectedLocalCheckpoint = SequenceNumbers.NO_OPS_PERFORMED;
            } else {
                expectedLocalCheckpoint = newGlobalCheckPoint;
            }
            // but you can not increment with a new primary term until the operations on the older primary term complete
            final Thread thread = new Thread(() -> {
                try {
                    barrier.await();
                } catch (final BrokenBarrierException | InterruptedException e) {
                    throw new RuntimeException(e);
                }
                ActionListener<Releasable> listener = new ActionListener<Releasable>() {
                    @Override
                    public void onResponse(Releasable releasable) {
                        assertThat(indexShard.getPrimaryTerm(), equalTo(newPrimaryTerm));
                        assertThat(TestTranslog.getCurrentTerm(getTranslog(indexShard)), equalTo(newPrimaryTerm));
                        assertThat(indexShard.getLocalCheckpoint(), equalTo(expectedLocalCheckpoint));
                        assertThat(indexShard.getGlobalCheckpoint(), equalTo(newGlobalCheckPoint));
                        onResponse.set(true);
                        releasable.close();
                        finish();
                    }

                    @Override
                    public void onFailure(Exception e) {
                        onFailure.set(e);
                        finish();
                    }

                    private void finish() {
                        try {
                            barrier.await();
                        } catch (final BrokenBarrierException | InterruptedException e) {
                            throw new RuntimeException(e);
                        }
                    }
                };
                try {
                    indexShard.acquireReplicaOperationPermit(
                        newPrimaryTerm,
                        newGlobalCheckPoint,
                        listener,
                        ThreadPool.Names.SAME, "");
                } catch (Exception e) {
                    listener.onFailure(e);
                }
            });
            thread.start();
            barrier.await();
            if (indexShard.state() == IndexShardState.CREATED || indexShard.state() == IndexShardState.RECOVERING) {
                barrier.await();
                assertThat(indexShard.getPrimaryTerm(), equalTo(primaryTerm));
                assertFalse(onResponse.get());
                assertThat(onFailure.get(), instanceOf(IndexShardNotStartedException.class));
                Releasables.close(operation1);
                Releasables.close(operation2);
            } else {
                // our operation should be blocked until the previous operations complete
                assertFalse(onResponse.get());
                assertNull(onFailure.get());
                assertThat(indexShard.getPrimaryTerm(), equalTo(primaryTerm));
                assertThat(TestTranslog.getCurrentTerm(getTranslog(indexShard)), equalTo(primaryTerm));
                Releasables.close(operation1);
                // our operation should still be blocked
                assertFalse(onResponse.get());
                assertNull(onFailure.get());
                assertThat(indexShard.getPrimaryTerm(), equalTo(primaryTerm));
                assertThat(TestTranslog.getCurrentTerm(getTranslog(indexShard)), equalTo(primaryTerm));
                Releasables.close(operation2);
                barrier.await();
                // now lock acquisition should have succeeded
                assertThat(indexShard.getPrimaryTerm(), equalTo(newPrimaryTerm));
                assertThat(TestTranslog.getCurrentTerm(getTranslog(indexShard)), equalTo(newPrimaryTerm));
                if (engineClosed) {
                    assertFalse(onResponse.get());
                    assertThat(onFailure.get(), instanceOf(AlreadyClosedException.class));
                } else {
                    assertTrue(onResponse.get());
                    assertNull(onFailure.get());
                    assertThat(getTranslog(indexShard).getGeneration().translogFileGeneration, equalTo(translogGen + 1));
                    assertThat(indexShard.getLocalCheckpoint(), equalTo(expectedLocalCheckpoint));
                    assertThat(indexShard.getGlobalCheckpoint(), equalTo(newGlobalCheckPoint));
                }
            }
            thread.join();
            assertEquals(0, indexShard.getActiveOperationsCount());
        }

        closeShards(indexShard);
    }

    public void testGlobalCheckpointSync() throws IOException {
        // create the primary shard with a callback that sets a boolean when the global checkpoint sync is invoked
        final ShardId shardId = new ShardId("index", "_na_", 0);
        final ShardRouting shardRouting =
                TestShardRouting.newShardRouting(
                        shardId,
                        randomAlphaOfLength(8),
                        true,
                        ShardRoutingState.INITIALIZING,
                        RecoverySource.StoreRecoverySource.EMPTY_STORE_INSTANCE);
        final Settings settings = Settings.builder()
                .put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 2)
                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                .build();
        final IndexMetaData.Builder indexMetadata = IndexMetaData.builder(shardRouting.getIndexName()).settings(settings).primaryTerm(0, 1);
        final AtomicBoolean synced = new AtomicBoolean();
        final IndexShard primaryShard = newShard(shardRouting, indexMetadata.build(), null, null, () -> { synced.set(true); });
        // add a replica
        recoverShardFromStore(primaryShard);
        final IndexShard replicaShard = newShard(shardId, false);
        recoverReplica(replicaShard, primaryShard);
        final int maxSeqNo = randomIntBetween(0, 128);
        for (int i = 0; i <= maxSeqNo; i++) {
            primaryShard.getEngine().getLocalCheckpointTracker().generateSeqNo();
        }
        final long checkpoint = rarely() ? maxSeqNo - scaledRandomIntBetween(0, maxSeqNo) : maxSeqNo;

        // set up local checkpoints on the shard copies
        primaryShard.updateLocalCheckpointForShard(shardRouting.allocationId().getId(), checkpoint);
        final int replicaLocalCheckpoint = randomIntBetween(0, Math.toIntExact(checkpoint));
        final String replicaAllocationId = replicaShard.routingEntry().allocationId().getId();
        primaryShard.updateLocalCheckpointForShard(replicaAllocationId, replicaLocalCheckpoint);

        // initialize the local knowledge on the primary of the global checkpoint on the replica shard
        final int replicaGlobalCheckpoint =
                randomIntBetween(Math.toIntExact(SequenceNumbers.NO_OPS_PERFORMED), Math.toIntExact(primaryShard.getGlobalCheckpoint()));
        primaryShard.updateGlobalCheckpointForShard(replicaAllocationId, replicaGlobalCheckpoint);

        // simulate a background maybe sync; it should only run if the knowledge on the replica of the global checkpoint lags the primary
        primaryShard.maybeSyncGlobalCheckpoint("test");
        assertThat(
                synced.get(),
                equalTo(maxSeqNo == primaryShard.getGlobalCheckpoint() && (replicaGlobalCheckpoint < checkpoint)));

        // simulate that the background sync advanced the global checkpoint on the replica
        primaryShard.updateGlobalCheckpointForShard(replicaAllocationId, primaryShard.getGlobalCheckpoint());

        // reset our boolean so that we can assert after another simulated maybe sync
        synced.set(false);

        primaryShard.maybeSyncGlobalCheckpoint("test");

        // this time there should not be a sync since all the replica copies are caught up with the primary
        assertFalse(synced.get());

        closeShards(replicaShard, primaryShard);
    }

    public void testRestoreLocalCheckpointTrackerFromTranslogOnPromotion() throws IOException, InterruptedException {
        final IndexShard indexShard = newStartedShard(false);
        final int operations = 1024 - scaledRandomIntBetween(0, 1024);
        indexOnReplicaWithGaps(indexShard, operations, Math.toIntExact(SequenceNumbers.NO_OPS_PERFORMED));

        final long maxSeqNo = indexShard.seqNoStats().getMaxSeqNo();
        final long globalCheckpointOnReplica = SequenceNumbers.UNASSIGNED_SEQ_NO;
        randomIntBetween(
                Math.toIntExact(SequenceNumbers.UNASSIGNED_SEQ_NO),
                Math.toIntExact(indexShard.getLocalCheckpoint()));
        indexShard.updateGlobalCheckpointOnReplica(globalCheckpointOnReplica, "test");

        final int globalCheckpoint =
                randomIntBetween(
                        Math.toIntExact(SequenceNumbers.UNASSIGNED_SEQ_NO),
                        Math.toIntExact(indexShard.getLocalCheckpoint()));

        final CountDownLatch latch = new CountDownLatch(1);
        indexShard.acquireReplicaOperationPermit(
                indexShard.getPrimaryTerm() + 1,
                globalCheckpoint,
                new ActionListener<Releasable>() {
                    @Override
                    public void onResponse(Releasable releasable) {
                        releasable.close();
                        latch.countDown();
                    }

                    @Override
                    public void onFailure(Exception e) {

                    }
                },
                ThreadPool.Names.SAME, "");

        latch.await();

        final ShardRouting newRouting = indexShard.routingEntry().moveActiveReplicaToPrimary();
        final CountDownLatch resyncLatch = new CountDownLatch(1);
        indexShard.updateShardState(
                newRouting,
                indexShard.getPrimaryTerm() + 1,
                (s, r) -> resyncLatch.countDown(),
                1L,
                Collections.singleton(newRouting.allocationId().getId()),
                new IndexShardRoutingTable.Builder(newRouting.shardId()).addShard(newRouting).build(),
                Collections.emptySet());
        resyncLatch.await();
        assertThat(indexShard.getLocalCheckpoint(), equalTo(maxSeqNo));
        assertThat(indexShard.seqNoStats().getMaxSeqNo(), equalTo(maxSeqNo));

        closeShards(indexShard);
    }

    public void testThrowBackLocalCheckpointOnReplica() throws IOException, InterruptedException {
        final IndexShard indexShard = newStartedShard(false);

        // most of the time this is large enough that most of the time there will be at least one gap
        final int operations = 1024 - scaledRandomIntBetween(0, 1024);
        indexOnReplicaWithGaps(indexShard, operations, Math.toIntExact(SequenceNumbers.NO_OPS_PERFORMED));

        final long globalCheckpointOnReplica =
                randomIntBetween(
                        Math.toIntExact(SequenceNumbers.UNASSIGNED_SEQ_NO),
                        Math.toIntExact(indexShard.getLocalCheckpoint()));
        indexShard.updateGlobalCheckpointOnReplica(globalCheckpointOnReplica, "test");

        final int globalCheckpoint =
                randomIntBetween(
                        Math.toIntExact(SequenceNumbers.UNASSIGNED_SEQ_NO),
                        Math.toIntExact(indexShard.getLocalCheckpoint()));
        final CountDownLatch latch = new CountDownLatch(1);
        indexShard.acquireReplicaOperationPermit(
                indexShard.primaryTerm + 1,
                globalCheckpoint,
                new ActionListener<Releasable>() {
                    @Override
                    public void onResponse(final Releasable releasable) {
                        releasable.close();
                        latch.countDown();
                    }

                    @Override
                    public void onFailure(final Exception e) {

                    }
                },
                ThreadPool.Names.SAME, "");

        latch.await();
        if (globalCheckpointOnReplica == SequenceNumbers.UNASSIGNED_SEQ_NO
                && globalCheckpoint == SequenceNumbers.UNASSIGNED_SEQ_NO) {
            assertThat(indexShard.getLocalCheckpoint(), equalTo(SequenceNumbers.NO_OPS_PERFORMED));
        } else {
            assertThat(indexShard.getLocalCheckpoint(), equalTo(Math.max(globalCheckpoint, globalCheckpointOnReplica)));
        }

        // ensure that after the local checkpoint throw back and indexing again, the local checkpoint advances
        final Result result = indexOnReplicaWithGaps(indexShard, operations, Math.toIntExact(indexShard.getLocalCheckpoint()));
        assertThat(indexShard.getLocalCheckpoint(), equalTo((long) result.localCheckpoint));

        closeShards(indexShard);
    }

    public void testConcurrentTermIncreaseOnReplicaShard() throws BrokenBarrierException, InterruptedException, IOException {
        final IndexShard indexShard = newStartedShard(false);

        final CyclicBarrier barrier = new CyclicBarrier(3);
        final CountDownLatch latch = new CountDownLatch(2);

        final long primaryTerm = indexShard.getPrimaryTerm();
        final AtomicLong counter = new AtomicLong();
        final AtomicReference<Exception> onFailure = new AtomicReference<>();

        final LongFunction<Runnable> function = increment -> () -> {
            assert increment > 0;
            try {
                barrier.await();
            } catch (final BrokenBarrierException | InterruptedException e) {
                throw new RuntimeException(e);
            }
            indexShard.acquireReplicaOperationPermit(
                    primaryTerm + increment,
                    indexShard.getGlobalCheckpoint(),
                    new ActionListener<Releasable>() {
                        @Override
                        public void onResponse(Releasable releasable) {
                            counter.incrementAndGet();
                            assertThat(indexShard.getPrimaryTerm(), equalTo(primaryTerm + increment));
                            latch.countDown();
                            releasable.close();
                        }

                        @Override
                        public void onFailure(Exception e) {
                            onFailure.set(e);
                            latch.countDown();
                        }
                    },
                    ThreadPool.Names.WRITE, "");
        };

        final long firstIncrement = 1 + (randomBoolean() ? 0 : 1);
        final long secondIncrement = 1 + (randomBoolean() ? 0 : 1);
        final Thread first = new Thread(function.apply(firstIncrement));
        final Thread second = new Thread(function.apply(secondIncrement));

        first.start();
        second.start();

        // the two threads synchronize attempting to acquire an operation permit
        barrier.await();

        // we wait for both operations to complete
        latch.await();

        first.join();
        second.join();

        final Exception e;
        if ((e = onFailure.get()) != null) {
            /*
             * If one thread tried to set the primary term to a higher value than the other thread and the thread with the higher term won
             * the race, then the other thread lost the race and only one operation should have been executed.
             */
            assertThat(e, instanceOf(IllegalStateException.class));
            assertThat(e, hasToString(matches("operation primary term \\[\\d+\\] is too old")));
            assertThat(counter.get(), equalTo(1L));
        } else {
            assertThat(counter.get(), equalTo(2L));
        }

        assertThat(indexShard.getPrimaryTerm(), equalTo(primaryTerm + Math.max(firstIncrement, secondIncrement)));

        closeShards(indexShard);
    }

    /***
     * test one can snapshot the store at various lifecycle stages
     */
    public void testSnapshotStore() throws IOException {
        final IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0");
        flushShard(shard);

        final IndexShard newShard = reinitShard(shard);
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);

        Store.MetadataSnapshot snapshot = newShard.snapshotStoreMetadata();
        assertThat(snapshot.getSegmentsFile().name(), equalTo("segments_3"));

        newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));

        snapshot = newShard.snapshotStoreMetadata();
        assertThat(snapshot.getSegmentsFile().name(), equalTo("segments_3"));

        assertTrue(newShard.recoverFromStore());

        snapshot = newShard.snapshotStoreMetadata();
        assertThat(snapshot.getSegmentsFile().name(), equalTo("segments_3"));

        IndexShardTestCase.updateRoutingEntry(newShard, newShard.routingEntry().moveToStarted());

        snapshot = newShard.snapshotStoreMetadata();
        assertThat(snapshot.getSegmentsFile().name(), equalTo("segments_3"));

        newShard.close("test", false);

        snapshot = newShard.snapshotStoreMetadata();
        assertThat(snapshot.getSegmentsFile().name(), equalTo("segments_3"));

        closeShards(newShard);
    }

    public void testAsyncFsync() throws InterruptedException, IOException {
        IndexShard shard = newStartedShard();
        Semaphore semaphore = new Semaphore(Integer.MAX_VALUE);
        Thread[] thread = new Thread[randomIntBetween(3, 5)];
        CountDownLatch latch = new CountDownLatch(thread.length);
        for (int i = 0; i < thread.length; i++) {
            thread[i] = new Thread() {
                @Override
                public void run() {
                    try {
                        latch.countDown();
                        latch.await();
                        for (int i = 0; i < 10000; i++) {
                            semaphore.acquire();
                            shard.sync(TranslogTests.randomTranslogLocation(), (ex) -> semaphore.release());
                        }
                    } catch (Exception ex) {
                        throw new RuntimeException(ex);
                    }
                }
            };
            thread[i].start();
        }

        for (int i = 0; i < thread.length; i++) {
            thread[i].join();
        }
        assertTrue(semaphore.tryAcquire(Integer.MAX_VALUE, 10, TimeUnit.SECONDS));

        closeShards(shard);
    }

    public void testMinimumCompatVersion() throws IOException {
        Version versionCreated = VersionUtils.randomVersion(random());
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, versionCreated.id)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard test = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(test);

        indexDoc(test, "_doc", "test");
        assertEquals(versionCreated.luceneVersion, test.minimumCompatibleVersion());
        indexDoc(test, "_doc", "test");
        assertEquals(versionCreated.luceneVersion, test.minimumCompatibleVersion());
        test.getEngine().flush();
        assertEquals(Version.CURRENT.luceneVersion, test.minimumCompatibleVersion());

        closeShards(test);
    }

    public void testShardStats() throws IOException {

        IndexShard shard = newStartedShard();
        ShardStats stats = new ShardStats(shard.routingEntry(), shard.shardPath(),
            new CommonStats(new IndicesQueryCache(Settings.EMPTY), shard, new CommonStatsFlags()), shard.commitStats(), shard.seqNoStats());
        assertEquals(shard.shardPath().getRootDataPath().toString(), stats.getDataPath());
        assertEquals(shard.shardPath().getRootStatePath().toString(), stats.getStatePath());
        assertEquals(shard.shardPath().isCustomDataPath(), stats.isCustomDataPath());

        // try to serialize it to ensure values survive the serialization
        BytesStreamOutput out = new BytesStreamOutput();
        stats.writeTo(out);
        StreamInput in = out.bytes().streamInput();
        stats = ShardStats.readShardStats(in);

        XContentBuilder builder = jsonBuilder();
        builder.startObject();
        stats.toXContent(builder, EMPTY_PARAMS);
        builder.endObject();
        String xContent = Strings.toString(builder);
        StringBuilder expectedSubSequence = new StringBuilder("\"shard_path\":{\"state_path\":\"");
        expectedSubSequence.append(shard.shardPath().getRootStatePath().toString());
        expectedSubSequence.append("\",\"data_path\":\"");
        expectedSubSequence.append(shard.shardPath().getRootDataPath().toString());
        expectedSubSequence.append("\",\"is_custom_data_path\":").append(shard.shardPath().isCustomDataPath()).append("}");
        if (Constants.WINDOWS) {
            // Some path weirdness on windows
        } else {
            assertTrue(xContent.contains(expectedSubSequence));
        }
        closeShards(shard);
    }

    public void testRefreshMetric() throws IOException {
        IndexShard shard = newStartedShard();
        assertThat(shard.refreshStats().getTotal(), equalTo(2L)); // refresh on: finalize and end of recovery
        long initialTotalTime = shard.refreshStats().getTotalTimeInMillis();
        // check time advances
        for (int i = 1; shard.refreshStats().getTotalTimeInMillis() == initialTotalTime; i++) {
            indexDoc(shard, "_doc", "test");
            assertThat(shard.refreshStats().getTotal(), equalTo(2L + i - 1));
            shard.refresh("test");
            assertThat(shard.refreshStats().getTotal(), equalTo(2L + i));
            assertThat(shard.refreshStats().getTotalTimeInMillis(), greaterThanOrEqualTo(initialTotalTime));
        }
        long refreshCount = shard.refreshStats().getTotal();
        indexDoc(shard, "_doc", "test");
        try (Engine.GetResult ignored = shard.get(new Engine.Get(true, false, "test", "test",
            new Term(IdFieldMapper.NAME, Uid.encodeId("test"))))) {
            assertThat(shard.refreshStats().getTotal(), equalTo(refreshCount+1));
        }
        indexDoc(shard, "_doc", "test");
        shard.writeIndexingBuffer();
        assertThat(shard.refreshStats().getTotal(), equalTo(refreshCount+2));
        closeShards(shard);
    }

    public void testIndexingOperationsListeners() throws IOException {
        IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0", "{\"foo\" : \"bar\"}");
        shard.updateLocalCheckpointForShard(shard.shardRouting.allocationId().getId(), 0);
        AtomicInteger preIndex = new AtomicInteger();
        AtomicInteger postIndexCreate = new AtomicInteger();
        AtomicInteger postIndexUpdate = new AtomicInteger();
        AtomicInteger postIndexException = new AtomicInteger();
        AtomicInteger preDelete = new AtomicInteger();
        AtomicInteger postDelete = new AtomicInteger();
        AtomicInteger postDeleteException = new AtomicInteger();
        shard.close("simon says", true);
        shard = reinitShard(shard, new IndexingOperationListener() {
            @Override
            public Engine.Index preIndex(ShardId shardId, Engine.Index operation) {
                preIndex.incrementAndGet();
                return operation;
            }

            @Override
            public void postIndex(ShardId shardId, Engine.Index index, Engine.IndexResult result) {
                switch (result.getResultType()) {
                    case SUCCESS:
                        if (result.isCreated()) {
                            postIndexCreate.incrementAndGet();
                        } else {
                            postIndexUpdate.incrementAndGet();
                        }
                        break;
                    case FAILURE:
                        postIndex(shardId, index, result.getFailure());
                        break;
                    default:
                        fail("unexpected result type:" + result.getResultType());
                }
            }

            @Override
            public void postIndex(ShardId shardId, Engine.Index index, Exception ex) {
                postIndexException.incrementAndGet();
            }

            @Override
            public Engine.Delete preDelete(ShardId shardId, Engine.Delete delete) {
                preDelete.incrementAndGet();
                return delete;
            }

            @Override
            public void postDelete(ShardId shardId, Engine.Delete delete, Engine.DeleteResult result) {
                switch (result.getResultType()) {
                    case SUCCESS:
                        postDelete.incrementAndGet();
                        break;
                    case FAILURE:
                        postDelete(shardId, delete, result.getFailure());
                        break;
                    default:
                        fail("unexpected result type:" + result.getResultType());
                }
            }

            @Override
            public void postDelete(ShardId shardId, Engine.Delete delete, Exception ex) {
                postDeleteException.incrementAndGet();

            }
        });
        recoverShardFromStore(shard);

        indexDoc(shard, "_doc", "1");
        assertEquals(1, preIndex.get());
        assertEquals(1, postIndexCreate.get());
        assertEquals(0, postIndexUpdate.get());
        assertEquals(0, postIndexException.get());
        assertEquals(0, preDelete.get());
        assertEquals(0, postDelete.get());
        assertEquals(0, postDeleteException.get());

        indexDoc(shard, "_doc", "1");
        assertEquals(2, preIndex.get());
        assertEquals(1, postIndexCreate.get());
        assertEquals(1, postIndexUpdate.get());
        assertEquals(0, postIndexException.get());
        assertEquals(0, preDelete.get());
        assertEquals(0, postDelete.get());
        assertEquals(0, postDeleteException.get());

        deleteDoc(shard, "_doc", "1");

        assertEquals(2, preIndex.get());
        assertEquals(1, postIndexCreate.get());
        assertEquals(1, postIndexUpdate.get());
        assertEquals(0, postIndexException.get());
        assertEquals(1, preDelete.get());
        assertEquals(1, postDelete.get());
        assertEquals(0, postDeleteException.get());

        shard.close("Unexpected close", true);
        shard.state = IndexShardState.STARTED; // It will generate exception

        try {
            indexDoc(shard, "_doc", "1");
            fail();
        } catch (AlreadyClosedException e) {

        }

        assertEquals(2, preIndex.get());
        assertEquals(1, postIndexCreate.get());
        assertEquals(1, postIndexUpdate.get());
        assertEquals(0, postIndexException.get());
        assertEquals(1, preDelete.get());
        assertEquals(1, postDelete.get());
        assertEquals(0, postDeleteException.get());
        try {
            deleteDoc(shard, "_doc", "1");
            fail();
        } catch (AlreadyClosedException e) {

        }

        assertEquals(2, preIndex.get());
        assertEquals(1, postIndexCreate.get());
        assertEquals(1, postIndexUpdate.get());
        assertEquals(0, postIndexException.get());
        assertEquals(1, preDelete.get());
        assertEquals(1, postDelete.get());
        assertEquals(0, postDeleteException.get());

        closeShards(shard);
    }

    public void testLockingBeforeAndAfterRelocated() throws Exception {
        final IndexShard shard = newStartedShard(true);
        IndexShardTestCase.updateRoutingEntry(shard, ShardRoutingHelper.relocate(shard.routingEntry(), "other_node"));
        CountDownLatch latch = new CountDownLatch(1);
        Thread recoveryThread = new Thread(() -> {
            latch.countDown();
            try {
                shard.relocated(primaryContext -> {});
            } catch (InterruptedException e) {
                throw new RuntimeException(e);
            }
        });

        try (Releasable ignored = acquirePrimaryOperationPermitBlockingly(shard)) {
            // start finalization of recovery
            recoveryThread.start();
            latch.await();
            // recovery can only be finalized after we release the current primaryOperationLock
            assertTrue(shard.isPrimaryMode());
        }
        // recovery can be now finalized
        recoveryThread.join();
        assertFalse(shard.isPrimaryMode());
        try (Releasable ignored = acquirePrimaryOperationPermitBlockingly(shard)) {
            // lock can again be acquired
            assertFalse(shard.isPrimaryMode());
        }

        closeShards(shard);
    }

    public void testDelayedOperationsBeforeAndAfterRelocated() throws Exception {
        final IndexShard shard = newStartedShard(true);
        IndexShardTestCase.updateRoutingEntry(shard, ShardRoutingHelper.relocate(shard.routingEntry(), "other_node"));
        Thread recoveryThread = new Thread(() -> {
            try {
                shard.relocated(primaryContext -> {});
            } catch (InterruptedException e) {
                throw new RuntimeException(e);
            }
        });

        recoveryThread.start();
        List<PlainActionFuture<Releasable>> onLockAcquiredActions = new ArrayList<>();
        for (int i = 0; i < 10; i++) {
            PlainActionFuture<Releasable> onLockAcquired = new PlainActionFuture<Releasable>() {
                @Override
                public void onResponse(Releasable releasable) {
                    releasable.close();
                    super.onResponse(releasable);
                }
            };
            shard.acquirePrimaryOperationPermit(onLockAcquired, ThreadPool.Names.WRITE, "i_" + i);
            onLockAcquiredActions.add(onLockAcquired);
        }

        for (PlainActionFuture<Releasable> onLockAcquired : onLockAcquiredActions) {
            assertNotNull(onLockAcquired.get(30, TimeUnit.SECONDS));
        }

        recoveryThread.join();

        closeShards(shard);
    }

    public void testStressRelocated() throws Exception {
        final IndexShard shard = newStartedShard(true);
        assertTrue(shard.isPrimaryMode());
        IndexShardTestCase.updateRoutingEntry(shard, ShardRoutingHelper.relocate(shard.routingEntry(), "other_node"));
        final int numThreads = randomIntBetween(2, 4);
        Thread[] indexThreads = new Thread[numThreads];
        CountDownLatch allPrimaryOperationLocksAcquired = new CountDownLatch(numThreads);
        CyclicBarrier barrier = new CyclicBarrier(numThreads + 1);
        for (int i = 0; i < indexThreads.length; i++) {
            indexThreads[i] = new Thread() {
                @Override
                public void run() {
                    try (Releasable operationLock = acquirePrimaryOperationPermitBlockingly(shard)) {
                        allPrimaryOperationLocksAcquired.countDown();
                        barrier.await();
                    } catch (InterruptedException | BrokenBarrierException | ExecutionException e) {
                        throw new RuntimeException(e);
                    }
                }
            };
            indexThreads[i].start();
        }
        AtomicBoolean relocated = new AtomicBoolean();
        final Thread recoveryThread = new Thread(() -> {
            try {
                shard.relocated(primaryContext -> {});
            } catch (InterruptedException e) {
                throw new RuntimeException(e);
            }
            relocated.set(true);
        });
        // ensure we wait for all primary operation locks to be acquired
        allPrimaryOperationLocksAcquired.await();
        // start recovery thread
        recoveryThread.start();
        assertThat(relocated.get(), equalTo(false));
        assertThat(shard.getActiveOperationsCount(), greaterThan(0));
        // ensure we only transition after pending operations completed
        assertTrue(shard.isPrimaryMode());
        // complete pending operations
        barrier.await();
        // complete recovery/relocation
        recoveryThread.join();
        // ensure relocated successfully once pending operations are done
        assertThat(relocated.get(), equalTo(true));
        assertFalse(shard.isPrimaryMode());
        assertThat(shard.getActiveOperationsCount(), equalTo(0));

        for (Thread indexThread : indexThreads) {
            indexThread.join();
        }

        closeShards(shard);
    }

    public void testRelocatedShardCanNotBeRevived() throws IOException, InterruptedException {
        final IndexShard shard = newStartedShard(true);
        final ShardRouting originalRouting = shard.routingEntry();
        IndexShardTestCase.updateRoutingEntry(shard, ShardRoutingHelper.relocate(originalRouting, "other_node"));
        shard.relocated(primaryContext -> {});
        expectThrows(IllegalIndexShardStateException.class, () -> IndexShardTestCase.updateRoutingEntry(shard, originalRouting));
        closeShards(shard);
    }

    public void testShardCanNotBeMarkedAsRelocatedIfRelocationCancelled() throws IOException {
        final IndexShard shard = newStartedShard(true);
        final ShardRouting originalRouting = shard.routingEntry();
        IndexShardTestCase.updateRoutingEntry(shard, ShardRoutingHelper.relocate(originalRouting, "other_node"));
        IndexShardTestCase.updateRoutingEntry(shard, originalRouting);
        expectThrows(IllegalIndexShardStateException.class, () ->  shard.relocated(primaryContext -> {}));
        closeShards(shard);
    }

    public void testRelocatedShardCanNotBeRevivedConcurrently() throws IOException, InterruptedException, BrokenBarrierException {
        final IndexShard shard = newStartedShard(true);
        final ShardRouting originalRouting = shard.routingEntry();
        IndexShardTestCase.updateRoutingEntry(shard, ShardRoutingHelper.relocate(originalRouting, "other_node"));
        CyclicBarrier cyclicBarrier = new CyclicBarrier(3);
        AtomicReference<Exception> relocationException = new AtomicReference<>();
        Thread relocationThread = new Thread(new AbstractRunnable() {
            @Override
            public void onFailure(Exception e) {
                relocationException.set(e);
            }

            @Override
            protected void doRun() throws Exception {
                cyclicBarrier.await();
                shard.relocated(primaryContext -> {});
            }
        });
        relocationThread.start();
        AtomicReference<Exception> cancellingException = new AtomicReference<>();
        Thread cancellingThread = new Thread(new AbstractRunnable() {
            @Override
            public void onFailure(Exception e) {
                cancellingException.set(e);
            }

            @Override
            protected void doRun() throws Exception {
                cyclicBarrier.await();
                IndexShardTestCase.updateRoutingEntry(shard, originalRouting);
            }
        });
        cancellingThread.start();
        cyclicBarrier.await();
        relocationThread.join();
        cancellingThread.join();
        if (shard.isPrimaryMode() == false) {
            logger.debug("shard was relocated successfully");
            assertThat(cancellingException.get(), instanceOf(IllegalIndexShardStateException.class));
            assertThat("current routing:" + shard.routingEntry(), shard.routingEntry().relocating(), equalTo(true));
            assertThat(relocationException.get(), nullValue());
        } else {
            logger.debug("shard relocation was cancelled");
            assertThat(relocationException.get(), instanceOf(IllegalIndexShardStateException.class));
            assertThat("current routing:" + shard.routingEntry(), shard.routingEntry().relocating(), equalTo(false));
            assertThat(cancellingException.get(), nullValue());

        }
        closeShards(shard);
    }

    public void testRecoverFromStoreWithOutOfOrderDelete() throws IOException {
        /*
         * The flow of this test:
         * - delete #1
         * - roll generation (to create gen 2)
         * - index #0
         * - index #3
         * - flush (commit point has max_seqno 3, and local checkpoint 1 -> points at gen 2, previous commit point is maintained)
         * - index #2
         * - index #5
         * - If flush and then recover from the existing store, delete #1 will be removed while index #0 is still retained and replayed.
         */
        final IndexShard shard = newStartedShard(false);
        shard.applyDeleteOperationOnReplica(1, 2, "_doc", "id", VersionType.EXTERNAL);
        shard.getEngine().rollTranslogGeneration(); // isolate the delete in it's own generation
        shard.applyIndexOperationOnReplica(0, 1, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false,
            SourceToParse.source(shard.shardId().getIndexName(), "_doc", "id", new BytesArray("{}"), XContentType.JSON));
        shard.applyIndexOperationOnReplica(3, 3, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false,
            SourceToParse.source(shard.shardId().getIndexName(), "_doc", "id-3", new BytesArray("{}"), XContentType.JSON));
        // Flushing a new commit with local checkpoint=1 allows to skip the translog gen #1 in recovery.
        shard.flush(new FlushRequest().force(true).waitIfOngoing(true));
        shard.applyIndexOperationOnReplica(2, 3, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false,
            SourceToParse.source(shard.shardId().getIndexName(), "_doc", "id-2", new BytesArray("{}"), XContentType.JSON));
        shard.applyIndexOperationOnReplica(5, 1, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false,
            SourceToParse.source(shard.shardId().getIndexName(), "_doc", "id-5", new BytesArray("{}"), XContentType.JSON));

        final int translogOps;
        if (randomBoolean()) {
            // Advance the global checkpoint to remove the 1st commit; this shard will recover the 2nd commit.
            shard.updateGlobalCheckpointOnReplica(3, "test");
            logger.info("--> flushing shard");
            shard.flush(new FlushRequest().force(true).waitIfOngoing(true));
            translogOps = 4; // delete #1 won't be replayed.
        } else if (randomBoolean())  {
            shard.getEngine().rollTranslogGeneration();
            translogOps = 5;
        } else {
            translogOps = 5;
        }

        final ShardRouting replicaRouting = shard.routingEntry();
        IndexShard newShard = reinitShard(shard,
            newShardRouting(replicaRouting.shardId(), replicaRouting.currentNodeId(), true, ShardRoutingState.INITIALIZING,
                RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE));
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));
        assertTrue(newShard.recoverFromStore());
        assertEquals(translogOps, newShard.recoveryState().getTranslog().recoveredOperations());
        assertEquals(translogOps, newShard.recoveryState().getTranslog().totalOperations());
        assertEquals(translogOps, newShard.recoveryState().getTranslog().totalOperationsOnStart());
        assertEquals(100.0f, newShard.recoveryState().getTranslog().recoveredPercent(), 0.01f);
        updateRoutingEntry(newShard, ShardRoutingHelper.moveToStarted(newShard.routingEntry()));
        assertDocCount(newShard, 3);
        closeShards(newShard);
    }

    public void testRecoverFromStore() throws IOException {
        final IndexShard shard = newStartedShard(true);
        int totalOps = randomInt(10);
        int translogOps = totalOps;
        for (int i = 0; i < totalOps; i++) {
            indexDoc(shard, "_doc", Integer.toString(i));
        }
        if (randomBoolean()) {
            shard.updateLocalCheckpointForShard(shard.shardRouting.allocationId().getId(), totalOps - 1);
            flushShard(shard);
            translogOps = 0;
        }
        IndexShard newShard = reinitShard(shard);
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));
        assertTrue(newShard.recoverFromStore());
        assertEquals(translogOps, newShard.recoveryState().getTranslog().recoveredOperations());
        assertEquals(translogOps, newShard.recoveryState().getTranslog().totalOperations());
        assertEquals(translogOps, newShard.recoveryState().getTranslog().totalOperationsOnStart());
        assertEquals(100.0f, newShard.recoveryState().getTranslog().recoveredPercent(), 0.01f);
        IndexShardTestCase.updateRoutingEntry(newShard, newShard.routingEntry().moveToStarted());
        // check that local checkpoint of new primary is properly tracked after recovery
        assertThat(newShard.getLocalCheckpoint(), equalTo(totalOps - 1L));
        assertThat(newShard.getReplicationTracker().getTrackedLocalCheckpointForShard(newShard.routingEntry().allocationId().getId())
                .getLocalCheckpoint(), equalTo(totalOps - 1L));
        assertDocCount(newShard, totalOps);
        closeShards(newShard);
    }

    public void testPrimaryHandOffUpdatesLocalCheckpoint() throws IOException {
        final IndexShard primarySource = newStartedShard(true);
        int totalOps = randomInt(10);
        for (int i = 0; i < totalOps; i++) {
            indexDoc(primarySource, "_doc", Integer.toString(i));
        }
        IndexShardTestCase.updateRoutingEntry(primarySource, primarySource.routingEntry().relocate(randomAlphaOfLength(10), -1));
        final IndexShard primaryTarget = newShard(primarySource.routingEntry().getTargetRelocatingShard());
        updateMappings(primaryTarget, primarySource.indexSettings().getIndexMetaData());
        recoverReplica(primaryTarget, primarySource);

        // check that local checkpoint of new primary is properly tracked after primary relocation
        assertThat(primaryTarget.getLocalCheckpoint(), equalTo(totalOps - 1L));
        assertThat(primaryTarget.getReplicationTracker().getTrackedLocalCheckpointForShard(
            primaryTarget.routingEntry().allocationId().getId()).getLocalCheckpoint(), equalTo(totalOps - 1L));
        assertDocCount(primaryTarget, totalOps);
        closeShards(primarySource, primaryTarget);
    }

    /* This test just verifies that we fill up local checkpoint up to max seen seqID on primary recovery */
    public void testRecoverFromStoreWithNoOps() throws IOException {
        final IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0");
        Engine.IndexResult test = indexDoc(shard, "_doc", "1");
        // start a replica shard and index the second doc
        final IndexShard otherShard = newStartedShard(false);
        updateMappings(otherShard, shard.indexSettings().getIndexMetaData());
        SourceToParse sourceToParse = SourceToParse.source(shard.shardId().getIndexName(), "_doc", "1",
            new BytesArray("{}"), XContentType.JSON);
        otherShard.applyIndexOperationOnReplica(1, 1,
            VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false, sourceToParse);

        final ShardRouting primaryShardRouting = shard.routingEntry();
        IndexShard newShard = reinitShard(otherShard, ShardRoutingHelper.initWithSameId(primaryShardRouting,
            RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE));
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));
        assertTrue(newShard.recoverFromStore());
        assertEquals(1, newShard.recoveryState().getTranslog().recoveredOperations());
        assertEquals(1, newShard.recoveryState().getTranslog().totalOperations());
        assertEquals(1, newShard.recoveryState().getTranslog().totalOperationsOnStart());
        assertEquals(100.0f, newShard.recoveryState().getTranslog().recoveredPercent(), 0.01f);
        try (Translog.Snapshot snapshot = getTranslog(newShard).newSnapshot()) {
            Translog.Operation operation;
            int numNoops = 0;
            while ((operation = snapshot.next()) != null) {
                if (operation.opType() == Translog.Operation.Type.NO_OP) {
                    numNoops++;
                    assertEquals(newShard.getPrimaryTerm(), operation.primaryTerm());
                    assertEquals(0, operation.seqNo());
                }
            }
            assertEquals(1, numNoops);
        }
        IndexShardTestCase.updateRoutingEntry(newShard, newShard.routingEntry().moveToStarted());
        assertDocCount(newShard, 1);
        assertDocCount(shard, 2);

        for (int i = 0; i < 2; i++) {
            newShard = reinitShard(newShard, ShardRoutingHelper.initWithSameId(primaryShardRouting,
                RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE));
            newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));
            assertTrue(newShard.recoverFromStore());
            try (Translog.Snapshot snapshot = getTranslog(newShard).newSnapshot()) {
                assertThat(snapshot.totalOperations(), equalTo(2));
            }
        }
        closeShards(newShard, shard);
    }

    public void testRecoverFromCleanStore() throws IOException {
        final IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0");
        if (randomBoolean()) {
            flushShard(shard);
        }
        final ShardRouting shardRouting = shard.routingEntry();
        IndexShard newShard = reinitShard(shard,
            ShardRoutingHelper.initWithSameId(shardRouting, RecoverySource.StoreRecoverySource.EMPTY_STORE_INSTANCE)
        );

        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));
        assertTrue(newShard.recoverFromStore());
        assertEquals(0, newShard.recoveryState().getTranslog().recoveredOperations());
        assertEquals(0, newShard.recoveryState().getTranslog().totalOperations());
        assertEquals(0, newShard.recoveryState().getTranslog().totalOperationsOnStart());
        assertEquals(100.0f, newShard.recoveryState().getTranslog().recoveredPercent(), 0.01f);
        IndexShardTestCase.updateRoutingEntry(newShard, newShard.routingEntry().moveToStarted());
        assertDocCount(newShard, 0);
        closeShards(newShard);
    }

    public void testFailIfIndexNotPresentInRecoverFromStore() throws Exception {
        final IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0");
        if (randomBoolean()) {
            flushShard(shard);
        }

        Store store = shard.store();
        store.incRef();
        closeShards(shard);
        cleanLuceneIndex(store.directory());
        store.decRef();
        IndexShard newShard = reinitShard(shard);
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        ShardRouting routing = newShard.routingEntry();
        newShard.markAsRecovering("store", new RecoveryState(routing, localNode, null));
        try {
            newShard.recoverFromStore();
            fail("index not there!");
        } catch (IndexShardRecoveryException ex) {
            assertTrue(ex.getMessage().contains("failed to fetch index version after copying it over"));
        }

        routing = ShardRoutingHelper.moveToUnassigned(routing, new UnassignedInfo(UnassignedInfo.Reason.INDEX_CREATED, "because I say so"));
        routing = ShardRoutingHelper.initialize(routing, newShard.routingEntry().currentNodeId());
        assertTrue("it's already recovering, we should ignore new ones", newShard.ignoreRecoveryAttempt());
        try {
            newShard.markAsRecovering("store", new RecoveryState(routing, localNode, null));
            fail("we are already recovering, can't mark again");
        } catch (IllegalIndexShardStateException e) {
            // OK!
        }

        newShard = reinitShard(newShard,
            ShardRoutingHelper.initWithSameId(routing, RecoverySource.StoreRecoverySource.EMPTY_STORE_INSTANCE));
        newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));
        assertTrue("recover even if there is nothing to recover", newShard.recoverFromStore());

        IndexShardTestCase.updateRoutingEntry(newShard, newShard.routingEntry().moveToStarted());
        assertDocCount(newShard, 0);
        // we can't issue this request through a client because of the inconsistencies we created with the cluster state
        // doing it directly instead
        indexDoc(newShard, "_doc", "0");
        newShard.refresh("test");
        assertDocCount(newShard, 1);

        closeShards(newShard);
    }

    public void testRecoverFromStoreRemoveStaleOperations() throws Exception {
        final IndexShard shard = newStartedShard(false);
        final String indexName = shard.shardId().getIndexName();
        // Index #0, index #1
        shard.applyIndexOperationOnReplica(0, 1, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false,
            SourceToParse.source(indexName, "_doc", "doc-0", new BytesArray("{}"), XContentType.JSON));
        flushShard(shard);
        shard.updateGlobalCheckpointOnReplica(0, "test"); // stick the global checkpoint here.
        shard.applyIndexOperationOnReplica(1, 1, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false,
            SourceToParse.source(indexName, "_doc", "doc-1", new BytesArray("{}"), XContentType.JSON));
        flushShard(shard);
        assertThat(getShardDocUIDs(shard), containsInAnyOrder("doc-0", "doc-1"));
        // Simulate resync (without rollback): Noop #1, index #2
        acquireReplicaOperationPermitBlockingly(shard, shard.primaryTerm + 1);
        shard.markSeqNoAsNoop(1, "test");
        shard.applyIndexOperationOnReplica(2, 1, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false,
            SourceToParse.source(indexName, "_doc", "doc-2", new BytesArray("{}"), XContentType.JSON));
        flushShard(shard);
        assertThat(getShardDocUIDs(shard), containsInAnyOrder("doc-0", "doc-1", "doc-2"));
        // Recovering from store should discard doc #1
        final ShardRouting replicaRouting = shard.routingEntry();
        IndexShard newShard = reinitShard(shard,
            newShardRouting(replicaRouting.shardId(), replicaRouting.currentNodeId(), true, ShardRoutingState.INITIALIZING,
                RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE));
        newShard.primaryTerm++;
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(), localNode, null));
        assertTrue(newShard.recoverFromStore());
        assertThat(getShardDocUIDs(newShard), containsInAnyOrder("doc-0", "doc-2"));
        closeShards(newShard);
    }

    public void testRecoveryFailsAfterMovingToRelocatedState() throws InterruptedException, IOException {
        final IndexShard shard = newStartedShard(true);
        ShardRouting origRouting = shard.routingEntry();
        assertThat(shard.state(), equalTo(IndexShardState.STARTED));
        ShardRouting inRecoveryRouting = ShardRoutingHelper.relocate(origRouting, "some_node");
        IndexShardTestCase.updateRoutingEntry(shard, inRecoveryRouting);
        shard.relocated(primaryContext -> {});
        assertFalse(shard.isPrimaryMode());
        try {
            IndexShardTestCase.updateRoutingEntry(shard, origRouting);
            fail("Expected IndexShardRelocatedException");
        } catch (IndexShardRelocatedException expected) {
        }

        closeShards(shard);
    }

    public void testRestoreShard() throws IOException {
        final IndexShard source = newStartedShard(true);
        IndexShard target = newStartedShard(true);

        indexDoc(source, "_doc", "0");
        if (randomBoolean()) {
            source.refresh("test");
        }
        indexDoc(target, "_doc", "1");
        target.refresh("test");
        assertDocs(target, "1");
        flushShard(source); // only flush source
        ShardRouting routing = ShardRoutingHelper.initWithSameId(target.routingEntry(),
            RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE);
        final Snapshot snapshot = new Snapshot("foo", new SnapshotId("bar", UUIDs.randomBase64UUID()));
        routing = ShardRoutingHelper.newWithRestoreSource(routing,
            new RecoverySource.SnapshotRecoverySource(snapshot, Version.CURRENT, "test"));
        target = reinitShard(target, routing);
        Store sourceStore = source.store();
        Store targetStore = target.store();

        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        target.markAsRecovering("store", new RecoveryState(routing, localNode, null));
        assertTrue(target.restoreFromRepository(new RestoreOnlyRepository("test") {
            @Override
            public void restoreShard(IndexShard shard, SnapshotId snapshotId, Version version, IndexId indexId, ShardId snapshotShardId,
                                     RecoveryState recoveryState) {
                try {
                    cleanLuceneIndex(targetStore.directory());
                    for (String file : sourceStore.directory().listAll()) {
                        if (file.equals("write.lock") || file.startsWith("extra")) {
                            continue;
                        }
                        targetStore.directory().copyFrom(sourceStore.directory(), file, file, IOContext.DEFAULT);
                    }
                } catch (Exception ex) {
                    throw new RuntimeException(ex);
                }
            }
        }));
        assertThat(target.getLocalCheckpoint(), equalTo(0L));
        assertThat(target.seqNoStats().getMaxSeqNo(), equalTo(0L));
        assertThat(target.getReplicationTracker().getGlobalCheckpoint(), equalTo(0L));
        IndexShardTestCase.updateRoutingEntry(target, routing.moveToStarted());
        assertThat(target.getReplicationTracker().getTrackedLocalCheckpointForShard(
            target.routingEntry().allocationId().getId()).getLocalCheckpoint(), equalTo(0L));

        assertDocs(target, "0");

        closeShards(source, target);
    }

    public void testSearcherWrapperIsUsed() throws IOException {
        IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0", "{\"foo\" : \"bar\"}");
        indexDoc(shard, "_doc", "1", "{\"foobar\" : \"bar\"}");
        shard.refresh("test");

        Engine.GetResult getResult = shard.get(new Engine.Get(false, false, "test", "1", new Term(IdFieldMapper.NAME, Uid.encodeId("1"))));
        assertTrue(getResult.exists());
        assertNotNull(getResult.searcher());
        getResult.release();
        try (Engine.Searcher searcher = shard.acquireSearcher("test")) {
            TopDocs search = searcher.searcher().search(new TermQuery(new Term("foo", "bar")), 10);
            assertEquals(search.totalHits, 1);
            search = searcher.searcher().search(new TermQuery(new Term("foobar", "bar")), 10);
            assertEquals(search.totalHits, 1);
        }
        IndexSearcherWrapper wrapper = new IndexSearcherWrapper() {
            @Override
            public DirectoryReader wrap(DirectoryReader reader) throws IOException {
                return new FieldMaskingReader("foo", reader);
            }

            @Override
            public IndexSearcher wrap(IndexSearcher searcher) throws EngineException {
                return searcher;
            }
        };
        closeShards(shard);
        IndexShard newShard = newShard(
            ShardRoutingHelper.initWithSameId(shard.routingEntry(), RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE),
            shard.shardPath(), shard.indexSettings().getIndexMetaData(), wrapper, null, () -> {}, EMPTY_EVENT_LISTENER);

        recoverShardFromStore(newShard);

        try (Engine.Searcher searcher = newShard.acquireSearcher("test")) {
            TopDocs search = searcher.searcher().search(new TermQuery(new Term("foo", "bar")), 10);
            assertEquals(search.totalHits, 0);
            search = searcher.searcher().search(new TermQuery(new Term("foobar", "bar")), 10);
            assertEquals(search.totalHits, 1);
        }
        getResult = newShard.get(new Engine.Get(false, false, "test", "1", new Term(IdFieldMapper.NAME, Uid.encodeId("1"))));
        assertTrue(getResult.exists());
        assertNotNull(getResult.searcher()); // make sure get uses the wrapped reader
        assertTrue(getResult.searcher().reader() instanceof FieldMaskingReader);
        getResult.release();

        closeShards(newShard);
    }

    public void testSearcherWrapperWorksWithGlobalOrdinals() throws IOException {
        IndexSearcherWrapper wrapper = new IndexSearcherWrapper() {
            @Override
            public DirectoryReader wrap(DirectoryReader reader) throws IOException {
                return new FieldMaskingReader("foo", reader);
            }

            @Override
            public IndexSearcher wrap(IndexSearcher searcher) throws EngineException {
                return searcher;
            }
        };

        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\", \"fielddata\": true }}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard shard = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, wrapper);
        recoverShardFromStore(shard);
        indexDoc(shard, "_doc", "0", "{\"foo\" : \"bar\"}");
        shard.refresh("created segment 1");
        indexDoc(shard, "_doc", "1", "{\"foobar\" : \"bar\"}");
        shard.refresh("created segment 2");

        // test global ordinals are evicted
        MappedFieldType foo = shard.mapperService().fullName("foo");
        IndicesFieldDataCache indicesFieldDataCache = new IndicesFieldDataCache(shard.indexSettings.getNodeSettings(),
            new IndexFieldDataCache.Listener() {});
        IndexFieldDataService indexFieldDataService = new IndexFieldDataService(shard.indexSettings, indicesFieldDataCache,
            new NoneCircuitBreakerService(), shard.mapperService());
        IndexFieldData.Global ifd = indexFieldDataService.getForField(foo);
        FieldDataStats before = shard.fieldData().stats("foo");
        assertThat(before.getMemorySizeInBytes(), equalTo(0L));
        FieldDataStats after = null;
        try (Engine.Searcher searcher = shard.acquireSearcher("test")) {
            assertThat("we have to have more than one segment", searcher.getDirectoryReader().leaves().size(), greaterThan(1));
            ifd.loadGlobal(searcher.getDirectoryReader());
            after = shard.fieldData().stats("foo");
            assertEquals(after.getEvictions(), before.getEvictions());
            // If a field doesn't exist an empty IndexFieldData is returned and that isn't cached:
            assertThat(after.getMemorySizeInBytes(), equalTo(0L));
        }
        assertEquals(shard.fieldData().stats("foo").getEvictions(), before.getEvictions());
        assertEquals(shard.fieldData().stats("foo").getMemorySizeInBytes(), after.getMemorySizeInBytes());
        shard.flush(new FlushRequest().force(true).waitIfOngoing(true));
        shard.refresh("test");
        assertEquals(shard.fieldData().stats("foo").getMemorySizeInBytes(), before.getMemorySizeInBytes());
        assertEquals(shard.fieldData().stats("foo").getEvictions(), before.getEvictions());

        closeShards(shard);
    }

    public void testIndexingOperationListenersIsInvokedOnRecovery() throws IOException {
        IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0", "{\"foo\" : \"bar\"}");
        deleteDoc(shard, "_doc", "0");
        indexDoc(shard, "_doc", "1", "{\"foo\" : \"bar\"}");
        shard.refresh("test");

        final AtomicInteger preIndex = new AtomicInteger();
        final AtomicInteger postIndex = new AtomicInteger();
        final AtomicInteger preDelete = new AtomicInteger();
        final AtomicInteger postDelete = new AtomicInteger();
        IndexingOperationListener listener = new IndexingOperationListener() {
            @Override
            public Engine.Index preIndex(ShardId shardId, Engine.Index operation) {
                preIndex.incrementAndGet();
                return operation;
            }

            @Override
            public void postIndex(ShardId shardId, Engine.Index index, Engine.IndexResult result) {
                postIndex.incrementAndGet();
            }

            @Override
            public Engine.Delete preDelete(ShardId shardId, Engine.Delete delete) {
                preDelete.incrementAndGet();
                return delete;
            }

            @Override
            public void postDelete(ShardId shardId, Engine.Delete delete, Engine.DeleteResult result) {
                postDelete.incrementAndGet();

            }
        };
        final IndexShard newShard = reinitShard(shard, listener);
        recoverShardFromStore(newShard);
        IndexingStats indexingStats = newShard.indexingStats();
        // ensure we are not influencing the indexing stats
        assertEquals(0, indexingStats.getTotal().getDeleteCount());
        assertEquals(0, indexingStats.getTotal().getDeleteCurrent());
        assertEquals(0, indexingStats.getTotal().getIndexCount());
        assertEquals(0, indexingStats.getTotal().getIndexCurrent());
        assertEquals(0, indexingStats.getTotal().getIndexFailedCount());
        assertEquals(2, preIndex.get());
        assertEquals(2, postIndex.get());
        assertEquals(1, preDelete.get());
        assertEquals(1, postDelete.get());

        closeShards(newShard);
    }

    public void testSearchIsReleaseIfWrapperFails() throws IOException {
        IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0", "{\"foo\" : \"bar\"}");
        shard.refresh("test");
        IndexSearcherWrapper wrapper = new IndexSearcherWrapper() {
            @Override
            public DirectoryReader wrap(DirectoryReader reader) throws IOException {
                throw new RuntimeException("boom");
            }

            @Override
            public IndexSearcher wrap(IndexSearcher searcher) throws EngineException {
                return searcher;
            }
        };

        closeShards(shard);
        IndexShard newShard = newShard(
            ShardRoutingHelper.initWithSameId(shard.routingEntry(), RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE),
            shard.shardPath(), shard.indexSettings().getIndexMetaData(), wrapper, null, () -> {}, EMPTY_EVENT_LISTENER);

        recoverShardFromStore(newShard);

        try {
            newShard.acquireSearcher("test");
            fail("exception expected");
        } catch (RuntimeException ex) {
            //
        }
        closeShards(newShard);
    }

    public void testTranslogRecoverySyncsTranslog() throws IOException {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);

        indexDoc(primary, "_doc", "0", "{\"foo\" : \"bar\"}");
        IndexShard replica = newShard(primary.shardId(), false, "n2", metaData, null);
        recoverReplica(replica, primary, (shard, discoveryNode) ->
            new RecoveryTarget(shard, discoveryNode, recoveryListener, aLong -> {
            }) {
                @Override
                public long indexTranslogOperations(List<Translog.Operation> operations, int totalTranslogOps) throws IOException {
                    final long localCheckpoint = super.indexTranslogOperations(operations, totalTranslogOps);
                    assertFalse(replica.isSyncNeeded());
                    return localCheckpoint;
                }
            }, true);

        closeShards(primary, replica);
    }

    public void testRecoverFromTranslog() throws IOException {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, randomLongBetween(1, Long.MAX_VALUE)).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        List<Translog.Operation> operations = new ArrayList<>();
        int numTotalEntries = randomIntBetween(0, 10);
        int numCorruptEntries = 0;
        for (int i = 0; i < numTotalEntries; i++) {
            if (randomBoolean()) {
                operations.add(new Translog.Index("_doc", "1", 0, primary.getPrimaryTerm(), 1, VersionType.INTERNAL,
                    "{\"foo\" : \"bar\"}".getBytes(Charset.forName("UTF-8")), null, -1));
            } else {
                // corrupt entry
                operations.add(new Translog.Index("_doc", "2", 1,  primary.getPrimaryTerm(), 1, VersionType.INTERNAL,
                    "{\"foo\" : \"bar}".getBytes(Charset.forName("UTF-8")), null, -1));
                numCorruptEntries++;
            }
        }

        Iterator<Translog.Operation> iterator = operations.iterator();
        Translog.Snapshot snapshot = new Translog.Snapshot() {

            @Override
            public void close() {

            }

            @Override
            public int totalOperations() {
                return numTotalEntries;
            }

            @Override
            public Translog.Operation next() throws IOException {
                return iterator.hasNext() ? iterator.next() : null;
            }
        };
        primary.markAsRecovering("store", new RecoveryState(primary.routingEntry(),
            getFakeDiscoNode(primary.routingEntry().currentNodeId()),
            null));
        primary.recoverFromStore();

        primary.state = IndexShardState.RECOVERING; // translog recovery on the next line would otherwise fail as we are in POST_RECOVERY
        primary.runTranslogRecovery(primary.getEngine(), snapshot);
        assertThat(primary.recoveryState().getTranslog().totalOperationsOnStart(), equalTo(numTotalEntries));
        assertThat(primary.recoveryState().getTranslog().totalOperations(), equalTo(numTotalEntries));
        assertThat(primary.recoveryState().getTranslog().recoveredOperations(), equalTo(numTotalEntries - numCorruptEntries));

        closeShards(primary);
    }

    public void testShardActiveDuringInternalRecovery() throws IOException {
        IndexShard shard = newStartedShard(true);
        indexDoc(shard, "_doc", "0");
        shard = reinitShard(shard);
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        shard.markAsRecovering("for testing", new RecoveryState(shard.routingEntry(), localNode, null));
        // Shard is still inactive since we haven't started recovering yet
        assertFalse(shard.isActive());
        shard.prepareForIndexRecovery();
        // Shard is still inactive since we haven't started recovering yet
        assertFalse(shard.isActive());
        shard.openEngineAndRecoverFromTranslog();
        // Shard should now be active since we did recover:
        assertTrue(shard.isActive());
        closeShards(shard);
    }

    public void testShardActiveDuringPeerRecovery() throws IOException {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);

        indexDoc(primary, "_doc", "0", "{\"foo\" : \"bar\"}");
        IndexShard replica = newShard(primary.shardId(), false, "n2", metaData, null);
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        replica.markAsRecovering("for testing", new RecoveryState(replica.routingEntry(), localNode, localNode));
        // Shard is still inactive since we haven't started recovering yet
        assertFalse(replica.isActive());
        recoverReplica(replica, primary, (shard, discoveryNode) ->
            new RecoveryTarget(shard, discoveryNode, recoveryListener, aLong -> {
            }) {
                @Override
                public long indexTranslogOperations(List<Translog.Operation> operations, int totalTranslogOps) throws IOException {
                    final long localCheckpoint = super.indexTranslogOperations(operations, totalTranslogOps);
                    // Shard should now be active since we did recover:
                    assertTrue(replica.isActive());
                    return localCheckpoint;
                }
            }, false);

        closeShards(primary, replica);
    }

    public void testRefreshListenersDuringPeerRecovery() throws IOException {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);

        indexDoc(primary, "_doc", "0", "{\"foo\" : \"bar\"}");
        Consumer<IndexShard> assertListenerCalled = shard -> {
            AtomicBoolean called = new AtomicBoolean();
            shard.addRefreshListener(null, b -> {
                assertFalse(b);
                called.set(true);
            });
            assertTrue(called.get());
        };
        IndexShard replica = newShard(primary.shardId(), false, "n2", metaData, null);
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        replica.markAsRecovering("for testing", new RecoveryState(replica.routingEntry(), localNode, localNode));
        assertListenerCalled.accept(replica);
        recoverReplica(replica, primary, (shard, discoveryNode) ->
            new RecoveryTarget(shard, discoveryNode, recoveryListener, aLong -> {
            }) {
            // we're only checking that listeners are called when the engine is open, before there is no point
                @Override
                public void prepareForTranslogOperations(boolean fileBasedRecovery, int totalTranslogOps) throws IOException {
                    super.prepareForTranslogOperations(fileBasedRecovery, totalTranslogOps);
                    assertListenerCalled.accept(replica);
                }

                @Override
                public long indexTranslogOperations(List<Translog.Operation> operations, int totalTranslogOps) throws IOException {
                    final long localCheckpoint = super.indexTranslogOperations(operations, totalTranslogOps);
                    assertListenerCalled.accept(replica);
                    return localCheckpoint;
                }

                @Override
                public void finalizeRecovery(long globalCheckpoint) throws IOException {
                    super.finalizeRecovery(globalCheckpoint);
                    assertListenerCalled.accept(replica);
                }
            }, false);

        closeShards(primary, replica);
    }

    public void testRecoverFromLocalShard() throws IOException {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("source")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();

        IndexShard sourceShard = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(sourceShard);

        indexDoc(sourceShard, "_doc", "0", "{\"foo\" : \"bar\"}");
        indexDoc(sourceShard, "_doc", "1", "{\"foo\" : \"bar\"}");
        sourceShard.refresh("test");


        ShardRouting targetRouting = newShardRouting(new ShardId("index_1", "index_1", 0), "n1", true,
            ShardRoutingState.INITIALIZING, RecoverySource.LocalShardsRecoverySource.INSTANCE);

        final IndexShard targetShard;
        DiscoveryNode localNode = new DiscoveryNode("foo", buildNewFakeTransportAddress(), emptyMap(), emptySet(), Version.CURRENT);
        Map<String, MappingMetaData> requestedMappingUpdates = ConcurrentCollections.newConcurrentMap();
        {
            targetShard = newShard(targetRouting);
            targetShard.markAsRecovering("store", new RecoveryState(targetShard.routingEntry(), localNode, null));

            BiConsumer<String, MappingMetaData> mappingConsumer = (type, mapping) -> {
                assertNull(requestedMappingUpdates.put(type, mapping));
            };

            final IndexShard differentIndex = newShard(new ShardId("index_2", "index_2", 0), true);
            recoverShardFromStore(differentIndex);
            expectThrows(IllegalArgumentException.class, () -> {
                targetShard.recoverFromLocalShards(mappingConsumer, Arrays.asList(sourceShard, differentIndex));
            });
            closeShards(differentIndex);

            assertTrue(targetShard.recoverFromLocalShards(mappingConsumer, Arrays.asList(sourceShard)));
            RecoveryState recoveryState = targetShard.recoveryState();
            assertEquals(RecoveryState.Stage.DONE, recoveryState.getStage());
            assertTrue(recoveryState.getIndex().fileDetails().size() > 0);
            for (RecoveryState.File file : recoveryState.getIndex().fileDetails()) {
                if (file.reused()) {
                    assertEquals(file.recovered(), 0);
                } else {
                    assertEquals(file.recovered(), file.length());
                }
            }
            // check that local checkpoint of new primary is properly tracked after recovery
            assertThat(targetShard.getLocalCheckpoint(), equalTo(1L));
            assertThat(targetShard.getReplicationTracker().getGlobalCheckpoint(), equalTo(1L));
            IndexShardTestCase.updateRoutingEntry(targetShard, ShardRoutingHelper.moveToStarted(targetShard.routingEntry()));
            assertThat(targetShard.getReplicationTracker().getTrackedLocalCheckpointForShard(
                targetShard.routingEntry().allocationId().getId()).getLocalCheckpoint(), equalTo(1L));
            assertDocCount(targetShard, 2);
        }
        // now check that it's persistent ie. that the added shards are committed
        {
            final IndexShard newShard = reinitShard(targetShard);
            recoverShardFromStore(newShard);
            assertDocCount(newShard, 2);
            closeShards(newShard);
        }

        assertThat(requestedMappingUpdates, hasKey("_doc"));
        assertThat(requestedMappingUpdates.get("_doc").get().source().string(), equalTo("{\"properties\":{\"foo\":{\"type\":\"text\"}}}"));

        closeShards(sourceShard, targetShard);
    }

    public void testDocStats() throws IOException {
        IndexShard indexShard = null;
        try {
            indexShard = newStartedShard();
            final long numDocs = randomIntBetween(2, 32); // at least two documents so we have docs to delete
            // Delete at least numDocs/10 documents otherwise the number of deleted docs will be below 10%
            // and forceMerge will refuse to expunge deletes
            final long numDocsToDelete = randomIntBetween((int) Math.ceil(Math.nextUp(numDocs / 10.0)), Math.toIntExact(numDocs));
            for (int i = 0; i < numDocs; i++) {
                final String id = Integer.toString(i);
                indexDoc(indexShard, "_doc", id);
            }
            if (randomBoolean()) {
                indexShard.refresh("test");
            } else {
                indexShard.flush(new FlushRequest());
            }
            {
                final DocsStats docsStats = indexShard.docStats();
                assertThat(docsStats.getCount(), equalTo(numDocs));
                try (Engine.Searcher searcher = indexShard.acquireSearcher("test")) {
                    assertTrue(searcher.reader().numDocs() <= docsStats.getCount());
                }
                assertThat(docsStats.getDeleted(), equalTo(0L));
                assertThat(docsStats.getAverageSizeInBytes(), greaterThan(0L));
            }

            final List<Integer> ids = randomSubsetOf(
                Math.toIntExact(numDocsToDelete),
                IntStream.range(0, Math.toIntExact(numDocs)).boxed().collect(Collectors.toList()));
            for (final Integer i : ids) {
                final String id = Integer.toString(i);
                deleteDoc(indexShard, "_doc", id);
                indexDoc(indexShard, "_doc", id);
            }

            // flush the buffered deletes
            final FlushRequest flushRequest = new FlushRequest();
            flushRequest.force(false);
            flushRequest.waitIfOngoing(false);
            indexShard.flush(flushRequest);

            if (randomBoolean()) {
                indexShard.refresh("test");
            }
            {
                final DocsStats docStats = indexShard.docStats();
                try (Engine.Searcher searcher = indexShard.acquireSearcher("test")) {
                    assertTrue(searcher.reader().numDocs() <= docStats.getCount());
                }
                assertThat(docStats.getCount(), equalTo(numDocs));
                // Lucene will delete a segment if all docs are deleted from it; this means that we lose the deletes when deleting all docs
                assertThat(docStats.getDeleted(), equalTo(numDocsToDelete == numDocs ? 0 : numDocsToDelete));
            }

            // merge them away
            final ForceMergeRequest forceMergeRequest = new ForceMergeRequest();
            forceMergeRequest.onlyExpungeDeletes(randomBoolean());
            forceMergeRequest.maxNumSegments(1);
            indexShard.forceMerge(forceMergeRequest);

            if (randomBoolean()) {
                indexShard.refresh("test");
            } else {
                indexShard.flush(new FlushRequest());
            }
            {
                final DocsStats docStats = indexShard.docStats();
                assertThat(docStats.getCount(), equalTo(numDocs));
                assertThat(docStats.getDeleted(), equalTo(0L));
                assertThat(docStats.getAverageSizeInBytes(), greaterThan(0L));
            }
        } finally {
            closeShards(indexShard);
        }
    }

    public void testEstimateTotalDocSize() throws Exception {
        IndexShard indexShard = null;
        try {
            indexShard = newStartedShard(true);

            int numDoc = randomIntBetween(100, 200);
            for (int i = 0; i < numDoc; i++) {
                String doc = Strings.toString(XContentFactory.jsonBuilder()
                    .startObject()
                        .field("count", randomInt())
                        .field("point", randomFloat())
                        .field("description", randomUnicodeOfCodepointLength(100))
                    .endObject());
                indexDoc(indexShard, "_doc", Integer.toString(i), doc);
            }

            assertThat("Without flushing, segment sizes should be zero",
                indexShard.docStats().getTotalSizeInBytes(), equalTo(0L));

            if (randomBoolean()) {
                indexShard.flush(new FlushRequest());
            } else {
                indexShard.refresh("test");
            }
            {
                final DocsStats docsStats = indexShard.docStats();
                final StoreStats storeStats = indexShard.storeStats();
                assertThat(storeStats.sizeInBytes(), greaterThan(numDoc * 100L)); // A doc should be more than 100 bytes.

                assertThat("Estimated total document size is too small compared with the stored size",
                    docsStats.getTotalSizeInBytes(), greaterThanOrEqualTo(storeStats.sizeInBytes() * 80/100));
                assertThat("Estimated total document size is too large compared with the stored size",
                    docsStats.getTotalSizeInBytes(), lessThanOrEqualTo(storeStats.sizeInBytes() * 120/100));
            }

            // Do some updates and deletes, then recheck the correlation again.
            for (int i = 0; i < numDoc / 2; i++) {
                if (randomBoolean()) {
                    deleteDoc(indexShard, "doc", Integer.toString(i));
                } else {
                    indexDoc(indexShard, "_doc", Integer.toString(i), "{\"foo\": \"bar\"}");
                }
            }
            if (randomBoolean()) {
                indexShard.flush(new FlushRequest());
            } else {
                indexShard.refresh("test");
            }
            {
                final DocsStats docsStats = indexShard.docStats();
                final StoreStats storeStats = indexShard.storeStats();
                assertThat("Estimated total document size is too small compared with the stored size",
                    docsStats.getTotalSizeInBytes(), greaterThanOrEqualTo(storeStats.sizeInBytes() * 80/100));
                assertThat("Estimated total document size is too large compared with the stored size",
                    docsStats.getTotalSizeInBytes(), lessThanOrEqualTo(storeStats.sizeInBytes() * 120/100));
            }

        } finally {
            closeShards(indexShard);
        }
    }

    /**
     * here we are simulating the scenario that happens when we do async shard fetching from GatewaySerivce while we are finishing
     * a recovery and concurrently clean files. This should always be possible without any exception. Yet there was a bug where IndexShard
     * acquired the index writer lock before it called into the store that has it's own locking for metadata reads
     */
    public void testReadSnapshotConcurrently() throws IOException, InterruptedException {
        IndexShard indexShard = newStartedShard();
        indexDoc(indexShard, "_doc", "0", "{}");
        if (randomBoolean()) {
            indexShard.refresh("test");
        }
        indexDoc(indexShard, "_doc", "1", "{}");
        indexShard.flush(new FlushRequest());
        closeShards(indexShard);

        final IndexShard newShard = reinitShard(indexShard);
        Store.MetadataSnapshot storeFileMetaDatas = newShard.snapshotStoreMetadata();
        assertTrue("at least 2 files, commit and data: " +storeFileMetaDatas.toString(), storeFileMetaDatas.size() > 1);
        AtomicBoolean stop = new AtomicBoolean(false);
        CountDownLatch latch = new CountDownLatch(1);
        expectThrows(AlreadyClosedException.class, () -> newShard.getEngine()); // no engine
        Thread thread = new Thread(() -> {
            latch.countDown();
            while(stop.get() == false){
                try {
                    Store.MetadataSnapshot readMeta = newShard.snapshotStoreMetadata();
                    assertEquals(0, storeFileMetaDatas.recoveryDiff(readMeta).different.size());
                    assertEquals(0, storeFileMetaDatas.recoveryDiff(readMeta).missing.size());
                    assertEquals(storeFileMetaDatas.size(), storeFileMetaDatas.recoveryDiff(readMeta).identical.size());
                } catch (IOException e) {
                    throw new AssertionError(e);
                }
            }
        });
        thread.start();
        latch.await();

        int iters = iterations(10, 100);
        for (int i = 0; i < iters; i++) {
            newShard.store().cleanupAndVerify("test", storeFileMetaDatas);
        }
        assertTrue(stop.compareAndSet(false, true));
        thread.join();
        closeShards(newShard);
    }

    /**
     * Simulates a scenario that happens when we are async fetching snapshot metadata from GatewayService
     * and checking index concurrently. This should always be possible without any exception.
     */
    public void testReadSnapshotAndCheckIndexConcurrently() throws Exception {
        final boolean isPrimary = randomBoolean();
        IndexShard indexShard = newStartedShard(isPrimary);
        final long numDocs = between(10, 100);
        for (long i = 0; i < numDocs; i++) {
            indexDoc(indexShard, "_doc", Long.toString(i), "{}");
            if (randomBoolean()) {
                indexShard.refresh("test");
            }
        }
        indexShard.flush(new FlushRequest());
        closeShards(indexShard);

        final ShardRouting shardRouting = ShardRoutingHelper.initWithSameId(indexShard.routingEntry(),
            isPrimary ? RecoverySource.StoreRecoverySource.EXISTING_STORE_INSTANCE : RecoverySource.PeerRecoverySource.INSTANCE
        );
        final IndexMetaData indexMetaData = IndexMetaData.builder(indexShard.indexSettings().getIndexMetaData())
            .settings(Settings.builder()
                .put(indexShard.indexSettings.getSettings())
                .put(IndexSettings.INDEX_CHECK_ON_STARTUP.getKey(), randomFrom("false", "true", "checksum", "fix")))
            .build();
        final IndexShard newShard = newShard(shardRouting, indexShard.shardPath(), indexMetaData,
            null, indexShard.engineFactory, indexShard.getGlobalCheckpointSyncer(), EMPTY_EVENT_LISTENER);

        Store.MetadataSnapshot storeFileMetaDatas = newShard.snapshotStoreMetadata();
        assertTrue("at least 2 files, commit and data: " + storeFileMetaDatas.toString(), storeFileMetaDatas.size() > 1);
        AtomicBoolean stop = new AtomicBoolean(false);
        CountDownLatch latch = new CountDownLatch(1);
        Thread snapshotter = new Thread(() -> {
            latch.countDown();
            while (stop.get() == false) {
                try {
                    Store.MetadataSnapshot readMeta = newShard.snapshotStoreMetadata();
                    assertThat(readMeta.getNumDocs(), equalTo(numDocs));
                    assertThat(storeFileMetaDatas.recoveryDiff(readMeta).different.size(), equalTo(0));
                    assertThat(storeFileMetaDatas.recoveryDiff(readMeta).missing.size(), equalTo(0));
                    assertThat(storeFileMetaDatas.recoveryDiff(readMeta).identical.size(), equalTo(storeFileMetaDatas.size()));
                } catch (IOException e) {
                    throw new AssertionError(e);
                }
            }
        });
        snapshotter.start();

        if (isPrimary) {
            newShard.markAsRecovering("store", new RecoveryState(newShard.routingEntry(),
                getFakeDiscoNode(newShard.routingEntry().currentNodeId()), null));
        } else {
            newShard.markAsRecovering("peer", new RecoveryState(newShard.routingEntry(),
                getFakeDiscoNode(newShard.routingEntry().currentNodeId()), getFakeDiscoNode(newShard.routingEntry().currentNodeId())));
        }
        int iters = iterations(10, 100);
        latch.await();
        for (int i = 0; i < iters; i++) {
            newShard.checkIndex();
        }
        assertTrue(stop.compareAndSet(false, true));
        snapshotter.join();
        closeShards(newShard);
    }

    class Result {
        private final int localCheckpoint;
        private final int maxSeqNo;
        private final boolean gap;

        Result(final int localCheckpoint, final int maxSeqNo, final boolean gap) {
            this.localCheckpoint = localCheckpoint;
            this.maxSeqNo = maxSeqNo;
            this.gap = gap;
        }
    }

    /**
     * Index on the specified shard while introducing sequence number gaps.
     *
     * @param indexShard the shard
     * @param operations the number of operations
     * @param offset     the starting sequence number
     * @return a pair of the maximum sequence number and whether or not a gap was introduced
     * @throws IOException if an I/O exception occurs while indexing on the shard
     */
    private Result indexOnReplicaWithGaps(
            final IndexShard indexShard,
            final int operations,
            final int offset) throws IOException {
        int localCheckpoint = offset;
        int max = offset;
        boolean gap = false;
        for (int i = offset + 1; i < operations; i++) {
            if (!rarely() || i == operations - 1) { // last operation can't be a gap as it's not a gap anymore
                final String id = Integer.toString(i);
                SourceToParse sourceToParse = SourceToParse.source(indexShard.shardId().getIndexName(), "_doc", id,
                        new BytesArray("{}"), XContentType.JSON);
                indexShard.applyIndexOperationOnReplica(i,
                    1, VersionType.EXTERNAL, IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP, false, sourceToParse);
                if (!gap && i == localCheckpoint + 1) {
                    localCheckpoint++;
                }
                max = i;
            } else {
                gap = true;
            }
        }
        assert localCheckpoint == indexShard.getLocalCheckpoint();
        assert !gap || (localCheckpoint != max);
        return new Result(localCheckpoint, max, gap);
    }

    /** A dummy repository for testing which just needs restore overridden */
    private abstract static class RestoreOnlyRepository extends AbstractLifecycleComponent implements Repository {
        private final String indexName;

        RestoreOnlyRepository(String indexName) {
            super(Settings.EMPTY);
            this.indexName = indexName;
        }

        @Override
        protected void doStart() {
        }

        @Override
        protected void doStop() {
        }

        @Override
        protected void doClose() {
        }

        @Override
        public RepositoryMetaData getMetadata() {
            return null;
        }

        @Override
        public SnapshotInfo getSnapshotInfo(SnapshotId snapshotId) {
            return null;
        }

        @Override
        public MetaData getSnapshotGlobalMetaData(SnapshotId snapshotId) {
            return null;
        }

        @Override
        public IndexMetaData getSnapshotIndexMetaData(SnapshotId snapshotId, IndexId index) throws IOException {
            return null;
        }

        @Override
        public RepositoryData getRepositoryData() {
            Map<IndexId, Set<SnapshotId>> map = new HashMap<>();
            map.put(new IndexId(indexName, "blah"), emptySet());
            return new RepositoryData(EMPTY_REPO_GEN, Collections.emptyMap(), Collections.emptyMap(), map, Collections.emptyList());
        }

        @Override
        public void initializeSnapshot(SnapshotId snapshotId, List<IndexId> indices, MetaData metaData) {
        }

        @Override
        public SnapshotInfo finalizeSnapshot(SnapshotId snapshotId, List<IndexId> indices, long startTime, String failure,
                                             int totalShards, List<SnapshotShardFailure> shardFailures, long repositoryStateId,
                                             boolean includeGlobalState) {
            return null;
        }

        @Override
        public void deleteSnapshot(SnapshotId snapshotId, long repositoryStateId) {
        }

        @Override
        public long getSnapshotThrottleTimeInNanos() {
            return 0;
        }

        @Override
        public long getRestoreThrottleTimeInNanos() {
            return 0;
        }

        @Override
        public String startVerification() {
            return null;
        }

        @Override
        public void endVerification(String verificationToken) {
        }

        @Override
        public boolean isReadOnly() {
            return false;
        }

        @Override
        public void snapshotShard(IndexShard shard, SnapshotId snapshotId, IndexId indexId, IndexCommit snapshotIndexCommit, IndexShardSnapshotStatus snapshotStatus) {
        }

        @Override
        public IndexShardSnapshotStatus getShardSnapshotStatus(SnapshotId snapshotId, Version version, IndexId indexId, ShardId shardId) {
            return null;
        }

        @Override
        public void verify(String verificationToken, DiscoveryNode localNode) {
        }
    }

    public void testIsSearchIdle() throws Exception {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);
        indexDoc(primary, "_doc", "0", "{\"foo\" : \"bar\"}");
        assertTrue(primary.getEngine().refreshNeeded());
        assertTrue(primary.scheduledRefresh());
        assertFalse(primary.isSearchIdle());

        IndexScopedSettings scopedSettings = primary.indexSettings().getScopedSettings();
        settings = Settings.builder().put(settings).put(IndexSettings.INDEX_SEARCH_IDLE_AFTER.getKey(), TimeValue.ZERO).build();
        scopedSettings.applySettings(settings);
        assertTrue(primary.isSearchIdle());

        settings = Settings.builder().put(settings).put(IndexSettings.INDEX_SEARCH_IDLE_AFTER.getKey(), TimeValue.timeValueMinutes(1))
            .build();
        scopedSettings.applySettings(settings);
        assertFalse(primary.isSearchIdle());

        settings = Settings.builder().put(settings).put(IndexSettings.INDEX_SEARCH_IDLE_AFTER.getKey(), TimeValue.timeValueMillis(10))
            .build();
        scopedSettings.applySettings(settings);

        assertBusy(() -> assertTrue(primary.isSearchIdle()));
        do {
            // now loop until we are fast enough... shouldn't take long
            primary.awaitShardSearchActive(aBoolean -> {});
        } while (primary.isSearchIdle());
        closeShards(primary);
    }

    public void testScheduledRefresh() throws IOException, InterruptedException {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);
        indexDoc(primary, "_doc", "0", "{\"foo\" : \"bar\"}");
        assertTrue(primary.getEngine().refreshNeeded());
        assertTrue(primary.scheduledRefresh());
        IndexScopedSettings scopedSettings = primary.indexSettings().getScopedSettings();
        settings = Settings.builder().put(settings).put(IndexSettings.INDEX_SEARCH_IDLE_AFTER.getKey(), TimeValue.ZERO).build();
        scopedSettings.applySettings(settings);

        assertFalse(primary.getEngine().refreshNeeded());
        indexDoc(primary, "_doc", "1", "{\"foo\" : \"bar\"}");
        assertTrue(primary.getEngine().refreshNeeded());
        long lastSearchAccess = primary.getLastSearcherAccess();
        assertFalse(primary.scheduledRefresh());
        assertEquals(lastSearchAccess, primary.getLastSearcherAccess());
        // wait until the thread-pool has moved the timestamp otherwise we can't assert on this below
        awaitBusy(() -> primary.getThreadPool().relativeTimeInMillis() > lastSearchAccess);
        CountDownLatch latch = new CountDownLatch(10);
        for (int i = 0; i < 10; i++) {
            primary.awaitShardSearchActive(refreshed -> {
                assertTrue(refreshed);
                try (Engine.Searcher searcher = primary.acquireSearcher("test")) {
                    assertEquals(2, searcher.reader().numDocs());
                } finally {
                    latch.countDown();
                }
            });
        }
        assertNotEquals("awaitShardSearchActive must access a searcher to remove search idle state", lastSearchAccess,
            primary.getLastSearcherAccess());
        assertTrue(lastSearchAccess < primary.getLastSearcherAccess());
        try (Engine.Searcher searcher = primary.acquireSearcher("test")) {
            assertEquals(1, searcher.reader().numDocs());
        }
        assertTrue(primary.getEngine().refreshNeeded());
        assertTrue(primary.scheduledRefresh());
        latch.await();
        CountDownLatch latch1 = new CountDownLatch(1);
        primary.awaitShardSearchActive(refreshed -> {
            assertFalse(refreshed);
            try (Engine.Searcher searcher = primary.acquireSearcher("test")) {
                assertEquals(2, searcher.reader().numDocs());
            } finally {
                latch1.countDown();
            }

        });
        latch1.await();

        indexDoc(primary, "_doc", "2", "{\"foo\" : \"bar\"}");
        assertFalse(primary.scheduledRefresh());
        assertTrue(primary.isSearchIdle());
        primary.checkIdle(0);
        assertTrue(primary.scheduledRefresh()); // make sure we refresh once the shard is inactive
        try (Engine.Searcher searcher = primary.acquireSearcher("test")) {
            assertEquals(3, searcher.reader().numDocs());
        }
        closeShards(primary);
    }

    public void testRefreshIsNeededWithRefreshListeners() throws IOException, InterruptedException {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);
        indexDoc(primary, "_doc", "0", "{\"foo\" : \"bar\"}");
        assertTrue(primary.getEngine().refreshNeeded());
        assertTrue(primary.scheduledRefresh());
        Engine.IndexResult doc = indexDoc(primary, "_doc", "1", "{\"foo\" : \"bar\"}");
        CountDownLatch latch = new CountDownLatch(1);
        primary.addRefreshListener(doc.getTranslogLocation(), r -> latch.countDown());
        assertEquals(1, latch.getCount());
        assertTrue(primary.getEngine().refreshNeeded());
        assertTrue(primary.scheduledRefresh());
        latch.await();

        IndexScopedSettings scopedSettings = primary.indexSettings().getScopedSettings();
        settings = Settings.builder().put(settings).put(IndexSettings.INDEX_SEARCH_IDLE_AFTER.getKey(), TimeValue.ZERO).build();
        scopedSettings.applySettings(settings);

        doc = indexDoc(primary, "_doc", "2", "{\"foo\" : \"bar\"}");
        CountDownLatch latch1 = new CountDownLatch(1);
        primary.addRefreshListener(doc.getTranslogLocation(), r -> latch1.countDown());
        assertEquals(1, latch1.getCount());
        assertTrue(primary.getEngine().refreshNeeded());
        assertTrue(primary.scheduledRefresh());
        latch1.await();
        closeShards(primary);
    }

    public void testSegmentMemoryTrackedInBreaker() throws Exception {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);
        indexDoc(primary, "_doc", "0", "{\"foo\" : \"foo\"}");
        primary.refresh("forced refresh");

        SegmentsStats ss = primary.segmentStats(randomBoolean());
        CircuitBreaker breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        assertThat(ss.getMemoryInBytes(), equalTo(breaker.getUsed()));
        final long preRefreshBytes = ss.getMemoryInBytes();

        indexDoc(primary, "_doc", "1", "{\"foo\" : \"bar\"}");
        indexDoc(primary, "_doc", "2", "{\"foo\" : \"baz\"}");
        indexDoc(primary, "_doc", "3", "{\"foo\" : \"eggplant\"}");

        ss = primary.segmentStats(randomBoolean());
        breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        assertThat(preRefreshBytes, equalTo(breaker.getUsed()));

        primary.refresh("refresh");

        ss = primary.segmentStats(randomBoolean());
        breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        assertThat(breaker.getUsed(), equalTo(ss.getMemoryInBytes()));
        assertThat(breaker.getUsed(), greaterThan(preRefreshBytes));

        indexDoc(primary, "_doc", "4", "{\"foo\": \"potato\"}");
        // Forces a refresh with the INTERNAL scope
        ((InternalEngine) primary.getEngine()).writeIndexingBuffer();

        ss = primary.segmentStats(randomBoolean());
        breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        assertThat(breaker.getUsed(), equalTo(ss.getMemoryInBytes()));
        assertThat(breaker.getUsed(), greaterThan(preRefreshBytes));
        final long postRefreshBytes = ss.getMemoryInBytes();

        // Deleting a doc causes its memory to be freed from the breaker
        deleteDoc(primary, "_doc", "0");
        primary.refresh("force refresh");

        ss = primary.segmentStats(randomBoolean());
        breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        assertThat(breaker.getUsed(), lessThan(postRefreshBytes));

        closeShards(primary);

        breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        assertThat(breaker.getUsed(), equalTo(0L));
    }

    public void testSegmentMemoryTrackedWithRandomSearchers() throws Exception {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metaData.getIndex(), 0), true, "n1", metaData, null);
        recoverShardFromStore(primary);

        int threadCount = randomIntBetween(2, 6);
        List<Thread> threads = new ArrayList<>(threadCount);
        int iterations = randomIntBetween(50, 100);
        List<Engine.Searcher> searchers = Collections.synchronizedList(new ArrayList<>());

        logger.info("--> running with {} threads and {} iterations each", threadCount, iterations);
        for (int threadId = 0; threadId < threadCount; threadId++) {
            final String threadName = "thread-" + threadId;
            Runnable r = () -> {
                for (int i = 0; i < iterations; i++) {
                    try {
                        if (randomBoolean()) {
                            String id = "id-" + threadName + "-" + i;
                            logger.debug("--> {} indexing {}", threadName, id);
                            indexDoc(primary, "_doc", id, "{\"foo\" : \"" + randomAlphaOfLength(10) + "\"}");
                        }

                        if (randomBoolean() && i > 10) {
                            String id = "id-" + threadName + "-" + randomIntBetween(0, i - 1);
                            logger.debug("--> {}, deleting {}", threadName, id);
                            deleteDoc(primary, "_doc", id);
                        }

                        if (randomBoolean()) {
                            logger.debug("--> {} refreshing", threadName);
                            primary.refresh("forced refresh");
                        }

                        if (randomBoolean()) {
                            String searcherName = "searcher-" + threadName + "-" + i;
                            logger.debug("--> {} acquiring new searcher {}", threadName, searcherName);
                            // Acquire a new searcher, adding it to the list
                            searchers.add(primary.acquireSearcher(searcherName));
                        }

                        if (randomBoolean() && searchers.size() > 1) {
                            // Close one of the searchers at random
                            synchronized (searchers) {
                                // re-check because it could have decremented after the check
                                if (searchers.size() > 1) {
                                    Engine.Searcher searcher = searchers.remove(0);
                                    logger.debug("--> {} closing searcher {}", threadName, searcher.source());
                                    IOUtils.close(searcher);
                                }
                            }
                        }
                    } catch (Exception e) {
                        logger.warn("--> got exception: ", e);
                        fail("got an exception we didn't expect");
                    }
                }

            };
            threads.add(new Thread(r, threadName));
        }
        threads.stream().forEach(t -> t.start());

        for (Thread t : threads) {
            t.join();
        }

        // We need to wait for all ongoing merges to complete. The reason is that during a merge the
        // IndexWriter holds the core cache key open and causes the memory to be registered in the breaker
        primary.forceMerge(new ForceMergeRequest().maxNumSegments(1).flush(true));

        // Close remaining searchers
        IOUtils.close(searchers);

        SegmentsStats ss = primary.segmentStats(randomBoolean());
        CircuitBreaker breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        long segmentMem = ss.getMemoryInBytes();
        long breakerMem = breaker.getUsed();
        logger.info("--> comparing segmentMem: {} - breaker: {} => {}", segmentMem, breakerMem, segmentMem == breakerMem);
        assertThat(segmentMem, equalTo(breakerMem));

        // Close shard
        closeShards(primary);

        // Check that the breaker was successfully reset to 0, meaning that all the accounting was correctly applied
        breaker = primary.circuitBreakerService.getBreaker(CircuitBreaker.ACCOUNTING);
        assertThat(breaker.getUsed(), equalTo(0L));
    }

    public void testFlushOnInactive() throws Exception {
        Settings settings = Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        IndexMetaData metaData = IndexMetaData.builder("test")
            .putMapping("_doc", "{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        ShardRouting shardRouting = TestShardRouting.newShardRouting(new ShardId(metaData.getIndex(), 0), "n1", true, ShardRoutingState
            .INITIALIZING, RecoverySource.StoreRecoverySource.EMPTY_STORE_INSTANCE);
        final ShardId shardId = shardRouting.shardId();
        final NodeEnvironment.NodePath nodePath = new NodeEnvironment.NodePath(createTempDir());
        ShardPath shardPath = new ShardPath(false, nodePath.resolve(shardId), nodePath.resolve(shardId), shardId);
        AtomicBoolean markedInactive = new AtomicBoolean();
        AtomicReference<IndexShard> primaryRef = new AtomicReference<>();
        IndexShard primary = newShard(shardRouting, shardPath, metaData, null, null, () -> {
        }, new IndexEventListener() {
            @Override
            public void onShardInactive(IndexShard indexShard) {
                markedInactive.set(true);
                primaryRef.get().flush(new FlushRequest());
            }
        });
        primaryRef.set(primary);
        recoverShardFromStore(primary);
        for (int i = 0; i < 3; i++) {
            indexDoc(primary, "_doc", "" + i, "{\"foo\" : \"" + randomAlphaOfLength(10) + "\"}");
            primary.refresh("test"); // produce segments
        }
        List<Segment> segments = primary.segments(false);
        Set<String> names = new HashSet<>();
        for (Segment segment : segments) {
            assertFalse(segment.committed);
            assertTrue(segment.search);
            names.add(segment.getName());
        }
        assertEquals(3, segments.size());
        primary.flush(new FlushRequest());
        primary.forceMerge(new ForceMergeRequest().maxNumSegments(1).flush(false));
        primary.refresh("test");
        segments = primary.segments(false);
        for (Segment segment : segments) {
            if (names.contains(segment.getName())) {
                assertTrue(segment.committed);
                assertFalse(segment.search);
            } else {
                assertFalse(segment.committed);
                assertTrue(segment.search);
            }
        }
        assertEquals(4, segments.size());

        assertFalse(markedInactive.get());
        assertBusy(() -> {
            primary.checkIdle(0);
            assertFalse(primary.isActive());
        });

        assertTrue(markedInactive.get());
        segments = primary.segments(false);
        assertEquals(1, segments.size());
        for (Segment segment : segments) {
            assertTrue(segment.committed);
            assertTrue(segment.search);
        }
        closeShards(primary);
    }

}
