/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.action.datastreams;

import org.elasticsearch.TransportVersions;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.IndicesRequest;
import org.elasticsearch.action.admin.indices.rollover.RolloverConfiguration;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.master.MasterNodeReadRequest;
import org.elasticsearch.cluster.SimpleDiffable;
import org.elasticsearch.cluster.health.ClusterHealthStatus;
import org.elasticsearch.cluster.metadata.DataStream;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xcontent.ToXContentObject;
import org.elasticsearch.xcontent.XContentBuilder;

import java.io.IOException;
import java.time.Instant;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.Objects;

import static org.elasticsearch.TransportVersions.DATA_STREAM_RESPONSE_INDEX_PROPERTIES;

public class GetDataStreamAction extends ActionType<GetDataStreamAction.Response> {

    public static final GetDataStreamAction INSTANCE = new GetDataStreamAction();
    public static final String NAME = "indices:admin/data_stream/get";

    private GetDataStreamAction() {
        super(NAME, Response::new);
    }

    public static class Request extends MasterNodeReadRequest<Request> implements IndicesRequest.Replaceable {

        private String[] names;
        private IndicesOptions indicesOptions = IndicesOptions.fromOptions(false, true, true, true, false, false, true, false);
        private boolean includeDefaults = false;

        public Request(String[] names) {
            this.names = names;
        }

        public Request(String[] names, boolean includeDefaults) {
            this.names = names;
            this.includeDefaults = includeDefaults;
        }

        public String[] getNames() {
            return names;
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        public Request(StreamInput in) throws IOException {
            super(in);
            this.names = in.readOptionalStringArray();
            this.indicesOptions = IndicesOptions.readIndicesOptions(in);
            if (in.getTransportVersion().onOrAfter(TransportVersions.V_8_500_020)) {
                this.includeDefaults = in.readBoolean();
            } else {
                this.includeDefaults = false;
            }
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeOptionalStringArray(names);
            indicesOptions.writeIndicesOptions(out);
            if (out.getTransportVersion().onOrAfter(TransportVersions.V_8_500_020)) {
                out.writeBoolean(includeDefaults);
            }
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Request request = (Request) o;
            return Arrays.equals(names, request.names)
                && indicesOptions.equals(request.indicesOptions)
                && includeDefaults == request.includeDefaults;
        }

        @Override
        public int hashCode() {
            int result = Objects.hash(indicesOptions, includeDefaults);
            result = 31 * result + Arrays.hashCode(names);
            return result;
        }

        @Override
        public String[] indices() {
            return names;
        }

        @Override
        public IndicesOptions indicesOptions() {
            return indicesOptions;
        }

        public boolean includeDefaults() {
            return includeDefaults;
        }

        public Request indicesOptions(IndicesOptions indicesOptions) {
            this.indicesOptions = indicesOptions;
            return this;
        }

        @Override
        public boolean includeDataStreams() {
            return true;
        }

        @Override
        public IndicesRequest indices(String... indices) {
            this.names = indices;
            return this;
        }

        public Request includeDefaults(boolean includeDefaults) {
            this.includeDefaults = includeDefaults;
            return this;
        }
    }

    public static class Response extends ActionResponse implements ToXContentObject {

        public enum ManagedBy {
            ILM("Index Lifecycle Management"),
            LIFECYCLE("Data stream lifecycle"),
            UNMANAGED("Unmanaged");

            public final String displayValue;

            ManagedBy(String displayValue) {
                this.displayValue = displayValue;
            }
        }

        public static final ParseField DATA_STREAMS_FIELD = new ParseField("data_streams");

        public static class DataStreamInfo implements SimpleDiffable<DataStreamInfo>, ToXContentObject {

