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

package org.elasticsearch.cluster.action.shard;

import org.apache.lucene.index.CorruptIndexException;
import org.elasticsearch.Version;
import org.elasticsearch.action.support.replication.ClusterStateCreationUtils;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateObserver;
import org.elasticsearch.cluster.NotMasterException;
import org.elasticsearch.cluster.action.shard.ShardStateAction.FailedShardEntry;
import org.elasticsearch.cluster.action.shard.ShardStateAction.StartedShardEntry;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.IndexRoutingTable;
import org.elasticsearch.cluster.routing.RoutingService;
import org.elasticsearch.cluster.routing.RoutingTable;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.ShardsIterator;
import org.elasticsearch.cluster.routing.allocation.AllocationService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.discovery.Discovery;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.test.transport.CapturingTransport;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.NodeDisconnectedException;
import org.elasticsearch.transport.NodeNotConnectedException;
import org.elasticsearch.transport.TransportException;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.transport.TransportResponse;
import org.elasticsearch.transport.TransportService;
import org.junit.After;
import org.junit.AfterClass;
import org.junit.Before;
import org.junit.BeforeClass;

import java.io.IOException;
import java.util.Collections;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Phaser;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.LongConsumer;
import java.util.function.Predicate;

import static org.elasticsearch.test.ClusterServiceUtils.createClusterService;
import static org.elasticsearch.test.ClusterServiceUtils.setState;
import static org.hamcrest.CoreMatchers.equalTo;
import static org.hamcrest.CoreMatchers.instanceOf;
import static org.hamcrest.CoreMatchers.sameInstance;
import static org.hamcrest.Matchers.arrayWithSize;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.nullValue;

public class ShardStateActionTests extends ESTestCase {
    private static ThreadPool THREAD_POOL;

    private TestShardStateAction shardStateAction;
    private CapturingTransport transport;
    private TransportService transportService;
    private ClusterService clusterService;

    private static class TestShardStateAction extends ShardStateAction {
        TestShardStateAction(Settings settings, ClusterService clusterService, TransportService transportService, AllocationService allocationService, RoutingService routingService) {
            super(settings, clusterService, transportService, allocationService, routingService, THREAD_POOL);
        }

        private Runnable onBeforeWaitForNewMasterAndRetry;

        public void setOnBeforeWaitForNewMasterAndRetry(Runnable onBeforeWaitForNewMasterAndRetry) {
            this.onBeforeWaitForNewMasterAndRetry = onBeforeWaitForNewMasterAndRetry;
        }

        private Runnable onAfterWaitForNewMasterAndRetry;

        public void setOnAfterWaitForNewMasterAndRetry(Runnable onAfterWaitForNewMasterAndRetry) {
            this.onAfterWaitForNewMasterAndRetry = onAfterWaitForNewMasterAndRetry;
        }

        @Override
        protected void waitForNewMasterAndRetry(String actionName, ClusterStateObserver observer, TransportRequest request, Listener listener, Predicate<ClusterState> changePredicate) {
            onBeforeWaitForNewMasterAndRetry.run();
            super.waitForNewMasterAndRetry(actionName, observer, request, listener, changePredicate);
            onAfterWaitForNewMasterAndRetry.run();
        }
    }

    @BeforeClass
    public static void startThreadPool() {
        THREAD_POOL = new TestThreadPool("ShardStateActionTest");
    }

    @Override
    @Before
    public void setUp() throws Exception {
        super.setUp();
        this.transport = new CapturingTransport();
        clusterService = createClusterService(THREAD_POOL);
        transportService = transport.createCapturingTransportService(clusterService.getSettings(), THREAD_POOL,
            TransportService.NOOP_TRANSPORT_INTERCEPTOR, x -> clusterService.localNode(), null, Collections.emptySet());
        transportService.start();
        transportService.acceptIncomingRequests();
        shardStateAction = new TestShardStateAction(Settings.EMPTY, clusterService, transportService, null, null);
        shardStateAction.setOnBeforeWaitForNewMasterAndRetry(() -> {
        });
        shardStateAction.setOnAfterWaitForNewMasterAndRetry(() -> {
        });
    }

    @Override
    @After
    public void tearDown() throws Exception {
        clusterService.close();
        transportService.close();
        super.tearDown();
        assertThat(shardStateAction.remoteShardFailedCacheSize(), equalTo(0));
    }

