/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.datafeed;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisConfig;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

public final class DatafeedJobValidator {

    private DatafeedJobValidator() {}

    /**
     * Validates a datafeedConfig in relation to the job it refers to
     * @param datafeedConfig the datafeed config
     * @param job the job
     */
    public static void validate(DatafeedConfig datafeedConfig, Job job) {
        AnalysisConfig analysisConfig = job.getAnalysisConfig();
        if (analysisConfig.getLatency() != null && analysisConfig.getLatency().seconds() > 0) {
            throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.DATAFEED_DOES_NOT_SUPPORT_JOB_WITH_LATENCY));
        }
        if (datafeedConfig.hasAggregations()) {
            checkSummaryCountFieldNameIsSet(analysisConfig);
            checkValidHistogramInterval(datafeedConfig, analysisConfig);
            checkFrequencyIsMultipleOfHistogramInterval(datafeedConfig);
        }
    }

    private static void checkSummaryCountFieldNameIsSet(AnalysisConfig analysisConfig) {
        if (Strings.isNullOrEmpty(analysisConfig.getSummaryCountFieldName())) {
            throw ExceptionsHelper.badRequestException(Messages.getMessage(
                    Messages.DATAFEED_AGGREGATIONS_REQUIRES_JOB_WITH_SUMMARY_COUNT_FIELD));
        }
    }

    private static void checkValidHistogramInterval(DatafeedConfig datafeedConfig, AnalysisConfig analysisConfig) {
        long histogramIntervalMillis = datafeedConfig.getHistogramIntervalMillis();
        long bucketSpanMillis = analysisConfig.getBucketSpan().millis();
        if (histogramIntervalMillis > bucketSpanMillis) {
            throw ExceptionsHelper.badRequestException(Messages.getMessage(
                    Messages.DATAFEED_AGGREGATIONS_INTERVAL_MUST_LESS_OR_EQUAL_TO_BUCKET_SPAN,
                    TimeValue.timeValueMillis(histogramIntervalMillis).getStringRep(),
                    TimeValue.timeValueMillis(bucketSpanMillis).getStringRep()));
        }

        if (bucketSpanMillis % histogramIntervalMillis != 0) {
            throw ExceptionsHelper.badRequestException(Messages.getMessage(
                    Messages.DATAFEED_AGGREGATIONS_INTERVAL_MUST_BE_DIVISOR_OF_BUCKET_SPAN,
                    TimeValue.timeValueMillis(histogramIntervalMillis).getStringRep(),
                    TimeValue.timeValueMillis(bucketSpanMillis).getStringRep()));
        }
    }

    private static void checkFrequencyIsMultipleOfHistogramInterval(DatafeedConfig datafeedConfig) {
        TimeValue frequency = datafeedConfig.getFrequency();
        if (frequency != null) {
            long histogramIntervalMillis = datafeedConfig.getHistogramIntervalMillis();
            long frequencyMillis = frequency.millis();
            if (frequencyMillis % histogramIntervalMillis != 0) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(
                        Messages.DATAFEED_FREQUENCY_MUST_BE_MULTIPLE_OF_AGGREGATIONS_INTERVAL,
                        frequency, TimeValue.timeValueMillis(histogramIntervalMillis).getStringRep()));
            }
        }
    }
}
