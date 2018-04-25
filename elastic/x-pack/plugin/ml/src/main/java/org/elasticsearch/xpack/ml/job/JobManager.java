/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.AckedClusterStateUpdateTask;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.component.AbstractComponent;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.analysis.AnalysisRegistry;
import org.elasticsearch.persistent.PersistentTasksCustomMetaData;
import org.elasticsearch.xpack.core.ml.MLMetadataField;
import org.elasticsearch.xpack.core.ml.MachineLearningField;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.action.DeleteJobAction;
import org.elasticsearch.xpack.core.ml.action.PutJobAction;
import org.elasticsearch.xpack.core.ml.action.RevertModelSnapshotAction;
import org.elasticsearch.xpack.core.ml.action.UpdateJobAction;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisLimits;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.config.JobUpdate;
import org.elasticsearch.xpack.core.ml.job.config.MlFilter;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.job.persistence.JobStorageDeletionTask;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSizeStats;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.job.persistence.JobProvider;
import org.elasticsearch.xpack.ml.job.persistence.JobResultsPersister;
import org.elasticsearch.xpack.ml.job.process.autodetect.UpdateParams;
import org.elasticsearch.xpack.ml.notifications.Auditor;
import org.elasticsearch.xpack.ml.utils.ChainTaskExecutor;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Date;
import java.util.HashSet;
import java.util.List;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Allows interactions with jobs. The managed interactions include:
 * <ul>
 * <li>creation</li>
 * <li>deletion</li>
 * <li>updating</li>
 * <li>starting/stopping of datafeed jobs</li>
 * </ul>
 */
public class JobManager extends AbstractComponent {

    private static final DeprecationLogger DEPRECATION_LOGGER =
            new DeprecationLogger(Loggers.getLogger(JobManager.class));

    private final Environment environment;
    private final JobProvider jobProvider;
    private final ClusterService clusterService;
    private final Auditor auditor;
    private final Client client;
    private final UpdateJobProcessNotifier updateJobProcessNotifier;

    private volatile ByteSizeValue maxModelMemoryLimit;

    /**
     * Create a JobManager
     */
    public JobManager(Environment environment, Settings settings, JobProvider jobProvider, ClusterService clusterService, Auditor auditor,
                      Client client, UpdateJobProcessNotifier updateJobProcessNotifier) {
        super(settings);
        this.environment = environment;
        this.jobProvider = Objects.requireNonNull(jobProvider);
        this.clusterService = Objects.requireNonNull(clusterService);
        this.auditor = Objects.requireNonNull(auditor);
        this.client = Objects.requireNonNull(client);
        this.updateJobProcessNotifier = updateJobProcessNotifier;

        maxModelMemoryLimit = MachineLearningField.MAX_MODEL_MEMORY_LIMIT.get(settings);
        clusterService.getClusterSettings()
                .addSettingsUpdateConsumer(MachineLearningField.MAX_MODEL_MEMORY_LIMIT, this::setMaxModelMemoryLimit);
    }

    private void setMaxModelMemoryLimit(ByteSizeValue maxModelMemoryLimit) {
        this.maxModelMemoryLimit = maxModelMemoryLimit;
    }

    /**
     * Gets the job that matches the given {@code jobId}.
     *
     * @param jobId the jobId
     * @return The {@link Job} matching the given {code jobId}
     * @throws ResourceNotFoundException if no job matches {@code jobId}
     */
    public Job getJobOrThrowIfUnknown(String jobId) {
        return getJobOrThrowIfUnknown(jobId, clusterService.state());
    }

    /**
     * Gets the job that matches the given {@code jobId}.
     *
     * @param jobId the jobId
     * @param clusterState the cluster state
     * @return The {@link Job} matching the given {code jobId}
     * @throws ResourceNotFoundException if no job matches {@code jobId}
     */
    public static Job getJobOrThrowIfUnknown(String jobId, ClusterState clusterState) {
        MlMetadata mlMetadata = clusterState.getMetaData().custom(MLMetadataField.TYPE);
        Job job = (mlMetadata == null) ? null : mlMetadata.getJobs().get(jobId);
        if (job == null) {
            throw ExceptionsHelper.missingJobException(jobId);
        }
        return job;
    }

