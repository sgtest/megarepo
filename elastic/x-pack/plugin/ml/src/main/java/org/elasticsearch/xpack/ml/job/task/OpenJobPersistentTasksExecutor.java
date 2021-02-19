/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.job.task;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.engine.DocumentMissingException;
import org.elasticsearch.persistent.AllocatedPersistentTask;
import org.elasticsearch.persistent.PersistentTaskState;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata.Assignment;
import org.elasticsearch.persistent.decider.EnableAssignmentDecider;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.xpack.core.ml.MlConfigIndex;
import org.elasticsearch.xpack.core.ml.MlMetaIndex;
import org.elasticsearch.xpack.core.ml.MlTasks;
import org.elasticsearch.xpack.core.ml.action.FinalizeJobExecutionAction;
import org.elasticsearch.xpack.core.ml.action.GetJobsAction;
import org.elasticsearch.xpack.core.ml.action.OpenJobAction;
import org.elasticsearch.xpack.core.ml.action.RevertModelSnapshotAction;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.config.JobTaskState;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndex;
import org.elasticsearch.xpack.core.ml.job.persistence.ElasticsearchMappings;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.datafeed.persistence.DatafeedConfigProvider;
import org.elasticsearch.xpack.ml.job.JobNodeSelector;
import org.elasticsearch.xpack.ml.job.persistence.JobResultsProvider;
import org.elasticsearch.xpack.ml.job.process.autodetect.AutodetectProcessManager;
import org.elasticsearch.xpack.ml.notifications.AnomalyDetectionAuditor;
import org.elasticsearch.xpack.ml.process.MlMemoryTracker;
import org.elasticsearch.xpack.ml.task.AbstractJobPersistentTasksExecutor;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;
import static org.elasticsearch.xpack.core.ml.MlTasks.AWAITING_UPGRADE;
import static org.elasticsearch.xpack.ml.job.JobNodeSelector.AWAITING_LAZY_ASSIGNMENT;

public class OpenJobPersistentTasksExecutor extends AbstractJobPersistentTasksExecutor<OpenJobAction.JobParams> {

    private static final Logger logger = LogManager.getLogger(OpenJobPersistentTasksExecutor.class);

    // Resuming a job with a running datafeed from its current snapshot was added in 7.11 and
    // can only be done if the master node is on or after that version.
    private static final Version MIN_MASTER_NODE_VERSION_FOR_REVERTING_TO_CURRENT_SNAPSHOT = Version.V_7_11_0;

    public static String[] indicesOfInterest(String resultsIndex) {
        if (resultsIndex == null) {
            return new String[]{AnomalyDetectorsIndex.jobStateIndexPattern(), MlMetaIndex.indexName(),
                MlConfigIndex.indexName()};
        }
        return new String[]{AnomalyDetectorsIndex.jobStateIndexPattern(), resultsIndex, MlMetaIndex.indexName(),
            MlConfigIndex.indexName()};
    }

    private final AutodetectProcessManager autodetectProcessManager;
    private final DatafeedConfigProvider datafeedConfigProvider;
    private final Client client;
    private final JobResultsProvider jobResultsProvider;
    private final AnomalyDetectionAuditor auditor;

    private volatile ClusterState clusterState;

    public OpenJobPersistentTasksExecutor(Settings settings,
                                          ClusterService clusterService,
                                          AutodetectProcessManager autodetectProcessManager,
                                          DatafeedConfigProvider datafeedConfigProvider,
                                          MlMemoryTracker memoryTracker,
                                          Client client,
                                          IndexNameExpressionResolver expressionResolver) {
        super(MlTasks.JOB_TASK_NAME, MachineLearning.UTILITY_THREAD_POOL_NAME, settings, clusterService, memoryTracker, expressionResolver);
        this.autodetectProcessManager = Objects.requireNonNull(autodetectProcessManager);
        this.datafeedConfigProvider = Objects.requireNonNull(datafeedConfigProvider);
        this.client = Objects.requireNonNull(client);
        this.jobResultsProvider = new JobResultsProvider(client, settings, expressionResolver);
        this.auditor = new AnomalyDetectionAuditor(client, clusterService);
        clusterService.addListener(event -> clusterState = event.state());
    }

