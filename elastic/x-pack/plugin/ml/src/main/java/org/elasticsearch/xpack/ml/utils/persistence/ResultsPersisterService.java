/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.utils.persistence;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.bulk.BulkAction;
import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.support.RetryableAction;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.component.LifecycleListener;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.CancellableThreads;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ClientHelper;

import java.io.IOException;
import java.time.Duration;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import java.util.function.Supplier;
import java.util.stream.Collectors;

import static org.elasticsearch.ExceptionsHelper.status;
import static org.elasticsearch.xpack.ml.MachineLearning.UTILITY_THREAD_POOL_NAME;

public class ResultsPersisterService {
    /**
     * List of rest statuses that we consider irrecoverable
     */
    public static final Set<RestStatus> IRRECOVERABLE_REST_STATUSES = Collections.unmodifiableSet(new HashSet<>(
        Arrays.asList(
            RestStatus.GONE,
            RestStatus.NOT_IMPLEMENTED,
            // Not found is returned when we require an alias but the index is NOT an alias.
            RestStatus.NOT_FOUND,
            RestStatus.BAD_REQUEST,
            RestStatus.UNAUTHORIZED,
            RestStatus.FORBIDDEN,
            RestStatus.METHOD_NOT_ALLOWED,
            RestStatus.NOT_ACCEPTABLE
        )
    ));

    private static final Logger LOGGER = LogManager.getLogger(ResultsPersisterService.class);

    public static final Setting<Integer> PERSIST_RESULTS_MAX_RETRIES = Setting.intSetting(
        "xpack.ml.persist_results_max_retries",
        20,
        0,
        50,
        Setting.Property.OperatorDynamic,
        Setting.Property.NodeScope);
    private static final int MAX_RETRY_SLEEP_MILLIS = (int)Duration.ofMinutes(15).toMillis();
    private static final int MIN_RETRY_SLEEP_MILLIS = 50;
    // Having an exponent higher than this causes integer overflow
    private static final int MAX_RETRY_EXPONENT = 24;

    private final ThreadPool threadPool;
    private final OriginSettingClient client;
    private final Map<Object, RetryableAction<?>> onGoingRetryableActions = ConcurrentCollections.newConcurrentMap();
    private volatile int maxFailureRetries;
    private volatile boolean isShutdown = false;

    // Visible for testing
    public ResultsPersisterService(ThreadPool threadPool,
                                   OriginSettingClient client,
                                   ClusterService clusterService,
                                   Settings settings) {
        this.threadPool = threadPool;
        this.client = client;
        this.maxFailureRetries = PERSIST_RESULTS_MAX_RETRIES.get(settings);
        clusterService.getClusterSettings()
            .addSettingsUpdateConsumer(PERSIST_RESULTS_MAX_RETRIES, this::setMaxFailureRetries);
        clusterService.addLifecycleListener(new LifecycleListener() {
            @Override
            public void beforeStop() {
                shutdown();
            }
        });
    }

    void shutdown() {
        isShutdown = true;
        if (onGoingRetryableActions.isEmpty()) {
            return;
        }
        final RuntimeException exception = new CancellableThreads.ExecutionCancelledException("Node is shutting down");
        for (RetryableAction<?> action : onGoingRetryableActions.values()) {
            action.cancel(exception);
        }
        onGoingRetryableActions.clear();
    }

    void setMaxFailureRetries(int value) {
        this.maxFailureRetries = value;
    }

    public BulkResponse indexWithRetry(String jobId,
                                       String indexName,
                                       ToXContent object,
                                       ToXContent.Params params,
                                       WriteRequest.RefreshPolicy refreshPolicy,
                                       String id,
                                       boolean requireAlias,
                                       Supplier<Boolean> shouldRetry,
                                       Consumer<String> retryMsgHandler) throws IOException {
        BulkRequest bulkRequest = new BulkRequest().setRefreshPolicy(refreshPolicy);
        try (XContentBuilder content = object.toXContent(XContentFactory.jsonBuilder(), params)) {
            bulkRequest.add(new IndexRequest(indexName).id(id).source(content).setRequireAlias(requireAlias));
        }
        return bulkIndexWithRetry(bulkRequest, jobId, shouldRetry, retryMsgHandler);
    }

