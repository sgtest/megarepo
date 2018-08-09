/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */
package org.elasticsearch.protocol.xpack.ml.job.config;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ObjectParser.ValueType;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.protocol.xpack.ml.job.util.TimeUtil;

import java.io.IOException;
import java.util.Collections;
import java.util.Date;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * This class represents a configured and created Job. The creation time is set
 * to the time the object was constructed and the finished time and last
 * data time fields are {@code null} until the job has seen some data or it is
 * finished respectively.
 */
public class Job implements ToXContentObject {

    public static final String ANOMALY_DETECTOR_JOB_TYPE = "anomaly_detector";

    /*
     * Field names used in serialization
     */
    public static final ParseField ID = new ParseField("job_id");
    public static final ParseField JOB_TYPE = new ParseField("job_type");
    public static final ParseField GROUPS = new ParseField("groups");
    public static final ParseField ANALYSIS_CONFIG = AnalysisConfig.ANALYSIS_CONFIG;
    public static final ParseField ANALYSIS_LIMITS = new ParseField("analysis_limits");
    public static final ParseField CREATE_TIME = new ParseField("create_time");
    public static final ParseField CUSTOM_SETTINGS = new ParseField("custom_settings");
    public static final ParseField DATA_DESCRIPTION = new ParseField("data_description");
    public static final ParseField DESCRIPTION = new ParseField("description");
    public static final ParseField FINISHED_TIME = new ParseField("finished_time");
    public static final ParseField LAST_DATA_TIME = new ParseField("last_data_time");
    public static final ParseField ESTABLISHED_MODEL_MEMORY = new ParseField("established_model_memory");
    public static final ParseField MODEL_PLOT_CONFIG = new ParseField("model_plot_config");
    public static final ParseField RENORMALIZATION_WINDOW_DAYS = new ParseField("renormalization_window_days");
    public static final ParseField BACKGROUND_PERSIST_INTERVAL = new ParseField("background_persist_interval");
    public static final ParseField MODEL_SNAPSHOT_RETENTION_DAYS = new ParseField("model_snapshot_retention_days");
    public static final ParseField RESULTS_RETENTION_DAYS = new ParseField("results_retention_days");
    public static final ParseField MODEL_SNAPSHOT_ID = new ParseField("model_snapshot_id");
    public static final ParseField RESULTS_INDEX_NAME = new ParseField("results_index_name");
    public static final ParseField DELETED = new ParseField("deleted");

    public static final ObjectParser<Builder, Void> PARSER = new ObjectParser<>("job_details", true, Builder::new);

    static {
        PARSER.declareString(Builder::setId, ID);
        PARSER.declareString(Builder::setJobType, JOB_TYPE);
        PARSER.declareStringArray(Builder::setGroups, GROUPS);
        PARSER.declareStringOrNull(Builder::setDescription, DESCRIPTION);
        PARSER.declareField(Builder::setCreateTime,
            (p) -> TimeUtil.parseTimeField(p, CREATE_TIME.getPreferredName()),
            CREATE_TIME,
            ValueType.VALUE);
        PARSER.declareField(Builder::setFinishedTime,
            (p) -> TimeUtil.parseTimeField(p, FINISHED_TIME.getPreferredName()),
            FINISHED_TIME,
            ValueType.VALUE);
        PARSER.declareField(Builder::setLastDataTime,
            (p) -> TimeUtil.parseTimeField(p, LAST_DATA_TIME.getPreferredName()),
            LAST_DATA_TIME,
            ValueType.VALUE);
        PARSER.declareLong(Builder::setEstablishedModelMemory, ESTABLISHED_MODEL_MEMORY);
        PARSER.declareObject(Builder::setAnalysisConfig, AnalysisConfig.PARSER, ANALYSIS_CONFIG);
        PARSER.declareObject(Builder::setAnalysisLimits, AnalysisLimits.PARSER, ANALYSIS_LIMITS);
        PARSER.declareObject(Builder::setDataDescription, DataDescription.PARSER, DATA_DESCRIPTION);
        PARSER.declareObject(Builder::setModelPlotConfig, ModelPlotConfig.PARSER, MODEL_PLOT_CONFIG);
        PARSER.declareLong(Builder::setRenormalizationWindowDays, RENORMALIZATION_WINDOW_DAYS);
        PARSER.declareString((builder, val) -> builder.setBackgroundPersistInterval(
            TimeValue.parseTimeValue(val, BACKGROUND_PERSIST_INTERVAL.getPreferredName())), BACKGROUND_PERSIST_INTERVAL);
        PARSER.declareLong(Builder::setResultsRetentionDays, RESULTS_RETENTION_DAYS);
        PARSER.declareLong(Builder::setModelSnapshotRetentionDays, MODEL_SNAPSHOT_RETENTION_DAYS);
        PARSER.declareField(Builder::setCustomSettings, (p, c) -> p.map(), CUSTOM_SETTINGS, ValueType.OBJECT);
        PARSER.declareStringOrNull(Builder::setModelSnapshotId, MODEL_SNAPSHOT_ID);
        PARSER.declareString(Builder::setResultsIndexName, RESULTS_INDEX_NAME);
        PARSER.declareBoolean(Builder::setDeleted, DELETED);
    }