    @Override
    public Assignment getAssignment(OpenJobAction.JobParams params, ClusterState clusterState) {
        // If the task parameters do not have a job field then the job
        // was first opened on a pre v6.6 node and has not been migrated
        Job job = params.getJob();
        if (job == null) {
            return AWAITING_MIGRATION;
        }
        boolean isMemoryTrackerRecentlyRefreshed = memoryTracker.isRecentlyRefreshed();
        Optional<Assignment> optionalAssignment = getPotentialAssignment(params, clusterState, isMemoryTrackerRecentlyRefreshed);
        if (optionalAssignment.isPresent()) {
            return optionalAssignment.get();
        }

        JobNodeSelector jobNodeSelector = new JobNodeSelector(clusterState, params.getJobId(), MlTasks.JOB_TASK_NAME, memoryTracker,
            job.allowLazyOpen() ? Integer.MAX_VALUE : maxLazyMLNodes, node -> nodeFilter(node, job));
        Assignment assignment = jobNodeSelector.selectNode(
            maxOpenJobs,
            maxConcurrentJobAllocations,
            maxMachineMemoryPercent,
            maxNodeMemory,
            isMemoryTrackerRecentlyRefreshed,
            useAutoMemoryPercentage);
        auditRequireMemoryIfNecessary(params.getJobId(), auditor, assignment, jobNodeSelector, isMemoryTrackerRecentlyRefreshed);
        return assignment;
    }

    private static boolean nodeSupportsModelSnapshotVersion(DiscoveryNode node, Job job) {
        if (job.getModelSnapshotId() == null || job.getModelSnapshotMinVersion() == null) {
            // There is no snapshot to restore or the min model snapshot version is 5.5.0
            // which is OK as we have already checked the node is >= 5.5.0.
            return true;
        }
        return node.getVersion().onOrAfter(job.getModelSnapshotMinVersion());
    }

    public static String nodeFilter(DiscoveryNode node, Job job) {

        String jobId = job.getId();

        if (nodeSupportsModelSnapshotVersion(node, job) == false) {
            return "Not opening job [" + jobId + "] on node [" + JobNodeSelector.nodeNameAndVersion(node)
                + "], because the job's model snapshot requires a node of version ["
                + job.getModelSnapshotMinVersion() + "] or higher";
        }

        if (Job.getCompatibleJobTypes(node.getVersion()).contains(job.getJobType()) == false) {
            return "Not opening job [" + jobId + "] on node [" + JobNodeSelector.nodeNameAndVersion(node) +
                "], because this node does not support jobs of type [" + job.getJobType() + "]";
        }

        return null;
    }

    static void validateJobAndId(String jobId, Job job) {
        if (job == null) {
            throw ExceptionsHelper.missingJobException(jobId);
        }
        if (job.isDeleting()) {
            throw ExceptionsHelper.conflictStatusException("Cannot open job [{}] because it is being deleted", jobId);
        }
        if (job.getJobVersion() == null) {
            throw ExceptionsHelper.badRequestException(
                "Cannot open job [{}] because jobs created prior to version 5.5 are not supported",
                jobId);
        }
    }

    @Override
    public void validate(OpenJobAction.JobParams params, ClusterState clusterState) {
        final Job job = params.getJob();
        final String jobId = params.getJobId();
        validateJobAndId(jobId, job);
        // If we already know that we can't find an ml node because all ml nodes are running at capacity or
        // simply because there are no ml nodes in the cluster then we fail quickly here:
        PersistentTasksCustomMetadata.Assignment assignment = getAssignment(params, clusterState);
        if (assignment.equals(AWAITING_UPGRADE)) {
            throw makeCurrentlyBeingUpgradedException(logger, params.getJobId());
        }

        if (assignment.getExecutorNode() == null && assignment.equals(AWAITING_LAZY_ASSIGNMENT) == false) {
            throw makeNoSuitableNodesException(logger, params.getJobId(), assignment.getExplanation());
        }
    }

