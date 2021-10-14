/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.deprecation.logging;

import co.elastic.logging.log4j2.EcsLayout;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.core.LoggerContext;
import org.apache.logging.log4j.core.config.Configuration;
import org.elasticsearch.action.bulk.BackoffPolicy;
import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkProcessor;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.common.component.AbstractLifecycleComponent;
import org.elasticsearch.common.logging.ECSJsonLayout;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.logging.RateLimitingFilter;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.concurrent.EsExecutors;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.xpack.core.ClientHelper;

import java.util.Arrays;
import java.util.List;
import java.util.function.Consumer;
import java.util.stream.Collectors;

/**
 * This component manages the construction and lifecycle of the {@link DeprecationIndexingAppender}.
 * It also starts and stops the appender
 */
public class DeprecationIndexingComponent extends AbstractLifecycleComponent {
    private static final Logger logger = LogManager.getLogger(DeprecationIndexingComponent.class);

    private final DeprecationIndexingAppender appender;
    private final BulkProcessor processor;
    private final RateLimitingFilter rateLimitingFilterForIndexing;

    public DeprecationIndexingComponent(Client client, Settings settings, RateLimitingFilter rateLimitingFilterForIndexing,
                                        boolean enableDeprecationLogIndexingDefault) {
        this.rateLimitingFilterForIndexing = rateLimitingFilterForIndexing;

        this.processor = getBulkProcessor(new OriginSettingClient(client, ClientHelper.DEPRECATION_ORIGIN), settings);
        final Consumer<IndexRequest> consumer = this.processor::add;

        final LoggerContext context = (LoggerContext) LogManager.getContext(false);
        final Configuration configuration = context.getConfiguration();

        final EcsLayout ecsLayout = ECSJsonLayout.newBuilder()
            .setDataset("deprecation.elasticsearch")
            .setConfiguration(configuration)
            .build();

        this.appender = new DeprecationIndexingAppender("deprecation_indexing_appender",
            rateLimitingFilterForIndexing, ecsLayout, consumer);
        enableDeprecationLogIndexing(enableDeprecationLogIndexingDefault);
    }

    @Override
    protected void doStart() {
        this.appender.start();
        Loggers.addAppender(LogManager.getLogger("org.elasticsearch.deprecation"), this.appender);
    }

    @Override
    protected void doStop() {
        Loggers.removeAppender(LogManager.getLogger("org.elasticsearch.deprecation"), this.appender);
        this.appender.stop();
    }

    @Override
    protected void doClose() {
        this.processor.close();
    }


    public void enableDeprecationLogIndexing(boolean newEnabled) {
        if (appender.isEnabled() != newEnabled) {
            appender.setEnabled(newEnabled);

            // We've flipped from disabled to enabled. Make sure we start with a clean cache of
            // previously-seen keys, otherwise we won't index anything.
            if (newEnabled) {
                this.rateLimitingFilterForIndexing.reset();
            } else {
                // we have flipped from enabled to disabled. A processor could have accumulated some requests, so we have to flush it
                this.processor.flush();
            }
        }
    }

    /**
     * Constructs a bulk processor for writing documents
     *
     * @param client   the client to use
     * @param settings the settings to use
     * @return an initialised bulk processor
     */
    private BulkProcessor getBulkProcessor(Client client, Settings settings) {
        final BulkProcessor.Listener listener = new DeprecationBulkListener();

        // This configuration disables the size count and size thresholds,
        // and instead uses a scheduled flush only. This means that calling
        // processor.add() will not block the calling thread.
        return BulkProcessor.builder(client::bulk, listener, "deprecation-indexing")
            .setBackoffPolicy(BackoffPolicy.exponentialBackoff(TimeValue.timeValueMillis(1000), 3))
            .setConcurrentRequests(Math.max(2, EsExecutors.allocatedProcessors(settings)))
            .setBulkActions(-1)
            .setBulkSize(new ByteSizeValue(-1, ByteSizeUnit.BYTES))
            .setFlushInterval(TimeValue.timeValueSeconds(5))
            .build();
    }

    private static class DeprecationBulkListener implements BulkProcessor.Listener {
        @Override
        public void beforeBulk(long executionId, BulkRequest request) {}

        @Override
        public void afterBulk(long executionId, BulkRequest request, BulkResponse response) {
            long numberOfActions = request.numberOfActions();
            if (logger.isTraceEnabled()) {
                logger.trace(
                    "indexed [{}] deprecation documents into [{}]",
                    numberOfActions,
                    Arrays.stream(response.getItems()).map(BulkItemResponse::getIndex).distinct().collect(Collectors.joining(","))
                );
            }

            if (response.hasFailures()) {
                List<String> failures = Arrays.stream(response.getItems())
                    .filter(BulkItemResponse::isFailed)
                    .map(r -> r.getId() + " " + r.getFailureMessage() )
                    .collect(Collectors.toList());
                logger.error("Bulk write of deprecation logs encountered some failures: [{}]", failures);
            }
        }

        @Override
        public void afterBulk(long executionId, BulkRequest request, Throwable failure) {
            logger.error("Bulk write of " + request.numberOfActions() + " deprecation logs failed: " + failure.getMessage(), failure);
        }
    }
}