            public static final ParseField STATUS_FIELD = new ParseField("status");
            public static final ParseField INDEX_TEMPLATE_FIELD = new ParseField("template");
            public static final ParseField PREFER_ILM = new ParseField("prefer_ilm");
            public static final ParseField MANAGED_BY = new ParseField("managed_by");
            public static final ParseField NEXT_GENERATION_INDEX_MANAGED_BY = new ParseField("next_generation_managed_by");
            public static final ParseField ILM_POLICY_FIELD = new ParseField("ilm_policy");
            public static final ParseField LIFECYCLE_FIELD = new ParseField("lifecycle");
            public static final ParseField HIDDEN_FIELD = new ParseField("hidden");
            public static final ParseField SYSTEM_FIELD = new ParseField("system");
            public static final ParseField ALLOW_CUSTOM_ROUTING = new ParseField("allow_custom_routing");
            public static final ParseField REPLICATED = new ParseField("replicated");
            public static final ParseField TIME_SERIES = new ParseField("time_series");
            public static final ParseField TEMPORAL_RANGES = new ParseField("temporal_ranges");
            public static final ParseField TEMPORAL_RANGE_START = new ParseField("start");
            public static final ParseField TEMPORAL_RANGE_END = new ParseField("end");

            private final DataStream dataStream;
            private final ClusterHealthStatus dataStreamStatus;
            @Nullable
            private final String indexTemplate;
            @Nullable
            private final String ilmPolicyName;
            @Nullable
            private final TimeSeries timeSeries;
            private final Map<Index, IndexProperties> indexSettingsValues;
            private final boolean templatePreferIlmValue;

            public DataStreamInfo(
                DataStream dataStream,
                ClusterHealthStatus dataStreamStatus,
                @Nullable String indexTemplate,
                @Nullable String ilmPolicyName,
                @Nullable TimeSeries timeSeries,
                Map<Index, IndexProperties> indexSettingsValues,
                boolean templatePreferIlmValue
            ) {
                this.dataStream = dataStream;
                this.dataStreamStatus = dataStreamStatus;
                this.indexTemplate = indexTemplate;
                this.ilmPolicyName = ilmPolicyName;
                this.timeSeries = timeSeries;
                this.indexSettingsValues = indexSettingsValues;
                this.templatePreferIlmValue = templatePreferIlmValue;
            }

            @SuppressWarnings("unchecked")
            DataStreamInfo(StreamInput in) throws IOException {
                this(
                    new DataStream(in),
                    ClusterHealthStatus.readFrom(in),
                    in.readOptionalString(),
                    in.readOptionalString(),
                    in.getTransportVersion().onOrAfter(TransportVersions.V_8_3_0) ? in.readOptionalWriteable(TimeSeries::new) : null,
                    in.getTransportVersion().onOrAfter(DATA_STREAM_RESPONSE_INDEX_PROPERTIES)
                        ? in.readMap(Index::new, IndexProperties::new)
                        : Map.of(),
                    in.getTransportVersion().onOrAfter(DATA_STREAM_RESPONSE_INDEX_PROPERTIES) ? in.readBoolean() : true
                );
            }

            public DataStream getDataStream() {
                return dataStream;
            }

            public ClusterHealthStatus getDataStreamStatus() {
                return dataStreamStatus;
            }

            @Nullable
            public String getIndexTemplate() {
                return indexTemplate;
            }

            @Nullable
            public String getIlmPolicy() {
                return ilmPolicyName;
            }

            @Nullable
            public TimeSeries getTimeSeries() {
                return timeSeries;
            }

            public Map<Index, IndexProperties> getIndexSettingsValues() {
                return indexSettingsValues;
            }

            public boolean templatePreferIlmValue() {
                return templatePreferIlmValue;
            }

            @Override
            public void writeTo(StreamOutput out) throws IOException {
                dataStream.writeTo(out);
                dataStreamStatus.writeTo(out);
                out.writeOptionalString(indexTemplate);
                out.writeOptionalString(ilmPolicyName);
                if (out.getTransportVersion().onOrAfter(TransportVersions.V_8_3_0)) {
                    out.writeOptionalWriteable(timeSeries);
                }
                if (out.getTransportVersion().onOrAfter(DATA_STREAM_RESPONSE_INDEX_PROPERTIES)) {
                    out.writeMap(indexSettingsValues);
                    out.writeBoolean(templatePreferIlmValue);
                }
            }

            @Override
            public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
                return toXContent(builder, params, null);
            }

