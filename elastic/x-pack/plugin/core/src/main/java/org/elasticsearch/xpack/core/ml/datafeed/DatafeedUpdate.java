/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.datafeed;

import org.elasticsearch.Version;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.stream.Collectors;

import static org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig.AGG_TRANSFORMER;
import static org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig.QUERY_TRANSFORMER;
import static org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig.lazyAggParser;
import static org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig.lazyQueryParser;

/**
 * A datafeed update contains partial properties to update a {@link DatafeedConfig}.
 * The main difference between this class and {@link DatafeedConfig} is that here all
 * fields are nullable.
 */
public class DatafeedUpdate implements Writeable, ToXContentObject {

    public static final ObjectParser<Builder, Void> PARSER = new ObjectParser<>("datafeed_update", Builder::new);

    static {
        PARSER.declareString(Builder::setId, DatafeedConfig.ID);
        PARSER.declareString(Builder::setJobId, Job.ID);
        PARSER.declareStringArray(Builder::setIndices, DatafeedConfig.INDEXES);
        PARSER.declareStringArray(Builder::setIndices, DatafeedConfig.INDICES);
        PARSER.declareString((builder, val) -> builder.setQueryDelay(
                TimeValue.parseTimeValue(val, DatafeedConfig.QUERY_DELAY.getPreferredName())), DatafeedConfig.QUERY_DELAY);
        PARSER.declareString((builder, val) -> builder.setFrequency(
                TimeValue.parseTimeValue(val, DatafeedConfig.FREQUENCY.getPreferredName())), DatafeedConfig.FREQUENCY);
        PARSER.declareObject(Builder::setQuery, (p, c) -> p.mapOrdered(), DatafeedConfig.QUERY);
        PARSER.declareObject(Builder::setAggregationsSafe, (p, c) -> p.mapOrdered(), DatafeedConfig.AGGREGATIONS);
        PARSER.declareObject(Builder::setAggregationsSafe,(p, c) -> p.mapOrdered(), DatafeedConfig.AGGS);
        PARSER.declareObject(Builder::setScriptFields, (p, c) -> {
                List<SearchSourceBuilder.ScriptField> parsedScriptFields = new ArrayList<>();
                while (p.nextToken() != XContentParser.Token.END_OBJECT) {
                    parsedScriptFields.add(new SearchSourceBuilder.ScriptField(p));
            }
            parsedScriptFields.sort(Comparator.comparing(SearchSourceBuilder.ScriptField::fieldName));
            return parsedScriptFields;
        }, DatafeedConfig.SCRIPT_FIELDS);
        PARSER.declareInt(Builder::setScrollSize, DatafeedConfig.SCROLL_SIZE);
        PARSER.declareObject(Builder::setChunkingConfig, ChunkingConfig.STRICT_PARSER, DatafeedConfig.CHUNKING_CONFIG);
        PARSER.declareObject(Builder::setDelayedDataCheckConfig,
            DelayedDataCheckConfig.STRICT_PARSER,
            DatafeedConfig.DELAYED_DATA_CHECK_CONFIG);
    }

    private final String id;
    private final String jobId;
    private final TimeValue queryDelay;
    private final TimeValue frequency;
    private final List<String> indices;
    private final Map<String, Object> query;
    private final Map<String, Object> aggregations;
    private final List<SearchSourceBuilder.ScriptField> scriptFields;
    private final Integer scrollSize;
    private final ChunkingConfig chunkingConfig;
    private final DelayedDataCheckConfig delayedDataCheckConfig;

    private DatafeedUpdate(String id, String jobId, TimeValue queryDelay, TimeValue frequency, List<String> indices,
                           Map<String, Object> query, Map<String, Object> aggregations, List<SearchSourceBuilder.ScriptField> scriptFields,
                           Integer scrollSize, ChunkingConfig chunkingConfig, DelayedDataCheckConfig delayedDataCheckConfig) {
        this.id = id;
        this.jobId = jobId;
        this.queryDelay = queryDelay;
        this.frequency = frequency;
        this.indices = indices;
        this.query = query;
        this.aggregations = aggregations;
        this.scriptFields = scriptFields;
        this.scrollSize = scrollSize;
        this.chunkingConfig = chunkingConfig;
        this.delayedDataCheckConfig = delayedDataCheckConfig;
    }

