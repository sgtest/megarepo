/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.inference.external.http.sender;

import org.elasticsearch.threadpool.ThreadPool;

import java.util.Objects;

import static org.elasticsearch.xpack.inference.InferencePlugin.UTILITY_THREAD_POOL_NAME;

abstract class BaseRequestManager implements RequestManager {
    private final ThreadPool threadPool;
    private final String inferenceEntityId;
    private final Object rateLimitGroup;

    BaseRequestManager(ThreadPool threadPool, String inferenceEntityId, Object rateLimitGroup) {
        this.threadPool = Objects.requireNonNull(threadPool);
        this.inferenceEntityId = Objects.requireNonNull(inferenceEntityId);
        this.rateLimitGroup = Objects.requireNonNull(rateLimitGroup);
    }

    protected void execute(Runnable runnable) {
        threadPool.executor(UTILITY_THREAD_POOL_NAME).execute(runnable);
    }

    @Override
    public String inferenceEntityId() {
        return inferenceEntityId;
    }

    @Override
    public Object rateLimitGrouping() {
        return rateLimitGroup;
    }
}
