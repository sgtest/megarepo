/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.action.support.ThreadedActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.index.reindex.AbstractBulkByScrollRequest;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.ml.action.DeleteExpiredDataAction;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.job.persistence.JobConfigProvider;
import org.elasticsearch.xpack.ml.job.persistence.JobResultsProvider;
import org.elasticsearch.xpack.ml.job.persistence.SearchAfterJobsIterator;
import org.elasticsearch.xpack.ml.job.retention.EmptyStateIndexRemover;
import org.elasticsearch.xpack.ml.job.retention.ExpiredForecastsRemover;
import org.elasticsearch.xpack.ml.job.retention.ExpiredModelSnapshotsRemover;
import org.elasticsearch.xpack.ml.job.retention.ExpiredResultsRemover;
import org.elasticsearch.xpack.ml.job.retention.MlDataRemover;
import org.elasticsearch.xpack.ml.job.retention.UnusedStateRemover;
import org.elasticsearch.xpack.ml.job.retention.UnusedStatsRemover;
import org.elasticsearch.xpack.ml.notifications.AnomalyDetectionAuditor;
import org.elasticsearch.xpack.ml.utils.VolatileCursorIterator;
import org.elasticsearch.xpack.ml.utils.persistence.WrappedBatchedJobsIterator;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.util.Arrays;
import java.util.Iterator;
import java.util.List;
import java.util.function.Supplier;
import java.util.stream.Collectors;

