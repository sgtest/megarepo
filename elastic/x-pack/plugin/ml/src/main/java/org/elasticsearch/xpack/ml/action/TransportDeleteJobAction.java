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
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksAction;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksRequest;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.ParentTaskAssigningClient;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.persistent.PersistentTasksService;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.MlTasks;
import org.elasticsearch.xpack.core.ml.action.DeleteJobAction;
import org.elasticsearch.xpack.core.ml.action.KillProcessAction;
import org.elasticsearch.xpack.core.ml.action.PutJobAction;
import org.elasticsearch.xpack.core.ml.action.ResetJobAction;
import org.elasticsearch.xpack.core.ml.job.config.Blocked;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.config.JobTaskState;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MlConfigMigrationEligibilityCheck;
import org.elasticsearch.xpack.ml.datafeed.persistence.DatafeedConfigProvider;
import org.elasticsearch.xpack.ml.job.JobManager;
import org.elasticsearch.xpack.ml.job.persistence.JobConfigProvider;
import org.elasticsearch.xpack.ml.notifications.AnomalyDetectionAuditor;
import org.elasticsearch.xpack.ml.process.MlMemoryTracker;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

public class TransportDeleteJobAction extends AcknowledgedTransportMasterNodeAction<DeleteJobAction.Request> {

    private static final Logger logger = LogManager.getLogger(TransportDeleteJobAction.class);

    private final Client client;
    private final PersistentTasksService persistentTasksService;
    private final AnomalyDetectionAuditor auditor;
    private final JobConfigProvider jobConfigProvider;
    private final JobManager jobManager;
    private final DatafeedConfigProvider datafeedConfigProvider;
    private final MlMemoryTracker memoryTracker;
    private final MlConfigMigrationEligibilityCheck migrationEligibilityCheck;

    /**
     * A map of task listeners by job_id.
     * Subsequent delete requests store their listeners in the corresponding list in this map
     * and wait to be notified when the first deletion task completes.
     * This is guarded by synchronizing on its lock.
     */
    private final Map<String, List<ActionListener<AcknowledgedResponse>>> listenersByJobId;

    @Inject
    public TransportDeleteJobAction(Settings settings, TransportService transportService, ClusterService clusterService,
                                    ThreadPool threadPool, ActionFilters actionFilters,
                                    IndexNameExpressionResolver indexNameExpressionResolver, PersistentTasksService persistentTasksService,
                                    Client client, AnomalyDetectionAuditor auditor,
                                    JobConfigProvider jobConfigProvider, DatafeedConfigProvider datafeedConfigProvider,
                                    MlMemoryTracker memoryTracker, JobManager jobManager) {
        super(DeleteJobAction.NAME, transportService, clusterService, threadPool, actionFilters,
                DeleteJobAction.Request::new, indexNameExpressionResolver, ThreadPool.Names.SAME);
        this.client = client;
        this.persistentTasksService = persistentTasksService;
        this.auditor = auditor;
        this.jobConfigProvider = jobConfigProvider;
        this.datafeedConfigProvider = datafeedConfigProvider;
        this.memoryTracker = memoryTracker;
        this.migrationEligibilityCheck = new MlConfigMigrationEligibilityCheck(settings, clusterService);
        this.listenersByJobId = new HashMap<>();
        this.jobManager = jobManager;
    }

