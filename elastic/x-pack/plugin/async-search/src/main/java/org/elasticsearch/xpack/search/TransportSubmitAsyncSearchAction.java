/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.search;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchTask;
import org.elasticsearch.action.search.TransportSearchAction;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.index.engine.DocumentMissingException;
import org.elasticsearch.index.engine.VersionConflictEngineException;
import org.elasticsearch.search.SearchService;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.tasks.CancellableTask;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskCancelledException;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.async.AsyncExecutionId;
import org.elasticsearch.xpack.core.async.AsyncTaskIndexService;
import org.elasticsearch.xpack.core.search.action.AsyncSearchResponse;
import org.elasticsearch.xpack.core.search.action.SubmitAsyncSearchAction;
import org.elasticsearch.xpack.core.search.action.SubmitAsyncSearchRequest;

import java.util.Map;
import java.util.function.Function;
import java.util.function.Supplier;

import static org.elasticsearch.xpack.core.ClientHelper.ASYNC_SEARCH_ORIGIN;

public class TransportSubmitAsyncSearchAction extends HandledTransportAction<SubmitAsyncSearchRequest, AsyncSearchResponse> {
    private static final Logger logger = LogManager.getLogger(TransportSubmitAsyncSearchAction.class);

    private final NodeClient nodeClient;
    private final Function<SearchRequest, InternalAggregation.ReduceContext> requestToAggReduceContextBuilder;
    private final TransportSearchAction searchAction;
    private final ThreadContext threadContext;
    private final AsyncTaskIndexService<AsyncSearchResponse> store;

    @Inject
    public TransportSubmitAsyncSearchAction(ClusterService clusterService,
                                            TransportService transportService,
                                            ActionFilters actionFilters,
                                            NamedWriteableRegistry registry,
                                            Client client,
                                            NodeClient nodeClient,
                                            SearchService searchService,
                                            TransportSearchAction searchAction) {
        super(SubmitAsyncSearchAction.NAME, transportService, actionFilters, SubmitAsyncSearchRequest::new);
        this.nodeClient = nodeClient;
        this.requestToAggReduceContextBuilder = request -> searchService.aggReduceContextBuilder(request).forFinalReduction();
        this.searchAction = searchAction;
        this.threadContext = transportService.getThreadPool().getThreadContext();
        this.store = new AsyncTaskIndexService<>(AsyncSearch.INDEX, clusterService, threadContext, client,
            ASYNC_SEARCH_ORIGIN, AsyncSearchResponse::new, registry);
    }

    @Override
    protected void doExecute(Task task, SubmitAsyncSearchRequest request, ActionListener<AsyncSearchResponse> submitListener) {
        CancellableTask submitTask = (CancellableTask) task;
        final SearchRequest searchRequest = createSearchRequest(request, submitTask, request.getKeepAlive());
        AsyncSearchTask searchTask = (AsyncSearchTask) taskManager.register("transport", SearchAction.INSTANCE.name(), searchRequest);
        searchAction.execute(searchTask, searchRequest, searchTask.getSearchProgressActionListener());
        searchTask.addCompletionListener(
            new ActionListener<>() {
                @Override
                public void onResponse(AsyncSearchResponse searchResponse) {
                    if (searchResponse.isRunning() || request.isKeepOnCompletion()) {
                        // the task is still running and the user cannot wait more so we create
                        // a document for further retrieval
                        try {
                            if (submitTask.isCancelled()) {
                                // the user cancelled the submit so we don't store anything
                                // and propagate the failure
                                Exception cause = new TaskCancelledException(submitTask.getReasonCancelled());
                                onFatalFailure(searchTask, cause, searchResponse.isRunning(), submitListener);
                            } else {
                                final String docId = searchTask.getExecutionId().getDocId();
                                // creates the fallback response if the node crashes/restarts in the middle of the request
                                // TODO: store intermediate results ?
                                AsyncSearchResponse initialResp = searchResponse.clone(searchResponse.getId());
                                store.storeInitialResponse(docId, searchTask.getOriginHeaders(), initialResp,
                                    new ActionListener<>() {
                                        @Override
                                        public void onResponse(IndexResponse r) {
                                            if (searchResponse.isRunning()) {
                                                try {
                                                    // store the final response on completion unless the submit is cancelled
                                                    searchTask.addCompletionListener(finalResponse ->
                                                        onFinalResponse(submitTask, searchTask, finalResponse, () -> {}));
                                                } finally {
                                                    submitListener.onResponse(searchResponse);
                                                }
                                            } else {
                                                onFinalResponse(submitTask, searchTask, searchResponse,
                                                    () -> submitListener.onResponse(searchResponse));
                                            }
                                        }

                                        @Override
                                        public void onFailure(Exception exc) {
                                            onFatalFailure(searchTask, exc, searchResponse.isRunning(), submitListener);
                                        }
                                    });
                            }
                        } catch (Exception exc) {
                            onFatalFailure(searchTask, exc, searchResponse.isRunning(), submitListener);
                        }
                    } else {
                        // the task completed within the timeout so the response is sent back to the user
                        // with a null id since nothing was stored on the cluster.
                        taskManager.unregister(searchTask);
                        submitListener.onResponse(searchResponse.clone(null));
                    }
                }

                @Override
                public void onFailure(Exception exc) {
                    submitListener.onFailure(exc);
                }
            }, request.getWaitForCompletionTimeout());
    }

