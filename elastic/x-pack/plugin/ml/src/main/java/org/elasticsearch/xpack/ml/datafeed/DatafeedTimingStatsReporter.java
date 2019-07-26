/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.datafeed;

import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedTimingStats;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.DataCounts;
import org.elasticsearch.xpack.ml.job.persistence.JobResultsPersister;

import java.util.Objects;

/**
 * {@link DatafeedTimingStatsReporter} class handles the logic of persisting {@link DatafeedTimingStats} if they changed significantly
 * since the last time they were persisted.
 *
 * This class is not thread-safe.
 */
public class DatafeedTimingStatsReporter {

    /** Persisted timing stats. May be stale. */
    private DatafeedTimingStats persistedTimingStats;
    /** Current timing stats. */
    private volatile DatafeedTimingStats currentTimingStats;
    /** Object used to persist current timing stats. */
    private final JobResultsPersister jobResultsPersister;

    public DatafeedTimingStatsReporter(DatafeedTimingStats timingStats, JobResultsPersister jobResultsPersister) {
        Objects.requireNonNull(timingStats);
        this.persistedTimingStats = new DatafeedTimingStats(timingStats);
        this.currentTimingStats = new DatafeedTimingStats(timingStats);
        this.jobResultsPersister = Objects.requireNonNull(jobResultsPersister);
    }

    public DatafeedTimingStats getCurrentTimingStats() {
        return new DatafeedTimingStats(currentTimingStats);
    }

    /**
     * Reports how much time did the search request execution take.
     */
    public void reportSearchDuration(TimeValue searchDuration) {
        if (searchDuration == null) {
            return;
        }
        currentTimingStats.incrementSearchTimeMs(searchDuration.millis());
        flushIfDifferSignificantly();
    }

    /**
     * Reports the data counts received from the autodetect process.
     */
    public void reportDataCounts(DataCounts dataCounts) {
        if (dataCounts == null) {
            return;
        }
        currentTimingStats.incrementBucketCount(dataCounts.getBucketCount());
        if (dataCounts.getLatestRecordTimeStamp() != null) {
            currentTimingStats.setLatestRecordTimestamp(dataCounts.getLatestRecordTimeStamp().toInstant());
        }
        flushIfDifferSignificantly();
    }

    private void flushIfDifferSignificantly() {
        if (differSignificantly(currentTimingStats, persistedTimingStats)) {
            flush();
        }
    }

    private void flush() {
        persistedTimingStats = new DatafeedTimingStats(currentTimingStats);
        // TODO: Consider changing refresh policy to NONE here and only do IMMEDIATE on datafeed _stop action
        jobResultsPersister.persistDatafeedTimingStats(persistedTimingStats, WriteRequest.RefreshPolicy.IMMEDIATE);
    }

    /**
     * Returns true if given stats objects differ from each other by more than 10% for at least one of the statistics.
     */
    public static boolean differSignificantly(DatafeedTimingStats stats1, DatafeedTimingStats stats2) {
        return countsDifferSignificantly(stats1.getSearchCount(), stats2.getSearchCount())
            || differSignificantly(stats1.getTotalSearchTimeMs(), stats2.getTotalSearchTimeMs())
            || differSignificantly(stats1.getAvgSearchTimePerBucketMs(), stats2.getAvgSearchTimePerBucketMs())
            || differSignificantly(stats1.getExponentialAvgSearchTimePerHourMs(), stats2.getExponentialAvgSearchTimePerHourMs());
    }

    /**
     * Returns {@code true} if one of the ratios { value1 / value2, value2 / value1 } is smaller than MIN_VALID_RATIO.
     * This can be interpreted as values { value1, value2 } differing significantly from each other.
     */
    private static boolean countsDifferSignificantly(long value1, long value2) {
        return (((double) value2) / value1 < MIN_VALID_RATIO)
            || (((double) value1) / value2 < MIN_VALID_RATIO);
    }

    /**
     * Returns {@code true} if one of the ratios { value1 / value2, value2 / value1 } is smaller than MIN_VALID_RATIO or
     * the absolute difference |value1 - value2| is greater than MAX_VALID_ABS_DIFFERENCE_MS.
     * This can be interpreted as values { value1, value2 } differing significantly from each other.
     * This method also returns:
     *   - {@code true} in case one value is {@code null} while the other is not.
     *   - {@code false} in case both values are {@code null}.
     */
    private static boolean differSignificantly(Double value1, Double value2) {
        if (value1 != null && value2 != null) {
            return (value2 / value1 < MIN_VALID_RATIO)
                || (value1 / value2 < MIN_VALID_RATIO)
                || Math.abs(value1 - value2) > MAX_VALID_ABS_DIFFERENCE_MS;
        }
        return (value1 != null) || (value2 != null);
    }

    /**
     * Minimum ratio of values that is interpreted as values being similar.
     * If the values ratio is less than MIN_VALID_RATIO, the values are interpreted as significantly different.
     */
    private static final double MIN_VALID_RATIO = 0.9;

    /**
     * Maximum absolute difference of values that is interpreted as values being similar.
     * If the values absolute difference is greater than MAX_VALID_ABS_DIFFERENCE, the values are interpreted as significantly different.
     */
    private static final double MAX_VALID_ABS_DIFFERENCE_MS = 10000.0;  // 10s
}