    public DatafeedUpdate(StreamInput in) throws IOException {
        this.id = in.readString();
        this.jobId = in.readOptionalString();
        this.queryDelay = in.readOptionalTimeValue();
        this.frequency = in.readOptionalTimeValue();
        if (in.readBoolean()) {
            this.indices = in.readStringList();
        } else {
            this.indices = null;
        }
        // This consumes the list of types if there was one.
        if (in.getVersion().before(Version.V_7_0_0)) {
            if (in.readBoolean()) {
                in.readStringList();
            }
        }
        if (in.getVersion().before(Version.V_7_1_0)) {
            this.query = QUERY_TRANSFORMER.toMap(in.readOptionalNamedWriteable(QueryBuilder.class));
            this.aggregations = AGG_TRANSFORMER.toMap(in.readOptionalWriteable(AggregatorFactories.Builder::new));
        } else {
            this.query = in.readMap();
            if (in.readBoolean()) {
                this.aggregations = in.readMap();
            } else {
                this.aggregations = null;
            }
        }
        if (in.readBoolean()) {
            this.scriptFields = in.readList(SearchSourceBuilder.ScriptField::new);
        } else {
            this.scriptFields = null;
        }
        this.scrollSize = in.readOptionalVInt();
        this.chunkingConfig = in.readOptionalWriteable(ChunkingConfig::new);
        if (in.getVersion().onOrAfter(Version.V_6_6_0)) {
            delayedDataCheckConfig = in.readOptionalWriteable(DelayedDataCheckConfig::new);
        } else {
            delayedDataCheckConfig = null;
        }
    }

    /**
     * Get the id of the datafeed to update
     */
    public String getId() {
        return id;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeString(id);
        out.writeOptionalString(jobId);
        out.writeOptionalTimeValue(queryDelay);
        out.writeOptionalTimeValue(frequency);
        if (indices != null) {
            out.writeBoolean(true);
            out.writeStringCollection(indices);
        } else {
            out.writeBoolean(false);
        }
        // Write the now removed types to prior versions.
        // An empty list is expected
        if (out.getVersion().before(Version.V_7_0_0)) {
            out.writeBoolean(true);
            out.writeStringCollection(Collections.emptyList());
        }
        if (out.getVersion().before(Version.V_7_1_0)) {
            out.writeOptionalNamedWriteable(lazyQueryParser.apply(query, id, new ArrayList<>()));
            out.writeOptionalWriteable(lazyAggParser.apply(aggregations, id, new ArrayList<>()));
        } else {
            out.writeMap(query);
            out.writeBoolean(aggregations != null);
            if (aggregations != null) {
                out.writeMap(aggregations);
            }
        }
        if (scriptFields != null) {
            out.writeBoolean(true);
            out.writeList(scriptFields);
        } else {
            out.writeBoolean(false);
        }
        out.writeOptionalVInt(scrollSize);
        out.writeOptionalWriteable(chunkingConfig);
        if (out.getVersion().onOrAfter(Version.V_6_6_0)) {
            out.writeOptionalWriteable(delayedDataCheckConfig);
        }
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(DatafeedConfig.ID.getPreferredName(), id);
        addOptionalField(builder, Job.ID, jobId);
        if (queryDelay != null) {
            builder.field(DatafeedConfig.QUERY_DELAY.getPreferredName(), queryDelay.getStringRep());
        }
        if (frequency != null) {
            builder.field(DatafeedConfig.FREQUENCY.getPreferredName(), frequency.getStringRep());
        }
        addOptionalField(builder, DatafeedConfig.INDICES, indices);
        addOptionalField(builder, DatafeedConfig.QUERY, query);
        addOptionalField(builder, DatafeedConfig.AGGREGATIONS, aggregations);
        if (scriptFields != null) {
            builder.startObject(DatafeedConfig.SCRIPT_FIELDS.getPreferredName());
            for (SearchSourceBuilder.ScriptField scriptField : scriptFields) {
                scriptField.toXContent(builder, params);
            }
            builder.endObject();
        }
        addOptionalField(builder, DatafeedConfig.SCROLL_SIZE, scrollSize);
        addOptionalField(builder, DatafeedConfig.CHUNKING_CONFIG, chunkingConfig);
        addOptionalField(builder, DatafeedConfig.DELAYED_DATA_CHECK_CONFIG, delayedDataCheckConfig);
        builder.endObject();
        return builder;
    }

    private void addOptionalField(XContentBuilder builder, ParseField field, Object value) throws IOException {
        if (value != null) {
            builder.field(field.getPreferredName(), value);
        }
    }

    public String getJobId() {
        return jobId;
    }

