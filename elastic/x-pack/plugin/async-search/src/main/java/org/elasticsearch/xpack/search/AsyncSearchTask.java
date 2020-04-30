/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.search;

import org.apache.lucene.search.TotalHits;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksRequest;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksResponse;
import org.elasticsearch.action.search.SearchProgressActionListener;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchResponse.Clusters;
import org.elasticsearch.action.search.SearchShard;
import org.elasticsearch.action.search.SearchTask;
import org.elasticsearch.action.search.ShardSearchFailure;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.io.stream.DelayableWriteable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.EsRejectedExecutionException;
import org.elasticsearch.search.SearchShardTarget;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.search.aggregations.InternalAggregations;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.threadpool.Scheduler.Cancellable;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.async.AsyncExecutionId;
import org.elasticsearch.xpack.core.async.AsyncTask;
import org.elasticsearch.xpack.core.search.action.AsyncSearchResponse;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import java.util.function.Supplier;

import static java.util.Collections.singletonList;

/**
 * Task that tracks the progress of a currently running {@link SearchRequest}.
 */
final class AsyncSearchTask extends SearchTask implements AsyncTask {
    private final BooleanSupplier checkSubmitCancellation;
    private final AsyncExecutionId searchId;
    private final Client client;
    private final ThreadPool threadPool;
    private final Supplier<InternalAggregation.ReduceContext> aggReduceContextSupplier;
    private final Listener progressListener;

    private final Map<String, String> originHeaders;

    private boolean hasInitialized;
    private boolean hasCompleted;
    private long completionId;
    private final List<Runnable> initListeners = new ArrayList<>();
    private final Map<Long, Consumer<AsyncSearchResponse>> completionListeners = new HashMap<>();

    private volatile long expirationTimeMillis;
    private final AtomicBoolean isCancelling = new AtomicBoolean(false);

    private final AtomicReference<MutableSearchResponse> searchResponse = new AtomicReference<>();

    /**
     * Creates an instance of {@link AsyncSearchTask}.
     *
     * @param id The id of the task.
     * @param type The type of the task.
     * @param action The action name.
     * @param parentTaskId The parent task id.
     * @param checkSubmitCancellation A boolean supplier that checks if the submit task has been cancelled.
     * @param originHeaders All the request context headers.
     * @param taskHeaders The filtered request headers for the task.
     * @param searchId The {@link AsyncExecutionId} of the task.
     * @param threadPool The threadPool to schedule runnable.
     * @param aggReduceContextSupplier A supplier to create final reduce contexts.
     */
    AsyncSearchTask(long id,
                    String type,
                    String action,
                    TaskId parentTaskId,
                    BooleanSupplier checkSubmitCancellation,
                    TimeValue keepAlive,
                    Map<String, String> originHeaders,
                    Map<String, String> taskHeaders,
                    AsyncExecutionId searchId,
                    Client client,
                    ThreadPool threadPool,
                    Supplier<InternalAggregation.ReduceContext> aggReduceContextSupplier) {
        super(id, type, action, "async_search", parentTaskId, taskHeaders);
        this.checkSubmitCancellation = checkSubmitCancellation;
        this.expirationTimeMillis = getStartTime() + keepAlive.getMillis();
        this.originHeaders = originHeaders;
        this.searchId = searchId;
        this.client = client;
        this.threadPool = threadPool;
        this.aggReduceContextSupplier = aggReduceContextSupplier;
        this.progressListener = new Listener();
        setProgressListener(progressListener);
    }

    /**
     * Returns all of the request contexts headers
     */
    @Override
    public Map<String, String> getOriginHeaders() {
        return originHeaders;
    }

    /**
     * Returns the {@link AsyncExecutionId} of the task
     */
    @Override
    public AsyncExecutionId getExecutionId() {
        return searchId;
    }

    Listener getSearchProgressActionListener() {
        return progressListener;
    }

    /**
     * Update the expiration time of the (partial) response.
     */
    public void setExpirationTime(long expirationTimeMillis) {
        this.expirationTimeMillis = expirationTimeMillis;
    }

    /**
     * Cancels the running task and its children.
     */
    public void cancelTask(Runnable runnable) {
        if (isCancelled() == false && isCancelling.compareAndSet(false, true)) {
            CancelTasksRequest req = new CancelTasksRequest().setTaskId(searchId.getTaskId());
            client.admin().cluster().cancelTasks(req, new ActionListener<>() {
                @Override
                public void onResponse(CancelTasksResponse cancelTasksResponse) {
                    runnable.run();
                }

                @Override
                public void onFailure(Exception exc) {
                    // cancelling failed
                    isCancelling.compareAndSet(true, false);
                    runnable.run();
                }
            });
        } else {
            runnable.run();
       }
    }

