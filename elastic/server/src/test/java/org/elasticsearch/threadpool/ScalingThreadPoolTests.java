/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.threadpool;

import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.EsThreadPoolExecutor;

import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executor;
import java.util.concurrent.TimeUnit;
import java.util.function.BiConsumer;
import java.util.function.Function;

import static org.hamcrest.CoreMatchers.instanceOf;
import static org.hamcrest.Matchers.equalTo;

public class ScalingThreadPoolTests extends ESThreadPoolTestCase {

    public void testScalingThreadPoolConfiguration() throws InterruptedException {
        final String threadPoolName = randomThreadPool(ThreadPool.ThreadPoolType.SCALING);
        final Settings.Builder builder = Settings.builder();

        final int core;
        if (randomBoolean()) {
            core = randomIntBetween(0, 8);
            builder.put("thread_pool." + threadPoolName + ".core", core);
        } else {
            core = "generic".equals(threadPoolName) ? 4 : 1; // the defaults
        }

        final int availableProcessors = Runtime.getRuntime().availableProcessors();
        final int maxBasedOnNumberOfProcessors;
        final int processors;
        if (randomBoolean()) {
            processors = randomIntBetween(1, availableProcessors);
            maxBasedOnNumberOfProcessors = expectedSize(threadPoolName, processors);
            builder.put("node.processors", processors);
        } else {
            maxBasedOnNumberOfProcessors = expectedSize(threadPoolName, availableProcessors);
            processors = availableProcessors;
        }

        final int expectedMax;
        if (maxBasedOnNumberOfProcessors < core || randomBoolean()) {
            expectedMax = randomIntBetween(Math.max(1, core), 16);
            builder.put("thread_pool." + threadPoolName + ".max", expectedMax);
        }  else {
            expectedMax = maxBasedOnNumberOfProcessors;
        }

        final long keepAlive;
        if (randomBoolean()) {
            keepAlive = randomIntBetween(1, 300);
            builder.put("thread_pool." + threadPoolName + ".keep_alive", keepAlive + "s");
        } else {
            keepAlive = "generic".equals(threadPoolName) ? 30 : 300; // the defaults
        }

        runScalingThreadPoolTest(builder.build(), (clusterSettings, threadPool) -> {
            final Executor executor = threadPool.executor(threadPoolName);
            assertThat(executor, instanceOf(EsThreadPoolExecutor.class));
            final EsThreadPoolExecutor esThreadPoolExecutor = (EsThreadPoolExecutor)executor;
            final ThreadPool.Info info = info(threadPool, threadPoolName);

            assertThat(info.getName(), equalTo(threadPoolName));
            assertThat(info.getThreadPoolType(), equalTo(ThreadPool.ThreadPoolType.SCALING));

            assertThat(info.getKeepAlive().seconds(), equalTo(keepAlive));
            assertThat(esThreadPoolExecutor.getKeepAliveTime(TimeUnit.SECONDS), equalTo(keepAlive));

            assertNull(info.getQueueSize());
            assertThat(esThreadPoolExecutor.getQueue().remainingCapacity(), equalTo(Integer.MAX_VALUE));

            assertThat(info.getMin(), equalTo(core));
            assertThat(esThreadPoolExecutor.getCorePoolSize(), equalTo(core));
            assertThat(info.getMax(), equalTo(expectedMax));
            assertThat(esThreadPoolExecutor.getMaximumPoolSize(), equalTo(expectedMax));
        });

    }

    private int expectedSize(final String threadPoolName, final int numberOfProcessors) {
        final Map<String, Function<Integer, Integer>> sizes = new HashMap<>();
        sizes.put(ThreadPool.Names.GENERIC, n -> ThreadPool.boundedBy(4 * n, 128, 512));
        sizes.put(ThreadPool.Names.MANAGEMENT, n -> ThreadPool.boundedBy(n, 1, 5));
        sizes.put(ThreadPool.Names.FLUSH, ThreadPool::halfAllocatedProcessorsMaxFive);
        sizes.put(ThreadPool.Names.REFRESH, ThreadPool::halfAllocatedProcessorsMaxTen);
        sizes.put(ThreadPool.Names.WARMER, ThreadPool::halfAllocatedProcessorsMaxFive);
        sizes.put(ThreadPool.Names.SNAPSHOT, ThreadPool::halfAllocatedProcessorsMaxFive);
        sizes.put(ThreadPool.Names.FETCH_SHARD_STARTED, ThreadPool::twiceAllocatedProcessors);
        sizes.put(ThreadPool.Names.FETCH_SHARD_STORE, ThreadPool::twiceAllocatedProcessors);
        return sizes.get(threadPoolName).apply(numberOfProcessors);
    }