    private final String jobId;
    private final String jobType;

    private final List<String> groups;
    private final String description;
    private final Date createTime;
    private final Date finishedTime;
    private final Date lastDataTime;
    private final Long establishedModelMemory;
    private final AnalysisConfig analysisConfig;
    private final AnalysisLimits analysisLimits;
    private final DataDescription dataDescription;
    private final ModelPlotConfig modelPlotConfig;
    private final Long renormalizationWindowDays;
    private final TimeValue backgroundPersistInterval;
    private final Long modelSnapshotRetentionDays;
    private final Long resultsRetentionDays;
    private final Map<String, Object> customSettings;
    private final String modelSnapshotId;
    private final String resultsIndexName;
    private final boolean deleted;

    private Job(String jobId, String jobType, List<String> groups, String description, Date createTime,
                Date finishedTime, Date lastDataTime, Long establishedModelMemory,
                AnalysisConfig analysisConfig, AnalysisLimits analysisLimits, DataDescription dataDescription,
                ModelPlotConfig modelPlotConfig, Long renormalizationWindowDays, TimeValue backgroundPersistInterval,
                Long modelSnapshotRetentionDays, Long resultsRetentionDays, Map<String, Object> customSettings,
                String modelSnapshotId, String resultsIndexName, boolean deleted) {

        this.jobId = jobId;
        this.jobType = jobType;
        this.groups = Collections.unmodifiableList(groups);
        this.description = description;
        this.createTime = createTime;
        this.finishedTime = finishedTime;
        this.lastDataTime = lastDataTime;
        this.establishedModelMemory = establishedModelMemory;
        this.analysisConfig = analysisConfig;
        this.analysisLimits = analysisLimits;
        this.dataDescription = dataDescription;
        this.modelPlotConfig = modelPlotConfig;
        this.renormalizationWindowDays = renormalizationWindowDays;
        this.backgroundPersistInterval = backgroundPersistInterval;
        this.modelSnapshotRetentionDays = modelSnapshotRetentionDays;
        this.resultsRetentionDays = resultsRetentionDays;
        this.customSettings = customSettings == null ? null : Collections.unmodifiableMap(customSettings);
        this.modelSnapshotId = modelSnapshotId;
        this.resultsIndexName = resultsIndexName;
        this.deleted = deleted;
    }

    /**
     * Return the Job Id.
     *
     * @return The job Id string
     */
    public String getId() {
        return jobId;
    }

    public String getJobType() {
        return jobType;
    }

    public List<String> getGroups() {
        return groups;
    }

    /**
     * Private version of getResultsIndexName so that a job can be built from another
     * job and pass index name validation
     *
     * @return The job's index name, minus prefix
     */
    private String getResultsIndexNameNoPrefix() {
        return resultsIndexName;
    }

    /**
     * The job description
     *
     * @return job description
     */
    public String getDescription() {
        return description;
    }

    /**
     * The Job creation time. This name is preferred when serialising to the
     * REST API.
     *
     * @return The date the job was created
     */
    public Date getCreateTime() {
        return createTime;
    }

    /**
     * The time the job was finished or <code>null</code> if not finished.
     *
     * @return The date the job was last retired or <code>null</code>
     */
    public Date getFinishedTime() {
        return finishedTime;
    }

