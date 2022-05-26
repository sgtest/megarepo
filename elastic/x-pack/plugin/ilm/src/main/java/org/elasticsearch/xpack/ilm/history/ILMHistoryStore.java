/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ilm.history;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.DocWriteRequest;
import org.elasticsearch.action.bulk.BackoffPolicy;
import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkProcessor;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.client.internal.OriginSettingClient;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xcontent.ToXContent;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentFactory;

import java.io.Closeable;
import java.io.IOException;
import java.util.Arrays;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;

import static java.util.stream.Collectors.joining;
import static org.elasticsearch.core.Strings.format;
import static org.elasticsearch.xpack.core.ClientHelper.INDEX_LIFECYCLE_ORIGIN;
import static org.elasticsearch.xpack.core.ilm.LifecycleSettings.LIFECYCLE_HISTORY_INDEX_ENABLED_SETTING;
import static org.elasticsearch.xpack.ilm.history.ILMHistoryTemplateRegistry.ILM_TEMPLATE_NAME;
import static org.elasticsearch.xpack.ilm.history.ILMHistoryTemplateRegistry.INDEX_TEMPLATE_VERSION;

/**
 * The {@link ILMHistoryStore} handles indexing {@link ILMHistoryItem} documents into the
 * appropriate index. It sets up a {@link BulkProcessor} for indexing in bulk, and handles creation
 * of the index/alias as needed for ILM policies.
 */
public class ILMHistoryStore implements Closeable {
    private static final Logger logger = LogManager.getLogger(ILMHistoryStore.class);

    public static final String ILM_HISTORY_DATA_STREAM = "ilm-history-" + INDEX_TEMPLATE_VERSION;

    private static int ILM_HISTORY_BULK_SIZE = StrictMath.toIntExact(
        ByteSizeValue.parseBytesSizeValue(
            System.getProperty("es.indices.lifecycle.history.bulk.size", "50MB"),
            "es.indices.lifecycle.history.bulk.size"
        ).getBytes()
    );

    private volatile boolean ilmHistoryEnabled = true;
    private final BulkProcessor processor;
    private final ThreadPool threadPool;

    public ILMHistoryStore(Client client, ClusterService clusterService, ThreadPool threadPool) {
        this.setIlmHistoryEnabled(LIFECYCLE_HISTORY_INDEX_ENABLED_SETTING.get(clusterService.getSettings()));
        clusterService.getClusterSettings().addSettingsUpdateConsumer(LIFECYCLE_HISTORY_INDEX_ENABLED_SETTING, this::setIlmHistoryEnabled);

        this.threadPool = threadPool;

        this.processor = BulkProcessor.builder(new OriginSettingClient(client, INDEX_LIFECYCLE_ORIGIN)::bulk, new BulkProcessor.Listener() {
            @Override
            public void beforeBulk(long executionId, BulkRequest request) {
                if (clusterService.state().getMetadata().templatesV2().containsKey(ILM_TEMPLATE_NAME) == false) {
                    ElasticsearchException e = new ElasticsearchException("no ILM history template");
                    logger.warn(
                        () -> format(
                            "unable to index the following ILM history items:\n%s",
                            request.requests()
                                .stream()
                                .filter(dwr -> (dwr instanceof IndexRequest))
                                .map(dwr -> ((IndexRequest) dwr))
                                .map(IndexRequest::sourceAsMap)
                                .map(Object::toString)
                                .collect(joining("\n"))
                        ),
                        e
                    );
                    throw new ElasticsearchException(e);
                }
                if (logger.isTraceEnabled()) {
                    logger.info(
                        "about to index: {}",
                        request.requests()
                            .stream()
                            .map(dwr -> ((IndexRequest) dwr).sourceAsMap())
                            .map(Objects::toString)
                            .collect(Collectors.joining(","))
                    );
                }
            }

            @Override
            public void afterBulk(long executionId, BulkRequest request, BulkResponse response) {
                long items = request.numberOfActions();
                if (logger.isTraceEnabled()) {
                    logger.trace(
                        "indexed [{}] items into ILM history index [{}]",
                        items,
                        Arrays.stream(response.getItems()).map(BulkItemResponse::getIndex).distinct().collect(Collectors.joining(","))
                    );
                }
                if (response.hasFailures()) {
                    Map<String, String> failures = Arrays.stream(response.getItems())
                        .filter(BulkItemResponse::isFailed)
                        .collect(
                            Collectors.toMap(
                                BulkItemResponse::getId,
                                BulkItemResponse::getFailureMessage,
                                (msg1, msg2) -> Objects.equals(msg1, msg2) ? msg1 : msg1 + "," + msg2
                            )
                        );
                    logger.error("failures: [{}]", failures);
                }
            }

            @Override
            public void afterBulk(long executionId, BulkRequest request, Throwable failure) {
                long items = request.numberOfActions();
                logger.error(() -> "failed to index " + items + " items into ILM history index", failure);
            }
        }, "ilm-history-store")
            .setBulkActions(-1)
            .setBulkSize(ByteSizeValue.ofBytes(ILM_HISTORY_BULK_SIZE))
            .setFlushInterval(TimeValue.timeValueSeconds(5))
            .setConcurrentRequests(1)
            .setBackoffPolicy(BackoffPolicy.exponentialBackoff(TimeValue.timeValueMillis(1000), 3))
            .build();
    }

    /**
     * Attempts to asynchronously index an ILM history entry
     */
    public void putAsync(ILMHistoryItem item) {
        if (ilmHistoryEnabled == false) {
            logger.trace(
                "not recording ILM history item because [{}] is [false]: [{}]",
                LIFECYCLE_HISTORY_INDEX_ENABLED_SETTING.getKey(),
                item
            );
            return;
        }
        logger.trace("queueing ILM history item for indexing [{}]: [{}]", ILM_HISTORY_DATA_STREAM, item);
        try (XContentBuilder builder = XContentFactory.jsonBuilder()) {
            item.toXContent(builder, ToXContent.EMPTY_PARAMS);
            IndexRequest request = new IndexRequest(ILM_HISTORY_DATA_STREAM).source(builder).opType(DocWriteRequest.OpType.CREATE);
            // TODO: remove the threadpool wrapping when the .add call is non-blocking
            // (it can currently execute the bulk request occasionally)
            // see: https://github.com/elastic/elasticsearch/issues/50440
            threadPool.executor(ThreadPool.Names.GENERIC).execute(() -> {
                try {
                    processor.add(request);
                } catch (Exception e) {
                    logger.error(
                        () -> format("failed add ILM history item to queue for index [%s]: [%s]", ILM_HISTORY_DATA_STREAM, item),
                        e
                    );
                }
            });
        } catch (IOException exception) {
            logger.error(() -> format("failed to queue ILM history item in index [%s]: [%s]", ILM_HISTORY_DATA_STREAM, item), exception);
        }
    }

    @Override
    public void close() {
        try {
            processor.awaitClose(10, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            logger.warn("failed to shut down ILM history bulk processor after 10 seconds", e);
        }
    }

    public void setIlmHistoryEnabled(boolean ilmHistoryEnabled) {
        this.ilmHistoryEnabled = ilmHistoryEnabled;
    }
}