    private Set<String> expandJobIds(String expression, boolean allowNoJobs, ClusterState clusterState) {
        MlMetadata mlMetadata = clusterState.getMetaData().custom(MLMetadataField.TYPE);
        if (mlMetadata == null) {
            mlMetadata = MlMetadata.EMPTY_METADATA;
        }
        return mlMetadata.expandJobIds(expression, allowNoJobs);
    }

    /**
     * Get the jobs that match the given {@code expression}.
     * Note that when the {@code jobId} is {@link MetaData#ALL} all jobs are returned.
     *
     * @param expression   the jobId or an expression matching jobIds
     * @param clusterState the cluster state
     * @param allowNoJobs  if {@code false}, an error is thrown when no job matches the {@code jobId}
     * @return A {@link QueryPage} containing the matching {@code Job}s
     */
    public QueryPage<Job> expandJobs(String expression, boolean allowNoJobs, ClusterState clusterState) {
        Set<String> expandedJobIds = expandJobIds(expression, allowNoJobs, clusterState);
        MlMetadata mlMetadata = clusterState.getMetaData().custom(MLMetadataField.TYPE);
        List<Job> jobs = new ArrayList<>();
        for (String expandedJobId : expandedJobIds) {
            jobs.add(mlMetadata.getJobs().get(expandedJobId));
        }
        logger.debug("Returning jobs matching [" + expression + "]");
        return new QueryPage<>(jobs, jobs.size(), Job.RESULTS_FIELD);
    }

    public JobState getJobState(String jobId) {
        PersistentTasksCustomMetaData tasks = clusterService.state().getMetaData().custom(PersistentTasksCustomMetaData.TYPE);
        return MlMetadata.getJobState(jobId, tasks);
    }

    /**
     * Stores a job in the cluster state
     */
    public void putJob(PutJobAction.Request request, AnalysisRegistry analysisRegistry, ClusterState state,
                       ActionListener<PutJobAction.Response> actionListener) throws IOException {

        request.getJobBuilder().validateAnalysisLimitsAndSetDefaults(maxModelMemoryLimit);
        request.getJobBuilder().validateCategorizationAnalyzer(analysisRegistry, environment);

        Job job = request.getJobBuilder().build(new Date());
        if (job.getDataDescription() != null && job.getDataDescription().getFormat() == DataDescription.DataFormat.DELIMITED) {
            DEPRECATION_LOGGER.deprecated("Creating jobs with delimited data format is deprecated. Please use xcontent instead.");
        }

        MlMetadata currentMlMetadata = state.metaData().custom(MLMetadataField.TYPE);
        if (currentMlMetadata != null && currentMlMetadata.getJobs().containsKey(job.getId())) {
            actionListener.onFailure(ExceptionsHelper.jobAlreadyExists(job.getId()));
            return;
        }

        ActionListener<Boolean> putJobListener = new ActionListener<Boolean>() {
            @Override
            public void onResponse(Boolean indicesCreated) {

                clusterService.submitStateUpdateTask("put-job-" + job.getId(),
                        new AckedClusterStateUpdateTask<PutJobAction.Response>(request, actionListener) {
                            @Override
                            protected PutJobAction.Response newResponse(boolean acknowledged) {
                                auditor.info(job.getId(), Messages.getMessage(Messages.JOB_AUDIT_CREATED));
                                return new PutJobAction.Response(job);
                            }

                            @Override
                            public ClusterState execute(ClusterState currentState) {
                                return updateClusterState(job, false, currentState);
                            }
                        });
            }

            @Override
            public void onFailure(Exception e) {
                if (e instanceof IllegalArgumentException) {
                    // the underlying error differs depending on which way around the clashing fields are seen
                    Matcher matcher = Pattern.compile("(?:mapper|Can't merge a non object mapping) \\[(.*)\\] (?:of different type, " +
                            "current_type \\[.*\\], merged_type|with an object mapping) \\[.*\\]").matcher(e.getMessage());
                    if (matcher.matches()) {
                        String msg = Messages.getMessage(Messages.JOB_CONFIG_MAPPING_TYPE_CLASH, matcher.group(1));
                        actionListener.onFailure(ExceptionsHelper.badRequestException(msg, e));
                        return;
                    }
                }
                actionListener.onFailure(e);
            }
        };

        ActionListener<Boolean> checkForLeftOverDocs = ActionListener.wrap(
                response -> {
                    jobProvider.createJobResultIndex(job, state, putJobListener);
                },
                actionListener::onFailure
        );

        jobProvider.checkForLeftOverDocuments(job, checkForLeftOverDocs);
    }

