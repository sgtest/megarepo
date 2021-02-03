/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.persistent.PersistentTasksService;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.ml.MlConfigIndex;
import org.elasticsearch.xpack.core.ml.MlTasks;
import org.elasticsearch.xpack.core.ml.action.NodeAcknowledgedResponse;
import org.elasticsearch.xpack.core.ml.action.OpenJobAction;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.config.JobTaskState;
import org.elasticsearch.xpack.core.ml.job.config.JobUpdate;
import org.elasticsearch.xpack.core.ml.job.persistence.ElasticsearchMappings;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MlConfigMigrationEligibilityCheck;
import org.elasticsearch.xpack.ml.job.JobNodeSelector;
import org.elasticsearch.xpack.ml.job.persistence.JobConfigProvider;
import org.elasticsearch.xpack.ml.process.MlMemoryTracker;

import java.util.Optional;
import java.util.function.Predicate;

import static org.elasticsearch.xpack.ml.job.task.OpenJobPersistentTasksExecutor.checkAssignmentState;

/*
 This class extends from TransportMasterNodeAction for cluster state observing purposes.
 The close job api also redirect the elected master node.
 The master node will wait for the job to be opened by checking the persistent task's status and then return.
 To ensure that a subsequent close job call will see that same task status (and sanity validation doesn't fail)
 both open and close job apis redirect to the elected master node.
 In case of instability persistent tasks checks may fail and that is ok, in that case all bets are off.
 The open job api is a low through put api, so the fact that we redirect to elected master node shouldn't be an issue.
*/
public class TransportOpenJobAction extends TransportMasterNodeAction<OpenJobAction.Request, NodeAcknowledgedResponse> {

    private static final Logger logger = LogManager.getLogger(TransportOpenJobAction.class);

    private final XPackLicenseState licenseState;
    private final PersistentTasksService persistentTasksService;
    private final JobConfigProvider jobConfigProvider;
    private final MlMemoryTracker memoryTracker;
    private final MlConfigMigrationEligibilityCheck migrationEligibilityCheck;
    private final Client client;

    @Inject
    public TransportOpenJobAction(Settings settings, TransportService transportService, ThreadPool threadPool,
                                  XPackLicenseState licenseState, ClusterService clusterService,
                                  PersistentTasksService persistentTasksService, ActionFilters actionFilters,
                                  IndexNameExpressionResolver indexNameExpressionResolver,
                                  JobConfigProvider jobConfigProvider, MlMemoryTracker memoryTracker, Client client) {
        super(OpenJobAction.NAME, transportService, clusterService, threadPool, actionFilters,OpenJobAction.Request::new,
            indexNameExpressionResolver, NodeAcknowledgedResponse::new, ThreadPool.Names.SAME);
        this.licenseState = licenseState;
        this.persistentTasksService = persistentTasksService;
        this.jobConfigProvider = jobConfigProvider;
        this.memoryTracker = memoryTracker;
        this.migrationEligibilityCheck = new MlConfigMigrationEligibilityCheck(settings, clusterService);
        this.client = client;
    }

