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
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.analysis.AnalysisRegistry;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.ml.action.DeleteJobAction;
import org.elasticsearch.xpack.core.ml.action.PutDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.PutJobAction;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.ml.datafeed.DatafeedManager;
import org.elasticsearch.xpack.ml.job.JobManager;

public class TransportPutJobAction extends TransportMasterNodeAction<PutJobAction.Request, PutJobAction.Response> {

    private static final Logger logger = LogManager.getLogger(TransportPutJobAction.class);
    private final JobManager jobManager;
    private final DatafeedManager datafeedManager;
    private final XPackLicenseState licenseState;
    private final AnalysisRegistry analysisRegistry;
    private final SecurityContext securityContext;


    @Inject
    public TransportPutJobAction(Settings settings, TransportService transportService, ClusterService clusterService,
                                 ThreadPool threadPool, XPackLicenseState licenseState, ActionFilters actionFilters,
                                 IndexNameExpressionResolver indexNameExpressionResolver, JobManager jobManager,
                                 DatafeedManager datafeedManager, AnalysisRegistry analysisRegistry) {
        super(PutJobAction.NAME, transportService, clusterService, threadPool, actionFilters, PutJobAction.Request::new,
            indexNameExpressionResolver, PutJobAction.Response::new, ThreadPool.Names.SAME);
        this.licenseState = licenseState;
        this.jobManager = jobManager;
        this.analysisRegistry = analysisRegistry;
        this.datafeedManager = datafeedManager;
        this.securityContext = XPackSettings.SECURITY_ENABLED.get(settings) ?
            new SecurityContext(settings, threadPool.getThreadContext()) : null;
    }

    @Override
    protected void masterOperation(Task task, PutJobAction.Request request, ClusterState state,
                                   ActionListener<PutJobAction.Response> listener) throws Exception {
        jobManager.putJob(request, analysisRegistry, state, ActionListener.wrap(
            jobCreated -> {
                if (jobCreated.getResponse().getDatafeedConfig().isPresent() == false) {
                    listener.onResponse(jobCreated);
                    return;
                }
                datafeedManager.putDatafeed(
                    new PutDatafeedAction.Request(jobCreated.getResponse().getDatafeedConfig().get()),
                    // Use newer state from cluster service as the job creation may have created shared indexes
                    clusterService.state(),
                    licenseState,
                    securityContext,
                    threadPool,
                    ActionListener.wrap(
                        createdDatafeed -> listener.onResponse(jobCreated),
                        failed -> jobManager.deleteJob(
                            new DeleteJobAction.Request(request.getJobBuilder().getId()),
                            state,
                            ActionListener.wrap(
                                deleted -> listener.onFailure(failed),
                                deleteFailed -> {
                                    logger.warn(
                                        () -> new ParameterizedMessage(
                                            "[{}] failed to cleanup job after datafeed creation failure",
                                            request.getJobBuilder().getId()
                                        ),
                                        deleteFailed);
                                    ElasticsearchException ex = new ElasticsearchException(
                                        "failed to cleanup job after datafeed creation failure",
                                        failed
                                    );
                                    ex.addSuppressed(deleteFailed);
                                    listener.onFailure(ex);
                                }
                            )
                        )
                    ));
            },
            listener::onFailure
        ));
    }

    @Override
    protected ClusterBlockException checkBlock(PutJobAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    @Override
    protected void doExecute(Task task, PutJobAction.Request request, ActionListener<PutJobAction.Response> listener) {
        if (licenseState.checkFeature(XPackLicenseState.Feature.MACHINE_LEARNING)) {
            super.doExecute(task, request, listener);
        } else {
            listener.onFailure(LicenseUtils.newComplianceException(XPackField.MACHINE_LEARNING));
        }
    }
}
