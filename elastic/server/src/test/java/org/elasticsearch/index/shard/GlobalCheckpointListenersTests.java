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
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.index.Index;
import org.elasticsearch.test.ESTestCase;
import org.mockito.ArgumentCaptor;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.List;
import java.util.concurrent.BrokenBarrierException;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.CyclicBarrier;
import java.util.concurrent.Executor;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

import static org.elasticsearch.index.seqno.SequenceNumbers.NO_OPS_PERFORMED;
import static org.elasticsearch.index.seqno.SequenceNumbers.UNASSIGNED_SEQ_NO;
import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.reset;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;

public class GlobalCheckpointListenersTests extends ESTestCase {

    final ShardId shardId = new ShardId(new Index("index", "uuid"), 0);

    public void testGlobalCheckpointUpdated() throws IOException {
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, Runnable::run, logger);
        globalCheckpointListeners.globalCheckpointUpdated(NO_OPS_PERFORMED);
        final int numberOfListeners = randomIntBetween(0, 16);
        final long[] globalCheckpoints = new long[numberOfListeners];
        for (int i = 0; i < numberOfListeners; i++) {
            final int index = i;
            final AtomicBoolean invoked = new AtomicBoolean();
            final GlobalCheckpointListeners.GlobalCheckpointListener listener =
                    (g, e) -> {
                        if (invoked.compareAndSet(false, true) == false) {
                            throw new IllegalStateException("listener invoked twice");
                        }
                        assert g != UNASSIGNED_SEQ_NO;
                        assert e == null;
                        globalCheckpoints[index] = g;
                    };
            globalCheckpointListeners.add(NO_OPS_PERFORMED, listener);
        }
        final long globalCheckpoint = randomLongBetween(NO_OPS_PERFORMED, Long.MAX_VALUE);
        globalCheckpointListeners.globalCheckpointUpdated(globalCheckpoint);
        for (int i = 0; i < numberOfListeners; i++) {
            assertThat(globalCheckpoints[i], equalTo(globalCheckpoint));
        }

        // test the listeners are not invoked twice
        final long nextGlobalCheckpoint = randomLongBetween(globalCheckpoint + 1, Long.MAX_VALUE);
        globalCheckpointListeners.globalCheckpointUpdated(nextGlobalCheckpoint);
        for (int i = 0; i < numberOfListeners; i++) {
            assertThat(globalCheckpoints[i], equalTo(globalCheckpoint));
        }

