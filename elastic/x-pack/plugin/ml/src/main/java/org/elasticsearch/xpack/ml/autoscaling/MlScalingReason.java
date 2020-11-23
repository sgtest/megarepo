/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ml.autoscaling;

import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.autoscaling.capacity.AutoscalingCapacity;
import org.elasticsearch.xpack.autoscaling.capacity.AutoscalingDeciderResult;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

public class MlScalingReason implements AutoscalingDeciderResult.Reason {

    static final String NAME = "ml";
    static final String WAITING_ANALYTICS_JOBS = "waiting_analytics_jobs";
    static final String WAITING_ANOMALY_JOBS = "waiting_anomaly_jobs";
    static final String CONFIGURATION = "configuration";
    static final String LARGEST_WAITING_ANALYTICS_JOB = "largest_waiting_analytics_job";
    static final String LARGEST_WAITING_ANOMALY_JOB = "largest_waiting_anomaly_job";
    static final String CURRENT_CAPACITY = "current_capacity";
    static final String REASON = "reason";

    private final List<String> waitingAnalyticsJobs;
    private final List<String> waitingAnomalyJobs;
    private final Settings passedConfiguration;
    private final Long largestWaitingAnalyticsJob;
    private final Long largestWaitingAnomalyJob;
    private final AutoscalingCapacity currentMlCapacity;
    private final String simpleReason;

    public MlScalingReason(StreamInput in) throws IOException {
        this.waitingAnalyticsJobs = in.readStringList();
        this.waitingAnomalyJobs = in.readStringList();
        this.passedConfiguration = Settings.readSettingsFromStream(in);;
        this.currentMlCapacity = new AutoscalingCapacity(in);
        this.largestWaitingAnalyticsJob = in.readOptionalVLong();
        this.largestWaitingAnomalyJob = in.readOptionalVLong();
        this.simpleReason = in.readString();
    }

    MlScalingReason(List<String> waitingAnalyticsJobs,
                    List<String> waitingAnomalyJobs,
                    Settings passedConfiguration,
                    Long largestWaitingAnalyticsJob,
                    Long largestWaitingAnomalyJob,
                    AutoscalingCapacity currentMlCapacity,
                    String simpleReason) {
        this.waitingAnalyticsJobs = waitingAnalyticsJobs == null ? Collections.emptyList() : waitingAnalyticsJobs;
        this.waitingAnomalyJobs = waitingAnomalyJobs == null ? Collections.emptyList() : waitingAnomalyJobs;
        this.passedConfiguration = ExceptionsHelper.requireNonNull(passedConfiguration, CONFIGURATION);
        this.largestWaitingAnalyticsJob = largestWaitingAnalyticsJob;
        this.largestWaitingAnomalyJob = largestWaitingAnomalyJob;
        this.currentMlCapacity = ExceptionsHelper.requireNonNull(currentMlCapacity, CURRENT_CAPACITY);
        this.simpleReason = ExceptionsHelper.requireNonNull(simpleReason, REASON);
    }

    public static Builder builder() {
        return new Builder();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        MlScalingReason that = (MlScalingReason) o;
        return Objects.equals(waitingAnalyticsJobs, that.waitingAnalyticsJobs) &&
            Objects.equals(waitingAnomalyJobs, that.waitingAnomalyJobs) &&
            Objects.equals(passedConfiguration, that.passedConfiguration) &&
            Objects.equals(largestWaitingAnalyticsJob, that.largestWaitingAnalyticsJob) &&
            Objects.equals(largestWaitingAnomalyJob, that.largestWaitingAnomalyJob) &&
            Objects.equals(currentMlCapacity, that.currentMlCapacity) &&
            Objects.equals(simpleReason, that.simpleReason);
    }

    @Override
    public int hashCode() {
        return Objects.hash(waitingAnalyticsJobs,
            waitingAnomalyJobs,
            passedConfiguration,
            largestWaitingAnalyticsJob,
            largestWaitingAnomalyJob,
            currentMlCapacity,
            simpleReason);
    }

    @Override
    public String summary() {
        return simpleReason;
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeStringCollection(this.waitingAnalyticsJobs);
        out.writeStringCollection(this.waitingAnomalyJobs);
        Settings.writeSettingsToStream(this.passedConfiguration, out);
        this.currentMlCapacity.writeTo(out);
        out.writeOptionalVLong(largestWaitingAnalyticsJob);
        out.writeOptionalVLong(largestWaitingAnomalyJob);
        out.writeString(this.simpleReason);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(WAITING_ANALYTICS_JOBS, waitingAnalyticsJobs);
        builder.field(WAITING_ANOMALY_JOBS, waitingAnalyticsJobs);
        builder.field(CONFIGURATION, passedConfiguration);
        if (largestWaitingAnalyticsJob != null) {
            builder.field(LARGEST_WAITING_ANALYTICS_JOB, largestWaitingAnalyticsJob);
        }
        if (largestWaitingAnomalyJob != null) {
            builder.field(LARGEST_WAITING_ANOMALY_JOB, largestWaitingAnomalyJob);
        }
        builder.field(CURRENT_CAPACITY, currentMlCapacity);
        builder.field(REASON, simpleReason);
        builder.endObject();
        return builder;
    }

    static class Builder {
        private List<String> waitingAnalyticsJobs = Collections.emptyList();
        private List<String> waitingAnomalyJobs = Collections.emptyList();
        private Settings passedConfiguration;
        private Long largestWaitingAnalyticsJob;
        private Long largestWaitingAnomalyJob;
        private AutoscalingCapacity currentMlCapacity;
        private String simpleReason;

        public Builder setWaitingAnalyticsJobs(List<String> waitingAnalyticsJobs) {
            this.waitingAnalyticsJobs = waitingAnalyticsJobs;
            return this;
        }

        public Builder setWaitingAnomalyJobs(List<String> waitingAnomalyJobs) {
            this.waitingAnomalyJobs = waitingAnomalyJobs;
            return this;
        }

        public Builder setPassedConfiguration(Settings passedConfiguration) {
            this.passedConfiguration = passedConfiguration;
            return this;
        }

        public Builder setLargestWaitingAnalyticsJob(Long largestWaitingAnalyticsJob) {
            this.largestWaitingAnalyticsJob = largestWaitingAnalyticsJob;
            return this;
        }

        public Builder setLargestWaitingAnomalyJob(Long largestWaitingAnomalyJob) {
            this.largestWaitingAnomalyJob = largestWaitingAnomalyJob;
            return this;
        }

        public Builder setCurrentMlCapacity(AutoscalingCapacity currentMlCapacity) {
            this.currentMlCapacity = currentMlCapacity;
            return this;
        }

        public Builder setSimpleReason(String simpleReason) {
            this.simpleReason = simpleReason;
            return this;
        }

        public MlScalingReason build() {
            return new MlScalingReason(
                waitingAnalyticsJobs,
                waitingAnomalyJobs,
                passedConfiguration,
                largestWaitingAnalyticsJob,
                largestWaitingAnomalyJob,
                currentMlCapacity,
                simpleReason
            );
        }
    }
}
