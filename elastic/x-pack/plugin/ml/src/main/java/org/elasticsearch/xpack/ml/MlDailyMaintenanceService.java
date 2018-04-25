/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml;

import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.common.lease.Releasable;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.EsRejectedExecutionException;
import org.elasticsearch.common.util.concurrent.FutureUtils;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.action.DeleteExpiredDataAction;
import org.joda.time.DateTime;
import org.joda.time.chrono.ISOChronology;

import java.util.Objects;
import java.util.Random;
import java.util.concurrent.ScheduledFuture;
import java.util.function.Supplier;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

/**
 * A service that runs once a day and triggers maintenance tasks.
 */
public class MlDailyMaintenanceService implements Releasable {

    private static final Logger LOGGER = Loggers.getLogger(MlDailyMaintenanceService.class);

    private static final int MAX_TIME_OFFSET_MINUTES = 120;

    private final ThreadPool threadPool;
    private final Client client;

    /**
     * An interface to abstract the calculation of the delay to the next execution.
     * Needed to enable testing.
     */
    private final Supplier<TimeValue> schedulerProvider;

    private volatile ScheduledFuture<?> future;

    MlDailyMaintenanceService(ThreadPool threadPool, Client client, Supplier<TimeValue> scheduleProvider) {
        this.threadPool = Objects.requireNonNull(threadPool);
        this.client = Objects.requireNonNull(client);
        this.schedulerProvider = Objects.requireNonNull(scheduleProvider);
    }

    public MlDailyMaintenanceService(ClusterName clusterName, ThreadPool threadPool, Client client) {
        this(threadPool, client, () -> delayToNextTime(clusterName));
    }

    /**
     * Calculates the delay until the next time the maintenance should be triggered.
     * The next time is 30 minutes past midnight of the following day plus a random
     * offset. The random offset is added in order to avoid multiple clusters
     * running the maintenance tasks at the same time. A cluster with a given name
     * shall have the same offset throughout its life.
     *
     * @param clusterName the cluster name is used to seed the random offset
     * @return the delay to the next time the maintenance should be triggered
     */
    private static TimeValue delayToNextTime(ClusterName clusterName) {
        Random random = new Random(clusterName.hashCode());
        int minutesOffset = random.ints(0, MAX_TIME_OFFSET_MINUTES).findFirst().getAsInt();
        DateTime now = DateTime.now(ISOChronology.getInstance());
        DateTime next = now.plusDays(1).withTimeAtStartOfDay().plusMinutes(30).plusMinutes(minutesOffset);
        return TimeValue.timeValueMillis(next.getMillis() - now.getMillis());
    }

    public void start() {
        LOGGER.debug("Starting ML daily maintenance service");
        scheduleNext();
    }

    public void stop() {
        LOGGER.debug("Stopping ML daily maintenance service");
        if (future != null && future.isCancelled() == false) {
            FutureUtils.cancel(future);
        }
    }

    public boolean isStarted() {
        return future != null;
    }

    @Override
    public void close() {
        stop();
    }

    private void scheduleNext() {
        try {
            future = threadPool.schedule(schedulerProvider.get(), ThreadPool.Names.GENERIC, this::triggerTasks);
        } catch (EsRejectedExecutionException e) {
            if (e.isExecutorShutdown()) {
                LOGGER.debug("failed to schedule next maintenance task; shutting down", e);
            } else {
                throw e;
            }
        }
    }

    private void triggerTasks() {
        LOGGER.info("triggering scheduled [ML] maintenance tasks");
        executeAsyncWithOrigin(client, ML_ORIGIN, DeleteExpiredDataAction.INSTANCE, new DeleteExpiredDataAction.Request(),
                ActionListener.wrap(
                        response -> LOGGER.info("Successfully completed [ML] maintenance tasks"),
                        e -> LOGGER.error("An error occurred during maintenance tasks execution", e)));
        scheduleNext();
    }
}