    @Override
    protected ClusterBlockException checkBlock(OpenJobAction.Request request, ClusterState state) {
        // We only delegate here to PersistentTasksService, but if there is a metadata writeblock,
        // then delegating to PersistentTasksService doesn't make a whole lot of sense,
        // because PersistentTasksService will then fail.
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    @Override
    protected void masterOperation(Task task, OpenJobAction.Request request, ClusterState state,
                                   ActionListener<NodeAcknowledgedResponse> listener) {
        if (migrationEligibilityCheck.jobIsEligibleForMigration(request.getJobParams().getJobId(), state)) {
            listener.onFailure(ExceptionsHelper.configHasNotBeenMigrated("open job", request.getJobParams().getJobId()));
            return;
        }

        OpenJobAction.JobParams jobParams = request.getJobParams();
        if (licenseState.checkFeature(XPackLicenseState.Feature.MACHINE_LEARNING)) {

            // Clear job finished time once the job is started and respond
            ActionListener<NodeAcknowledgedResponse> clearJobFinishTime = ActionListener.wrap(
                response -> {
                    if (response.isAcknowledged()) {
                        clearJobFinishedTime(response, state, jobParams.getJobId(), listener);
                    } else {
                        listener.onResponse(response);
                    }
                },
                listener::onFailure
            );

            // Wait for job to be started
            ActionListener<PersistentTasksCustomMetadata.PersistentTask<OpenJobAction.JobParams>> waitForJobToStart =
                    new ActionListener<PersistentTasksCustomMetadata.PersistentTask<OpenJobAction.JobParams>>() {
                @Override
                public void onResponse(PersistentTasksCustomMetadata.PersistentTask<OpenJobAction.JobParams> task) {
                    waitForJobStarted(task.getId(), jobParams, clearJobFinishTime);
                }

                @Override
                public void onFailure(Exception e) {
                    if (ExceptionsHelper.unwrapCause(e) instanceof ResourceAlreadyExistsException) {
                        e = new ElasticsearchStatusException(
                            "Cannot open job [{}] because it has already been opened",
                            RestStatus.CONFLICT,
                            e,
                            jobParams.getJobId());
                    }
                    listener.onFailure(e);
                }
            };

            // Start job task
            ActionListener<Long> memoryRequirementRefreshListener = ActionListener.wrap(
                mem -> persistentTasksService.sendStartRequest(MlTasks.jobTaskId(jobParams.getJobId()), MlTasks.JOB_TASK_NAME, jobParams,
                    waitForJobToStart),
                listener::onFailure
            );

            // Tell the job tracker to refresh the memory requirement for this job and all other jobs that have persistent tasks
            ActionListener<Boolean> getJobHandler = ActionListener.wrap(
                response -> memoryTracker.refreshAnomalyDetectorJobMemoryAndAllOthers(jobParams.getJobId(),
                    memoryRequirementRefreshListener),
                listener::onFailure
            );

            // Get the job config
            jobConfigProvider.getJob(jobParams.getJobId(), ActionListener.wrap(
                    builder -> {
                        jobParams.setJob(builder.build());
                        getJobHandler.onResponse(null);
                    },
                    listener::onFailure
            ));
        } else {
            listener.onFailure(LicenseUtils.newComplianceException(XPackField.MACHINE_LEARNING));
        }
    }

    private void waitForJobStarted(String taskId, OpenJobAction.JobParams jobParams, ActionListener<NodeAcknowledgedResponse> listener) {
        JobPredicate predicate = new JobPredicate();
        persistentTasksService.waitForPersistentTaskCondition(taskId, predicate, jobParams.getTimeout(),
                new PersistentTasksService.WaitForPersistentTaskListener<OpenJobAction.JobParams>() {
            @Override
            public void onResponse(PersistentTasksCustomMetadata.PersistentTask<OpenJobAction.JobParams> persistentTask) {
                if (predicate.exception != null) {
                    if (predicate.shouldCancel) {
                        // We want to return to the caller without leaving an unassigned persistent task, to match
                        // what would have happened if the error had been detected in the "fast fail" validation
                        cancelJobStart(persistentTask, predicate.exception, listener);
                    } else {
                        listener.onFailure(predicate.exception);
                    }
                } else {
                    listener.onResponse(new NodeAcknowledgedResponse(true, predicate.node));
                }
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }

            @Override
            public void onTimeout(TimeValue timeout) {
                listener.onFailure(new ElasticsearchException("Opening job [{}] timed out after [{}]", jobParams.getJob(), timeout));
            }
        });
    }

    private void clearJobFinishedTime(NodeAcknowledgedResponse response,
                                      ClusterState clusterState,
                                      String jobId,
                                      ActionListener<NodeAcknowledgedResponse> listener) {
        final JobUpdate update = new JobUpdate.Builder(jobId).setClearFinishTime(true).build();
        ActionListener<Job> clearedTimeListener = ActionListener.wrap(
            job -> listener.onResponse(response),
            e -> {
                logger.error(new ParameterizedMessage("[{}] Failed to clear finished_time", jobId), e);
                // Not a critical error so continue
                listener.onResponse(response);
            }
        );
        ActionListener<Boolean> mappingsUpdatedListener = ActionListener.wrap(
            mappingUpdateResponse -> jobConfigProvider.updateJob(jobId, update, null, clearedTimeListener),
            e -> {
                logger.error(new ParameterizedMessage("[{}] Failed to update mapping; not clearing finished_time", jobId), e);
                // Not a critical error so continue without attempting to clear finish time
                listener.onResponse(response);
            }
        );
        ElasticsearchMappings.addDocMappingIfMissing(
            MlConfigIndex.indexName(),
            MlConfigIndex::mapping,
            client,
            clusterState,
            mappingsUpdatedListener);
    }

    private void cancelJobStart(PersistentTasksCustomMetadata.PersistentTask<OpenJobAction.JobParams> persistentTask, Exception exception,
                                ActionListener<NodeAcknowledgedResponse> listener) {
        persistentTasksService.sendRemoveRequest(persistentTask.getId(),
                new ActionListener<PersistentTasksCustomMetadata.PersistentTask<?>>() {
                    @Override
                    public void onResponse(PersistentTasksCustomMetadata.PersistentTask<?> task) {
                        // We succeeded in cancelling the persistent task, but the
                        // problem that caused us to cancel it is the overall result
                        listener.onFailure(exception);
                    }

                    @Override
                    public void onFailure(Exception e) {
                        logger.error(
                            () -> new ParameterizedMessage(
                                "[{}] Failed to cancel persistent task that could not be assigned due to [{}]",
                                persistentTask.getParams().getJobId(),
                                exception.getMessage()),
                            e);
                        listener.onFailure(exception);
                    }
                }
        );
    }

    /**
     * This class contains the wait logic for waiting for a job's persistent task to be allocated on
     * job opening.  It should only be used in the open job action, and never at other times the job's
     * persistent task may be assigned to a node, for example on recovery from node failures.
     *
     * Important: the methods of this class must NOT throw exceptions.  If they did then the callers
     * of endpoints waiting for a condition tested by this predicate would never get a response.
     */
    private static class JobPredicate implements Predicate<PersistentTasksCustomMetadata.PersistentTask<?>> {

        private volatile Exception exception;
        private volatile String node = "";
        private volatile boolean shouldCancel;

        @Override
        public boolean test(PersistentTasksCustomMetadata.PersistentTask<?> persistentTask) {
            JobState jobState = JobState.CLOSED;
            String reason = null;
            if (persistentTask != null) {
                JobTaskState jobTaskState = (JobTaskState) persistentTask.getState();
                jobState = jobTaskState == null ? JobState.OPENING : jobTaskState.getState();
                reason = jobTaskState == null ? null : jobTaskState.getReason();

                PersistentTasksCustomMetadata.Assignment assignment = persistentTask.getAssignment();

                // This means we are awaiting a new node to be spun up, ok to return back to the user to await node creation
                if (assignment != null && assignment.equals(JobNodeSelector.AWAITING_LAZY_ASSIGNMENT)) {
                    return true;
                }

                // This logic is only appropriate when opening a job, not when reallocating following a failure,
                // and this is why this class must only be used when opening a job
                OpenJobAction.JobParams params = (OpenJobAction.JobParams) persistentTask.getParams();
                Optional<ElasticsearchException> assignmentException = checkAssignmentState(assignment, params.getJobId(), logger);
                if (assignmentException.isPresent()) {
                    exception = assignmentException.get();
                    // The persistent task should be cancelled so that the observed outcome is the
                    // same as if the "fast fail" validation on the coordinating node had failed
                    shouldCancel = true;
                    return true;
                }
            }
            switch (jobState) {
                // The OPENING case here is expected to be incredibly short-lived, just occurring during the
                // time period when a job has successfully been assigned to a node but the request to update
                // its task state is still in-flight.  (The long-lived OPENING case when a lazy node needs to
                // be added to the cluster to accommodate the job was dealt with higher up this method when the
                // magic AWAITING_LAZY_ASSIGNMENT assignment was checked for.)
                case OPENING:
                case CLOSED:
                    return false;
                case OPENED:
                    node = persistentTask.getExecutorNode();
                    return true;
                case CLOSING:
                    exception = ExceptionsHelper.conflictStatusException(
                        "The job has been {} while waiting to be {}",
                        JobState.CLOSED,
                        JobState.OPENED);
                    return true;
                case FAILED:
                default:
                    // Default http status is SERVER ERROR
                    exception = ExceptionsHelper.serverError(
                        "Unexpected job state [{}] {}while waiting for job to be {}",
                        jobState,
                        reason == null ? "" : "with reason [" + reason + "] ",
                        JobState.OPENED
                    );
                    return true;
            }
        }
    }
}