            /**
             * Converts the response to XContent and passes the RolloverConditions, when provided, to the data stream.
             */
            public XContentBuilder toXContent(XContentBuilder builder, Params params, @Nullable RolloverConfiguration rolloverConfiguration)
                throws IOException {
                builder.startObject();
                builder.field(DataStream.NAME_FIELD.getPreferredName(), dataStream.getName());
                builder.field(DataStream.TIMESTAMP_FIELD_FIELD.getPreferredName())
                    .startObject()
                    .field(DataStream.NAME_FIELD.getPreferredName(), DataStream.TIMESTAMP_FIELD_NAME)
                    .endObject();

                builder.field(DataStream.INDICES_FIELD.getPreferredName());
                if (dataStream.getIndices() == null) {
                    builder.nullValue();
                } else {
                    builder.startArray();
                    for (Index index : dataStream.getIndices()) {
                        builder.startObject();
                        index.toXContentFragment(builder);
                        IndexProperties indexProperties = indexSettingsValues.get(index);
                        if (indexProperties != null) {
                            builder.field(PREFER_ILM.getPreferredName(), indexProperties.preferIlm());
                            if (indexProperties.ilmPolicyName() != null) {
                                builder.field(ILM_POLICY_FIELD.getPreferredName(), indexProperties.ilmPolicyName());
                            }
                            builder.field(MANAGED_BY.getPreferredName(), indexProperties.managedBy.displayValue);
                        }
                        builder.endObject();
                    }
                    builder.endArray();
                }
                builder.field(DataStream.GENERATION_FIELD.getPreferredName(), dataStream.getGeneration());
                if (dataStream.getMetadata() != null) {
                    builder.field(DataStream.METADATA_FIELD.getPreferredName(), dataStream.getMetadata());
                }
                builder.field(STATUS_FIELD.getPreferredName(), dataStreamStatus);
                if (indexTemplate != null) {
                    builder.field(INDEX_TEMPLATE_FIELD.getPreferredName(), indexTemplate);
                }
                if (dataStream.getLifecycle() != null) {
                    builder.field(LIFECYCLE_FIELD.getPreferredName());
                    dataStream.getLifecycle().toXContent(builder, params, rolloverConfiguration);
                }
                if (ilmPolicyName != null) {
                    builder.field(ILM_POLICY_FIELD.getPreferredName(), ilmPolicyName);
                }
                builder.field(NEXT_GENERATION_INDEX_MANAGED_BY.getPreferredName(), getNextGenerationManagedBy().displayValue);
                builder.field(PREFER_ILM.getPreferredName(), templatePreferIlmValue);
                builder.field(HIDDEN_FIELD.getPreferredName(), dataStream.isHidden());
                builder.field(SYSTEM_FIELD.getPreferredName(), dataStream.isSystem());
                builder.field(ALLOW_CUSTOM_ROUTING.getPreferredName(), dataStream.isAllowCustomRouting());
                builder.field(REPLICATED.getPreferredName(), dataStream.isReplicated());
                if (timeSeries != null) {
                    builder.startObject(TIME_SERIES.getPreferredName());
                    builder.startArray(TEMPORAL_RANGES.getPreferredName());
                    for (var range : timeSeries.temporalRanges()) {
                        builder.startObject();
                        Instant start = range.v1();
                        builder.field(TEMPORAL_RANGE_START.getPreferredName(), DateFieldMapper.DEFAULT_DATE_TIME_FORMATTER.format(start));
                        Instant end = range.v2();
                        builder.field(TEMPORAL_RANGE_END.getPreferredName(), DateFieldMapper.DEFAULT_DATE_TIME_FORMATTER.format(end));
                        builder.endObject();
                    }
                    builder.endArray();
                    builder.endObject();
                }
                builder.endObject();
                return builder;
            }

            /**
             * Computes and returns which system will manage the next generation for this data stream.
             */
            public ManagedBy getNextGenerationManagedBy() {
                // both ILM and DSL are configured so let's check the prefer_ilm setting to see which system takes precedence
                if (ilmPolicyName != null && dataStream.getLifecycle() != null && dataStream.getLifecycle().isEnabled()) {
                    return templatePreferIlmValue ? ManagedBy.ILM : ManagedBy.LIFECYCLE;
                }

                if (ilmPolicyName != null) {
                    return ManagedBy.ILM;
                }

                if (dataStream.getLifecycle() != null && dataStream.getLifecycle().isEnabled()) {
                    return ManagedBy.LIFECYCLE;
                }

                return ManagedBy.UNMANAGED;
            }