    public BulkResponse bulkIndexWithRetry(BulkRequest bulkRequest,
                                           String jobId,
                                           Supplier<Boolean> shouldRetry,
                                           Consumer<String> retryMsgHandler) {
        return bulkIndexWithRetry(bulkRequest,
            jobId,
            shouldRetry,
            retryMsgHandler,
            client::bulk);
    }

    public BulkResponse bulkIndexWithHeadersWithRetry(Map<String, String> headers,
                                                      BulkRequest bulkRequest,
                                                      String jobId,
                                                      Supplier<Boolean> shouldRetry,
                                                      Consumer<String> retryMsgHandler) {
        return bulkIndexWithRetry(bulkRequest,
            jobId,
            shouldRetry,
            retryMsgHandler,
            (providedBulkRequest, listener) -> ClientHelper.executeWithHeadersAsync(
                headers,
                ClientHelper.ML_ORIGIN,
                client,
                BulkAction.INSTANCE,
                providedBulkRequest,
                listener));
    }

    private BulkResponse bulkIndexWithRetry(BulkRequest bulkRequest,
                                            String jobId,
                                            Supplier<Boolean> shouldRetry,
                                            Consumer<String> retryMsgHandler,
                                            BiConsumer<BulkRequest, ActionListener<BulkResponse>> actionExecutor) {
        final PlainActionFuture<BulkResponse> getResponse = PlainActionFuture.newFuture();
        final Object key = new Object();
        final ActionListener<BulkResponse> removeListener = ActionListener.runBefore(
            getResponse,
            () -> onGoingRetryableActions.remove(key)
        );
        BulkRetryableAction bulkRetryableAction = new BulkRetryableAction(
            jobId,
            new BulkRequestRewriter(bulkRequest),
            shouldRetryWrapper(shouldRetry),
            retryMsgHandler,
            actionExecutor,
            removeListener
        );
        onGoingRetryableActions.put(key, bulkRetryableAction);
        bulkRetryableAction.run();
        if (isShutdown) {
            bulkRetryableAction.cancel(new CancellableThreads.ExecutionCancelledException("Node is shutting down"));
        }
        return getResponse.actionGet();
    }

    public SearchResponse searchWithRetry(SearchRequest searchRequest,
                                          String jobId,
                                          Supplier<Boolean> shouldRetry,
                                          Consumer<String> retryMsgHandler) {
        final PlainActionFuture<SearchResponse> getResponse = PlainActionFuture.newFuture();
        final Object key = new Object();
        final ActionListener<SearchResponse> removeListener = ActionListener.runBefore(
            getResponse,
            () -> onGoingRetryableActions.remove(key)
        );
        SearchRetryableAction mlRetryableAction = new SearchRetryableAction(
            jobId,
            searchRequest,
            client,
            shouldRetryWrapper(shouldRetry),
            retryMsgHandler,
            removeListener);
        onGoingRetryableActions.put(key, mlRetryableAction);
        mlRetryableAction.run();
        if (isShutdown) {
            mlRetryableAction.cancel(new CancellableThreads.ExecutionCancelledException("Node is shutting down"));
        }
        return getResponse.actionGet();
    }

    private Supplier<Boolean> shouldRetryWrapper(Supplier<Boolean> shouldRetry) {
        return () -> (isShutdown == false) && shouldRetry.get();
    }

    static class RecoverableException extends Exception { }
    static class IrrecoverableException extends ElasticsearchStatusException {
        IrrecoverableException(String msg, RestStatus status, Throwable cause, Object... args) {
            super(msg, status, cause, args);
        }
    }
    /**
     * @param ex The exception to check
     * @return true when the failure will persist no matter how many times we retry.
     */
    private static boolean isIrrecoverable(Exception ex) {
        Throwable t = ExceptionsHelper.unwrapCause(ex);
        return IRRECOVERABLE_REST_STATUSES.contains(status(t));
    }

