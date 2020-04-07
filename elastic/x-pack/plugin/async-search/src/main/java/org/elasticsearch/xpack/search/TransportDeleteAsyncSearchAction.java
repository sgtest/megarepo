/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.search;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionListenerResponseHandler;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportRequestOptions;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.search.action.DeleteAsyncSearchAction;

import java.io.IOException;

public class TransportDeleteAsyncSearchAction extends HandledTransportAction<DeleteAsyncSearchAction.Request, AcknowledgedResponse> {
    private final ClusterService clusterService;
    private final TransportService transportService;
    private final AsyncSearchIndexService store;

    @Inject
    public TransportDeleteAsyncSearchAction(TransportService transportService,
                                            ActionFilters actionFilters,
                                            ClusterService clusterService,
                                            ThreadPool threadPool,
                                            NamedWriteableRegistry registry,
                                            Client client) {
        super(DeleteAsyncSearchAction.NAME, transportService, actionFilters, DeleteAsyncSearchAction.Request::new);
        this.store = new AsyncSearchIndexService(clusterService, threadPool.getThreadContext(), client, registry);
        this.clusterService = clusterService;
        this.transportService = transportService;
    }

    @Override
    protected void doExecute(Task task, DeleteAsyncSearchAction.Request request, ActionListener<AcknowledgedResponse> listener) {
        try {
            AsyncSearchId searchId = AsyncSearchId.decode(request.getId());
            DiscoveryNode node = clusterService.state().nodes().get(searchId.getTaskId().getNodeId());
            if (clusterService.localNode().getId().equals(searchId.getTaskId().getNodeId()) || node == null) {
                cancelTaskAndDeleteResult(searchId, listener);
            } else {
                TransportRequestOptions.Builder builder = TransportRequestOptions.builder();
                transportService.sendRequest(node, DeleteAsyncSearchAction.NAME, request, builder.build(),
                    new ActionListenerResponseHandler<>(listener, AcknowledgedResponse::new, ThreadPool.Names.SAME));
            }
        } catch (Exception exc) {
            listener.onFailure(exc);
        }
    }

    private void cancelTaskAndDeleteResult(AsyncSearchId searchId, ActionListener<AcknowledgedResponse> listener) throws IOException {
        AsyncSearchTask task = store.getTask(taskManager, searchId);
        if (task != null) {
            task.cancelTask(() -> store.deleteResponse(searchId, false, listener));
        } else {
            // the task is not running anymore so we throw a not found exception if
            // the search id is also not present in the index (already deleted) or if the user
            // is not allowed to access it.
            store.getResponse(searchId, false,
                ActionListener.wrap(res -> store.deleteResponse(searchId, true, listener), listener::onFailure));
        }
    }
}
