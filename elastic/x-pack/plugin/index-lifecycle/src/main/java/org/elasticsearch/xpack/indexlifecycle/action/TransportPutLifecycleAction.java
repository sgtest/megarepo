/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.indexlifecycle.action;

import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.cluster.AckedClusterStateUpdateTask;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.indexlifecycle.IndexLifecycleMetadata;
import org.elasticsearch.xpack.core.indexlifecycle.LifecyclePolicyMetadata;
import org.elasticsearch.xpack.core.indexlifecycle.OperationMode;
import org.elasticsearch.xpack.core.indexlifecycle.action.PutLifecycleAction;
import org.elasticsearch.xpack.core.indexlifecycle.action.PutLifecycleAction.Request;
import org.elasticsearch.xpack.core.indexlifecycle.action.PutLifecycleAction.Response;
import org.elasticsearch.xpack.indexlifecycle.IndexLifecycleRunner;

import java.util.Map;
import java.util.SortedMap;
import java.util.TreeMap;
import java.util.stream.Collectors;

/**
 * This class is responsible for bootstrapping {@link IndexLifecycleMetadata} into the cluster-state, as well
 * as adding the desired new policy to be inserted.
 */
public class TransportPutLifecycleAction extends TransportMasterNodeAction<Request, Response> {

    @Inject
    public TransportPutLifecycleAction(Settings settings, TransportService transportService, ClusterService clusterService,
            ThreadPool threadPool, ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver) {
        super(settings, PutLifecycleAction.NAME, transportService, clusterService, threadPool, actionFilters, indexNameExpressionResolver,
                Request::new);
    }

    @Override
    protected String executor() {
        return ThreadPool.Names.SAME;
    }

    @Override
    protected Response newResponse() {
        return new Response();
    }

    @Override
    protected void masterOperation(Request request, ClusterState state, ActionListener<Response> listener) throws Exception {
        clusterService.submitStateUpdateTask("put-lifecycle-" + request.getPolicy().getName(),
                new AckedClusterStateUpdateTask<Response>(request, listener) {
                    @Override
                    protected Response newResponse(boolean acknowledged) {
                        return new Response(acknowledged);
                    }

                    @Override
                    public ClusterState execute(ClusterState currentState) throws Exception {
                        ClusterState.Builder newState = ClusterState.builder(currentState);
                        IndexLifecycleMetadata currentMetadata = currentState.metaData().custom(IndexLifecycleMetadata.TYPE);
                        if (currentMetadata == null) { // first time using index-lifecycle feature, bootstrap metadata
                            currentMetadata = IndexLifecycleMetadata.EMPTY;
                        }
                        if (currentMetadata.getPolicyMetadatas().containsKey(request.getPolicy().getName()) && IndexLifecycleRunner
                                .canUpdatePolicy(request.getPolicy().getName(), request.getPolicy(), currentState) == false) {
                            throw new ResourceAlreadyExistsException("Lifecycle policy already exists: {}",
                                    request.getPolicy().getName());
                        }
                        // NORELEASE Check if current step exists in new policy and if not move to next available step
                        SortedMap<String, LifecyclePolicyMetadata> newPolicies = new TreeMap<>(currentMetadata.getPolicyMetadatas());

                        Map<String, String> filteredHeaders = threadPool.getThreadContext().getHeaders().entrySet().stream()
                                .filter(e -> ClientHelper.SECURITY_HEADER_FILTERS.contains(e.getKey()))
                                .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue));
                        LifecyclePolicyMetadata lifecyclePolicyMetadata = new LifecyclePolicyMetadata(request.getPolicy(), filteredHeaders);
                        newPolicies.put(lifecyclePolicyMetadata.getName(), lifecyclePolicyMetadata);
                        IndexLifecycleMetadata newMetadata = new IndexLifecycleMetadata(newPolicies, OperationMode.RUNNING);
                        newState.metaData(MetaData.builder(currentState.getMetaData())
                                .putCustom(IndexLifecycleMetadata.TYPE, newMetadata).build());
                        return newState.build();
                    }
                });
    }

    @Override
    protected ClusterBlockException checkBlock(Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }
}