    public void testScalingThreadPoolIsBounded() throws InterruptedException {
        final String threadPoolName = randomThreadPool(ThreadPool.ThreadPoolType.SCALING);
        final int size = randomIntBetween(32, 512);
        final Settings settings = Settings.builder().put("thread_pool." + threadPoolName + ".max", size).build();
        runScalingThreadPoolTest(settings, (clusterSettings, threadPool) -> {
            final CountDownLatch latch = new CountDownLatch(1);
            final int numberOfTasks = 2 * size;
            final CountDownLatch taskLatch = new CountDownLatch(numberOfTasks);
            for (int i = 0; i < numberOfTasks; i++) {
                threadPool.executor(threadPoolName).execute(() -> {
                    try {
                        latch.await();
                        taskLatch.countDown();
                    } catch (final InterruptedException e) {
                        throw new RuntimeException(e);
                    }
                });
            }
            final ThreadPoolStats.Stats stats = stats(threadPool, threadPoolName);
            assertThat(stats.getQueue(), equalTo(numberOfTasks - size));
            assertThat(stats.getLargest(), equalTo(size));
            latch.countDown();
            try {
                taskLatch.await();
            } catch (InterruptedException e) {
                throw new RuntimeException(e);
            }
        });
    }

    public void testScalingThreadPoolThreadsAreTerminatedAfterKeepAlive() throws InterruptedException {
        final String threadPoolName = randomThreadPool(ThreadPool.ThreadPoolType.SCALING);
        final int min = "generic".equals(threadPoolName) ? 4 : 1;
        final Settings settings =
                Settings.builder()
                        .put("thread_pool." + threadPoolName + ".max", 128)
                        .put("thread_pool." + threadPoolName + ".keep_alive", "1ms")
                        .build();
        runScalingThreadPoolTest(settings, ((clusterSettings, threadPool) -> {
            final CountDownLatch latch = new CountDownLatch(1);
            final CountDownLatch taskLatch = new CountDownLatch(128);
            for (int i = 0; i < 128; i++) {
                threadPool.executor(threadPoolName).execute(() -> {
                    try {
                        latch.await();
                        taskLatch.countDown();
                    } catch (final InterruptedException e) {
                        throw new RuntimeException(e);
                    }
                });
            }
            int threads = stats(threadPool, threadPoolName).getThreads();
            assertEquals(128, threads);
            latch.countDown();
            // this while loop is the core of this test; if threads
            // are correctly idled down by the pool, the number of
            // threads in the pool will drop to the min for the pool
            // but if threads are not correctly idled down by the pool,
            // this test will just timeout waiting for them to idle
            // down
            do {
                spinForAtLeastOneMillisecond();
            } while (stats(threadPool, threadPoolName).getThreads() > min);
            try {
                taskLatch.await();
            } catch (InterruptedException e) {
                throw new RuntimeException(e);
            }
        }));
    }

    public void runScalingThreadPoolTest(
            final Settings settings,
            final BiConsumer<ClusterSettings, ThreadPool> consumer) throws InterruptedException {
        ThreadPool threadPool = null;
        try {
            final String test = Thread.currentThread().getStackTrace()[2].getMethodName();
            final Settings nodeSettings = Settings.builder().put(settings).put("node.name", test).build();
            threadPool = new ThreadPool(nodeSettings);
            final ClusterSettings clusterSettings = new ClusterSettings(nodeSettings, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS);
            consumer.accept(clusterSettings, threadPool);
        } finally {
            terminateThreadPoolIfNeeded(threadPool);
        }
    }
}
