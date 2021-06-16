/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.pytorch.process;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.xpack.ml.inference.deployment.PyTorchResult;

import java.util.Iterator;
import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

public class PyTorchResultProcessor {

    private static final Logger logger = LogManager.getLogger(PyTorchResultProcessor.class);

    private final ConcurrentMap<String, PendingResult> pendingResults = new ConcurrentHashMap<>();

    private final String deploymentId;
    private volatile boolean isStopping;

    public PyTorchResultProcessor(String deploymentId) {
        this.deploymentId = Objects.requireNonNull(deploymentId);
    }

    public void process(NativePyTorchProcess process) {
        try {
            Iterator<PyTorchResult> iterator = process.readResults();
            while (iterator.hasNext()) {
                PyTorchResult result = iterator.next();
                logger.trace(() -> new ParameterizedMessage("[{}] Parsed result with id [{}]", deploymentId, result.getRequestId()));
                PendingResult pendingResult = pendingResults.get(result.getRequestId());
                if (pendingResult == null) {
                    logger.warn(() -> new ParameterizedMessage("[{}] no pending result for [{}]", deploymentId, result.getRequestId()));
                } else {
                    pendingResult.result = result;
                    pendingResult.latch.countDown();
                }
            }
        } catch (Exception e) {
            // No need to report error as we're stopping
            if (isStopping == false) {
                logger.error(new ParameterizedMessage("[{}] Error processing results", deploymentId), e);
            }
        }
        logger.debug(() -> new ParameterizedMessage("[{}] Results processing finished", deploymentId));
    }

    public PyTorchResult waitForResult(String requestId, TimeValue timeout) throws InterruptedException {
        PendingResult pendingResult = pendingResults.computeIfAbsent(requestId, k -> new PendingResult());
        try {
            if (pendingResult.latch.await(timeout.millis(), TimeUnit.MILLISECONDS)) {
                return pendingResult.result;
            }
        } finally {
            pendingResults.remove(requestId);
        }
        return null;
    }

    public void stop() {
        isStopping = true;
    }

    private static class PendingResult {
        private volatile PyTorchResult result;
        private final CountDownLatch latch = new CountDownLatch(1);
    }
}
