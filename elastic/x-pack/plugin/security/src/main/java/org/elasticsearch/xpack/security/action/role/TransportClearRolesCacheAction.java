/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.action.role;

import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.nodes.TransportNodesAction;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.xpack.core.security.action.role.ClearRolesCacheAction;
import org.elasticsearch.xpack.core.security.action.role.ClearRolesCacheRequest;
import org.elasticsearch.xpack.core.security.action.role.ClearRolesCacheResponse;
import org.elasticsearch.xpack.security.authz.store.CompositeRolesStore;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.List;

public class TransportClearRolesCacheAction extends TransportNodesAction<ClearRolesCacheRequest, ClearRolesCacheResponse,
        ClearRolesCacheRequest.Node, ClearRolesCacheResponse.Node> {

    private final CompositeRolesStore rolesStore;

    @Inject
    public TransportClearRolesCacheAction(Settings settings, ThreadPool threadPool,
                                          ClusterService clusterService, TransportService transportService, ActionFilters actionFilters,
                                          CompositeRolesStore rolesStore, IndexNameExpressionResolver indexNameExpressionResolver) {
        super(settings, ClearRolesCacheAction.NAME, threadPool, clusterService, transportService,
              actionFilters, indexNameExpressionResolver, ClearRolesCacheRequest::new, ClearRolesCacheRequest.Node::new,
              ThreadPool.Names.MANAGEMENT, ClearRolesCacheResponse.Node.class);
        this.rolesStore = rolesStore;
    }

    @Override
    protected ClearRolesCacheResponse newResponse(ClearRolesCacheRequest request,
                                                  List<ClearRolesCacheResponse.Node> responses, List<FailedNodeException> failures) {
        return new ClearRolesCacheResponse(clusterService.getClusterName(), responses, failures);
    }

    @Override
    protected ClearRolesCacheRequest.Node newNodeRequest(String nodeId, ClearRolesCacheRequest request) {
        return new ClearRolesCacheRequest.Node(request, nodeId);
    }

    @Override
    protected ClearRolesCacheResponse.Node newNodeResponse() {
        return new ClearRolesCacheResponse.Node();
    }

    @Override
    protected ClearRolesCacheResponse.Node nodeOperation(ClearRolesCacheRequest.Node request) {
        if (request.getNames() == null || request.getNames().length == 0) {
            rolesStore.invalidateAll();
        } else {
            for (String role : request.getNames()) {
                rolesStore.invalidate(role);
            }
        }
        return new ClearRolesCacheResponse.Node(clusterService.localNode());
    }

}