            @Override
            public boolean equals(Object o) {
                if (this == o) {
                    return true;
                }
                if (o == null || getClass() != o.getClass()) {
                    return false;
                }
                DataStreamInfo that = (DataStreamInfo) o;
                return templatePreferIlmValue == that.templatePreferIlmValue
                    && Objects.equals(dataStream, that.dataStream)
                    && dataStreamStatus == that.dataStreamStatus
                    && Objects.equals(indexTemplate, that.indexTemplate)
                    && Objects.equals(ilmPolicyName, that.ilmPolicyName)
                    && Objects.equals(timeSeries, that.timeSeries)
                    && Objects.equals(indexSettingsValues, that.indexSettingsValues);
            }

            @Override
            public int hashCode() {
                return Objects.hash(
                    dataStream,
                    dataStreamStatus,
                    indexTemplate,
                    ilmPolicyName,
                    timeSeries,
                    indexSettingsValues,
                    templatePreferIlmValue
                );
            }
        }

        public record TimeSeries(List<Tuple<Instant, Instant>> temporalRanges) implements Writeable {

            TimeSeries(StreamInput in) throws IOException {
                this(in.readCollectionAsList(in1 -> new Tuple<>(in1.readInstant(), in1.readInstant())));
            }

            @Override
            public void writeTo(StreamOutput out) throws IOException {
                out.writeCollection(temporalRanges, (out1, value) -> {
                    out1.writeInstant(value.v1());
                    out1.writeInstant(value.v2());
                });
            }

            @Override
            public boolean equals(Object o) {
                if (this == o) return true;
                if (o == null || getClass() != o.getClass()) return false;
                TimeSeries that = (TimeSeries) o;
                return temporalRanges.equals(that.temporalRanges);
            }

            @Override
            public int hashCode() {
                return Objects.hash(temporalRanges);
            }
        }

        /**
         * Encapsulates the configured properties we want to display for each backing index.
         * They'll usually be settings values, but could also be additional properties derived from settings.
         */
        public record IndexProperties(boolean preferIlm, @Nullable String ilmPolicyName, ManagedBy managedBy) implements Writeable {
            public IndexProperties(StreamInput in) throws IOException {
                this(in.readBoolean(), in.readOptionalString(), in.readEnum(ManagedBy.class));
            }

            @Override
            public void writeTo(StreamOutput out) throws IOException {
                out.writeBoolean(preferIlm);
                out.writeOptionalString(ilmPolicyName);
                out.writeEnum(managedBy);
            }
        }

        private final List<DataStreamInfo> dataStreams;
        @Nullable
        private final RolloverConfiguration rolloverConfiguration;

        public Response(List<DataStreamInfo> dataStreams) {
            this(dataStreams, null);
        }

        public Response(List<DataStreamInfo> dataStreams, @Nullable RolloverConfiguration rolloverConfiguration) {
            this.dataStreams = dataStreams;
            this.rolloverConfiguration = rolloverConfiguration;
        }

        public Response(StreamInput in) throws IOException {
            this(
                in.readCollectionAsList(DataStreamInfo::new),
                in.getTransportVersion().onOrAfter(TransportVersions.V_8_500_020)
                    ? in.readOptionalWriteable(RolloverConfiguration::new)
                    : null
            );
        }

        public List<DataStreamInfo> getDataStreams() {
            return dataStreams;
        }

        @Nullable
        public RolloverConfiguration getRolloverConfiguration() {
            return rolloverConfiguration;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeCollection(dataStreams);
            if (out.getTransportVersion().onOrAfter(TransportVersions.V_8_500_020)) {
                out.writeOptionalWriteable(rolloverConfiguration);
            }
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.startArray(DATA_STREAMS_FIELD.getPreferredName());
            for (DataStreamInfo dataStream : dataStreams) {
                dataStream.toXContent(builder, params, rolloverConfiguration);
            }
            builder.endArray();
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Response response = (Response) o;
            return dataStreams.equals(response.dataStreams) && Objects.equals(rolloverConfiguration, response.rolloverConfiguration);
        }

        @Override
        public int hashCode() {
            return Objects.hash(dataStreams, rolloverConfiguration);
        }
    }

}
