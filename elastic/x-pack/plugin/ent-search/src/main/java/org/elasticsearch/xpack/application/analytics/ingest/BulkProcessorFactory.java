/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.analytics.ingest;

import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkProcessor2;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.logging.LogManager;
import org.elasticsearch.logging.Logger;

import java.util.Arrays;
import java.util.List;
import java.util.stream.Collectors;

/**
 * Event ingest is done through a {@link BulkProcessor2}. This class is responsible for instantiating the bulk processor.
 */
public class BulkProcessorFactory {
    private static final Logger logger = LogManager.getLogger(AnalyticsEventEmitter.class);

    private final BulkProcessorConfig config;

    @Inject
    public BulkProcessorFactory(BulkProcessorConfig config) {
        this.config = config;
    }

    public BulkProcessor2 create(Client client) {
        return BulkProcessor2.builder(client::bulk, new BulkProcessorListener(), client.threadPool())
            .setMaxNumberOfRetries(config.maxNumberOfRetries())
            .setBulkActions(config.maxNumberOfEventsPerBulk())
            .setBulkSize(new ByteSizeValue(-1, ByteSizeUnit.BYTES))
            .setFlushInterval(config.flushDelay())
            .build();
    }

    static class BulkProcessorListener implements BulkProcessor2.Listener {
        @Override
        public void beforeBulk(long executionId, BulkRequest request) {}

        @Override
        public void afterBulk(long executionId, BulkRequest request, BulkResponse response) {
            if (response.hasFailures()) {
                List<String> failures = Arrays.stream(response.getItems())
                    .filter(BulkItemResponse::isFailed)
                    .map(r -> r.getId() + " " + r.getFailureMessage())
                    .collect(Collectors.toList());
                logger.error("Bulk write of behavioral analytics events encountered some failures: [{}]", failures);
            }
        }

        @Override
        public void afterBulk(long executionId, BulkRequest request, Exception failure) {
            logger.error(
                "Bulk write of " + request.numberOfActions() + " behavioral analytics events logs failed: " + failure.getMessage(),
                failure
            );
        }
    }
}
