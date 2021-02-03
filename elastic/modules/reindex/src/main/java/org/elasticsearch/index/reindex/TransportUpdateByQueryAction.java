/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.reindex;

import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.ParentTaskAssigningClient;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.index.mapper.IdFieldMapper;
import org.elasticsearch.index.mapper.IndexFieldMapper;
import org.elasticsearch.index.mapper.RoutingFieldMapper;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.Map;
import java.util.function.BiFunction;

public class TransportUpdateByQueryAction extends HandledTransportAction<UpdateByQueryRequest, BulkByScrollResponse> {

    private final ThreadPool threadPool;
    private final Client client;
    private final ScriptService scriptService;
    private final ClusterService clusterService;

    @Inject
    public TransportUpdateByQueryAction(ThreadPool threadPool, ActionFilters actionFilters, Client client,
                                        TransportService transportService, ScriptService scriptService, ClusterService clusterService) {
        super(UpdateByQueryAction.NAME, transportService, actionFilters,
            (Writeable.Reader<UpdateByQueryRequest>) UpdateByQueryRequest::new);
        this.threadPool = threadPool;
        this.client = client;
        this.scriptService = scriptService;
        this.clusterService = clusterService;
    }

    @Override
    protected void doExecute(Task task, UpdateByQueryRequest request, ActionListener<BulkByScrollResponse> listener) {
        BulkByScrollTask bulkByScrollTask = (BulkByScrollTask) task;
        BulkByScrollParallelizationHelper.startSlicedAction(request, bulkByScrollTask, UpdateByQueryAction.INSTANCE, listener, client,
            clusterService.localNode(),
            () -> {
                ClusterState state = clusterService.state();
                ParentTaskAssigningClient assigningClient = new ParentTaskAssigningClient(client, clusterService.localNode(),
                    bulkByScrollTask);
                new AsyncIndexBySearchAction(bulkByScrollTask, logger, assigningClient, threadPool, scriptService, request, state,
                    listener).start();
            }
        );
    }

    /**
     * Simple implementation of update-by-query using scrolling and bulk.
     */
    static class AsyncIndexBySearchAction extends AbstractAsyncBulkByScrollAction<UpdateByQueryRequest, TransportUpdateByQueryAction> {

        AsyncIndexBySearchAction(BulkByScrollTask task, Logger logger, ParentTaskAssigningClient client,
                                 ThreadPool threadPool, ScriptService scriptService, UpdateByQueryRequest request,
                                 ClusterState clusterState, ActionListener<BulkByScrollResponse> listener) {
            super(task,
                // use sequence number powered optimistic concurrency control
                false, true,
                logger, client, threadPool, request, listener, scriptService, null);
        }

        @Override
        public BiFunction<RequestWrapper<?>, ScrollableHitSource.Hit, RequestWrapper<?>> buildScriptApplier() {
            Script script = mainRequest.getScript();
            if (script != null) {
                return new UpdateByQueryScriptApplier(worker, scriptService, script, script.getParams());
            }
            return super.buildScriptApplier();
        }

        @Override
        protected RequestWrapper<IndexRequest> buildRequest(ScrollableHitSource.Hit doc) {
            IndexRequest index = new IndexRequest();
            index.index(doc.getIndex());
            index.id(doc.getId());
            index.source(doc.getSource(), doc.getXContentType());
            index.setIfSeqNo(doc.getSeqNo());
            index.setIfPrimaryTerm(doc.getPrimaryTerm());
            index.setPipeline(mainRequest.getPipeline());
            return wrap(index);
        }

        class UpdateByQueryScriptApplier extends ScriptApplier {

            UpdateByQueryScriptApplier(WorkerBulkByScrollTaskState taskWorker, ScriptService scriptService, Script script,
                                       Map<String, Object> params) {
                super(taskWorker, scriptService, script, params);
            }

            @Override
            protected void scriptChangedIndex(RequestWrapper<?> request, Object to) {
                throw new IllegalArgumentException("Modifying [" + IndexFieldMapper.NAME + "] not allowed");
            }

            @Override
            protected void scriptChangedId(RequestWrapper<?> request, Object to) {
                throw new IllegalArgumentException("Modifying [" + IdFieldMapper.NAME + "] not allowed");
            }

            @Override
            protected void scriptChangedVersion(RequestWrapper<?> request, Object to) {
                throw new IllegalArgumentException("Modifying [_version] not allowed");
            }

            @Override
            protected void scriptChangedRouting(RequestWrapper<?> request, Object to) {
                throw new IllegalArgumentException("Modifying [" + RoutingFieldMapper.NAME + "] not allowed");
            }

        }
    }
}
