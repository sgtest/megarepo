/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.license;

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
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

public class TransportPostStartBasicAction extends TransportMasterNodeAction<PostStartBasicRequest, PostStartBasicResponse> {

    private final LicenseService licenseService;

    @Inject
    public TransportPostStartBasicAction(Settings settings, TransportService transportService, ClusterService clusterService,
                                         LicenseService licenseService, ThreadPool threadPool, ActionFilters actionFilters,
                                         IndexNameExpressionResolver indexNameExpressionResolver) {
        super(settings, PostStartBasicAction.NAME, transportService, clusterService, threadPool, actionFilters,
                indexNameExpressionResolver, PostStartBasicRequest::new);
        this.licenseService = licenseService;
    }

    @Override
    protected String executor() {
        return ThreadPool.Names.SAME;
    }

    @Override
    protected PostStartBasicResponse newResponse() {
        return new PostStartBasicResponse();
    }

    @Override
    protected void masterOperation(PostStartBasicRequest request, ClusterState state,
                                   ActionListener<PostStartBasicResponse> listener) throws Exception {
        licenseService.startBasicLicense(request, listener);
    }

    @Override
    protected ClusterBlockException checkBlock(PostStartBasicRequest request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }
}