    private SearchRequest createSearchRequest(SubmitAsyncSearchRequest request, CancellableTask submitTask, TimeValue keepAlive) {
        String docID = UUIDs.randomBase64UUID();
        Map<String, String> originHeaders = nodeClient.threadPool().getThreadContext().getHeaders();
        SearchRequest searchRequest = new SearchRequest(request.getSearchRequest()) {
            @Override
            public AsyncSearchTask createTask(long id, String type, String action, TaskId parentTaskId, Map<String, String> taskHeaders) {
                AsyncExecutionId searchId = new AsyncExecutionId(docID, new TaskId(nodeClient.getLocalNodeId(), id));
                Supplier<InternalAggregation.ReduceContext> aggReduceContextSupplier =
                        () -> requestToAggReduceContextBuilder.apply(request.getSearchRequest());
                return new AsyncSearchTask(id, type, action, parentTaskId,
                    submitTask::isCancelled, keepAlive, originHeaders, taskHeaders, searchId, store.getClient(),
                    nodeClient.threadPool(), aggReduceContextSupplier);
            }
        };
        searchRequest.setParentTask(new TaskId(nodeClient.getLocalNodeId(), submitTask.getId()));
        return searchRequest;
    }

    private void onFatalFailure(AsyncSearchTask task, Exception error, boolean shouldCancel, ActionListener<AsyncSearchResponse> listener) {
        if (shouldCancel && task.isCancelled() == false) {
            task.cancelTask(() -> {
                try {
                    task.addCompletionListener(finalResponse -> taskManager.unregister(task));
                } finally {
                    listener.onFailure(error);
                }
            });
        } else {
            try {
                task.addCompletionListener(finalResponse -> taskManager.unregister(task));
            } finally {
                listener.onFailure(error);
            }
        }
    }

    private void onFinalResponse(CancellableTask submitTask,
                                 AsyncSearchTask searchTask,
                                 AsyncSearchResponse response,
                                 Runnable nextAction) {
        if (submitTask.isCancelled() || searchTask.isCancelled()) {
            // the task was cancelled so we ensure that there is nothing stored in the response index.
            store.deleteResponse(searchTask.getExecutionId(), ActionListener.wrap(
                resp -> unregisterTaskAndMoveOn(searchTask, nextAction),
                exc -> {
                    logger.error(() -> new ParameterizedMessage("failed to clean async-search [{}]", searchTask.getExecutionId()), exc);
                    unregisterTaskAndMoveOn(searchTask, nextAction);
                }));
            return;
        }

        try {
            store.storeFinalResponse(searchTask.getExecutionId().getDocId(), threadContext.getResponseHeaders(),response,
                ActionListener.wrap(resp -> unregisterTaskAndMoveOn(searchTask, nextAction),
                                    exc -> {
                                        Throwable cause = ExceptionsHelper.unwrapCause(exc);
                                        if (cause instanceof DocumentMissingException == false &&
                                                cause instanceof VersionConflictEngineException == false) {
                                            logger.error(() -> new ParameterizedMessage("failed to store async-search [{}]",
                                                searchTask.getExecutionId().getEncoded()), exc);
                                        }
                                        unregisterTaskAndMoveOn(searchTask, nextAction);
                                    }));
        } catch (Exception exc) {
            logger.error(() -> new ParameterizedMessage("failed to store async-search [{}]", searchTask.getExecutionId().getEncoded()),
                exc);
            unregisterTaskAndMoveOn(searchTask, nextAction);
        }
    }

    private void unregisterTaskAndMoveOn(SearchTask searchTask, Runnable nextAction) {
        taskManager.unregister(searchTask);
        nextAction.run();
    }
}