    @Override
    protected void onCancelled() {
        super.onCancelled();
        isCancelling.compareAndSet(true, false);
    }

    /**
     * Creates a listener that listens for an {@link AsyncSearchResponse} and executes the
     * consumer when the task is finished or when the provided <code>waitForCompletion</code>
     * timeout occurs. In such case the consumed {@link AsyncSearchResponse} will contain partial results.
     */
    public void addCompletionListener(ActionListener<AsyncSearchResponse> listener, TimeValue waitForCompletion) {
        boolean executeImmediately = false;
        long startTime = threadPool.relativeTimeInMillis();
        synchronized (this) {
            if (hasCompleted) {
                executeImmediately = true;
            } else {
                addInitListener(() -> {
                    final TimeValue remainingWaitForCompletion;
                    if (waitForCompletion.getMillis() > 0) {
                        long elapsedTime = threadPool.relativeTimeInMillis() - startTime;
                        // subtract the initialization time from the provided waitForCompletion.
                        remainingWaitForCompletion = TimeValue.timeValueMillis(Math.max(0, waitForCompletion.getMillis() - elapsedTime));
                    } else {
                        remainingWaitForCompletion = TimeValue.ZERO;
                    }
                    internalAddCompletionListener(listener, remainingWaitForCompletion);
                });
            }
        }
        if (executeImmediately) {
            listener.onResponse(getResponseWithHeaders());
        }
    }

    /**
     * Creates a listener that listens for an {@link AsyncSearchResponse} and executes the
     * consumer when the task is finished.
     */
    public void addCompletionListener(Consumer<AsyncSearchResponse>  listener) {
        boolean executeImmediately = false;
        synchronized (this) {
            if (hasCompleted) {
                executeImmediately = true;
            } else {
                completionListeners.put(completionId++, listener);
            }
        }
        if (executeImmediately) {
            listener.accept(getResponseWithHeaders());
        }
    }

    private void internalAddCompletionListener(ActionListener<AsyncSearchResponse> listener, TimeValue waitForCompletion) {
        boolean executeImmediately = false;
        synchronized (this) {
            if (hasCompleted || waitForCompletion.getMillis() == 0) {
                executeImmediately = true;
            } else {
                // ensure that we consumes the listener only once
                AtomicBoolean hasRun = new AtomicBoolean(false);
                long id = completionId++;

                final Cancellable cancellable;
                try {
                    cancellable = threadPool.schedule(() -> {
                        if (hasRun.compareAndSet(false, true)) {
                            // timeout occurred before completion
                            removeCompletionListener(id);
                            listener.onResponse(getResponseWithHeaders());
                        }
                    }, waitForCompletion, "generic");
                } catch (EsRejectedExecutionException exc) {
                    listener.onFailure(exc);
                    return;
                }
                completionListeners.put(id, resp -> {
                    if (hasRun.compareAndSet(false, true)) {
                        // completion occurred before timeout
                        cancellable.cancel();
                        listener.onResponse(resp);
                    }
                });
            }
        }
        if (executeImmediately) {
            listener.onResponse(getResponseWithHeaders());
        }
    }

    private void removeCompletionListener(long id) {
        synchronized (this) {
            if (hasCompleted == false) {
                completionListeners.remove(id);
            }
        }
    }

    private void addInitListener(Runnable listener) {
        boolean executeImmediately = false;
        synchronized (this) {
            if (hasInitialized) {
                executeImmediately = true;
            } else {
                initListeners.add(listener);
            }
        }
        if (executeImmediately) {
            listener.run();
        }
    }

    private void executeInitListeners() {
        synchronized (this) {
            if (hasInitialized) {
                return;
            }
            hasInitialized = true;
        }
        for (Runnable listener : initListeners) {
            listener.run();
        }
        initListeners.clear();
    }

    private void executeCompletionListeners() {
        synchronized (this) {
            if (hasCompleted) {
                return;
            }
            hasCompleted = true;
        }
        // we don't need to restore the response headers, they should be included in the current
        // context since we are called by the search action listener.
        AsyncSearchResponse finalResponse = getResponse();
        for (Consumer<AsyncSearchResponse> listener : completionListeners.values()) {
            listener.accept(finalResponse);
        }
        completionListeners.clear();
    }

