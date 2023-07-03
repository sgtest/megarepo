/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.rollup.action;

import org.elasticsearch.action.downsample.DownsampleConfig;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.tasks.CancellableTask;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.xpack.core.rollup.RollupField;

import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

public class RollupShardTask extends CancellableTask {
    private final String rollupIndex;
    private volatile long totalShardDocCount;
    private volatile long docsProcessed;
    private final long indexStartTimeMillis;
    private final long indexEndTimeMillis;
    private final DownsampleConfig config;
    private final ShardId shardId;
    private final long rollupStartTime;
    private final AtomicLong numReceived = new AtomicLong(0);
    private final AtomicLong numSent = new AtomicLong(0);
    private final AtomicLong numIndexed = new AtomicLong(0);
    private final AtomicLong numFailed = new AtomicLong(0);
    private final AtomicLong lastSourceTimestamp = new AtomicLong(0);
    private final AtomicLong lastTargetTimestamp = new AtomicLong(0);
    private final AtomicLong lastIndexingTimestamp = new AtomicLong(0);
    private final AtomicReference<RollupShardIndexerStatus> rollupShardIndexerStatus = new AtomicReference<>(
        RollupShardIndexerStatus.INITIALIZED
    );
    private final RollupBulkStats rollupBulkStats;
    private final AtomicReference<RollupBeforeBulkInfo> lastBeforeBulkInfo = new AtomicReference<>(null);
    private final AtomicReference<RollupAfterBulkInfo> lastAfterBulkInfo = new AtomicReference<>(null);

    public RollupShardTask(
        long id,
        String type,
        String action,
        TaskId parentTask,
        String rollupIndex,
        long indexStartTimeMillis,
        long indexEndTimeMillis,
        DownsampleConfig config,
        Map<String, String> headers,
        ShardId shardId
    ) {
        super(id, type, action, RollupField.NAME + "_" + rollupIndex + "[" + shardId.id() + "]", parentTask, headers);
        this.rollupIndex = rollupIndex;
        this.indexStartTimeMillis = indexStartTimeMillis;
        this.indexEndTimeMillis = indexEndTimeMillis;
        this.config = config;
        this.shardId = shardId;
        this.rollupStartTime = System.currentTimeMillis();
        this.rollupBulkStats = new RollupBulkStats();
    }

    public String getRollupIndex() {
        return rollupIndex;
    }

    public DownsampleConfig config() {
        return config;
    }

    public long getTotalShardDocCount() {
        return totalShardDocCount;
    }

    @Override
    public Status getStatus() {
        return new RollupShardStatus(
            shardId,
            rollupStartTime,
            numReceived.get(),
            numSent.get(),
            numIndexed.get(),
            numFailed.get(),
            totalShardDocCount,
            lastSourceTimestamp.get(),
            lastTargetTimestamp.get(),
            lastIndexingTimestamp.get(),
            indexStartTimeMillis,
            indexEndTimeMillis,
            docsProcessed,
            100.0F * docsProcessed / totalShardDocCount,
            rollupBulkStats.getRollupBulkInfo(),
            lastBeforeBulkInfo.get(),
            lastAfterBulkInfo.get(),
            rollupShardIndexerStatus.get()
        );
    }

    public long getNumReceived() {
        return numReceived.get();
    }

    public long getNumSent() {
        return numSent.get();
    }

    public long getNumIndexed() {
        return numIndexed.get();
    }

    public long getNumFailed() {
        return numFailed.get();
    }

    public long getLastSourceTimestamp() {
        return lastSourceTimestamp.get();
    }

    public long getLastTargetTimestamp() {
        return lastTargetTimestamp.get();
    }

    public RollupBeforeBulkInfo getLastBeforeBulkInfo() {
        return lastBeforeBulkInfo.get();
    }

    public RollupAfterBulkInfo getLastAfterBulkInfo() {
        return lastAfterBulkInfo.get();
    }

    public long getDocsProcessed() {
        return docsProcessed;
    }

    public long getRollupStartTime() {
        return rollupStartTime;
    }

    public RollupShardIndexerStatus getRollupShardIndexerStatus() {
        return rollupShardIndexerStatus.get();
    }

    public long getLastIndexingTimestamp() {
        return lastIndexingTimestamp.get();
    }

    public long getIndexStartTimeMillis() {
        return indexStartTimeMillis;
    }

    public long getIndexEndTimeMillis() {
        return indexEndTimeMillis;
    }

    public float getDocsProcessedPercentage() {
        return getTotalShardDocCount() <= 0 ? 0.0F : 100.0F * docsProcessed / totalShardDocCount;
    }

    public void addNumReceived(long count) {
        numReceived.addAndGet(count);
    }

    public void addNumSent(long count) {
        numSent.addAndGet(count);
    }

    public void addNumIndexed(long count) {
        numIndexed.addAndGet(count);
    }

    public void addNumFailed(long count) {
        numFailed.addAndGet(count);
    }

    public void setLastSourceTimestamp(long timestamp) {
        lastSourceTimestamp.set(timestamp);
    }

    public void setLastTargetTimestamp(long timestamp) {
        lastTargetTimestamp.set(timestamp);
    }

    public void setLastIndexingTimestamp(long timestamp) {
        lastIndexingTimestamp.set(timestamp);
    }

    public void setBeforeBulkInfo(final RollupBeforeBulkInfo beforeBulkInfo) {
        lastBeforeBulkInfo.set(beforeBulkInfo);
    }

    public void setAfterBulkInfo(final RollupAfterBulkInfo afterBulkInfo) {
        lastAfterBulkInfo.set(afterBulkInfo);
    }

    public void setRollupShardIndexerStatus(final RollupShardIndexerStatus status) {
        this.rollupShardIndexerStatus.set(status);
    }

    public void setTotalShardDocCount(int totalShardDocCount) {
        this.totalShardDocCount = totalShardDocCount;
    }

    public void setDocsProcessed(long docsProcessed) {
        this.docsProcessed = docsProcessed;
    }

    public void updateRollupBulkInfo(long bulkIngestTookMillis, long bulkTookMillis) {
        this.rollupBulkStats.update(bulkIngestTookMillis, bulkTookMillis);
    }

    public RollupBulkInfo getRollupBulkInfo() {
        return this.rollupBulkStats.getRollupBulkInfo();
    }
}