    @Override
    protected void nodeOperation(AllocatedPersistentTask task, OpenJobAction.JobParams params, PersistentTaskState state) {
        JobTask jobTask = (JobTask) task;
        jobTask.setAutodetectProcessManager(autodetectProcessManager);
        JobTaskState jobTaskState = (JobTaskState) state;
        JobState jobState = jobTaskState == null ? null : jobTaskState.getState();
        ActionListener<Boolean> resultsMappingUpdateHandler = ActionListener.wrap(
            mappingsUpdate -> jobResultsProvider.setRunningForecastsToFailed(params.getJobId(), ActionListener.wrap(
                r -> runJob(jobTask, jobState, params),
                e -> {
                    logger.warn(new ParameterizedMessage("[{}] failed to set forecasts to failed", params.getJobId()), e);
                    runJob(jobTask, jobState, params);
                }
            )),
            e -> {
                logger.error(new ParameterizedMessage("[{}] Failed to update results mapping", params.getJobId()), e);
                jobTask.markAsFailed(e);
            }
        );
        // We need to update the results index as we MAY update the current forecast results, setting the running forcasts to failed
        // This writes to the results index, which might need updating
        ElasticsearchMappings.addDocMappingIfMissing(
            AnomalyDetectorsIndex.jobResultsAliasedName(params.getJobId()),
            AnomalyDetectorsIndex::resultsMapping,
            client,
            clusterState,
            resultsMappingUpdateHandler);
    }

    private void runJob(JobTask jobTask, JobState jobState, OpenJobAction.JobParams params) {
        // If the job is closing, simply stop and return
        if (JobState.CLOSING.equals(jobState)) {
            // Mark as completed instead of using `stop` as stop assumes native processes have started
            logger.info("[{}] job got reassigned while stopping. Marking as completed", params.getJobId());
            jobTask.markAsCompleted();
            return;
        }
        // If the job is failed then the Persistent Task Service will
        // try to restart it on a node restart. Exiting here leaves the
        // job in the failed state and it must be force closed.
        if (JobState.FAILED.equals(jobState)) {
            return;
        }

        ActionListener<Boolean> hasRunningDatafeedTaskListener = ActionListener.wrap(
            hasRunningDatafeed -> {
                if (hasRunningDatafeed && clusterState.nodes().getMasterNode().getVersion().onOrAfter(
                    MIN_MASTER_NODE_VERSION_FOR_REVERTING_TO_CURRENT_SNAPSHOT)) {

                    // This job has a running datafeed attached to it.
                    // In order to prevent gaps in the model we revert to the current snapshot deleting intervening results.
                    revertToCurrentSnapshot(jobTask.getJobId(), ActionListener.wrap(response -> openJob(jobTask), jobTask::markAsFailed));
                } else {
                    openJob(jobTask);
                }
            },
            jobTask::markAsFailed
        );

        hasRunningDatafeedTask(jobTask.getJobId(), hasRunningDatafeedTaskListener);
    }

    private void hasRunningDatafeedTask(String jobId, ActionListener<Boolean> listener) {
        ActionListener<Set<String>> datafeedListener = ActionListener.wrap(
            datafeeds -> {
                assert datafeeds.size() <= 1;
                if (datafeeds.isEmpty()) {
                    listener.onResponse(false);
                    return;
                }

                String datafeedId = datafeeds.iterator().next();
                PersistentTasksCustomMetadata tasks = clusterState.getMetadata().custom(PersistentTasksCustomMetadata.TYPE);
                PersistentTasksCustomMetadata.PersistentTask<?> datafeedTask = MlTasks.getDatafeedTask(datafeedId, tasks);
                listener.onResponse(datafeedTask != null);
            },
            listener::onFailure
        );

        datafeedConfigProvider.findDatafeedsForJobIds(Collections.singleton(jobId), datafeedListener);
    }

    private void revertToCurrentSnapshot(String jobId, ActionListener<RevertModelSnapshotAction.Response> listener) {
        logger.info("[{}] job has running datafeed task; reverting to current snapshot", jobId);

        ActionListener<GetJobsAction.Response> jobListener = ActionListener.wrap(
            jobResponse -> {
                List<Job> jobPage = jobResponse.getResponse().results();
                // We requested a single concrete job so if it didn't exist we would get an error
                assert jobPage.size() == 1;

                String jobSnapshotId = jobPage.get(0).getModelSnapshotId();
                RevertModelSnapshotAction.Request request = new RevertModelSnapshotAction.Request(jobId,
                    jobSnapshotId == null ? ModelSnapshot.EMPTY_SNAPSHOT_ID : jobSnapshotId);
                request.setForce(true);
                request.setDeleteInterveningResults(true);
                executeAsyncWithOrigin(client, ML_ORIGIN, RevertModelSnapshotAction.INSTANCE, request, listener);
            },
            error -> listener.onFailure(ExceptionsHelper.serverError("[{}] error getting job", error, jobId))
        );

        // We need to refetch the job in order to learn what is its current model snapshot
        // as the one that exists in the task params is outdated.
        executeAsyncWithOrigin(client, ML_ORIGIN, GetJobsAction.INSTANCE, new GetJobsAction.Request(jobId), jobListener);
    }