    @AfterClass
    public static void stopThreadPool() {
        ThreadPool.terminate(THREAD_POOL, 30, TimeUnit.SECONDS);
        THREAD_POOL = null;
    }

    public void testSuccess() throws InterruptedException {
        final String index = "test";

        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));

        AtomicBoolean success = new AtomicBoolean();
        CountDownLatch latch = new CountDownLatch(1);

        ShardRouting shardRouting = getRandomShardRouting(index);
        shardStateAction.localShardFailed(shardRouting, "test", getSimulatedFailure(), new ShardStateAction.Listener() {
            @Override
            public void onSuccess() {
                success.set(true);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                success.set(false);
                latch.countDown();
                assert false;
            }
        });

        CapturingTransport.CapturedRequest[] capturedRequests = transport.getCapturedRequestsAndClear();
        assertEquals(1, capturedRequests.length);
        // the request is a shard failed request
        assertThat(capturedRequests[0].request, is(instanceOf(ShardStateAction.FailedShardEntry.class)));
        ShardStateAction.FailedShardEntry shardEntry = (ShardStateAction.FailedShardEntry) capturedRequests[0].request;
        // for the right shard
        assertEquals(shardEntry.shardId, shardRouting.shardId());
        assertEquals(shardEntry.allocationId, shardRouting.allocationId().getId());
        // sent to the master
        assertEquals(clusterService.state().nodes().getMasterNode().getId(), capturedRequests[0].node.getId());

        transport.handleResponse(capturedRequests[0].requestId, TransportResponse.Empty.INSTANCE);

        latch.await();
        assertTrue(success.get());
    }

    public void testNoMaster() throws InterruptedException {
        final String index = "test";

        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));

        DiscoveryNodes.Builder noMasterBuilder = DiscoveryNodes.builder(clusterService.state().nodes());
        noMasterBuilder.masterNodeId(null);
        setState(clusterService, ClusterState.builder(clusterService.state()).nodes(noMasterBuilder));

        CountDownLatch latch = new CountDownLatch(1);
        AtomicInteger retries = new AtomicInteger();
        AtomicBoolean success = new AtomicBoolean();

        setUpMasterRetryVerification(1, retries, latch, requestId -> {
        });

        ShardRouting failedShard = getRandomShardRouting(index);
        shardStateAction.localShardFailed(failedShard, "test", getSimulatedFailure(), new ShardStateAction.Listener() {
            @Override
            public void onSuccess() {
                success.set(true);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                success.set(false);
                latch.countDown();
                assert false;
            }
        });

        latch.await();

        assertThat(retries.get(), equalTo(1));
        assertTrue(success.get());
    }

    public void testMasterChannelException() throws InterruptedException {
        final String index = "test";

        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));

        CountDownLatch latch = new CountDownLatch(1);
        AtomicInteger retries = new AtomicInteger();
        AtomicBoolean success = new AtomicBoolean();
        AtomicReference<Throwable> throwable = new AtomicReference<>();

        LongConsumer retryLoop = requestId -> {
            if (randomBoolean()) {
                transport.handleRemoteError(
                        requestId,
                        randomFrom(new NotMasterException("simulated"), new Discovery.FailedToCommitClusterStateException("simulated")));
            } else {
                if (randomBoolean()) {
                    transport.handleLocalError(requestId, new NodeNotConnectedException(null, "simulated"));
                } else {
                    transport.handleError(requestId, new NodeDisconnectedException(null, ShardStateAction.SHARD_FAILED_ACTION_NAME));
                }
            }
        };

        final int numberOfRetries = randomIntBetween(1, 256);
        setUpMasterRetryVerification(numberOfRetries, retries, latch, retryLoop);

        ShardRouting failedShard = getRandomShardRouting(index);
        shardStateAction.localShardFailed(failedShard, "test", getSimulatedFailure(), new ShardStateAction.Listener() {
            @Override
            public void onSuccess() {
                success.set(true);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                success.set(false);
                throwable.set(e);
                latch.countDown();
                assert false;
            }
        });

        final CapturingTransport.CapturedRequest[] capturedRequests = transport.getCapturedRequestsAndClear();
        assertThat(capturedRequests.length, equalTo(1));
        assertFalse(success.get());
        assertThat(retries.get(), equalTo(0));
        retryLoop.accept(capturedRequests[0].requestId);

        latch.await();
        assertNull(throwable.get());
        assertThat(retries.get(), equalTo(numberOfRetries));
        assertTrue(success.get());
    }

    public void testUnhandledFailure() {
        final String index = "test";

        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));

        AtomicBoolean failure = new AtomicBoolean();

        ShardRouting failedShard = getRandomShardRouting(index);
        shardStateAction.localShardFailed(failedShard, "test", getSimulatedFailure(), new ShardStateAction.Listener() {
            @Override
            public void onSuccess() {
                failure.set(false);
                assert false;
            }

            @Override
            public void onFailure(Exception e) {
                failure.set(true);
            }
        });

        final CapturingTransport.CapturedRequest[] capturedRequests = transport.getCapturedRequestsAndClear();
        assertThat(capturedRequests.length, equalTo(1));
        assertFalse(failure.get());
        transport.handleRemoteError(capturedRequests[0].requestId, new TransportException("simulated"));

        assertTrue(failure.get());
    }

    public void testShardNotFound() throws InterruptedException {
        final String index = "test";

        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));

        AtomicBoolean success = new AtomicBoolean();
        CountDownLatch latch = new CountDownLatch(1);

        ShardRouting failedShard = getRandomShardRouting(index);
        RoutingTable routingTable = RoutingTable.builder(clusterService.state().getRoutingTable()).remove(index).build();
        setState(clusterService, ClusterState.builder(clusterService.state()).routingTable(routingTable));
        shardStateAction.localShardFailed(failedShard, "test", getSimulatedFailure(), new ShardStateAction.Listener() {
            @Override
            public void onSuccess() {
                success.set(true);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                success.set(false);
                latch.countDown();
                assert false;
            }
        });

        CapturingTransport.CapturedRequest[] capturedRequests = transport.getCapturedRequestsAndClear();
        transport.handleResponse(capturedRequests[0].requestId, TransportResponse.Empty.INSTANCE);

        latch.await();
        assertTrue(success.get());
    }

    public void testNoLongerPrimaryShardException() throws InterruptedException {
        final String index = "test";

        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));

        ShardRouting failedShard = getRandomShardRouting(index);

        AtomicReference<Throwable> failure = new AtomicReference<>();
        CountDownLatch latch = new CountDownLatch(1);

        long primaryTerm = clusterService.state().metaData().index(index).primaryTerm(failedShard.id());
        assertThat(primaryTerm, greaterThanOrEqualTo(1L));
        shardStateAction.remoteShardFailed(failedShard.shardId(), failedShard.allocationId().getId(), primaryTerm + 1, randomBoolean(), "test",
            getSimulatedFailure(), new ShardStateAction.Listener() {
            @Override
            public void onSuccess() {
                failure.set(null);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                failure.set(e);
                latch.countDown();
            }
        });

        ShardStateAction.NoLongerPrimaryShardException catastrophicError =
                new ShardStateAction.NoLongerPrimaryShardException(failedShard.shardId(), "dummy failure");
        CapturingTransport.CapturedRequest[] capturedRequests = transport.getCapturedRequestsAndClear();
        transport.handleRemoteError(capturedRequests[0].requestId, catastrophicError);

        latch.await();
        assertNotNull(failure.get());
        assertThat(failure.get(), instanceOf(ShardStateAction.NoLongerPrimaryShardException.class));
        assertThat(failure.get().getMessage(), equalTo(catastrophicError.getMessage()));
    }

    public void testCacheRemoteShardFailed() throws Exception {
        final String index = "test";
        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));
        ShardRouting failedShard = getRandomShardRouting(index);
        boolean markAsStale = randomBoolean();
        int numListeners = between(1, 100);
        CountDownLatch latch = new CountDownLatch(numListeners);
        long primaryTerm = randomLongBetween(1, Long.MAX_VALUE);
        for (int i = 0; i < numListeners; i++) {
            shardStateAction.remoteShardFailed(failedShard.shardId(), failedShard.allocationId().getId(),
                primaryTerm, markAsStale, "test", getSimulatedFailure(), new ShardStateAction.Listener() {
                    @Override
                    public void onSuccess() {
                        latch.countDown();
                    }
                    @Override
                    public void onFailure(Exception e) {
                        latch.countDown();
                    }
                });
        }
        CapturingTransport.CapturedRequest[] capturedRequests = transport.getCapturedRequestsAndClear();
        assertThat(capturedRequests, arrayWithSize(1));
        transport.handleResponse(capturedRequests[0].requestId, TransportResponse.Empty.INSTANCE);
        latch.await();
        assertThat(transport.capturedRequests(), arrayWithSize(0));
    }

    public void testRemoteShardFailedConcurrently() throws Exception {
        final String index = "test";
        final AtomicBoolean shutdown = new AtomicBoolean(false);
        setState(clusterService, ClusterStateCreationUtils.stateWithActivePrimary(index, true, randomInt(5)));
        ShardRouting[] failedShards = new ShardRouting[between(1, 5)];
        for (int i = 0; i < failedShards.length; i++) {
            failedShards[i] = getRandomShardRouting(index);
        }
        Thread[] clientThreads = new Thread[between(1, 6)];
        int iterationsPerThread = scaledRandomIntBetween(50, 500);
        Phaser barrier = new Phaser(clientThreads.length + 2); // one for master thread, one for the main thread
        Thread masterThread = new Thread(() -> {
            barrier.arriveAndAwaitAdvance();
            while (shutdown.get() == false) {
                for (CapturingTransport.CapturedRequest request : transport.getCapturedRequestsAndClear()) {
                    if (randomBoolean()) {
                        transport.handleResponse(request.requestId, TransportResponse.Empty.INSTANCE);
                    } else {
                        transport.handleRemoteError(request.requestId, randomFrom(getSimulatedFailure()));
                    }
                }
            }
        });
        masterThread.start();

        AtomicInteger notifiedResponses = new AtomicInteger();
        for (int t = 0; t < clientThreads.length; t++) {
            clientThreads[t] = new Thread(() -> {
                barrier.arriveAndAwaitAdvance();
                for (int i = 0; i < iterationsPerThread; i++) {
                    ShardRouting failedShard = randomFrom(failedShards);
                    shardStateAction.remoteShardFailed(failedShard.shardId(), failedShard.allocationId().getId(),
                        randomLongBetween(1, Long.MAX_VALUE), randomBoolean(), "test", getSimulatedFailure(), new ShardStateAction.Listener() {
                            @Override
                            public void onSuccess() {
                                notifiedResponses.incrementAndGet();
                            }
                            @Override
                            public void onFailure(Exception e) {
                                notifiedResponses.incrementAndGet();
                            }
                        });
                }
            });
            clientThreads[t].start();
        }
        barrier.arriveAndAwaitAdvance();
        for (Thread t : clientThreads) {
            t.join();
        }
        assertBusy(() -> assertThat(notifiedResponses.get(), equalTo(clientThreads.length * iterationsPerThread)));
        shutdown.set(true);
        masterThread.join();
    }

    private ShardRouting getRandomShardRouting(String index) {
        IndexRoutingTable indexRoutingTable = clusterService.state().routingTable().index(index);
        ShardsIterator shardsIterator = indexRoutingTable.randomAllActiveShardsIt();
        ShardRouting shardRouting = shardsIterator.nextOrNull();
        assert shardRouting != null;
        return shardRouting;
    }

    private void setUpMasterRetryVerification(int numberOfRetries, AtomicInteger retries, CountDownLatch latch, LongConsumer retryLoop) {
        shardStateAction.setOnBeforeWaitForNewMasterAndRetry(() -> {
            DiscoveryNodes.Builder masterBuilder = DiscoveryNodes.builder(clusterService.state().nodes());
            masterBuilder.masterNodeId(clusterService.state().nodes().getMasterNodes().iterator().next().value.getId());
            setState(clusterService, ClusterState.builder(clusterService.state()).nodes(masterBuilder));
        });

        shardStateAction.setOnAfterWaitForNewMasterAndRetry(() -> verifyRetry(numberOfRetries, retries, latch, retryLoop));
    }

    private void verifyRetry(int numberOfRetries, AtomicInteger retries, CountDownLatch latch, LongConsumer retryLoop) {
        // assert a retry request was sent
        final CapturingTransport.CapturedRequest[] capturedRequests = transport.getCapturedRequestsAndClear();
        if (capturedRequests.length == 1) {
            retries.incrementAndGet();
            if (retries.get() == numberOfRetries) {
                // finish the request
                transport.handleResponse(capturedRequests[0].requestId, TransportResponse.Empty.INSTANCE);
            } else {
                retryLoop.accept(capturedRequests[0].requestId);
            }
        } else {
            // there failed to be a retry request
            // release the driver thread to fail the test
            latch.countDown();
        }
    }

    private Exception getSimulatedFailure() {
        return new CorruptIndexException("simulated", (String) null);
    }

    public void testShardEntryBWCSerialize() throws Exception {
        final Version bwcVersion = randomValueOtherThanMany(
            version -> version.onOrAfter(Version.V_6_3_0), () -> VersionUtils.randomVersion(random()));
        final ShardId shardId = new ShardId(randomRealisticUnicodeOfLengthBetween(10, 100), UUID.randomUUID().toString(), between(0, 1000));
        final String allocationId = randomRealisticUnicodeOfCodepointLengthBetween(10, 100);
        final String reason = randomRealisticUnicodeOfCodepointLengthBetween(10, 100);
        try (StreamInput in = serialize(new StartedShardEntry(shardId, allocationId, reason), bwcVersion).streamInput()) {
            in.setVersion(bwcVersion);
            final FailedShardEntry failedShardEntry = new FailedShardEntry(in);
            assertThat(failedShardEntry.shardId, equalTo(shardId));
            assertThat(failedShardEntry.allocationId, equalTo(allocationId));
            assertThat(failedShardEntry.message, equalTo(reason));
            assertThat(failedShardEntry.failure, nullValue());
            assertThat(failedShardEntry.markAsStale, equalTo(true));
        }
        try (StreamInput in = serialize(new FailedShardEntry(shardId, allocationId, 0L, reason, null, false), bwcVersion).streamInput()) {
            in.setVersion(bwcVersion);
            final StartedShardEntry startedShardEntry = new StartedShardEntry(in);
            assertThat(startedShardEntry.shardId, equalTo(shardId));
            assertThat(startedShardEntry.allocationId, equalTo(allocationId));
            assertThat(startedShardEntry.message, equalTo(reason));
        }
    }

    BytesReference serialize(Writeable writeable, Version version) throws IOException {
        try (BytesStreamOutput out = new BytesStreamOutput()) {
            out.setVersion(version);
            writeable.writeTo(out);
            return out.bytes();
        }
    }

    public void testCompositeListener() throws Exception {
        AtomicInteger successCount = new AtomicInteger();
        AtomicInteger failureCount = new AtomicInteger();
        Exception failure = randomBoolean() ? getSimulatedFailure() : null;
        ShardStateAction.CompositeListener compositeListener = new ShardStateAction.CompositeListener(new ShardStateAction.Listener() {
            @Override
            public void onSuccess() {
                successCount.incrementAndGet();
            }
            @Override
            public void onFailure(Exception e) {
                assertThat(e, sameInstance(failure));
                failureCount.incrementAndGet();
            }
        });
        int iterationsPerThread = scaledRandomIntBetween(100, 1000);
        Thread[] threads = new Thread[between(1, 4)];
        Phaser barrier = new Phaser(threads.length + 1);
        for (int i = 0; i < threads.length; i++) {
            threads[i] = new Thread(() -> {
                barrier.arriveAndAwaitAdvance();
                for (int n = 0; n < iterationsPerThread; n++) {
                    compositeListener.addListener(new ShardStateAction.Listener() {
                        @Override
                        public void onSuccess() {
                            successCount.incrementAndGet();
                        }
                        @Override
                        public void onFailure(Exception e) {
                            assertThat(e, sameInstance(failure));
                            failureCount.incrementAndGet();
                        }
                    });
                }
            });
            threads[i].start();
        }
        barrier.arriveAndAwaitAdvance();
        if (failure != null) {
            compositeListener.onFailure(failure);
        } else {
            compositeListener.onSuccess();
        }
        for (Thread t : threads) {
            t.join();
        }
        assertBusy(() -> {
            if (failure != null) {
                assertThat(successCount.get(), equalTo(0));
                assertThat(failureCount.get(), equalTo(threads.length*iterationsPerThread + 1));
            } else {
                assertThat(successCount.get(), equalTo(threads.length*iterationsPerThread + 1));
                assertThat(failureCount.get(), equalTo(0));
            }
        });
    }
}
