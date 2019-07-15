/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.snapshotlifecycle;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateListener;
import org.elasticsearch.cluster.LocalNodeMasterListener;
import org.elasticsearch.cluster.metadata.RepositoriesMetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.indexlifecycle.OperationMode;
import org.elasticsearch.xpack.core.scheduler.CronSchedule;
import org.elasticsearch.xpack.core.scheduler.SchedulerEngine;
import org.elasticsearch.xpack.core.snapshotlifecycle.SnapshotLifecycleMetadata;
import org.elasticsearch.xpack.core.snapshotlifecycle.SnapshotLifecyclePolicy;
import org.elasticsearch.xpack.core.snapshotlifecycle.SnapshotLifecyclePolicyMetadata;

import java.io.Closeable;
import java.time.Clock;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.function.Supplier;
import java.util.regex.Pattern;
import java.util.stream.Collectors;

/**
 * {@code SnapshotLifecycleService} manages snapshot policy scheduling and triggering of the
 * {@link SnapshotLifecycleTask}. It reacts to new policies in the cluster state by scheduling a
 * task according to the policy's schedule.
 */
public class SnapshotLifecycleService implements LocalNodeMasterListener, Closeable, ClusterStateListener {

    private static final Logger logger = LogManager.getLogger(SnapshotLifecycleMetadata.class);
    private static final String JOB_PATTERN_SUFFIX = "-\\d+$";

    private final SchedulerEngine scheduler;
    private final ClusterService clusterService;
    private final SnapshotLifecycleTask snapshotTask;
    private final Map<String, SchedulerEngine.Job> scheduledTasks = ConcurrentCollections.newConcurrentMap();
    private volatile boolean isMaster = false;

    public SnapshotLifecycleService(Settings settings,
                                    Supplier<SnapshotLifecycleTask> taskSupplier,
                                    ClusterService clusterService,
                                    Clock clock) {
        this.scheduler = new SchedulerEngine(settings, clock);
        this.clusterService = clusterService;
        this.snapshotTask = taskSupplier.get();
        clusterService.addLocalNodeMasterListener(this); // TODO: change this not to use 'this'
        clusterService.addListener(this);
    }

    @Override
    public void clusterChanged(final ClusterChangedEvent event) {
        if (this.isMaster) {
            final ClusterState state = event.state();

            if (ilmStoppedOrStopping(state)) {
                if (scheduler.scheduledJobIds().size() > 0) {
                    cancelSnapshotJobs();
                }
                return;
            }

            scheduleSnapshotJobs(state);
            cleanupDeletedPolicies(state);
        }
    }

    @Override
    public void onMaster() {
        this.isMaster = true;
        scheduler.register(snapshotTask);
        final ClusterState state = clusterService.state();
        if (ilmStoppedOrStopping(state)) {
            // ILM is currently stopped, so don't schedule jobs
            return;
        }
        scheduleSnapshotJobs(state);
    }

    @Override
    public void offMaster() {
        this.isMaster = false;
        scheduler.unregister(snapshotTask);
        cancelSnapshotJobs();
    }

    // Only used for testing
    SchedulerEngine getScheduler() {
        return this.scheduler;
    }

    /**
     * Returns true if ILM is in the stopped or stopped state
     */
    private static boolean ilmStoppedOrStopping(ClusterState state) {
        return Optional.ofNullable((SnapshotLifecycleMetadata) state.metaData().custom(SnapshotLifecycleMetadata.TYPE))
            .map(SnapshotLifecycleMetadata::getOperationMode)
            .map(mode -> OperationMode.STOPPING == mode || OperationMode.STOPPED == mode)
            .orElse(false);
    }

    /**
     * Schedule all non-scheduled snapshot jobs contained in the cluster state
     */
    public void scheduleSnapshotJobs(final ClusterState state) {
        SnapshotLifecycleMetadata snapMeta = state.metaData().custom(SnapshotLifecycleMetadata.TYPE);
        if (snapMeta != null) {
            snapMeta.getSnapshotConfigurations().values().forEach(this::maybeScheduleSnapshot);
        }
    }

