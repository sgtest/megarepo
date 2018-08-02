/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.action.GetModelSnapshotsAction;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.ml.job.JobManager;
import org.elasticsearch.xpack.ml.job.persistence.JobResultsProvider;

import java.util.stream.Collectors;

public class TransportGetModelSnapshotsAction extends HandledTransportAction<GetModelSnapshotsAction.Request,
        GetModelSnapshotsAction.Response> {

    private final JobResultsProvider jobResultsProvider;
    private final JobManager jobManager;

    @Inject
    public TransportGetModelSnapshotsAction(Settings settings, TransportService transportService,
                                            ActionFilters actionFilters, JobResultsProvider jobResultsProvider, JobManager jobManager) {
        super(settings, GetModelSnapshotsAction.NAME, transportService, actionFilters,
            GetModelSnapshotsAction.Request::new);
        this.jobResultsProvider = jobResultsProvider;
        this.jobManager = jobManager;
    }

    @Override
    protected void doExecute(Task task, GetModelSnapshotsAction.Request request,
                             ActionListener<GetModelSnapshotsAction.Response> listener) {
        logger.debug("Get model snapshots for job {} snapshot ID {}. from = {}, size = {}"
                + " start = '{}', end='{}', sort={} descending={}",
                request.getJobId(), request.getSnapshotId(), request.getPageParams().getFrom(), request.getPageParams().getSize(),
                request.getStart(), request.getEnd(), request.getSort(), request.getDescOrder());

        jobManager.getJobOrThrowIfUnknown(request.getJobId());

        jobResultsProvider.modelSnapshots(request.getJobId(), request.getPageParams().getFrom(), request.getPageParams().getSize(),
                request.getStart(), request.getEnd(), request.getSort(), request.getDescOrder(), request.getSnapshotId(),
                page -> {
                    listener.onResponse(new GetModelSnapshotsAction.Response(clearQuantiles(page)));
                }, listener::onFailure);
    }

    public static QueryPage<ModelSnapshot> clearQuantiles(QueryPage<ModelSnapshot> page) {
        if (page.results() == null) {
            return page;
        }
        return new QueryPage<>(page.results().stream().map(snapshot ->
                new ModelSnapshot.Builder(snapshot).setQuantiles(null).build())
                .collect(Collectors.toList()), page.count(), page.getResultsField());
    }
}
