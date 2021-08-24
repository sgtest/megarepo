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

import java.time.Instant;
import java.util.Iterator;
import java.util.LongSummaryStatistics;
import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

public class PyTorchResultProcessor {

    private static final Logger logger = LogManager.getLogger(PyTorchResultProcessor.class);

    private final ConcurrentMap<String, PendingResult> pendingResults = new ConcurrentHashMap<>();

    private final String deploymentId;
    private volatile boolean isStopping;
    private volatile boolean stoppedProcessing;
    private final LongSummaryStatistics timingStats;
    private Instant lastUsed;

    public PyTorchResultProcessor(String deploymentId) {
        this.deploymentId = Objects.requireNonNull(deploymentId);
        this.timingStats = new LongSummaryStatistics();
    }

    public PendingResult registerRequest(String requestId) {
        return pendingResults.computeIfAbsent(requestId, k -> new PendingResult());
    }

    public void requestAccepted(String requestId) {
        pendingResults.remove(requestId);
    }

    public void process(NativePyTorchProcess process) {
        try {
            Iterator<PyTorchResult> iterator = process.readResults();
            while (iterator.hasNext()) {
                PyTorchResult result = iterator.next();
                logger.trace(() -> new ParameterizedMessage("[{}] Parsed result with id [{}]", deploymentId, result.getRequestId()));
                processResult(result);
                PendingResult pendingResult = pendingResults.get(result.getRequestId());
                if (pendingResult == null) {
                    logger.warn(() -> new ParameterizedMessage("[{}] no pending result for [{}]", deploymentId, result.getRequestId()));
                } else {
                    pendingResult.result.set(result);
                    pendingResult.latch.countDown();
                }
            }
        } catch (Exception e) {
            // No need to report error as we're stopping
            if (isStopping == false) {
                logger.error(new ParameterizedMessage("[{}] Error processing results", deploymentId), e);
            }
            pendingResults.forEach((id, pendingResults) -> {
                if (pendingResults.result.compareAndSet(null, new PyTorchResult(
                    id,
                    null,
                    null,
                    isStopping ?
                        "inference canceled as process is stopping" :
                        "inference native process died unexpectedly with failure [" + e.getMessage() + "]"))) {
                    pendingResults.latch.countDown();
                }
            });
            pendingResults.clear();
        } finally {
            pendingResults.forEach((id, pendingResults) -> {
                // Only set the result if it has not already been set
                if (pendingResults.result.compareAndSet(null, new PyTorchResult(
                    id,
                    null,
                    null,
                    "inference canceled as process is stopping"))) {
                    pendingResults.latch.countDown();
                }
            });
            pendingResults.clear();
        }
        stoppedProcessing = true;
        logger.debug(() -> new ParameterizedMessage("[{}] Results processing finished", deploymentId));
    }

    public synchronized LongSummaryStatistics getTimingStats() {
        return new LongSummaryStatistics(timingStats.getCount(),
            timingStats.getMin(),
            timingStats.getMax(),
            timingStats.getSum());
    }


    private synchronized void processResult(PyTorchResult result) {
        if (result.isError() == false) {
            timingStats.accept(result.getTimeMs());
            lastUsed = Instant.now();
        }
    }

    public PyTorchResult waitForResult(
        NativePyTorchProcess process,
        String requestId,
        PendingResult pendingResult,
        TimeValue timeout
    ) throws InterruptedException {
        if (process == null || stoppedProcessing || process.isProcessAlive() == false) {
            PyTorchResult storedResult = pendingResult.result.get();
            return storedResult == null ?
                new PyTorchResult(requestId, null, null, "native process no longer started") :
                storedResult;
        }
        if (pendingResult.latch.await(timeout.millis(), TimeUnit.MILLISECONDS)) {
            return pendingResult.result.get();
        }
        return null;
    }

    public synchronized Instant getLastUsed() {
        return lastUsed;
    }

    public void stop() {
        isStopping = true;
    }

    public static class PendingResult {
        private final AtomicReference<PyTorchResult> result = new AtomicReference<>();
        private final CountDownLatch latch = new CountDownLatch(1);
    }
}