    private void openJob(JobTask jobTask) {
        String jobId = jobTask.getJobId();
        autodetectProcessManager.openJob(jobTask, clusterState, (e2, shouldFinalizeJob) -> {
            if (e2 == null) {
                if (shouldFinalizeJob) {
                    FinalizeJobExecutionAction.Request finalizeRequest = new FinalizeJobExecutionAction.Request(new String[]{jobId});
                    executeAsyncWithOrigin(client, ML_ORIGIN, FinalizeJobExecutionAction.INSTANCE, finalizeRequest,
                        ActionListener.wrap(
                            response -> jobTask.markAsCompleted(),
                            e -> {
                                logger.error(new ParameterizedMessage("[{}] error finalizing job", jobId), e);
                                Throwable unwrapped = ExceptionsHelper.unwrapCause(e);
                                if (unwrapped instanceof DocumentMissingException || unwrapped instanceof ResourceNotFoundException) {
                                    jobTask.markAsCompleted();
                                } else {
                                    jobTask.markAsFailed(e);
                                }
                            }
                        ));
                } else {
                    jobTask.markAsCompleted();
                }
            } else {
                jobTask.markAsFailed(e2);
            }
        });
    }

    @Override
    protected AllocatedPersistentTask createTask(long id, String type, String action, TaskId parentTaskId,
                                                 PersistentTasksCustomMetadata.PersistentTask<OpenJobAction.JobParams> persistentTask,
                                                 Map<String, String> headers) {
        return new JobTask(persistentTask.getParams().getJobId(), id, type, action, parentTaskId, headers);
    }

    public static Optional<ElasticsearchException> checkAssignmentState(PersistentTasksCustomMetadata.Assignment assignment,
                                                                        String jobId,
                                                                        Logger logger) {
        if (assignment != null
            && assignment.equals(PersistentTasksCustomMetadata.INITIAL_ASSIGNMENT) == false
            && assignment.isAssigned() == false) {
            // Assignment has failed on the master node despite passing our "fast fail" validation
            if (assignment.equals(AWAITING_UPGRADE)) {
                return Optional.of(makeCurrentlyBeingUpgradedException(logger, jobId));
            } else if (assignment.getExplanation().contains("[" + EnableAssignmentDecider.ALLOCATION_NONE_EXPLANATION + "]")) {
                return Optional.of(makeAssignmentsNotAllowedException(logger, jobId));
            } else {
                return Optional.of(makeNoSuitableNodesException(logger, jobId, assignment.getExplanation()));
            }
        }
        return Optional.empty();
    }

    static ElasticsearchException makeNoSuitableNodesException(Logger logger, String jobId, String explanation) {
        String msg = "Could not open job because no suitable nodes were found, allocation explanation [" + explanation + "]";
        logger.warn("[{}] {}", jobId, msg);
        Exception detail = new IllegalStateException(msg);
        return new ElasticsearchStatusException("Could not open job because no ML nodes with sufficient capacity were found",
            RestStatus.TOO_MANY_REQUESTS, detail);
    }

    static ElasticsearchException makeAssignmentsNotAllowedException(Logger logger, String jobId) {
        String msg = "Cannot open jobs because persistent task assignment is disabled by the ["
            + EnableAssignmentDecider.CLUSTER_TASKS_ALLOCATION_ENABLE_SETTING.getKey() + "] setting";
        logger.warn("[{}] {}", jobId, msg);
        return new ElasticsearchStatusException(msg, RestStatus.TOO_MANY_REQUESTS);
    }

    static ElasticsearchException makeCurrentlyBeingUpgradedException(Logger logger, String jobId) {
        String msg = "Cannot open jobs when upgrade mode is enabled";
        logger.warn("[{}] {}", jobId, msg);
        return new ElasticsearchStatusException(msg, RestStatus.TOO_MANY_REQUESTS);
    }

    @Override
    protected String[] indicesOfInterest(OpenJobAction.JobParams params) {
        return indicesOfInterest(AnomalyDetectorsIndex.resultsWriteAlias(params.getJobId()));
    }

    @Override
    protected String getJobId(OpenJobAction.JobParams params) {
        return params.getJobId();
    }
}