    /**
     * The last time data was uploaded to the job or <code>null</code> if no
     * data has been seen.
     *
     * @return The date at which the last data was processed
     */
    public Date getLastDataTime() {
        return lastDataTime;
    }

    /**
     * The established model memory of the job, or <code>null</code> if model
     * memory has not reached equilibrium yet.
     *
     * @return The established model memory of the job
     */
    public Long getEstablishedModelMemory() {
        return establishedModelMemory;
    }

    /**
     * The analysis configuration object
     *
     * @return The AnalysisConfig
     */
    public AnalysisConfig getAnalysisConfig() {
        return analysisConfig;
    }

    /**
     * The analysis options object
     *
     * @return The AnalysisLimits
     */
    public AnalysisLimits getAnalysisLimits() {
        return analysisLimits;
    }

    public ModelPlotConfig getModelPlotConfig() {
        return modelPlotConfig;
    }

    /**
     * If not set the input data is assumed to be csv with a '_time' field in
     * epoch format.
     *
     * @return A DataDescription or <code>null</code>
     * @see DataDescription
     */
    public DataDescription getDataDescription() {
        return dataDescription;
    }

    /**
     * The duration of the renormalization window in days
     *
     * @return renormalization window in days
     */
    public Long getRenormalizationWindowDays() {
        return renormalizationWindowDays;
    }

    /**
     * The background persistence interval
     *
     * @return background persistence interval
     */
    public TimeValue getBackgroundPersistInterval() {
        return backgroundPersistInterval;
    }

    public Long getModelSnapshotRetentionDays() {
        return modelSnapshotRetentionDays;
    }

    public Long getResultsRetentionDays() {
        return resultsRetentionDays;
    }

    public Map<String, Object> getCustomSettings() {
        return customSettings;
    }

    public String getModelSnapshotId() {
        return modelSnapshotId;
    }

    public boolean isDeleted() {
        return deleted;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        final String humanReadableSuffix = "_string";

        builder.field(ID.getPreferredName(), jobId);
        builder.field(JOB_TYPE.getPreferredName(), jobType);

        if (groups.isEmpty() == false) {
            builder.field(GROUPS.getPreferredName(), groups);
        }
        if (description != null) {
            builder.field(DESCRIPTION.getPreferredName(), description);
        }
        builder.timeField(CREATE_TIME.getPreferredName(), CREATE_TIME.getPreferredName() + humanReadableSuffix, createTime.getTime());
        if (finishedTime != null) {
            builder.timeField(FINISHED_TIME.getPreferredName(), FINISHED_TIME.getPreferredName() + humanReadableSuffix,
                finishedTime.getTime());
        }
        if (lastDataTime != null) {
            builder.timeField(LAST_DATA_TIME.getPreferredName(), LAST_DATA_TIME.getPreferredName() + humanReadableSuffix,
                lastDataTime.getTime());
        }
        if (establishedModelMemory != null) {
            builder.field(ESTABLISHED_MODEL_MEMORY.getPreferredName(), establishedModelMemory);
        }
        builder.field(ANALYSIS_CONFIG.getPreferredName(), analysisConfig, params);
        if (analysisLimits != null) {
            builder.field(ANALYSIS_LIMITS.getPreferredName(), analysisLimits, params);
        }
        if (dataDescription != null) {
            builder.field(DATA_DESCRIPTION.getPreferredName(), dataDescription, params);
        }
        if (modelPlotConfig != null) {
            builder.field(MODEL_PLOT_CONFIG.getPreferredName(), modelPlotConfig, params);
        }
        if (renormalizationWindowDays != null) {
            builder.field(RENORMALIZATION_WINDOW_DAYS.getPreferredName(), renormalizationWindowDays);
        }
        if (backgroundPersistInterval != null) {
            builder.field(BACKGROUND_PERSIST_INTERVAL.getPreferredName(), backgroundPersistInterval.getStringRep());
        }
        if (modelSnapshotRetentionDays != null) {
            builder.field(MODEL_SNAPSHOT_RETENTION_DAYS.getPreferredName(), modelSnapshotRetentionDays);
        }
        if (resultsRetentionDays != null) {
            builder.field(RESULTS_RETENTION_DAYS.getPreferredName(), resultsRetentionDays);
        }
        if (customSettings != null) {
            builder.field(CUSTOM_SETTINGS.getPreferredName(), customSettings);
        }
        if (modelSnapshotId != null) {
            builder.field(MODEL_SNAPSHOT_ID.getPreferredName(), modelSnapshotId);
        }
        if (resultsIndexName != null) {
            builder.field(RESULTS_INDEX_NAME.getPreferredName(), resultsIndexName);
        }
        if (params.paramAsBoolean("all", false)) {
            builder.field(DELETED.getPreferredName(), deleted);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }

        if (other == null || getClass() != other.getClass()) {
            return false;
        }

        Job that = (Job) other;
        return Objects.equals(this.jobId, that.jobId)
            && Objects.equals(this.jobType, that.jobType)
            && Objects.equals(this.groups, that.groups)
            && Objects.equals(this.description, that.description)
            && Objects.equals(this.createTime, that.createTime)
            && Objects.equals(this.finishedTime, that.finishedTime)
            && Objects.equals(this.lastDataTime, that.lastDataTime)
            && Objects.equals(this.establishedModelMemory, that.establishedModelMemory)
            && Objects.equals(this.analysisConfig, that.analysisConfig)
            && Objects.equals(this.analysisLimits, that.analysisLimits)
            && Objects.equals(this.dataDescription, that.dataDescription)
            && Objects.equals(this.modelPlotConfig, that.modelPlotConfig)
            && Objects.equals(this.renormalizationWindowDays, that.renormalizationWindowDays)
            && Objects.equals(this.backgroundPersistInterval, that.backgroundPersistInterval)
            && Objects.equals(this.modelSnapshotRetentionDays, that.modelSnapshotRetentionDays)
            && Objects.equals(this.resultsRetentionDays, that.resultsRetentionDays)
            && Objects.equals(this.customSettings, that.customSettings)
            && Objects.equals(this.modelSnapshotId, that.modelSnapshotId)
            && Objects.equals(this.resultsIndexName, that.resultsIndexName)
            && Objects.equals(this.deleted, that.deleted);
    }