    @SuppressWarnings("NonAtomicOperationOnVolatileField")
    private static class BulkRequestRewriter {
        private volatile BulkRequest bulkRequest;
        BulkRequestRewriter(BulkRequest initialRequest) {
            this.bulkRequest = initialRequest;
        }

        void rewriteRequest(BulkResponse bulkResponse) {
            if (bulkResponse.hasFailures() == false) {
                return;
            }
            bulkRequest = buildNewRequestFromFailures(bulkRequest, bulkResponse);
        }

        BulkRequest getBulkRequest() {
            return bulkRequest;
        }

    }

    private class BulkRetryableAction extends MlRetryableAction<BulkRequest, BulkResponse> {
        private final BulkRequestRewriter bulkRequestRewriter;
        BulkRetryableAction(String jobId,
                            BulkRequestRewriter bulkRequestRewriter,
                            Supplier<Boolean> shouldRetry,
                            Consumer<String> msgHandler,
                            BiConsumer<BulkRequest, ActionListener<BulkResponse>> actionExecutor,
                            ActionListener<BulkResponse> listener) {
            super(jobId,
                shouldRetry,
                msgHandler,
                (request, retryableListener) -> actionExecutor.accept(request, ActionListener.wrap(
                    bulkResponse -> {
                        if (bulkResponse.hasFailures() == false) {
                            retryableListener.onResponse(bulkResponse);
                            return;
                        }
                        for (BulkItemResponse itemResponse : bulkResponse.getItems()) {
                            if (itemResponse.isFailed() && isIrrecoverable(itemResponse.getFailure().getCause())) {
                                Throwable unwrappedParticular = ExceptionsHelper.unwrapCause(itemResponse.getFailure().getCause());
                                LOGGER.warn(new ParameterizedMessage(
                                        "[{}] experienced failure that cannot be automatically retried. Bulk failure message [{}]",
                                        jobId,
                                        bulkResponse.buildFailureMessage()),
                                    unwrappedParticular);
                                retryableListener.onFailure(new IrrecoverableException(
                                    "{} experienced failure that cannot be automatically retried. See logs for bulk failures",
                                    status(unwrappedParticular),
                                    unwrappedParticular,
                                    jobId));
                                return;
                            }
                        }
                        bulkRequestRewriter.rewriteRequest(bulkResponse);
                        // Let the listener attempt again with the new bulk request
                        retryableListener.onFailure(new RecoverableException());
                    },
                    retryableListener::onFailure
                )),
                listener);
            this.bulkRequestRewriter = bulkRequestRewriter;
        }

        @Override
        public BulkRequest buildRequest() {
            return bulkRequestRewriter.getBulkRequest();
        }

        @Override
        public String getName() {
            return "index";
        }

    }

    private class SearchRetryableAction extends MlRetryableAction<SearchRequest, SearchResponse> {

        private final SearchRequest searchRequest;
        SearchRetryableAction(String jobId,
                              SearchRequest searchRequest,
                              // Pass the client to work around https://bugs.eclipse.org/bugs/show_bug.cgi?id=569557
                              OriginSettingClient client,
                              Supplier<Boolean> shouldRetry,
                              Consumer<String> msgHandler,
                              ActionListener<SearchResponse> listener) {
            super(jobId,
                shouldRetry,
                msgHandler,
                (request, retryableListener) -> client.search(request, ActionListener.wrap(
                    searchResponse -> {
                        if (RestStatus.OK.equals(searchResponse.status())) {
                            retryableListener.onResponse(searchResponse);
                            return;
                        }
                        retryableListener.onFailure(
                            new ElasticsearchStatusException(
                                "search failed with status {}",
                                searchResponse.status(),
                                searchResponse.status())
                        );
                    },
                    retryableListener::onFailure
                )),
                listener);
            this.searchRequest = searchRequest;
        }

        @Override
        public SearchRequest buildRequest() {
            return searchRequest;
        }

        @Override
        public String getName() {
            return "search";
        }
    }

    // This encapsulates a retryable action that implements our custom backoff retry logic
    private abstract class MlRetryableAction<Request, Response> extends RetryableAction<Response> {