    public void updateJob(UpdateJobAction.Request request, ActionListener<PutJobAction.Response> actionListener) {
        Job job = getJobOrThrowIfUnknown(request.getJobId());
        validate(request.getJobUpdate(), job, ActionListener.wrap(
                nullValue -> internalJobUpdate(request, actionListener),
                actionListener::onFailure));
    }

    private void validate(JobUpdate jobUpdate, Job job, ActionListener<Void> handler) {
        ChainTaskExecutor chainTaskExecutor = new ChainTaskExecutor(client.threadPool().executor(
                MachineLearning.UTILITY_THREAD_POOL_NAME), true);
        validateModelSnapshotIdUpdate(job, jobUpdate.getModelSnapshotId(), chainTaskExecutor);
        validateAnalysisLimitsUpdate(job, jobUpdate.getAnalysisLimits(), chainTaskExecutor);
        chainTaskExecutor.execute(handler);
    }

    private void validateModelSnapshotIdUpdate(Job job, String modelSnapshotId, ChainTaskExecutor chainTaskExecutor) {
        if (modelSnapshotId != null) {
            chainTaskExecutor.add(listener -> {
                jobProvider.getModelSnapshot(job.getId(), modelSnapshotId, newModelSnapshot -> {
                    if (newModelSnapshot == null) {
                        String message = Messages.getMessage(Messages.REST_NO_SUCH_MODEL_SNAPSHOT, modelSnapshotId,
                                job.getId());
                        listener.onFailure(new ResourceNotFoundException(message));
                        return;
                    }
                    jobProvider.getModelSnapshot(job.getId(), job.getModelSnapshotId(), oldModelSnapshot -> {
                        if (oldModelSnapshot != null
                                && newModelSnapshot.result.getTimestamp().before(oldModelSnapshot.result.getTimestamp())) {
                            String message = "Job [" + job.getId() + "] has a more recent model snapshot [" +
                                    oldModelSnapshot.result.getSnapshotId() + "]";
                            listener.onFailure(new IllegalArgumentException(message));
                        }
                        listener.onResponse(null);
                    }, listener::onFailure);
                }, listener::onFailure);
            });
        }
    }

    private void validateAnalysisLimitsUpdate(Job job, AnalysisLimits newLimits, ChainTaskExecutor chainTaskExecutor) {
        if (newLimits == null || newLimits.getModelMemoryLimit() == null) {
            return;
        }
        Long newModelMemoryLimit = newLimits.getModelMemoryLimit();
        chainTaskExecutor.add(listener -> {
            if (isJobOpen(clusterService.state(), job.getId())) {
                listener.onFailure(ExceptionsHelper.badRequestException("Cannot update " + Job.ANALYSIS_LIMITS.getPreferredName()
                        + " while the job is open"));
                return;
            }
            jobProvider.modelSizeStats(job.getId(), modelSizeStats -> {
                if (modelSizeStats != null) {
                    ByteSizeValue modelSize = new ByteSizeValue(modelSizeStats.getModelBytes(), ByteSizeUnit.BYTES);
                    if (newModelMemoryLimit < modelSize.getMb()) {
                        listener.onFailure(ExceptionsHelper.badRequestException(
                                Messages.getMessage(Messages.JOB_CONFIG_UPDATE_ANALYSIS_LIMITS_MODEL_MEMORY_LIMIT_CANNOT_BE_DECREASED,
                                        new ByteSizeValue(modelSize.getMb(), ByteSizeUnit.MB),
                                        new ByteSizeValue(newModelMemoryLimit, ByteSizeUnit.MB))));
                        return;
                    }
                }
                listener.onResponse(null);
            }, listener::onFailure);
        });
    }