public class TransportDeleteExpiredDataAction extends HandledTransportAction<DeleteExpiredDataAction.Request,
    DeleteExpiredDataAction.Response> {

    private static final Logger logger = LogManager.getLogger(TransportDeleteExpiredDataAction.class);

    static final Duration DEFAULT_MAX_DURATION = Duration.ofHours(8);

    private final ThreadPool threadPool;
    private final String executor;
    private final OriginSettingClient client;
    private final ClusterService clusterService;
    private final Clock clock;
    private final JobConfigProvider jobConfigProvider;
    private final JobResultsProvider jobResultsProvider;

    @Inject
    public TransportDeleteExpiredDataAction(ThreadPool threadPool, TransportService transportService,
                                            ActionFilters actionFilters, Client client, ClusterService clusterService,
                                            JobConfigProvider jobConfigProvider, JobResultsProvider jobResultsProvider) {
        this(threadPool, MachineLearning.UTILITY_THREAD_POOL_NAME, transportService, actionFilters, client, clusterService,
            jobConfigProvider, jobResultsProvider, Clock.systemUTC());
    }

    TransportDeleteExpiredDataAction(ThreadPool threadPool, String executor, TransportService transportService,
                                     ActionFilters actionFilters, Client client, ClusterService clusterService,
                                     JobConfigProvider jobConfigProvider, JobResultsProvider jobResultsProvider, Clock clock) {
        super(DeleteExpiredDataAction.NAME, transportService, actionFilters, DeleteExpiredDataAction.Request::new, executor);
        this.threadPool = threadPool;
        this.executor = executor;
        this.client = new OriginSettingClient(client, ClientHelper.ML_ORIGIN);
        this.clusterService = clusterService;
        this.clock = clock;
        this.jobConfigProvider = jobConfigProvider;
        this.jobResultsProvider = jobResultsProvider;
    }

    @Override
    protected void doExecute(Task task, DeleteExpiredDataAction.Request request,
                             ActionListener<DeleteExpiredDataAction.Response> listener) {
        logger.info("Deleting expired data");
        Instant timeoutTime = Instant.now(clock).plus(
            request.getTimeout() == null ? DEFAULT_MAX_DURATION : Duration.ofMillis(request.getTimeout().millis())
        );

        TaskId taskId = new TaskId(clusterService.localNode().getId(), task.getId());

        Supplier<Boolean> isTimedOutSupplier = () -> Instant.now(clock).isAfter(timeoutTime);
        AnomalyDetectionAuditor auditor = new AnomalyDetectionAuditor(client, clusterService.getNodeName());

        if (Strings.isNullOrEmpty(request.getJobId()) || Strings.isAllOrWildcard(new String[]{request.getJobId()})) {
            List<MlDataRemover> dataRemovers = createDataRemovers(client, taskId, auditor);
            threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME).execute(
                () -> deleteExpiredData(request, dataRemovers, listener, isTimedOutSupplier)
            );
        } else {
            jobConfigProvider.expandJobs(request.getJobId(), false, true, ActionListener.wrap(
                jobBuilders -> {
                    threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME).execute(() -> {
                            List<Job> jobs = jobBuilders.stream().map(Job.Builder::build).collect(Collectors.toList());
                            List<MlDataRemover> dataRemovers = createDataRemovers(jobs, taskId, auditor);
                            deleteExpiredData(request, dataRemovers, listener, isTimedOutSupplier);
                        }
                    );
                },
                listener::onFailure
            ));
        }
    }

    private void deleteExpiredData(DeleteExpiredDataAction.Request request,
                                   List<MlDataRemover> dataRemovers,
                                   ActionListener<DeleteExpiredDataAction.Response> listener,
                                   Supplier<Boolean> isTimedOutSupplier) {
        Iterator<MlDataRemover> dataRemoversIterator = new VolatileCursorIterator<>(dataRemovers);
        // If there is no throttle provided, default to none
        float requestsPerSec = request.getRequestsPerSecond() == null ? Float.POSITIVE_INFINITY : request.getRequestsPerSecond();
        int numberOfDatanodes = Math.max(clusterService.state().getNodes().getDataNodes().size(), 1);
        if (requestsPerSec == -1.0f) {
            // With DEFAULT_SCROLL_SIZE = 1000 and a single data node this implies we spread deletion of
            //   1 million documents over 5000 seconds ~= 83 minutes.
            // If we have > 5 data nodes, we don't set our throttling.
            requestsPerSec = numberOfDatanodes < 5 ?
                (float) (AbstractBulkByScrollRequest.DEFAULT_SCROLL_SIZE / 5) * numberOfDatanodes :
                Float.POSITIVE_INFINITY;
        }
        deleteExpiredData(dataRemoversIterator, requestsPerSec, listener, isTimedOutSupplier, true);
    }

    void deleteExpiredData(Iterator<MlDataRemover> mlDataRemoversIterator,
                           float requestsPerSecond,
                           ActionListener<DeleteExpiredDataAction.Response> listener,
                           Supplier<Boolean> isTimedOutSupplier,
                           boolean haveAllPreviousDeletionsCompleted) {
        if (haveAllPreviousDeletionsCompleted && mlDataRemoversIterator.hasNext()) {
            MlDataRemover remover = mlDataRemoversIterator.next();
            ActionListener<Boolean> nextListener = ActionListener.wrap(
                booleanResponse ->
                    deleteExpiredData(
                        mlDataRemoversIterator,
                        requestsPerSecond,
                        listener,
                        isTimedOutSupplier,
                        booleanResponse
                    ),
                listener::onFailure);
            // Removing expired ML data and artifacts requires multiple operations.
            // These are queued up and executed sequentially in the action listener,
            // the chained calls must all run the ML utility thread pool NOT the thread
            // the previous action returned in which in the case of a transport_client_boss
            // thread is a disaster.
            remover.remove(requestsPerSecond, new ThreadedActionListener<>(logger, threadPool, executor, nextListener, false),
                isTimedOutSupplier);
        } else {
            if (haveAllPreviousDeletionsCompleted) {
                logger.info("Completed deletion of expired ML data");
            } else {
                logger.info("Halted deletion of expired ML data until next invocation");
            }
            listener.onResponse(new DeleteExpiredDataAction.Response(haveAllPreviousDeletionsCompleted));
        }
    }

    private List<MlDataRemover> createDataRemovers(OriginSettingClient client,
                                                   TaskId parentTaskId,
                                                   AnomalyDetectionAuditor auditor) {
        return Arrays.asList(
            new ExpiredResultsRemover(client,
                new WrappedBatchedJobsIterator(new SearchAfterJobsIterator(client)), parentTaskId, auditor, threadPool),
            new ExpiredForecastsRemover(client, threadPool, parentTaskId),
            new ExpiredModelSnapshotsRemover(client,
                new WrappedBatchedJobsIterator(new SearchAfterJobsIterator(client)), threadPool, parentTaskId, jobResultsProvider, auditor),
            new UnusedStateRemover(client, clusterService, parentTaskId),
            new EmptyStateIndexRemover(client, parentTaskId),
            new UnusedStatsRemover(client, parentTaskId));
    }

    private List<MlDataRemover> createDataRemovers(List<Job> jobs, TaskId parentTaskId, AnomalyDetectionAuditor auditor) {
        return Arrays.asList(
            new ExpiredResultsRemover(client, new VolatileCursorIterator<>(jobs), parentTaskId, auditor, threadPool),
            new ExpiredForecastsRemover(client, threadPool, parentTaskId),
            new ExpiredModelSnapshotsRemover(client,
                new VolatileCursorIterator<>(jobs),
                threadPool, parentTaskId,
                jobResultsProvider,
                auditor),
            new UnusedStateRemover(client, clusterService, parentTaskId),
            new EmptyStateIndexRemover(client, parentTaskId),
            new UnusedStatsRemover(client, parentTaskId));
    }

}
