/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.action.GetCalendarEventsAction;
import org.elasticsearch.xpack.core.ml.action.GetCalendarsAction;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.calendars.ScheduledEvent;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.ml.job.persistence.ScheduledEventsQueryBuilder;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.job.persistence.JobProvider;

import java.util.Collections;
import java.util.List;

public class TransportGetCalendarEventsAction extends HandledTransportAction<GetCalendarEventsAction.Request,
        GetCalendarEventsAction.Response> {

    private final JobProvider jobProvider;
    private final ClusterService clusterService;

    @Inject
    public TransportGetCalendarEventsAction(Settings settings, ThreadPool threadPool,
                                            TransportService transportService, ActionFilters actionFilters,
                                            IndexNameExpressionResolver indexNameExpressionResolver,
                                            ClusterService clusterService, JobProvider jobProvider) {
        super(settings, GetCalendarEventsAction.NAME, threadPool, transportService, actionFilters,
                indexNameExpressionResolver, GetCalendarEventsAction.Request::new);
        this.jobProvider = jobProvider;
        this.clusterService = clusterService;
    }

    @Override
    protected void doExecute(GetCalendarEventsAction.Request request,
                             ActionListener<GetCalendarEventsAction.Response> listener) {
        ActionListener<Boolean> calendarExistsListener = ActionListener.wrap(
                r -> {
                    ScheduledEventsQueryBuilder query = new ScheduledEventsQueryBuilder()
                            .start(request.getStart())
                            .end(request.getEnd())
                            .from(request.getPageParams().getFrom())
                            .size(request.getPageParams().getSize());

                    if (GetCalendarsAction.Request.ALL.equals(request.getCalendarId()) == false) {
                        query.calendarIds(Collections.singletonList(request.getCalendarId()));
                    }

                    ActionListener<QueryPage<ScheduledEvent>> eventsListener = ActionListener.wrap(
                            events -> {
                                listener.onResponse(new GetCalendarEventsAction.Response(events));
                            },
                            listener::onFailure
                    );

                    if (request.getJobId() != null) {
                        ClusterState state = clusterService.state();
                        MlMetadata currentMlMetadata = MlMetadata.getMlMetadata(state);

                        List<String> jobGroups;
                        String requestId = request.getJobId();

                        Job job = currentMlMetadata.getJobs().get(request.getJobId());
                        if (job == null) {
                            // Check if the requested id is a job group
                            if (currentMlMetadata.isGroupOrJob(request.getJobId()) == false) {
                                listener.onFailure(ExceptionsHelper.missingJobException(request.getJobId()));
                                return;
                            }
                            jobGroups = Collections.singletonList(request.getJobId());
                            requestId = null;
                        } else {
                            jobGroups = job.getGroups();
                        }

                        jobProvider.scheduledEventsForJob(requestId, jobGroups, query, eventsListener);
                    } else {
                        jobProvider.scheduledEvents(query, eventsListener);
                    }
                },
                listener::onFailure);

        checkCalendarExists(request.getCalendarId(), calendarExistsListener);
    }

    private void checkCalendarExists(String calendarId, ActionListener<Boolean> listener) {
        if (GetCalendarsAction.Request.ALL.equals(calendarId)) {
            listener.onResponse(true);
            return;
        }

        jobProvider.calendar(calendarId, ActionListener.wrap(
                c -> listener.onResponse(true),
                listener::onFailure
        ));
    }
}