    /**
     * Returns the current {@link AsyncSearchResponse}.
     */
    private AsyncSearchResponse getResponse() {
        assert searchResponse.get() != null;
        checkCancellation();
        return searchResponse.get().toAsyncSearchResponse(this, expirationTimeMillis);
    }

    /**
     * Returns the current {@link AsyncSearchResponse} and restores the response headers
     * in the local thread context.
     */
    private AsyncSearchResponse getResponseWithHeaders() {
        assert searchResponse.get() != null;
        checkCancellation();
        return searchResponse.get().toAsyncSearchResponseWithHeaders(this, expirationTimeMillis);
    }



    // checks if the search task should be cancelled
    private synchronized void checkCancellation() {
        long now = System.currentTimeMillis();
        if (hasCompleted == false &&
                expirationTimeMillis < now || checkSubmitCancellation.getAsBoolean()) {
            // we cancel the search task if the initial submit task was cancelled,
            // this is needed because the task cancellation mechanism doesn't
            // handle the cancellation of grand-children.
            cancelTask(() -> {});
        }
    }

    class Listener extends SearchProgressActionListener {
        @Override
        protected void onQueryResult(int shardIndex) {
            checkCancellation();
        }

        @Override
        protected void onFetchResult(int shardIndex) {
            checkCancellation();
        }

        @Override
        protected void onQueryFailure(int shardIndex, SearchShardTarget shardTarget, Exception exc) {
            // best effort to cancel expired tasks
            checkCancellation();
            searchResponse.get().addShardFailure(shardIndex,
                // the nodeId is null if all replicas of this shard failed
                new ShardSearchFailure(exc, shardTarget.getNodeId() != null ? shardTarget : null));
        }

        @Override
        protected void onFetchFailure(int shardIndex, SearchShardTarget shardTarget, Exception exc) {
            checkCancellation();
            searchResponse.get().addShardFailure(shardIndex,
                // the nodeId is null if all replicas of this shard failed
                new ShardSearchFailure(exc, shardTarget.getNodeId() != null ? shardTarget : null));
        }

        @Override
        protected void onListShards(List<SearchShard> shards, List<SearchShard> skipped, Clusters clusters, boolean fetchPhase) {
            // best effort to cancel expired tasks
            checkCancellation();
            searchResponse.compareAndSet(null,
                new MutableSearchResponse(shards.size() + skipped.size(), skipped.size(), clusters, threadPool.getThreadContext()));
            executeInitListeners();
        }

        @Override
        public void onPartialReduce(List<SearchShard> shards, TotalHits totalHits,
                DelayableWriteable.Serialized<InternalAggregations> aggregations, int reducePhase) {
            // best effort to cancel expired tasks
            checkCancellation();
            // The way that the MutableSearchResponse will build the aggs.
            Supplier<InternalAggregations> reducedAggs;
            if (aggregations == null) {
                // There aren't any aggs to reduce.
                reducedAggs = () -> null;
            } else {
                /*
                 * Keep a reference to the serialized form of the partially
                 * reduced aggs and reduce it on the fly when someone asks
                 * for it. It's important that we wait until someone needs
                 * the result so we don't perform the final reduce only to
                 * throw it away. And it is important that we keep the reference
                 * to the serialized aggregations because SearchPhaseController
                 * *already* has that reference so we're not creating more garbage.
                 */
                reducedAggs = () ->
                    InternalAggregations.topLevelReduce(singletonList(aggregations.expand()), aggReduceContextSupplier.get());
            }
            searchResponse.get().updatePartialResponse(shards.size(), totalHits, reducedAggs, reducePhase);
        }

        @Override
        public void onFinalReduce(List<SearchShard> shards, TotalHits totalHits, InternalAggregations aggregations, int reducePhase) {
            // best effort to cancel expired tasks
            checkCancellation();
            searchResponse.get().updatePartialResponse(shards.size(), totalHits, () -> aggregations, reducePhase);
        }

        @Override
        public void onResponse(SearchResponse response) {
            searchResponse.get().updateFinalResponse(response);
            executeCompletionListeners();
        }

        @Override
        public void onFailure(Exception exc) {
            if (searchResponse.get() == null) {
                // if the failure occurred before calling onListShards
                searchResponse.compareAndSet(null,
                    new MutableSearchResponse(-1, -1, null, threadPool.getThreadContext()));
            }
            searchResponse.get().updateWithFailure(exc);
            executeInitListeners();
            executeCompletionListeners();
        }
    }
}