    @Override
    public int hashCode() {
        return Objects.hash(jobId, jobType, groups, description, createTime, finishedTime, lastDataTime, establishedModelMemory,
            analysisConfig, analysisLimits, dataDescription, modelPlotConfig, renormalizationWindowDays,
            backgroundPersistInterval, modelSnapshotRetentionDays, resultsRetentionDays, customSettings,
            modelSnapshotId, resultsIndexName, deleted);
    }

    @Override
    public final String toString() {
        return Strings.toString(this);
    }

    public static class Builder {

        private String id;
        private String jobType = ANOMALY_DETECTOR_JOB_TYPE;
        private List<String> groups = Collections.emptyList();
        private String description;
        private AnalysisConfig analysisConfig;
        private AnalysisLimits analysisLimits;
        private DataDescription dataDescription;
        private Date createTime;
        private Date finishedTime;
        private Date lastDataTime;
        private Long establishedModelMemory;
        private ModelPlotConfig modelPlotConfig;
        private Long renormalizationWindowDays;
        private TimeValue backgroundPersistInterval;
        private Long modelSnapshotRetentionDays;
        private Long resultsRetentionDays;
        private Map<String, Object> customSettings;
        private String modelSnapshotId;
        private String resultsIndexName;
        private boolean deleted;

        public Builder() {
        }

        public Builder(String id) {
            this.id = id;
        }

        public Builder(Job job) {
            this.id = job.getId();
            this.jobType = job.getJobType();
            this.groups = job.getGroups();
            this.description = job.getDescription();
            this.analysisConfig = job.getAnalysisConfig();
            this.analysisLimits = job.getAnalysisLimits();
            this.dataDescription = job.getDataDescription();
            this.createTime = job.getCreateTime();
            this.finishedTime = job.getFinishedTime();
            this.lastDataTime = job.getLastDataTime();
            this.establishedModelMemory = job.getEstablishedModelMemory();
            this.modelPlotConfig = job.getModelPlotConfig();
            this.renormalizationWindowDays = job.getRenormalizationWindowDays();
            this.backgroundPersistInterval = job.getBackgroundPersistInterval();
            this.modelSnapshotRetentionDays = job.getModelSnapshotRetentionDays();
            this.resultsRetentionDays = job.getResultsRetentionDays();
            this.customSettings = job.getCustomSettings();
            this.modelSnapshotId = job.getModelSnapshotId();
            this.resultsIndexName = job.getResultsIndexNameNoPrefix();
            this.deleted = job.isDeleted();
        }