    @Override
    protected ClusterBlockException checkBlock(DeleteJobAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    @Override
    protected void masterOperation(Task task, DeleteJobAction.Request request, ClusterState state,
                                   ActionListener<AcknowledgedResponse> listener) {

        if (migrationEligibilityCheck.jobIsEligibleForMigration(request.getJobId(), state)) {
            listener.onFailure(ExceptionsHelper.configHasNotBeenMigrated("delete job", request.getJobId()));
            return;
        }

        logger.debug(() -> new ParameterizedMessage("[{}] deleting job ", request.getJobId()));

        if (request.isForce() == false) {
            checkJobIsNotOpen(request.getJobId(), state);
        }

        TaskId taskId = new TaskId(clusterService.localNode().getId(), task.getId());
        ParentTaskAssigningClient parentTaskClient = new ParentTaskAssigningClient(client, taskId);

        // Check if there is a deletion task for this job already and if yes wait for it to complete
        synchronized (listenersByJobId) {
            if (listenersByJobId.containsKey(request.getJobId())) {
                logger.debug(() -> new ParameterizedMessage(
                    "[{}] Deletion task [{}] will wait for existing deletion task to complete",
                    request.getJobId(),
                    task.getId()
                ));
                listenersByJobId.get(request.getJobId()).add(listener);
                return;
            } else {
                List<ActionListener<AcknowledgedResponse>> listeners = new ArrayList<>();
                listeners.add(listener);
                listenersByJobId.put(request.getJobId(), listeners);
            }
        }

        // The listener that will be executed at the end of the chain will notify all listeners
        ActionListener<AcknowledgedResponse> finalListener = ActionListener.wrap(
                ack -> notifyListeners(request.getJobId(), ack, null),
                e -> {
                    notifyListeners(request.getJobId(), null, e);
                    if ((ExceptionsHelper.unwrapCause(e) instanceof ResourceNotFoundException) == false) {
                        auditor.error(request.getJobId(), Messages.getMessage(Messages.JOB_AUDIT_DELETING_FAILED, e.getMessage()));
                    }
                }
        );

        ActionListener<PutJobAction.Response> markAsDeletingListener = ActionListener.wrap(
                response -> {
                    if (request.isForce()) {
                        forceDeleteJob(parentTaskClient, request, state, finalListener);
                    } else {
                        normalDeleteJob(parentTaskClient, request, state, finalListener);
                    }
                },
                finalListener::onFailure);

        ActionListener<Boolean> jobExistsListener = ActionListener.wrap(
            response -> {
                auditor.info(request.getJobId(), Messages.getMessage(Messages.JOB_AUDIT_DELETING, taskId));
                markJobAsDeletingIfNotUsed(request.getJobId(), taskId, markAsDeletingListener);
            },
            e -> {
                if (request.isForce()
                    && MlTasks.getJobTask(request.getJobId(), state.getMetadata().custom(PersistentTasksCustomMetadata.TYPE)) != null) {
                    logger.info(
                        "[{}] config is missing but task exists. Attempting to delete tasks and stop process",
                        request.getJobId());
                    forceDeleteJob(parentTaskClient, request, state, finalListener);
                } else {
                    finalListener.onFailure(e);
                }
            });

        // First check that the job exists, because we don't want to audit
        // the beginning of its deletion if it didn't exist in the first place
        jobConfigProvider.jobExists(request.getJobId(), true, jobExistsListener);
    }

    private void notifyListeners(String jobId, @Nullable AcknowledgedResponse ack, @Nullable Exception error) {
        synchronized (listenersByJobId) {
            List<ActionListener<AcknowledgedResponse>> listeners = listenersByJobId.remove(jobId);
            if (listeners == null) {
                logger.error("[{}] No deletion job listeners could be found", jobId);
                return;
            }
            for (ActionListener<AcknowledgedResponse> listener : listeners) {
                if (error != null) {
                    listener.onFailure(error);
                } else {
                    listener.onResponse(ack);
                }
            }
        }
    }

    private void normalDeleteJob(ParentTaskAssigningClient parentTaskClient,
                                 DeleteJobAction.Request request,
                                 ClusterState state,
                                 ActionListener<AcknowledgedResponse> listener) {
        String jobId = request.getJobId();

        // We clean up the memory tracker on delete rather than close as close is not a master node action
        memoryTracker.removeAnomalyDetectorJob(jobId);
        jobManager.deleteJob(request, parentTaskClient, state, listener);
    }

    private void forceDeleteJob(
        ParentTaskAssigningClient parentTaskClient,
        DeleteJobAction.Request request,
        ClusterState state,
        ActionListener<AcknowledgedResponse> listener
    ) {

        final String jobId = request.getJobId();
        logger.debug(() -> new ParameterizedMessage("[{}] force deleting job", jobId));

        // 3. Delete the job
        ActionListener<Boolean> removeTaskListener = new ActionListener<Boolean>() {
            @Override
            public void onResponse(Boolean response) {
                // use clusterService.state() here so that the updated state without the task is available
                normalDeleteJob(parentTaskClient, request, clusterService.state(), listener);
            }

            @Override
            public void onFailure(Exception e) {
                if (ExceptionsHelper.unwrapCause(e) instanceof ResourceNotFoundException) {
                    // use clusterService.state() here so that the updated state without the task is available
                    normalDeleteJob(parentTaskClient, request, clusterService.state(), listener);
                } else {
                    listener.onFailure(e);
                }
            }
        };

        // 2. Cancel the persistent task. This closes the process gracefully so
        // the process should be killed first.
        ActionListener<KillProcessAction.Response> killJobListener = ActionListener.wrap(
                response -> removePersistentTask(jobId, state, removeTaskListener),
                e -> {
                    if (ExceptionsHelper.unwrapCause(e) instanceof ElasticsearchStatusException) {
                        // Killing the process marks the task as completed so it
                        // may have disappeared when we get here
                        removePersistentTask(jobId, state, removeTaskListener);
                    } else {
                        listener.onFailure(e);
                    }
                }
        );

        // 1. Kill the job's process
        killProcess(parentTaskClient, jobId, killJobListener);
    }

    private void killProcess(ParentTaskAssigningClient parentTaskClient, String jobId,
                             ActionListener<KillProcessAction.Response> listener) {
        KillProcessAction.Request killRequest = new KillProcessAction.Request(jobId);
        executeAsyncWithOrigin(parentTaskClient, ML_ORIGIN, KillProcessAction.INSTANCE, killRequest, listener);
    }

    private void removePersistentTask(String jobId, ClusterState currentState,
                                      ActionListener<Boolean> listener) {
        PersistentTasksCustomMetadata tasks = currentState.getMetadata().custom(PersistentTasksCustomMetadata.TYPE);

        PersistentTasksCustomMetadata.PersistentTask<?> jobTask = MlTasks.getJobTask(jobId, tasks);
        if (jobTask == null) {
            listener.onResponse(null);
        } else {
            persistentTasksService.sendRemoveRequest(jobTask.getId(), listener.delegateFailure((l, task) -> l.onResponse(Boolean.TRUE)));
        }
    }

    private void checkJobIsNotOpen(String jobId, ClusterState state) {
        PersistentTasksCustomMetadata tasks = state.metadata().custom(PersistentTasksCustomMetadata.TYPE);
        PersistentTasksCustomMetadata.PersistentTask<?> jobTask = MlTasks.getJobTask(jobId, tasks);
        if (jobTask != null) {
            JobTaskState jobTaskState = (JobTaskState) jobTask.getState();
            throw ExceptionsHelper.conflictStatusException("Cannot delete job [" + jobId + "] because the job is "
                    + ((jobTaskState == null) ? JobState.OPENING : jobTaskState.getState()));
        }
    }

    private void markJobAsDeletingIfNotUsed(String jobId, TaskId taskId, ActionListener<PutJobAction.Response> listener) {

        datafeedConfigProvider.findDatafeedIdsForJobIds(Collections.singletonList(jobId), ActionListener.wrap(
                datafeedIds -> {
                    if (datafeedIds.isEmpty() == false) {
                        listener.onFailure(ExceptionsHelper.conflictStatusException("Cannot delete job [" + jobId + "] because datafeed ["
                                + datafeedIds.iterator().next() + "] refers to it"));
                        return;
                    }
                    cancelResetTaskIfExists(jobId, ActionListener.wrap(
                        response -> jobConfigProvider.updateJobBlockReason(jobId, new Blocked(Blocked.Reason.DELETE, taskId), listener),
                        listener::onFailure
                    ));
                },
                listener::onFailure
        ));
    }

    private void cancelResetTaskIfExists(String jobId, ActionListener<Boolean> listener) {
        ActionListener<Job.Builder> jobListener = ActionListener.wrap(
            jobBuilder -> {
                Job job = jobBuilder.build();
                if (job.getBlocked().getReason() == Blocked.Reason.RESET) {
                    logger.info("[{}] Cancelling reset task [{}] because delete was requested", jobId, job.getBlocked().getTaskId());
                    CancelTasksRequest cancelTasksRequest = new CancelTasksRequest();
                    cancelTasksRequest.setReason("deleting job");
                    cancelTasksRequest.setActions(ResetJobAction.NAME);
                    cancelTasksRequest.setTaskId(job.getBlocked().getTaskId());
                    executeAsyncWithOrigin(client, ML_ORIGIN, CancelTasksAction.INSTANCE, cancelTasksRequest, ActionListener.wrap(
                        cancelTasksResponse -> listener.onResponse(true),
                        e -> {
                            if (ExceptionsHelper.unwrapCause(e) instanceof ResourceNotFoundException) {
                                listener.onResponse(true);
                            } else {
                                listener.onFailure(e);
                            }
                        }
                    ));
                } else {
                    listener.onResponse(false);
                }
            },
            listener::onFailure
        );

        jobConfigProvider.getJob(jobId, jobListener);
    }
}