    public void cleanupDeletedPolicies(final ClusterState state) {
        SnapshotLifecycleMetadata snapMeta = state.metaData().custom(SnapshotLifecycleMetadata.TYPE);
        if (snapMeta != null) {
            // Retrieve all of the expected policy job ids from the policies in the metadata
            final Set<String> policyJobIds = snapMeta.getSnapshotConfigurations().values().stream()
                .map(SnapshotLifecycleService::getJobId)
                .collect(Collectors.toSet());

            // Cancel all jobs that are *NOT* in the scheduled tasks map
            scheduledTasks.keySet().stream()
                .filter(jobId -> policyJobIds.contains(jobId) == false)
                .forEach(this::cancelScheduledSnapshot);
        }
    }

    /**
     * Schedule the {@link SnapshotLifecyclePolicy} job if it does not already exist. First checks
     * to see if any previous versions of the policy were scheduled, and if so, cancels those. If
     * the same version of a policy has already been scheduled it does not overwrite the job.
     */
    public void maybeScheduleSnapshot(final SnapshotLifecyclePolicyMetadata snapshotLifecyclePolicy) {
        final String jobId = getJobId(snapshotLifecyclePolicy);
        final Pattern existingJobPattern = Pattern.compile(snapshotLifecyclePolicy.getPolicy().getId() + JOB_PATTERN_SUFFIX);

        // Find and cancel any existing jobs for this policy
        final boolean existingJobsFoundAndCancelled = scheduledTasks.keySet().stream()
            // Find all jobs matching the `jobid-\d+` pattern
            .filter(jId -> existingJobPattern.matcher(jId).matches())
            // Filter out a job that has not been changed (matches the id exactly meaning the version is the same)
            .filter(jId -> jId.equals(jobId) == false)
            .map(existingJobId -> {
                // Cancel existing job so the new one can be scheduled
                logger.debug("removing existing snapshot lifecycle job [{}] as it has been updated", existingJobId);
                scheduledTasks.remove(existingJobId);
                boolean existed = scheduler.remove(existingJobId);
                assert existed : "expected job for " + existingJobId + " to exist in scheduler";
                return existed;
            })
            .reduce(false, (a, b) -> a || b);

        // Now atomically schedule the new job and add it to the scheduled tasks map. If the jobId
        // is identical to an existing job (meaning the version has not changed) then this does
        // not reschedule it.
        scheduledTasks.computeIfAbsent(jobId, id -> {
            final SchedulerEngine.Job job = new SchedulerEngine.Job(jobId,
                new CronSchedule(snapshotLifecyclePolicy.getPolicy().getSchedule()));
            if (existingJobsFoundAndCancelled) {
                logger.info("rescheduling updated snapshot lifecycle job [{}]", jobId);
            } else {
                logger.info("scheduling snapshot lifecycle job [{}]", jobId);
            }
            scheduler.add(job);
            return job;
        });
    }

    /**
     * Generate the job id for a given policy metadata. The job id is {@code <policyid>-<version>}
     */
    public static String getJobId(SnapshotLifecyclePolicyMetadata policyMeta) {
        return policyMeta.getPolicy().getId() + "-" + policyMeta.getVersion();
    }

    /**
     * Cancel all scheduled snapshot jobs
     */
    public void cancelSnapshotJobs() {
        logger.trace("cancelling all snapshot lifecycle jobs");
        scheduler.scheduledJobIds().forEach(scheduler::remove);
        scheduledTasks.clear();
    }

    /**
     * Cancel the given policy job id (from {@link #getJobId(SnapshotLifecyclePolicyMetadata)}
     */
    public void cancelScheduledSnapshot(final String lifecycleJobId) {
        logger.debug("cancelling snapshot lifecycle job [{}] as it no longer exists", lifecycleJobId);
        scheduledTasks.remove(lifecycleJobId);
        scheduler.remove(lifecycleJobId);
    }

    /**
     * Validates that the {@code repository} exists as a registered snapshot repository
     * @throws IllegalArgumentException if the repository does not exist
     */
    public static void validateRepositoryExists(final String repository, final ClusterState state) {
        Optional.ofNullable((RepositoriesMetaData) state.metaData().custom(RepositoriesMetaData.TYPE))
            .map(repoMeta -> repoMeta.repository(repository))
            .orElseThrow(() -> new IllegalArgumentException("no such repository [" + repository + "]"));
    }

    @Override
    public String executorName() {
        return ThreadPool.Names.SNAPSHOT;
    }

    @Override
    public void close() {
        this.scheduler.stop();
    }
}