        public Builder setId(String id) {
            this.id = id;
            return this;
        }

        public String getId() {
            return id;
        }

        public Builder setJobType(String jobType) {
            this.jobType = jobType;
            return this;
        }

        public Builder setGroups(List<String> groups) {
            this.groups = groups == null ? Collections.emptyList() : groups;
            return this;
        }

        public Builder setCustomSettings(Map<String, Object> customSettings) {
            this.customSettings = customSettings;
            return this;
        }

        public Builder setDescription(String description) {
            this.description = description;
            return this;
        }

        public Builder setAnalysisConfig(AnalysisConfig.Builder configBuilder) {
            analysisConfig = Objects.requireNonNull(configBuilder, ANALYSIS_CONFIG.getPreferredName()).build();
            return this;
        }

        public Builder setAnalysisLimits(AnalysisLimits analysisLimits) {
            this.analysisLimits = Objects.requireNonNull(analysisLimits, ANALYSIS_LIMITS.getPreferredName());
            return this;
        }

        Builder setCreateTime(Date createTime) {
            this.createTime = createTime;
            return this;
        }

        Builder setFinishedTime(Date finishedTime) {
            this.finishedTime = finishedTime;
            return this;
        }

        /**
         * Set the wall clock time of the last data upload
         *
         * @param lastDataTime Wall clock time
         */
        public Builder setLastDataTime(Date lastDataTime) {
            this.lastDataTime = lastDataTime;
            return this;
        }

        public Builder setEstablishedModelMemory(Long establishedModelMemory) {
            this.establishedModelMemory = establishedModelMemory;
            return this;
        }

        public Builder setDataDescription(DataDescription.Builder description) {
            dataDescription = Objects.requireNonNull(description, DATA_DESCRIPTION.getPreferredName()).build();
            return this;
        }

        public Builder setModelPlotConfig(ModelPlotConfig modelPlotConfig) {
            this.modelPlotConfig = modelPlotConfig;
            return this;
        }

        public Builder setBackgroundPersistInterval(TimeValue backgroundPersistInterval) {
            this.backgroundPersistInterval = backgroundPersistInterval;
            return this;
        }

        public Builder setRenormalizationWindowDays(Long renormalizationWindowDays) {
            this.renormalizationWindowDays = renormalizationWindowDays;
            return this;
        }

        public Builder setModelSnapshotRetentionDays(Long modelSnapshotRetentionDays) {
            this.modelSnapshotRetentionDays = modelSnapshotRetentionDays;
            return this;
        }

        public Builder setResultsRetentionDays(Long resultsRetentionDays) {
            this.resultsRetentionDays = resultsRetentionDays;
            return this;
        }

        public Builder setModelSnapshotId(String modelSnapshotId) {
            this.modelSnapshotId = modelSnapshotId;
            return this;
        }

        public Builder setResultsIndexName(String resultsIndexName) {
            this.resultsIndexName = resultsIndexName;
            return this;
        }

        public Builder setDeleted(boolean deleted) {
            this.deleted = deleted;
            return this;
        }

        /**
         * Builds a job.
         *
         * @return The job
         */
        public Job build() {
            Objects.requireNonNull(id,  "[" + ID.getPreferredName() + "] must not be null");
            Objects.requireNonNull(jobType,  "[" + JOB_TYPE.getPreferredName() + "] must not be null");
            return new Job(
                id, jobType, groups, description, createTime, finishedTime, lastDataTime, establishedModelMemory,
                analysisConfig, analysisLimits, dataDescription, modelPlotConfig, renormalizationWindowDays,
                backgroundPersistInterval, modelSnapshotRetentionDays, resultsRetentionDays, customSettings,
                modelSnapshotId, resultsIndexName, deleted);
        }
    }
}