        final String jobId;
        final Supplier<Boolean> shouldRetry;
        final Consumer<String> msgHandler;
        final BiConsumer<Request, ActionListener<Response>> action;
        volatile int currentAttempt = 0;
        volatile long currentMin = MIN_RETRY_SLEEP_MILLIS;
        volatile long currentMax = MIN_RETRY_SLEEP_MILLIS;

        MlRetryableAction(String jobId,
                          Supplier<Boolean> shouldRetry,
                          Consumer<String> msgHandler,
                          BiConsumer<Request, ActionListener<Response>> action,
                          ActionListener<Response> listener) {
            super(
                LOGGER,
                threadPool,
                TimeValue.timeValueMillis(MIN_RETRY_SLEEP_MILLIS),
                TimeValue.MAX_VALUE,
                listener,
                UTILITY_THREAD_POOL_NAME);
            this.jobId = jobId;
            this.shouldRetry = shouldRetry;
            this.msgHandler = msgHandler;
            this.action = action;
        }

        public abstract Request buildRequest();

        public abstract String getName();

        @Override
        public void tryAction(ActionListener<Response> listener) {
            currentAttempt++;
            action.accept(buildRequest(), listener);
        }

        @Override
        public boolean shouldRetry(Exception e) {
            if (isIrrecoverable(e)) {
                LOGGER.warn(new ParameterizedMessage("[{}] experienced failure that cannot be automatically retried", jobId), e);
                return false;
            }

            // If the outside conditions have changed and retries are no longer needed, do not retry.
            if (shouldRetry.get() == false) {
                LOGGER.info(() -> new ParameterizedMessage(
                    "[{}] should not retry {} after [{}] attempts",
                    jobId,
                    getName(),
                    currentAttempt
                ), e);
                return false;
            }

            // If the configured maximum number of retries has been reached, do not retry.
            if (currentAttempt > maxFailureRetries) {
                LOGGER.warn(() -> new ParameterizedMessage(
                    "[{}] failed to {} after [{}] attempts.",
                    jobId,
                    getName(),
                    currentAttempt
                ), e);
                return false;
            }
            return true;
        }

        @Override
        protected long calculateDelay(long previousDelay) {
            // Since we exponentially increase, we don't want force randomness to have an excessively long sleep
            if (currentMax < MAX_RETRY_SLEEP_MILLIS) {
                currentMin = currentMax;
            }
            // Exponential backoff calculation taken from: https://en.wikipedia.org/wiki/Exponential_backoff
            int uncappedBackoff = ((1 << Math.min(currentAttempt, MAX_RETRY_EXPONENT)) - 1) * (50);
            currentMax = Math.min(uncappedBackoff, MAX_RETRY_SLEEP_MILLIS);
            // Its good to have a random window along the exponentially increasing curve
            // so that not all bulk requests rest for the same amount of time
            int randBound = (int)(1 + (currentMax - currentMin));
            String msg = new ParameterizedMessage(
                "failed to {} after [{}] attempts. Will attempt again.",
                getName(),
                currentAttempt)
                .getFormattedMessage();
            LOGGER.warn(() -> new ParameterizedMessage("[{}] {}", jobId, msg));
            msgHandler.accept(msg);
            return randBound;
        }

        @Override
        protected long minimumDelayMillis() {
            return currentMin;
        }

        @Override
        public void cancel(Exception e) {
            super.cancel(e);
            LOGGER.debug(() -> new ParameterizedMessage("[{}] retrying cancelled for action [{}]", jobId, getName()), e);
        }
    }

    private static BulkRequest buildNewRequestFromFailures(BulkRequest bulkRequest, BulkResponse bulkResponse) {
        // If we failed, lets set the bulkRequest to be a collection of the failed requests
        BulkRequest bulkRequestOfFailures = new BulkRequest();
        Set<String> failedDocIds = Arrays.stream(bulkResponse.getItems())
            .filter(BulkItemResponse::isFailed)
            .map(BulkItemResponse::getId)
            .collect(Collectors.toSet());
        bulkRequest.requests().forEach(docWriteRequest -> {
            if (failedDocIds.contains(docWriteRequest.id())) {
                bulkRequestOfFailures.add(docWriteRequest);
            }
        });
        return bulkRequestOfFailures;
    }

}