        // closing should also not notify the listeners
        globalCheckpointListeners.close();
        for (int i = 0; i < numberOfListeners; i++) {
            assertThat(globalCheckpoints[i], equalTo(globalCheckpoint));
        }
    }

    public void testListenersReadyToBeNotified() throws IOException {
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, Runnable::run, logger);
        final long globalCheckpoint = randomLongBetween(NO_OPS_PERFORMED + 1, Long.MAX_VALUE);
        globalCheckpointListeners.globalCheckpointUpdated(globalCheckpoint);
        final int numberOfListeners = randomIntBetween(0, 16);
        final long[] globalCheckpoints = new long[numberOfListeners];
        for (int i = 0; i < numberOfListeners; i++) {
            final int index = i;
            final AtomicBoolean invoked = new AtomicBoolean();
            final GlobalCheckpointListeners.GlobalCheckpointListener listener =
                    (g, e) -> {
                        if (invoked.compareAndSet(false, true) == false) {
                            throw new IllegalStateException("listener invoked twice");
                        }
                        assert g != UNASSIGNED_SEQ_NO;
                        assert e == null;
                        globalCheckpoints[index] = g;
                    };
            globalCheckpointListeners.add(randomLongBetween(NO_OPS_PERFORMED, globalCheckpoint - 1), listener);
            // the listener should be notified immediately
            assertThat(globalCheckpoints[index], equalTo(globalCheckpoint));
        }

        // test the listeners are not invoked twice
        final long nextGlobalCheckpoint = randomLongBetween(globalCheckpoint + 1, Long.MAX_VALUE);
        globalCheckpointListeners.globalCheckpointUpdated(nextGlobalCheckpoint);
        for (int i = 0; i < numberOfListeners; i++) {
            assertThat(globalCheckpoints[i], equalTo(globalCheckpoint));
        }

        // closing should also not notify the listeners
        globalCheckpointListeners.close();
        for (int i = 0; i < numberOfListeners; i++) {
            assertThat(globalCheckpoints[i], equalTo(globalCheckpoint));
        }
    }

    public void testFailingListenerReadyToBeNotified() {
        final Logger mockLogger = mock(Logger.class);
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, Runnable::run, mockLogger);
        final long globalCheckpoint = randomLongBetween(NO_OPS_PERFORMED + 1, Long.MAX_VALUE);
        globalCheckpointListeners.globalCheckpointUpdated(globalCheckpoint);
        final int numberOfListeners = randomIntBetween(0, 16);
        final long[] globalCheckpoints = new long[numberOfListeners];
        for (int i = 0; i < numberOfListeners; i++) {
            final int index = i;
            final boolean failure = randomBoolean();
            final GlobalCheckpointListeners.GlobalCheckpointListener listener =
                    (g, e) -> {
                        assert globalCheckpoint != UNASSIGNED_SEQ_NO;
                        assert e == null;
                        if (failure) {
                            globalCheckpoints[index] = Long.MIN_VALUE;
                            throw new RuntimeException("failure");
                        } else {
                            globalCheckpoints[index] = globalCheckpoint;
                        }
                    };
            globalCheckpointListeners.add(randomLongBetween(NO_OPS_PERFORMED, globalCheckpoint - 1), listener);
            // the listener should be notified immediately
            if (failure) {
                assertThat(globalCheckpoints[i], equalTo(Long.MIN_VALUE));
                final ArgumentCaptor<ParameterizedMessage> message = ArgumentCaptor.forClass(ParameterizedMessage.class);
                final ArgumentCaptor<RuntimeException> t = ArgumentCaptor.forClass(RuntimeException.class);
                verify(mockLogger).warn(message.capture(), t.capture());
                reset(mockLogger);
                assertThat(
                        message.getValue().getFormat(),
                        equalTo("error notifying global checkpoint listener of updated global checkpoint [{}]"));
                assertNotNull(message.getValue().getParameters());
                assertThat(message.getValue().getParameters().length, equalTo(1));
                assertThat(message.getValue().getParameters()[0], equalTo(globalCheckpoint));
                assertNotNull(t.getValue());
                assertThat(t.getValue().getMessage(), equalTo("failure"));
            } else {
                assertThat(globalCheckpoints[i], equalTo(globalCheckpoint));
            }
        }
    }

    public void testClose() throws IOException {
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, Runnable::run, logger);
        globalCheckpointListeners.globalCheckpointUpdated(NO_OPS_PERFORMED);
        final int numberOfListeners = randomIntBetween(0, 16);
        final IndexShardClosedException[] exceptions = new IndexShardClosedException[numberOfListeners];
        for (int i = 0; i < numberOfListeners; i++) {
            final int index = i;
            final AtomicBoolean invoked = new AtomicBoolean();
            final GlobalCheckpointListeners.GlobalCheckpointListener listener =
                    (globalCheckpoint, e) -> {
                        if (invoked.compareAndSet(false, true) == false) {
                            throw new IllegalStateException("listener invoked twice");
                        }
                        assert globalCheckpoint == UNASSIGNED_SEQ_NO;
                        assert e != null;
                        exceptions[index] = e;
                    };
            globalCheckpointListeners.add(NO_OPS_PERFORMED, listener);
        }
        globalCheckpointListeners.close();
        for (int i = 0; i < numberOfListeners; i++) {
            assertNotNull(exceptions[i]);
            assertThat(exceptions[i].getShardId(), equalTo(shardId));
        }

        // test the listeners are not invoked twice
        for (int i = 0; i < numberOfListeners; i++) {
            exceptions[i] = null;
        }
        globalCheckpointListeners.close();
        for (int i = 0; i < numberOfListeners; i++) {
            assertNull(exceptions[i]);
        }
    }

    public void testAddAfterClose() throws InterruptedException, IOException {
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, Runnable::run, logger);
        globalCheckpointListeners.globalCheckpointUpdated(NO_OPS_PERFORMED);
        globalCheckpointListeners.close();
        final AtomicBoolean invoked = new AtomicBoolean();
        final CountDownLatch latch = new CountDownLatch(1);
        final GlobalCheckpointListeners.GlobalCheckpointListener listener = (g, e) -> {
            assert g == UNASSIGNED_SEQ_NO;
            assert e != null;
            if (invoked.compareAndSet(false, true) == false) {
                latch.countDown();
                throw new IllegalStateException("listener invoked twice");
            }
            latch.countDown();
        };
        globalCheckpointListeners.add(randomLongBetween(NO_OPS_PERFORMED, Long.MAX_VALUE), listener);
        latch.await();
        assertTrue(invoked.get());
    }

    public void testFailingListenerOnUpdate() {
        final Logger mockLogger = mock(Logger.class);
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, Runnable::run, mockLogger);
        globalCheckpointListeners.globalCheckpointUpdated(NO_OPS_PERFORMED);
        final int numberOfListeners = randomIntBetween(0, 16);
        final boolean[] failures = new boolean[numberOfListeners];
        final long[] globalCheckpoints = new long[numberOfListeners];
        for (int i = 0; i < numberOfListeners; i++) {
            final int index = i;
            final boolean failure = randomBoolean();
            failures[index] = failure;
            final GlobalCheckpointListeners.GlobalCheckpointListener listener =
                    (g, e) -> {
                        assert g != UNASSIGNED_SEQ_NO;
                        assert e == null;
                        if (failure) {
                            globalCheckpoints[index] = Long.MIN_VALUE;
                            throw new RuntimeException("failure");
                        } else {
                            globalCheckpoints[index] = g;
                        }
                    };
            globalCheckpointListeners.add(NO_OPS_PERFORMED, listener);
        }
        final long globalCheckpoint = randomLongBetween(NO_OPS_PERFORMED, Long.MAX_VALUE);
        globalCheckpointListeners.globalCheckpointUpdated(globalCheckpoint);
        for (int i = 0; i < numberOfListeners; i++) {
            if (failures[i]) {
                assertThat(globalCheckpoints[i], equalTo(Long.MIN_VALUE));
            } else {
                assertThat(globalCheckpoints[i], equalTo(globalCheckpoint));
            }
        }
        int failureCount = 0;
        for (int i = 0; i < numberOfListeners; i++) {
            if (failures[i]) {
                failureCount++;
            }
        }
        if (failureCount > 0) {
            final ArgumentCaptor<ParameterizedMessage> message = ArgumentCaptor.forClass(ParameterizedMessage.class);
            final ArgumentCaptor<RuntimeException> t = ArgumentCaptor.forClass(RuntimeException.class);
            verify(mockLogger, times(failureCount)).warn(message.capture(), t.capture());
            assertThat(
                    message.getValue().getFormat(),
                    equalTo("error notifying global checkpoint listener of updated global checkpoint [{}]"));
            assertNotNull(message.getValue().getParameters());
            assertThat(message.getValue().getParameters().length, equalTo(1));
            assertThat(message.getValue().getParameters()[0], equalTo(globalCheckpoint));
            assertNotNull(t.getValue());
            assertThat(t.getValue().getMessage(), equalTo("failure"));
        }
    }

    public void testFailingListenerOnClose() throws IOException {
        final Logger mockLogger = mock(Logger.class);
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, Runnable::run, mockLogger);
        globalCheckpointListeners.globalCheckpointUpdated(NO_OPS_PERFORMED);
        final int numberOfListeners = randomIntBetween(0, 16);
        final boolean[] failures = new boolean[numberOfListeners];
        final IndexShardClosedException[] exceptions = new IndexShardClosedException[numberOfListeners];
        for (int i = 0; i < numberOfListeners; i++) {
            final int index = i;
            final boolean failure = randomBoolean();
            failures[index] = failure;
            final GlobalCheckpointListeners.GlobalCheckpointListener listener =
                    (g, e) -> {
                        assert g == UNASSIGNED_SEQ_NO;
                        assert e != null;
                        if (failure) {
                            throw new RuntimeException("failure");
                        } else {
                            exceptions[index] = e;
                        }
                    };
            globalCheckpointListeners.add(NO_OPS_PERFORMED, listener);
        }
        globalCheckpointListeners.close();
        for (int i = 0; i < numberOfListeners; i++) {
            if (failures[i]) {
                assertNull(exceptions[i]);
            } else {
                assertNotNull(exceptions[i]);
                assertThat(exceptions[i].getShardId(), equalTo(shardId));
            }
        }
        int failureCount = 0;
        for (int i = 0; i < numberOfListeners; i++) {
            if (failures[i]) {
                failureCount++;
            }
        }
        if (failureCount > 0) {
            final ArgumentCaptor<String> message = ArgumentCaptor.forClass(String.class);
            final ArgumentCaptor<RuntimeException> t = ArgumentCaptor.forClass(RuntimeException.class);
            verify(mockLogger, times(failureCount)).warn(message.capture(), t.capture());
            assertThat(message.getValue(), equalTo("error notifying global checkpoint listener of closed shard"));
            assertNotNull(t.getValue());
            assertThat(t.getValue().getMessage(), equalTo("failure"));
        }
    }

    public void testNotificationUsesExecutor() {
        final AtomicInteger count = new AtomicInteger();
        final Executor executor = command -> {
            count.incrementAndGet();
            command.run();
        };
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, executor, logger);
        globalCheckpointListeners.globalCheckpointUpdated(NO_OPS_PERFORMED);
        final int numberOfListeners = randomIntBetween(0, 16);
        for (int i = 0; i < numberOfListeners; i++) {
            globalCheckpointListeners.add(NO_OPS_PERFORMED, (g, e) -> {});
        }
        globalCheckpointListeners.globalCheckpointUpdated(randomLongBetween(NO_OPS_PERFORMED, Long.MAX_VALUE));
        assertThat(count.get(), equalTo(numberOfListeners == 0 ? 0 : 1));
    }

    public void testConcurrency() throws BrokenBarrierException, InterruptedException {
        final ExecutorService executor = Executors.newFixedThreadPool(randomIntBetween(1, 8));
        final GlobalCheckpointListeners globalCheckpointListeners = new GlobalCheckpointListeners(shardId, executor, logger);
        final AtomicLong globalCheckpoint = new AtomicLong(NO_OPS_PERFORMED);
        globalCheckpointListeners.globalCheckpointUpdated(globalCheckpoint.get());
        // we are going to synchronize the actions of three threads: the updating thread, the listener thread, and the main test thread
        final CyclicBarrier barrier = new CyclicBarrier(3);
        final int numberOfIterations = randomIntBetween(1, 1024);
        final AtomicBoolean closed = new AtomicBoolean();
        final Thread updatingThread = new Thread(() -> {
            // synchronize starting with the listener thread and the main test thread
            awaitQuietly(barrier);
            for (int i = 0; i < numberOfIterations; i++) {
                if (rarely() && closed.get() == false) {
                    closed.set(true);
                    try {
                        globalCheckpointListeners.close();
                    } catch (final IOException e) {
                        throw new UncheckedIOException(e);
                    }
                }
                if (closed.get() == false) {
                    globalCheckpointListeners.globalCheckpointUpdated(globalCheckpoint.incrementAndGet());
                }
            }
            // synchronize ending with the listener thread and the main test thread
            awaitQuietly(barrier);
        });

        final List<AtomicBoolean> invocations = new CopyOnWriteArrayList<>();
        final Thread listenersThread = new Thread(() -> {
            // synchronize starting with the updating thread and the main test thread
            awaitQuietly(barrier);
            for (int i = 0; i < numberOfIterations; i++) {
                final AtomicBoolean invocation = new AtomicBoolean();
                invocations.add(invocation);
                // sometimes this will notify the listener immediately
                globalCheckpointListeners.add(
                        globalCheckpoint.get(),
                        (g, e) -> {
                            if (invocation.compareAndSet(false, true) == false) {
                                throw new IllegalStateException("listener invoked twice");
                            }
                        });
            }
            // synchronize ending with the updating thread and the main test thread
            awaitQuietly(barrier);
        });
        updatingThread.start();
        listenersThread.start();
        // synchronize starting with the updating thread and the listener thread
        barrier.await();
        // synchronize ending with the updating thread and the listener thread
        barrier.await();
        // one last update to ensure all listeners are notified
        if (closed.get() == false) {
            globalCheckpointListeners.globalCheckpointUpdated(globalCheckpoint.incrementAndGet());
        }
        assertThat(globalCheckpointListeners.pendingListeners(), equalTo(0));
        executor.shutdown();
        executor.awaitTermination(Long.MAX_VALUE, TimeUnit.SECONDS);
        for (final AtomicBoolean invocation : invocations) {
            assertTrue(invocation.get());
        }
        updatingThread.join();
        listenersThread.join();
    }

    private void awaitQuietly(final CyclicBarrier barrier) {
        try {
            barrier.await();
        } catch (final BrokenBarrierException | InterruptedException e) {
            throw new AssertionError(e);
        }
    }

}