    private void internalJobUpdate(UpdateJobAction.Request request, ActionListener<PutJobAction.Response> actionListener) {
        if (request.isWaitForAck()) {
            // Use the ack cluster state update
            clusterService.submitStateUpdateTask("update-job-" + request.getJobId(),
                    new AckedClusterStateUpdateTask<PutJobAction.Response>(request, actionListener) {
                        private AtomicReference<Job> updatedJob = new AtomicReference<>();

                        @Override
                        protected PutJobAction.Response newResponse(boolean acknowledged) {
                            return new PutJobAction.Response(updatedJob.get());
                        }

                        @Override
                        public ClusterState execute(ClusterState currentState) {
                            Job job = getJobOrThrowIfUnknown(request.getJobId(), currentState);
                            updatedJob.set(request.getJobUpdate().mergeWithJob(job, maxModelMemoryLimit));
                            return updateClusterState(updatedJob.get(), true, currentState);
                        }

                        @Override
                        public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
                            afterClusterStateUpdate(newState, request);
                        }
                    });
        } else {
            clusterService.submitStateUpdateTask("update-job-" + request.getJobId(), new ClusterStateUpdateTask() {
                private AtomicReference<Job> updatedJob = new AtomicReference<>();

                @Override
                public ClusterState execute(ClusterState currentState) throws Exception {
                    Job job = getJobOrThrowIfUnknown(request.getJobId(), currentState);
                    updatedJob.set(request.getJobUpdate().mergeWithJob(job, maxModelMemoryLimit));
                    return updateClusterState(updatedJob.get(), true, currentState);
                }

                @Override
                public void onFailure(String source, Exception e) {
                    actionListener.onFailure(e);
                }

                @Override
                public void clusterStatePublished(ClusterChangedEvent clusterChangedEvent) {
                    afterClusterStateUpdate(clusterChangedEvent.state(), request);
                    actionListener.onResponse(new PutJobAction.Response(updatedJob.get()));
                }
            });
        }
    }

    private void afterClusterStateUpdate(ClusterState newState, UpdateJobAction.Request request) {
        JobUpdate jobUpdate = request.getJobUpdate();

        // Change is required if the fields that the C++ uses are being updated
        boolean processUpdateRequired = jobUpdate.isAutodetectProcessUpdate();

        if (processUpdateRequired && isJobOpen(newState, request.getJobId())) {
            updateJobProcessNotifier.submitJobUpdate(UpdateParams.fromJobUpdate(jobUpdate), ActionListener.wrap(
                    isUpdated -> {
                        if (isUpdated) {
                            auditJobUpdatedIfNotInternal(request);
                        }
                    }, e -> {
                        // No need to do anything
                    }
            ));
        } else {
            logger.debug("[{}] No process update required for job update: {}", () -> request.getJobId(), () -> {
                try {
                    XContentBuilder jsonBuilder = XContentFactory.jsonBuilder();
                    jobUpdate.toXContent(jsonBuilder, ToXContent.EMPTY_PARAMS);
                    return Strings.toString(jsonBuilder);
                } catch (IOException e) {
                    return "(unprintable due to " + e.getMessage() + ")";
                }
            });

            auditJobUpdatedIfNotInternal(request);
        }
    }

    private void auditJobUpdatedIfNotInternal(UpdateJobAction.Request request) {
        if (request.isInternal() == false) {
            auditor.info(request.getJobId(), Messages.getMessage(Messages.JOB_AUDIT_UPDATED, request.getJobUpdate().getUpdateFields()));
        }
    }

    private boolean isJobOpen(ClusterState clusterState, String jobId) {
        PersistentTasksCustomMetaData persistentTasks = clusterState.metaData().custom(PersistentTasksCustomMetaData.TYPE);
        JobState jobState = MlMetadata.getJobState(jobId, persistentTasks);
        return jobState == JobState.OPENED;
    }

    private ClusterState updateClusterState(Job job, boolean overwrite, ClusterState currentState) {
        MlMetadata.Builder builder = createMlMetadataBuilder(currentState);
        builder.putJob(job, overwrite);
        return buildNewClusterState(currentState, builder);
    }

    public void updateProcessOnFilterChanged(MlFilter filter) {
        ClusterState clusterState = clusterService.state();
        QueryPage<Job> jobs = expandJobs("*", true, clusterService.state());
        for (Job job : jobs.results()) {
            if (isJobOpen(clusterState, job.getId())) {
                Set<String> jobFilters = job.getAnalysisConfig().extractReferencedFilters();
                if (jobFilters.contains(filter.getId())) {
                    updateJobProcessNotifier.submitJobUpdate(UpdateParams.filterUpdate(job.getId(), filter), ActionListener.wrap(
                            isUpdated -> {
                                if (isUpdated) {
                                    auditor.info(job.getId(),
                                            Messages.getMessage(Messages.JOB_AUDIT_FILTER_UPDATED_ON_PROCESS, filter.getId()));
                                }
                            }, e -> {}
                    ));
                }
            }
        }
    }

    public void updateProcessOnCalendarChanged(List<String> calendarJobIds) {
        ClusterState clusterState = clusterService.state();
        Set<String> expandedJobIds = new HashSet<>();
        calendarJobIds.forEach(jobId -> expandedJobIds.addAll(expandJobIds(jobId, true, clusterState)));
        for (String jobId : expandedJobIds) {
            if (isJobOpen(clusterState, jobId)) {
                updateJobProcessNotifier.submitJobUpdate(UpdateParams.scheduledEventsUpdate(jobId), ActionListener.wrap(
                        isUpdated -> {
                            if (isUpdated) {
                                auditor.info(jobId, Messages.getMessage(Messages.JOB_AUDIT_CALENDARS_UPDATED_ON_PROCESS));
                            }
                        }, e -> {}
                ));
            }
        }
    }

    public void deleteJob(DeleteJobAction.Request request, JobStorageDeletionTask task,
                          ActionListener<DeleteJobAction.Response> actionListener) {

        String jobId = request.getJobId();
        logger.debug("Deleting job '" + jobId + "'");

        // Step 4. When the job has been removed from the cluster state, return a response
        // -------
        CheckedConsumer<Boolean, Exception> apiResponseHandler = jobDeleted -> {
            if (jobDeleted) {
                logger.info("Job [" + jobId + "] deleted");
                auditor.info(jobId, Messages.getMessage(Messages.JOB_AUDIT_DELETED));
                actionListener.onResponse(new DeleteJobAction.Response(true));
            } else {
                actionListener.onResponse(new DeleteJobAction.Response(false));
            }
        };

        // Step 3. When the physical storage has been deleted, remove from Cluster State
        // -------
        CheckedConsumer<Boolean, Exception> deleteJobStateHandler = response -> clusterService.submitStateUpdateTask("delete-job-" + jobId,
                new AckedClusterStateUpdateTask<Boolean>(request, ActionListener.wrap(apiResponseHandler, actionListener::onFailure)) {

                    @Override
                    protected Boolean newResponse(boolean acknowledged) {
                        return acknowledged && response;
                    }

                    @Override
                    public ClusterState execute(ClusterState currentState) throws Exception {
                        MlMetadata currentMlMetadata = currentState.metaData().custom(MLMetadataField.TYPE);
                        if (currentMlMetadata.getJobs().containsKey(jobId) == false) {
                            // We wouldn't have got here if the job never existed so
                            // the Job must have been deleted by another action.
                            // Don't error in this case
                            return currentState;
                        }

                        MlMetadata.Builder builder = new MlMetadata.Builder(currentMlMetadata);
                        builder.deleteJob(jobId, currentState.getMetaData().custom(PersistentTasksCustomMetaData.TYPE));
                        return buildNewClusterState(currentState, builder);
                    }
            });


        // Step 2. Remove the job from any calendars
        CheckedConsumer<Boolean, Exception> removeFromCalendarsHandler = response -> {
            jobProvider.removeJobFromCalendars(jobId, ActionListener.<Boolean>wrap(deleteJobStateHandler::accept,
                    actionListener::onFailure ));
        };


        // Step 1. Delete the physical storage

        // This task manages the physical deletion of the job state and results
        task.delete(jobId, client, clusterService.state(), removeFromCalendarsHandler, actionListener::onFailure);
    }

    public void revertSnapshot(RevertModelSnapshotAction.Request request, ActionListener<RevertModelSnapshotAction.Response> actionListener,
            ModelSnapshot modelSnapshot) {

        final ModelSizeStats modelSizeStats = modelSnapshot.getModelSizeStats();
        final JobResultsPersister persister = new JobResultsPersister(settings, client);

        // Step 3. After the model size stats is persisted, also persist the snapshot's quantiles and respond
        // -------
        CheckedConsumer<IndexResponse, Exception> modelSizeStatsResponseHandler = response -> {
            persister.persistQuantiles(modelSnapshot.getQuantiles(), WriteRequest.RefreshPolicy.IMMEDIATE,
                    ActionListener.wrap(quantilesResponse -> {
                        // The quantiles can be large, and totally dominate the output -
                        // it's clearer to remove them as they are not necessary for the revert op
                        ModelSnapshot snapshotWithoutQuantiles = new ModelSnapshot.Builder(modelSnapshot).setQuantiles(null).build();
                        actionListener.onResponse(new RevertModelSnapshotAction.Response(snapshotWithoutQuantiles));
                    }, actionListener::onFailure));
        };

        // Step 2. When the model_snapshot_id is updated on the job, persist the snapshot's model size stats with a touched log time
        // so that a search for the latest model size stats returns the reverted one.
        // -------
        CheckedConsumer<Boolean, Exception> updateHandler = response -> {
            if (response) {
                ModelSizeStats revertedModelSizeStats = new ModelSizeStats.Builder(modelSizeStats).setLogTime(new Date()).build();
                persister.persistModelSizeStats(revertedModelSizeStats, WriteRequest.RefreshPolicy.IMMEDIATE, ActionListener.wrap(
                        modelSizeStatsResponseHandler, actionListener::onFailure));
            }
        };

        // Step 1. Do the cluster state update
        // -------
        Consumer<Long> clusterStateHandler = response -> clusterService.submitStateUpdateTask("revert-snapshot-" + request.getJobId(),
                new AckedClusterStateUpdateTask<Boolean>(request, ActionListener.wrap(updateHandler, actionListener::onFailure)) {

            @Override
            protected Boolean newResponse(boolean acknowledged) {
                if (acknowledged) {
                    auditor.info(request.getJobId(), Messages.getMessage(Messages.JOB_AUDIT_REVERTED, modelSnapshot.getDescription()));
                    return true;
                }
                actionListener.onFailure(new IllegalStateException("Could not revert modelSnapshot on job ["
                        + request.getJobId() + "], not acknowledged by master."));
                return false;
            }

            @Override
            public ClusterState execute(ClusterState currentState) {
                Job job = getJobOrThrowIfUnknown(request.getJobId(), currentState);
                Job.Builder builder = new Job.Builder(job);
                builder.setModelSnapshotId(modelSnapshot.getSnapshotId());
                builder.setEstablishedModelMemory(response);
                return updateClusterState(builder.build(), true, currentState);
            }
        });

        // Step 0. Find the appropriate established model memory for the reverted job
        // -------
        jobProvider.getEstablishedMemoryUsage(request.getJobId(), modelSizeStats.getTimestamp(), modelSizeStats, clusterStateHandler,
                actionListener::onFailure);
    }

    private static MlMetadata.Builder createMlMetadataBuilder(ClusterState currentState) {
        MlMetadata currentMlMetadata = currentState.metaData().custom(MLMetadataField.TYPE);
        return new MlMetadata.Builder(currentMlMetadata);
    }

    private static ClusterState buildNewClusterState(ClusterState currentState, MlMetadata.Builder builder) {
        ClusterState.Builder newState = ClusterState.builder(currentState);
        newState.metaData(MetaData.builder(currentState.getMetaData()).putCustom(MLMetadataField.TYPE, builder.build()).build());
        return newState.build();
    }
}