    TimeValue getQueryDelay() {
        return queryDelay;
    }

    TimeValue getFrequency() {
        return frequency;
    }

    List<String> getIndices() {
        return indices;
    }

    Integer getScrollSize() {
        return scrollSize;
    }

    Map<String, Object> getQuery() {
        return query;
    }

    Map<String, Object> getAggregations() {
        return aggregations;
    }

    /**
     * @return {@code true} when there are non-empty aggregations, {@code false}
     *         otherwise
     */
    boolean hasAggregations() {
        return aggregations != null && aggregations.size() > 0;
    }

    List<SearchSourceBuilder.ScriptField> getScriptFields() {
        return scriptFields == null ? Collections.emptyList() : scriptFields;
    }

    ChunkingConfig getChunkingConfig() {
        return chunkingConfig;
    }

    public DelayedDataCheckConfig getDelayedDataCheckConfig() {
        return delayedDataCheckConfig;
    }

    /**
     * Applies the update to the given {@link DatafeedConfig}
     * @return a new {@link DatafeedConfig} that contains the update
     */
    public DatafeedConfig apply(DatafeedConfig datafeedConfig, Map<String, String> headers) {
        if (id.equals(datafeedConfig.getId()) == false) {
            throw new IllegalArgumentException("Cannot apply update to datafeedConfig with different id");
        }

        DatafeedConfig.Builder builder = new DatafeedConfig.Builder(datafeedConfig);
        if (jobId != null) {
            builder.setJobId(jobId);
        }
        if (queryDelay != null) {
            builder.setQueryDelay(queryDelay);
        }
        if (frequency != null) {
            builder.setFrequency(frequency);
        }
        if (indices != null) {
            builder.setIndices(indices);
        }
        if (query != null) {
            builder.setQuery(query);
        }
        if (aggregations != null) {
            DatafeedConfig.validateAggregations(lazyAggParser.apply(aggregations, id, new ArrayList<>()));
            builder.setAggregations(aggregations);
        }
        if (scriptFields != null) {
            builder.setScriptFields(scriptFields);
        }
        if (scrollSize != null) {
            builder.setScrollSize(scrollSize);
        }
        if (chunkingConfig != null) {
            builder.setChunkingConfig(chunkingConfig);
        }
        if (delayedDataCheckConfig != null) {
            builder.setDelayedDataCheckConfig(delayedDataCheckConfig);
        }

        if (headers.isEmpty() == false) {
            // Adjust the request, adding security headers from the current thread context
            Map<String, String> securityHeaders = headers.entrySet().stream()
                    .filter(e -> ClientHelper.SECURITY_HEADER_FILTERS.contains(e.getKey()))
                    .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue));
            builder.setHeaders(securityHeaders);
        }

        return builder.build();
    }

    /**
     * The lists of indices and types are compared for equality but they are not
     * sorted first so this test could fail simply because the indices and types
     * lists are in different orders.
     */
    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }

        if (other instanceof DatafeedUpdate == false) {
            return false;
        }

        DatafeedUpdate that = (DatafeedUpdate) other;

        return Objects.equals(this.id, that.id)
                && Objects.equals(this.jobId, that.jobId)
                && Objects.equals(this.frequency, that.frequency)
                && Objects.equals(this.queryDelay, that.queryDelay)
                && Objects.equals(this.indices, that.indices)
                && Objects.equals(this.query, that.query)
                && Objects.equals(this.scrollSize, that.scrollSize)
                && Objects.equals(this.aggregations, that.aggregations)
                && Objects.equals(this.delayedDataCheckConfig, that.delayedDataCheckConfig)
                && Objects.equals(this.scriptFields, that.scriptFields)
                && Objects.equals(this.chunkingConfig, that.chunkingConfig);
    }

    @Override
    public int hashCode() {
        return Objects.hash(id, jobId, frequency, queryDelay, indices, query, scrollSize, aggregations, scriptFields, chunkingConfig,
                delayedDataCheckConfig);
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }

    boolean isNoop(DatafeedConfig datafeed) {
        return (frequency == null || Objects.equals(frequency, datafeed.getFrequency()))
                && (queryDelay == null || Objects.equals(queryDelay, datafeed.getQueryDelay()))
                && (indices == null || Objects.equals(indices, datafeed.getIndices()))
                && (query == null || Objects.equals(query, datafeed.getQuery()))
                && (scrollSize == null || Objects.equals(scrollSize, datafeed.getQueryDelay()))
                && (aggregations == null || Objects.equals(aggregations, datafeed.getAggregations()))
                && (scriptFields == null || Objects.equals(scriptFields, datafeed.getScriptFields()))
                && (delayedDataCheckConfig == null || Objects.equals(delayedDataCheckConfig, datafeed.getDelayedDataCheckConfig()))
                && (chunkingConfig == null || Objects.equals(chunkingConfig, datafeed.getChunkingConfig()));
    }

    public static class Builder {

        private String id;
        private String jobId;
        private TimeValue queryDelay;
        private TimeValue frequency;
        private List<String> indices;
        private Map<String, Object> query;
        private Map<String, Object> aggregations;
        private List<SearchSourceBuilder.ScriptField> scriptFields;
        private Integer scrollSize;
        private ChunkingConfig chunkingConfig;
        private DelayedDataCheckConfig delayedDataCheckConfig;

        public Builder() {
        }

        public Builder(String id) {
            this.id = ExceptionsHelper.requireNonNull(id, DatafeedConfig.ID.getPreferredName());
        }

        public Builder(DatafeedUpdate config) {
            this.id = config.id;
            this.jobId = config.jobId;
            this.queryDelay = config.queryDelay;
            this.frequency = config.frequency;
            this.indices = config.indices;
            this.query = config.query;
            this.aggregations = config.aggregations;
            this.scriptFields = config.scriptFields;
            this.scrollSize = config.scrollSize;
            this.chunkingConfig = config.chunkingConfig;
            this.delayedDataCheckConfig = config.delayedDataCheckConfig;
        }

        public void setId(String datafeedId) {
            id = ExceptionsHelper.requireNonNull(datafeedId, DatafeedConfig.ID.getPreferredName());
        }

        public void setJobId(String jobId) {
            this.jobId = jobId;
        }

        public void setIndices(List<String> indices) {
            this.indices = indices;
        }

        public void setQueryDelay(TimeValue queryDelay) {
            this.queryDelay = queryDelay;
        }

        public void setFrequency(TimeValue frequency) {
            this.frequency = frequency;
        }

        public void setQuery(Map<String, Object> query) {
            this.query = query;
            try {
                QUERY_TRANSFORMER.fromMap(query);
            } catch(Exception ex) {
                String msg = Messages.getMessage(Messages.DATAFEED_CONFIG_QUERY_BAD_FORMAT, id);

                if (ex.getCause() instanceof IllegalArgumentException) {
                    ex = (Exception)ex.getCause();
                }
                throw ExceptionsHelper.badRequestException(msg, ex);
            }
        }

        private void setAggregationsSafe(Map<String, Object> aggregations) {
            if (this.aggregations != null) {
                throw ExceptionsHelper.badRequestException("Found two aggregation definitions: [aggs] and [aggregations]");
            }
            setAggregations(aggregations);
        }

        public void setAggregations(Map<String, Object> aggregations) {
            this.aggregations = aggregations;
            try {
                if (aggregations != null && aggregations.isEmpty()) {
                    throw new Exception("[aggregations] are empty");
                }
                AGG_TRANSFORMER.fromMap(aggregations);
            } catch(Exception ex) {
                String msg = Messages.getMessage(Messages.DATAFEED_CONFIG_AGG_BAD_FORMAT, id);

                if (ex.getCause() instanceof IllegalArgumentException) {
                    ex = (Exception)ex.getCause();
                }
                throw ExceptionsHelper.badRequestException(msg, ex);
            }
        }

        public void setScriptFields(List<SearchSourceBuilder.ScriptField> scriptFields) {
            List<SearchSourceBuilder.ScriptField> sorted = new ArrayList<>(scriptFields);
            sorted.sort(Comparator.comparing(SearchSourceBuilder.ScriptField::fieldName));
            this.scriptFields = sorted;
        }

        public void setDelayedDataCheckConfig(DelayedDataCheckConfig delayedDataCheckConfig) {
            this.delayedDataCheckConfig = delayedDataCheckConfig;
        }

        public void setScrollSize(int scrollSize) {
            this.scrollSize = scrollSize;
        }

        public void setChunkingConfig(ChunkingConfig chunkingConfig) {
            this.chunkingConfig = chunkingConfig;
        }

        public DatafeedUpdate build() {
            return new DatafeedUpdate(id, jobId, queryDelay, frequency, indices, query, aggregations, scriptFields, scrollSize,
                    chunkingConfig, delayedDataCheckConfig);
        }
    }
}
